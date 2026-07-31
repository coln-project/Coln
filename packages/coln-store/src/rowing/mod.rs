// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Structural row identity (hashcons) and canonical row ids.
//!
//! Why is it called rowing? Because it is comparing rows for equivalence by
//! looking at each value of the row one by one. And it sounds cool. Anyway it
//! will probably be renamed in the future.

use std::collections::{HashMap, HashSet};

use crate::{
    commit::hash_dict::HashMapper,
    rowing::uf::UnionFind,
    table::{CellValue, PackedCell, PackedRowId, RowId, Table, TableOid},
};

mod uf;

type NodeId = u32;

#[derive(Debug)]
pub(crate) enum ObservedOutcome {
    /// The row is new: store it with these values, whose id cells are
    /// canonicalized.
    Inserted {
        rid: RowId,
        values: Vec<CellValue>,
    },
    KeptOld(RowId),
    Swap {
        old: RowId,
        new: RowId,
    },
}

/// A stored row whose id cells went stale because a canonical id changed.
/// The store must rewrite the row's cells to `values` in `table`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CellFixup {
    pub(crate) table: TableOid,
    pub(crate) row: RowId,
    pub(crate) values: Vec<PackedCell>,
}

/// Result of observing one row: the outcome for the observed row itself,
/// plus cell rewrites for previously stored rows affected by a canonical id
/// change.
#[derive(Debug)]
pub(crate) struct Observed {
    pub(crate) outcome: ObservedOutcome,
    pub(crate) fixups: Vec<CellFixup>,
}

// #[derive(Debug, thiserror::Error)]
// pub enum RowingError {
//     #[error("Child row missing {rid}")]
//     MissingChild { rid: RowId },
//     #[error("duplicate rowid {rid} with different row values")]
//     InconsistentRow { rid: RowId },
// }

// A data structure that allows the tables to query what the canonical ids
// should be
pub(crate) struct Canonicaliser<'a> {
    uf: &'a UnionFind,
}

