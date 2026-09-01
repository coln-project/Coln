// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Structural row identity and canonical row ids.
//!
//! Why is it called rowing? Because it is comparing rows for equivalence by
//! looking at each value of the row one by one. And it sounds cool. Anyway it
//! will probably be renamed in the future.

mod uf;

use std::{cell::RefCell, collections::HashMap};

use ena::unify::{InPlace, Snapshot};

use crate::{
    id_packer::IdPacker,
    rollback::Rollback,
    rowing::uf::{NodeId, UnionFind},
    table::{PackedRowId, TableOid},
};

#[derive(Debug)]
pub(crate) struct Rowing {
    // interior mutability because uf.find requires &mut self
    uf: RefCell<UnionFind>,
    keys: HashMap<PackedRowId, NodeId>,
    // stale rowids
    displaced: Vec<PackedRowId>,

    pending_unions: Vec<(TableOid, PackedRowId, PackedRowId)>,
}

#[must_use]
pub(crate) struct RowingSnapshot {
    uf_snapshot: Snapshot<InPlace<NodeId>>,
    keys: HashMap<PackedRowId, NodeId>,
    displaced_len: usize,
    pending_unions: Vec<(TableOid, PackedRowId, PackedRowId)>,
}

impl Rollback for Rowing {
    type Snapshot = RowingSnapshot;

    fn snapshot(&mut self) -> Self::Snapshot {
        // TODO all doing cloning. If conflicts are rare, this is probably fine.
        RowingSnapshot {
            uf_snapshot: self.uf.get_mut().snapshot(),
            keys: self.keys.clone(),
            displaced_len: self.displaced.len(),
            pending_unions: self.pending_unions.clone(),
        }
    }

    fn commit_snapshot(&mut self, snapshot: Self::Snapshot) {
        self.uf.get_mut().commit(snapshot.uf_snapshot);
    }

    fn rollback(&mut self, snapshot: Self::Snapshot) {
        self.uf.get_mut().rollback_to(snapshot.uf_snapshot);
        self.keys = snapshot.keys;
        self.displaced.truncate(snapshot.displaced_len);
        self.pending_unions = snapshot.pending_unions;
    }
}

impl Rowing {
    pub(crate) fn new() -> Self {
        Self {
            uf: RefCell::new(UnionFind::new()),
            keys: HashMap::new(),
            displaced: Vec::new(),
            pending_unions: Vec::new(),
        }
    }

    // When we found that two ids have equivalent row structures, stage them in
    // the rowing data structure.
    // This is equivalent in egglog terms with a union(a, c), except don't allow
    // arbitrary union, but only identify structurally identical terms.
    pub(crate) fn stage_union(&mut self, table: TableOid, rid1: PackedRowId, rid2: PackedRowId) {
        tracing::debug!(table_id = %table, rid1 = ?rid1, rid2 = ?rid2, "staging unions");
        self.pending_unions.push((table, rid1, rid2));
    }

    // Apply all the staged unions and populate the displaced table. The displaced
    // table will then contain all the old rowids that need to be updated, which
    // can then be mapped to tables with `row_to_table`
    //? I think we can clear up pending unions after this so it is ready to be used
    // for tables during rebuild
    pub(crate) fn apply_unions(&mut self, id_packer: &IdPacker) {
        for (_tbl, r1, r2) in self.pending_unions.drain(..) {
            let mut uf = self.uf.borrow_mut();
            let unpacked1 = id_packer.unpack_row_id(r1);
            let unpacked2 = id_packer.unpack_row_id(r2);

            let k1 = *self.keys.entry(r1).or_insert_with(|| uf.new_key(unpacked1));
            let k2 = *self.keys.entry(r2).or_insert_with(|| uf.new_key(unpacked2));
            let canonical1 = uf.probe_value(k1);
            let canonical2 = uf.probe_value(k2);
            if canonical1 == canonical2 {
                continue;
            }

            uf.union(k1, k2);
            let displaced = canonical1.max(canonical2);
            self.displaced.push(
                id_packer
                    .lookup_row_id(displaced)
                    .expect("displaced row id was packed before union"),
            );
        }
    }

    pub(crate) fn canonical_id(&self, row_id: &PackedRowId, id_packer: &IdPacker) -> PackedRowId {
        let Some(&key) = self.keys.get(row_id) else {
            // if the keys is not known to rowing, then we have not done any uf
            // just return it
            return *row_id;
        };
        let canonical = self.uf.borrow_mut().probe_value(key);
        id_packer
            .lookup_row_id(canonical)
            .expect("canonical row id was packed before union")
    }