impl<'a> Canonicaliser<'a> {
    pub(crate) fn canonical_id(&self, rid: PackedRowId) -> PackedRowId {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Rowing {
    uf: UnionFind,
    // stale rowids
    displaced: Vec<PackedRowId>,
    // index into how many dispalced ids we have processed
    rebuild_cursor: usize,

    row_to_table: HashMap<PackedRowId, TableOid>,
}

impl Rowing {
    pub(crate) fn new() -> Self {
        todo!()
    }

    // When we found that two ids have equivalent row structures, stage them in
    // the rowing data structure.
    // This is equivalent in egglog terms with a union(a, c), except don't allow
    // arbitrary union, but only identify structurally identical terms.
    fn stage_identical_row(
        &mut self,
        table: TableOid,
        old_rid: PackedRowId,
        new_rid: PackedRowId,
        id_packer: &HashMapper,
    ) {
        todo!()
    }

    // Apply all the staged unions and populate the displaced table. The displaced
    // table will then contain all the old rowids that need to be updated, which
    // can then be mapped to tables with `row_to_table`
    fn apply_updates(&mut self) {
        todo!()
    }

    pub(crate) fn rebuilder(&self) -> Canonicaliser {
        todo!()
    }

    pub(crate) fn uf_len(&self) -> usize {
        todo!()
    }

    // pub(crate) fn observe(
    //     &mut self,
    //     table_oid: TableOid,
    //     tables: &HashMap<TableOid, Table>,
    //     dict: &mut HashMapper,
    //     rid: RowId,
    //     values: &[CellValue],
    // ) -> Observed {
    //     let table = tables
    //         .get(&table_oid)
    //         .expect("observed table is registered");
    //     let prid = PackedRowId::pack(rid, dict);
    //     let (canon_values, child_refs) = self.canonicalize_row_values(table, values, dict);
    //     self.node_for(prid);

    //     if let Some(_old) = self.row_to_table.get(&prid) {
    //         unreachable!(
    //             "We should have already checked that same rowid would not lead to different row values"
    //         );
    //         // If the rowid already exists, we assume it's the same content, so
    //         // we do nothing. This should be checked before we apply the commit.

    //         // We should check the canonical row id for consistency, it might be
    //         // the case that we are checking a non-canonical rid.
    //         // let physical = self.canonical_packed(prid);
    //         // let old_values = tables
    //         //     .get(&old_table_oid)
    //         //     .and_then(|table| table.packed_row_at(physical))
    //         //     .expect("an observed class has a physical table row");
    //         // if old_table_oid != table_oid || old_values != canon_values {
    //         //     return Err(RowingError::InconsistentRow { rid });
    //         // }
    //     } else {
    //         self.row_to_table.insert(prid, table_oid);
    //     }

    //     // Register the row as a parent of each referenced child class, so a
    //     // later change of the id that class member resolves to re-keys this
    //     // row.
    //     for child in child_refs {
    //         let child_node = self.node_for(child);
    //         let child_root = self.uf.find(child_node);
    //         let list = self.parents.entry(child_root).or_default();
    //         if !list.contains(&prid) {
    //             list.push(prid);
    //         }
    //     }

    //     let unpacked_con = canon_values.iter().map(|p| p.unpack_cell(dict)).collect();

    //     let inserted_outcome = Observed {
    //         outcome: ObservedOutcome::Inserted {
    //             rid,
    //             values: unpacked_con,
    //         },
    //         fixups: Vec::new(),
    //     };

    //     if !table.hashcons() {
    //         return inserted_outcome;
    //     }

    //     let existing = table
    //         .index_seek_packed(
    //             table
    //                 .hashcons_index()
    //                 .expect("hashcons index to exist on hashcons table"),
    //             &canon_values,
    //         )
    //         .expect("hashcons index id is valid for this table")
    //         .next();

    //     match existing {
    //         None => inserted_outcome,
    //         Some(existing) => self.merge(prid, existing, tables, dict),
    //     }
    // }

    // /// Canonical row values and packed child refs for parent registration.
    // /// Hashcons rows must be observed in dependency order: every referenced
    // /// child row has already been observed. Non-hashcons rows may reference
    // /// unobserved rows, whose canonical id is themselves. When a later merge
    // /// changes a referenced id, [`Self::rekey_parents`] re-keys the affected
    // /// rows.
    // fn canonicalize_row_values(
    //     &self,
    //     table: &Table,
    //     values: &[CellValue],
    //     dict: &mut HashMapper,
    // ) -> (Vec<PackedCell>, Vec<PackedRowId>) {
    //     let mut canon_values = Vec::with_capacity(values.len());
    //     let mut child_refs = Vec::new();
    //     for cell in values {
    //         match cell {
    //             CellValue::Id(child) => {
    //                 let packed = if table.hashcons() {
    //                     PackedRowId::lookup(*child, dict)
    //                         .filter(|packed| self.row_to_table.contains_key(packed))
    //                         .expect(&format!("hashcons child {child} must be known to us"))
    //                 } else {
    //                     PackedRowId::pack(*child, dict)
    //                 };
    //                 let canonical = self.canonical_packed(packed);
    //                 child_refs.push(canonical);
    //                 canon_values.push(PackedCell::Id(canonical));
    //             }
    //             CellValue::Int(i) => canon_values.push(PackedCell::Int(*i)),
    //             CellValue::Str(s) => canon_values.push(PackedCell::Str(s.clone())),
    //         }
    //     }
    //     (canon_values, child_refs)
    // }
    // /*
    // a b
    // f(a, b)

    // c
    // f(c, b)

    // a == c

    // a is canonical
    // then c needs to be changed
    // as is f(c, b)

    // c is canonical
    // then a needs to be changed, as is f(a, b)

    //  */
    // /// Union the classes of `prid` and `existing`, keeping the smaller
    // /// canonical id (compared as unpacked [`RowId`]s, for cross-store
    // /// determinism). When any member's resolved id changed, re-key the
    // /// class's parents and report their table fixups.
    // fn merge(
    //     &mut self,
    //     prid: PackedRowId,
    //     existing: PackedRowId,
    //     tables: &HashMap<TableOid, Table>,
    //     dict: &mut HashMapper,
    // ) -> Observed {
    //     let node = self.node_for(prid);
    //     let existing_node = self.node_for(existing);

    //     let old_canonical = self.canonical_packed(existing);
    //     let rid_canonical = self.canonical_packed(prid);
    //     let new_canonical = if rid_canonical.unpack(dict) < old_canonical.unpack(dict) {
    //         rid_canonical
    //     } else {
    //         old_canonical
    //     };

    //     // Roots before the union; the survivor is one of these two, so the
    //     // loser becomes a stale key in `canonical_row` and `parents`.
    //     let root_a = self.uf.find(node);
    //     let root_b = self.uf.find(existing_node);
    //     self.uf.union(node, existing_node);

    //     let root = self.uf.find(node);
    //     for stale_root in [root_a, root_b] {
    //         if stale_root == root {
    //             continue;
    //         }
    //         self.canonical_row.remove(&stale_root);
    //         // Parents lists follow the class to its surviving root.
    //         if let Some(moved) = self.parents.remove(&stale_root) {
    //             let list = self.parents.entry(root).or_default();
    //             for parent in moved {
    //                 if !list.contains(&parent) {
    //                     list.push(parent);
    //                 }
    //             }
    //         }
    //     }

    //     self.canonical_row.insert(root, new_canonical);

    //     // The losing side's members now resolve to `new_canonical`, so cells
    //     // embedding their previous canonical id are stale and must be
    //     // rewritten now, before any further lookup can miss them. When both
    //     // sides already resolved to the same id (a re-observation), nothing
    //     // changed.
    //     let fixups = if rid_canonical == old_canonical {
    //         Vec::new()
    //     } else {
    //         self.rekey_parents(root, tables, dict)
    //     };

    //     let outcome = if new_canonical == old_canonical {
    //         ObservedOutcome::KeptOld(old_canonical.unpack(dict))
    //     } else {
    //         ObservedOutcome::Swap {
    //             old: old_canonical.unpack(dict),
    //             new: new_canonical.unpack(dict),
    //         }
    //     };
    //     Observed { outcome, fixups }
    // }

    // /// Re-express the cells of every parent of `root`'s class in current
    // /// canonical ids, emit [`CellFixup`]s for the parent's table storage.
    // /// Called when the id some class member resolves to changed.
    // ///
    // /// One level of rekeying is sufficient. We can this in, for example
    // ///
    // /// B is currently canonical.
    // /// A arrives which is more canonical than B.
    // /// Pair(B, X) -> Pair(A, X).
    // /// But there is no need to rewrite further because A is completely new. And
    // /// all references to Pair(B, X) remain valid because its row_id remain the same.
    // fn rekey_parents(
    //     &mut self,
    //     root: NodeId,
    //     tables: &HashMap<TableOid, Table>,
    //     dict: &mut HashMapper,
    // ) -> Vec<CellFixup> {
    //     let mut fixups = Vec::new();
    //     // Parents in one class share one physical row; fix it up once.
    //     let mut fixed_rows: HashSet<(TableOid, PackedRowId)> = HashSet::new();
    //     let parent_rids = self.parents.get(&root).cloned().unwrap_or_default();
    //     for prid in parent_rids {
    //         let ptbl = self
    //             .row_to_table
    //             .get(&prid)
    //             .expect("registered parents were observed")
    //             .to_owned();
    //         let target = self.canonical_packed(prid);
    //         if !fixed_rows.insert((ptbl, target)) {
    //             continue;
    //         }
    //         let old_values = tables
    //             .get(&ptbl)
    //             .and_then(|table| table.packed_row_at(target))
    //             .expect("an observed parent class has a physical table row");
    //         let new_values: Vec<PackedCell> = old_values
    //             .iter()
    //             .map(|cell| match cell {
    //                 PackedCell::Id(packed) => PackedCell::Id(self.canonical_packed(*packed)),
    //                 other => other.clone(),
    //             })
    //             .collect();
    //         if new_values == old_values {
    //             continue;
    //         }

    //         fixups.push(CellFixup {
    //             table: ptbl,
    //             row: target.unpack(dict),
    //             values: new_values,
    //         });
    //     }
    //     fixups
    // }
}