    /// Ids a union made stale. A rebuild pass resolves each one to its current
    /// canonical id itself, since it also has to canonicalise the cells of the
    /// rows that refer to them.
    pub(crate) fn displaced(&self) -> impl Iterator<Item = PackedRowId> {
        self.displaced.iter().copied()
    }

    pub(crate) fn has_displaced(&self) -> bool {
        !self.displaced.is_empty()
    }

    /// Drop the worklist once a rebuild pass has consumed it, so the next pass
    /// only sees ids displaced by unions staged after this point.
    pub(crate) fn clear_displaced(&mut self) {
        self.displaced.clear();
    }
}

#[cfg(test)]
mod tests {
    use crate::commit::hash::CommitHash;
    use crate::table::RowId;

    use super::*;

    fn row_id(byte: u8) -> RowId {
        RowId {
            commit: CommitHash([byte; 32]),
            counter: 0,
        }
    }

    #[test]
    fn union_uses_unpacked_order_and_records_displaced_id() {
        let mut packer = IdPacker::new();
        let low = packer.pack_row_id(row_id(1));
        let high = packer.pack_row_id(row_id(2));
        let unseen = packer.pack_row_id(row_id(3));
        let mut rowing = Rowing::new();

        rowing.stage_union(0, high, low);
        rowing.apply_unions(&packer);

        assert_eq!(rowing.canonical_id(&high, &packer), low);
        assert_eq!(rowing.canonical_id(&low, &packer), low);
        assert_eq!(rowing.canonical_id(&unseen, &packer), unseen);
        assert_eq!(rowing.displaced().collect::<Vec<_>>(), [high]);

        rowing.stage_union(0, low, high);
        rowing.apply_unions(&packer);
        assert_eq!(rowing.displaced().collect::<Vec<_>>(), [high]);
    }

    #[test]
    fn transitive_union_displaces_each_previous_canonical_id() {
        let mut packer = IdPacker::new();
        let low = packer.pack_row_id(row_id(1));
        let middle = packer.pack_row_id(row_id(2));
        let high = packer.pack_row_id(row_id(3));
        let mut rowing = Rowing::new();

        rowing.stage_union(0, high, middle);
        rowing.stage_union(0, high, low);
        rowing.apply_unions(&packer);

        assert_eq!(rowing.canonical_id(&high, &packer), low);
        assert_eq!(rowing.canonical_id(&middle, &packer), low);
        assert_eq!(rowing.displaced().collect::<Vec<_>>(), [high, middle]);
    }

    #[test]
    fn rollback_restores_union_state() {
        let mut packer = IdPacker::new();
        let low = packer.pack_row_id(row_id(1));
        let high = packer.pack_row_id(row_id(2));
        let mut rowing = Rowing::new();
        let snapshot = rowing.snapshot();

        rowing.stage_union(0, high, low);
        rowing.apply_unions(&packer);
        rowing.rollback(snapshot);

        assert_eq!(rowing.canonical_id(&high, &packer), high);
        assert_eq!(rowing.canonical_id(&low, &packer), low);
        assert!(!rowing.has_displaced());
    }

    #[test]
    fn clearing_displaced_keeps_canonical_ids_and_empties_the_worklist() {
        let mut packer = IdPacker::new();
        let first = packer.pack_row_id(row_id(1));
        let second = packer.pack_row_id(row_id(2));
        let third = packer.pack_row_id(row_id(3));
        let fourth = packer.pack_row_id(row_id(4));
        let mut rowing = Rowing::new();

        rowing.stage_union(0, second, first);
        rowing.stage_union(0, fourth, third);
        rowing.apply_unions(&packer);
        rowing.clear_displaced();

        assert!(!rowing.has_displaced());
        assert_eq!(rowing.canonical_id(&second, &packer), first);
        assert_eq!(rowing.canonical_id(&fourth, &packer), third);

        // Merging two classes that are both already known adds no union-find
        // key, but it still displaces an id and so still needs a rebuild pass.
        rowing.stage_union(0, third, first);
        rowing.apply_unions(&packer);

        assert_eq!(rowing.displaced().collect::<Vec<_>>(), [third]);
        assert_eq!(rowing.canonical_id(&fourth, &packer), first);
    }
}
