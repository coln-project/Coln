// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Structural row identity (hashcons) and canonical row ids.
//!
//! Why is it called rowing? Because it is comparing rows for equivalence by
//! looking at each value of the row one by one. And it sounds cool. Anyway it
//! will probably be renamed in the future.
//!
//! State is split in two:
//!
//! - Store-wide equivalence state: a union-find over rows plus the canonical
//!   id of each class. Equality is inherently cross-table, because a parent
//!   row's identity depends on the canonical ids of the child rows it
//!   references.
//! - Per-table structural indexes: one [`HexaneIndex`] per hashcons table,
//!   keyed on every column, holding the canonicalized cells of each class
//!   with exactly one entry per class. A lookup canonicalizes the incoming
//!   row's id cells first, so the index itself never needs to consult other
//!   tables.
//!
//! Table storage holds canonical id cells as well: [`Rowing::observe`]
//! reports the canonicalized values to insert, and when a canonical id later
//! changes it reports [`CellFixup`]s for every stored row whose cells embed
//! the affected class, so the store can rewrite them. Together these keep
//! the invariant that id cells in tables always name a stored, canonical
//! row.
//!
//! Everything is keyed by [`PackedRowId`] against the store-wide
//! [`HashMapper`]. Canonical id selection compares unpacked [`RowId`]s:
//! packed ids order by dictionary insertion order, which is not
//! deterministic across stores.

use std::collections::{HashMap, HashSet};

use coln_flir_rs::ir;
use petgraph::unionfind::UnionFind;

use crate::{
    commit::hash_dict::HashMapper,
    table::{CellValue, PackedRowId, RowId, Table, index::HexaneIndex},
};

type NodeId = u32;

/// One cell of an observed row in packed form.
#[derive(Debug, PartialEq, Eq, Clone)]
enum PackedCell {
    Id(PackedRowId),
    Int(i64),
    Str(String),
}

/// The cells of an observed row, as last expressed in `by_row`. Id cells
/// hold canonical ids as of the last observation or re-key;
/// [`Rowing::rekey_parents`] keeps them current.
#[derive(Debug, PartialEq, Eq, Clone)]
struct StoredRow {
    table: ir::Path,
    cells: Vec<PackedCell>,
}

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
    pub(crate) table: ir::Path,
    pub(crate) row: RowId,
    pub(crate) values: Vec<CellValue>,
}

/// Result of observing one row: the outcome for the observed row itself,
/// plus cell rewrites for previously stored rows affected by a canonical id
/// change.
#[derive(Debug)]
pub(crate) struct Observed {
    pub(crate) outcome: ObservedOutcome,
    pub(crate) fixups: Vec<CellFixup>,
}

#[derive(Debug, thiserror::Error)]
pub enum RowingError {
    #[error("Child row missing {rid}")]
    MissingChild { rid: RowId },
    #[error("duplicate rowid {rid} with different row values")]
    InconsistentRow { rid: RowId },
}

fn unpack_cells(cells: &[PackedCell], dict: &HashMapper) -> Vec<CellValue> {
    cells
        .iter()
        .map(|cell| match cell {
            PackedCell::Id(id) => CellValue::Id(id.unpack(dict)),
            PackedCell::Int(i) => CellValue::Int(*i),
            PackedCell::Str(s) => CellValue::Str(s.clone()),
        })
        .collect()
}

#[derive(Debug, Clone)]
pub(crate) struct Rowing {
    row_to_node: HashMap<PackedRowId, NodeId>,
    uf: UnionFind<NodeId>,
    // root node -> canonical row id of the class
    canonical_row: HashMap<NodeId, PackedRowId>,

    /// Class root -> observed rows whose cells embed a member of that class,
    /// hashcons or not. When the id a member resolves to changes, these rows
    /// hold stale cells in `by_row`, stale keys in their table's index (for
    /// hashcons rows), and stale cells in table storage, and must be re-keyed
    /// (congruence maintenance, as in e-graph rebuilding). Lists are merged
    /// into the surviving root on union.
    parents: HashMap<NodeId, Vec<PackedRowId>>,

    /// Every observed row. Serves the missing-child and inconsistent-row
    /// checks, and preserves each row's current canonical cells so re-keys
    /// can locate its index entry and compute its table fixup.
    by_row: HashMap<PackedRowId, StoredRow>,

    /// One structural index per hashcons table, keyed on all columns over
    /// canonicalized cells, with one entry per equivalence class. This is
    /// where hashcons lookup happens.
    indexes: HashMap<ir::Path, HexaneIndex>,
}

impl Rowing {
    pub(crate) fn new() -> Self {
        Self {
            row_to_node: HashMap::new(),
            uf: UnionFind::new_empty(),
            canonical_row: HashMap::new(),
            parents: HashMap::new(),
            by_row: HashMap::new(),
            indexes: HashMap::new(),
        }
    }

    fn canonical_packed(&self, id: PackedRowId) -> PackedRowId {
        self.row_to_node
            .get(&id)
            .map(|&node| self.uf.find(node))
            .and_then(|root| self.canonical_row.get(&root))
            .copied()
            .unwrap_or(id)
    }

    /// Canonical id of `row_id`'s class; `row_id` itself when never observed.
    ///
    /// This is how the store resolves row ids held by callers: an id that
    /// was deduplicated or swapped away still resolves to the stored row of
    /// its class.
    pub(super) fn canonical(&self, row_id: RowId, dict: &HashMapper) -> RowId {
        match PackedRowId::lookup(row_id, dict) {
            None => row_id,
            Some(packed) => self.canonical_packed(packed).unpack(dict),
        }
    }

    fn node_for(&mut self, id: PackedRowId) -> NodeId {
        if let Some(node) = self.row_to_node.get(&id) {
            *node
        } else {
            let node = self.uf.new_set();
            self.row_to_node.insert(id, node);
            node
        }
    }

    /// Packed cells of the observed row, with id cells canonicalized.
    /// Hashcons rows must be observed in dependency order: every referenced
    /// child row has already been observed. Non-hashcons rows may reference
    /// unobserved rows, whose canonical id is themselves. When a later merge
    /// changes a referenced id, [`Self::rekey_parents`] re-keys the affected
    /// rows.
    fn row_cells(
        &self,
        table: &Table,
        dict: &mut HashMapper,
        values: &[CellValue],
    ) -> Result<Vec<PackedCell>, RowingError> {
        values
            .iter()
            .map(|cell| match cell {
                CellValue::Id(child) => {
                    let packed = if table.hashcons() {
                        PackedRowId::lookup(*child, dict)
                            .filter(|packed| self.by_row.contains_key(packed))
                            .ok_or(RowingError::MissingChild { rid: *child })?
                    } else {
                        PackedRowId::pack(*child, dict)
                    };
                    Ok(PackedCell::Id(self.canonical_packed(packed)))
                }
                CellValue::Int(i) => Ok(PackedCell::Int(*i)),
                CellValue::Str(s) => Ok(PackedCell::Str(s.to_owned())),
            })
            .collect()
    }

    pub(crate) fn observe(
        &mut self,
        table: &Table,
        dict: &mut HashMapper,
        rid: RowId,
        values: &[CellValue],
    ) -> Result<Observed, RowingError> {
        let prid = PackedRowId::pack(rid, dict);
        let cells = self.row_cells(table, dict, values)?;
        let stored = StoredRow {
            table: table.path().clone(),
            cells,
        };
        self.node_for(prid);

        if let Some(old) = self.by_row.get(&prid) {
            // If we already saw this exact row id, it should have the same
            // cells.
            if *old != stored {
                return Err(RowingError::InconsistentRow { rid });
            }
        } else {
            self.by_row.insert(prid, stored.clone());
        }

        // Register the row as a parent of each referenced child class, so a
        // later change of the id that class member resolves to re-keys this
        // row.
        for cell in &stored.cells {
            if let PackedCell::Id(child) = cell {
                let child_node = self.node_for(*child);
                let child_root = self.uf.find(child_node);
                let list = self.parents.entry(child_root).or_default();
                if !list.contains(&prid) {
                    list.push(prid);
                }
            }
        }

        if !table.hashcons() {
            return Ok(Observed {
                outcome: ObservedOutcome::Inserted {
                    rid,
                    values: unpack_cells(&stored.cells, dict),
                },
                fixups: Vec::new(),
            });
        }

        let canon_values = unpack_cells(&stored.cells, dict);
        let refs: Vec<&CellValue> = canon_values.iter().collect();
        if !self.indexes.contains_key(table.path()) {
            let key_cols = (0..table.schema().columns.len()).collect();
            self.indexes.insert(
                table.path().clone(),
                HexaneIndex::new(key_cols, table.schema()),
            );
        }
        let index = self
            .indexes
            .get_mut(table.path())
            .expect("index inserted above");

        let existing = index.packed_row_ids_for(&refs, dict).next();
        match existing {
            None => {
                index.insert(&canon_values, prid, dict);
                Ok(Observed {
                    outcome: ObservedOutcome::Inserted {
                        rid,
                        values: canon_values,
                    },
                    fixups: Vec::new(),
                })
            }
            Some(existing) => Ok(self.merge(prid, existing, dict)),
        }
    }

    /// Union the classes of `prid` and `existing`, keeping the smaller
    /// canonical id (compared as unpacked [`RowId`]s, for cross-store
    /// determinism). When any member's resolved id changed, re-key the
    /// class's parents and report their table fixups.
    fn merge(
        &mut self,
        prid: PackedRowId,
        existing: PackedRowId,
        dict: &mut HashMapper,
    ) -> Observed {
        let node = self.node_for(prid);
        let existing_node = self.node_for(existing);

        let old_canonical = self.canonical_packed(existing);
        let rid_canonical = self.canonical_packed(prid);
        let new_canonical = if rid_canonical.unpack(dict) < old_canonical.unpack(dict) {
            rid_canonical
        } else {
            old_canonical
        };

        // Roots before the union; the survivor is one of these two, so the
        // loser becomes a stale key in `canonical_row` and `parents`.
        let root_a = self.uf.find(node);
        let root_b = self.uf.find(existing_node);
        self.uf.union(node, existing_node);

        let root = self.uf.find(node);
        for stale_root in [root_a, root_b] {
            if stale_root == root {
                continue;
            }
            self.canonical_row.remove(&stale_root);
            // Parents lists follow the class to its surviving root.
            if let Some(moved) = self.parents.remove(&stale_root) {
                let list = self.parents.entry(root).or_default();
                for parent in moved {
                    if !list.contains(&parent) {
                        list.push(parent);
                    }
                }
            }
        }
        self.canonical_row.insert(root, new_canonical);

        // The losing side's members now resolve to `new_canonical`, so cells
        // embedding their previous canonical id are stale and must be
        // rewritten now, before any further lookup can miss them. When both
        // sides already resolved to the same id (a re-observation), nothing
        // changed.
        let fixups = if rid_canonical == old_canonical {
            Vec::new()
        } else {
            self.rekey_parents(root, dict)
        };

        let outcome = if new_canonical == old_canonical {
            ObservedOutcome::KeptOld(old_canonical.unpack(dict))
        } else {
            ObservedOutcome::Swap {
                old: old_canonical.unpack(dict),
                new: new_canonical.unpack(dict),
            }
        };
        Observed { outcome, fixups }
    }

    /// Re-express the cells of every parent of `root`'s class in current
    /// canonical ids, in `by_row`, in the parent's table index (for hashcons
    /// parents), and as [`CellFixup`]s for the parent's table storage.
    /// Called when the id some class member resolves to changed.
    ///
    /// A re-key never collides with an entry from a different class: unions
    /// always merge a freshly observed row into an existing class, and
    /// hashcons rows only reference already-observed rows, so the
    /// substitution replaces the old id with one no other index entry
    /// contains, mapping distinct keys to distinct keys. For the same reason
    /// a re-key never changes a canonical id, so one level suffices and
    /// grandparent cells stay valid. If unions of two pre-existing classes
    /// are ever introduced, collisions become possible and must cascade into
    /// parent class merges; the assert below guards that assumption.
    fn rekey_parents(&mut self, root: NodeId, dict: &mut HashMapper) -> Vec<CellFixup> {
        let mut fixups = Vec::new();
        // Parents in one class share one physical row; fix it up once.
        let mut fixed_rows: HashSet<PackedRowId> = HashSet::new();
        let parent_rids = self.parents.get(&root).cloned().unwrap_or_default();
        for prid in parent_rids {
            let stored = self
                .by_row
                .get(&prid)
                .expect("registered parents were observed")
                .clone();
            let new_cells: Vec<PackedCell> = stored
                .cells
                .iter()
                .map(|cell| match cell {
                    PackedCell::Id(id) => PackedCell::Id(self.canonical_packed(*id)),
                    other => other.clone(),
                })
                .collect();
            if new_cells == stored.cells {
                continue;
            }

            let old_values = unpack_cells(&stored.cells, dict);
            let new_values = unpack_cells(&new_cells, dict);

            // Non-hashcons parents have no structural index; only their
            // stored cells and table fixup need updating.
            if self.indexes.contains_key(&stored.table) {
                let old_refs: Vec<&CellValue> = old_values.iter().collect();
                let new_refs: Vec<&CellValue> = new_values.iter().collect();

                // Parents in the same class share one entry; the first one
                // moves it and the rest only update their stored cells.
                let index = self
                    .indexes
                    .get_mut(&stored.table)
                    .expect("checked just above");
                let entry = index.packed_row_ids_for(&old_refs, dict).next();
                let collided = match entry {
                    None => None,
                    Some(entry) => {
                        index.remove(&old_values, entry, dict);
                        index.packed_row_ids_for(&new_refs, dict).next()
                    }
                };
                if let Some(entry) = entry {
                    match collided {
                        None => {
                            self.indexes
                                .get_mut(&stored.table)
                                .expect("checked just above")
                                .insert(&new_values, entry, dict);
                        }
                        Some(other) => {
                            assert_eq!(
                                self.canonical_packed(other),
                                self.canonical_packed(entry),
                                "re-key collided with a different class"
                            );
                        }
                    }
                }
            }

            // The physical row for this parent is its class canonical:
            // hashcons parents may be non-canonical members sharing one
            // stored row.
            let target = self.canonical_packed(prid);
            if fixed_rows.insert(target) {
                fixups.push(CellFixup {
                    table: stored.table.clone(),
                    row: target.unpack(dict),
                    values: new_values,
                });
            }

            self.by_row.insert(
                prid,
                StoredRow {
                    table: stored.table,
                    cells: new_cells,
                },
            );
        }
        fixups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::hash::CommitHash;
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};

    fn row_id(commit_byte: u8, counter: u32) -> RowId {
        RowId {
            commit: CommitHash([commit_byte; 32]),
            counter,
        }
    }

    fn int_table(path: &str, hashcons: bool) -> Table {
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("value"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        };
        let mut table = Table::new(Path::from(path), schema);
        table.set_hashcons_for_test(hashcons);
        table
    }

    fn id_table(path: &str, arity: usize, hashcons: bool) -> Table {
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: (0..arity)
                .map(|i| ColumnEntry {
                    path: Path::from(format!("c{i}").as_str()),
                    col_type: ColType::RowId {
                        path: Path::from(path),
                    },
                })
                .collect(),
            primary_key: None,
        };
        let mut table = Table::new(Path::from(path), schema);
        table.set_hashcons_for_test(hashcons);
        table
    }

    /// [`Rowing`] together with a store-wide dictionary, as owned by a store.
    struct Fixture {
        rowing: Rowing,
        dict: HashMapper,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                rowing: Rowing::new(),
                dict: HashMapper::new(),
            }
        }

        fn observe(
            &mut self,
            table: &Table,
            rid: RowId,
            values: &[CellValue],
        ) -> Result<Observed, RowingError> {
            self.rowing.observe(table, &mut self.dict, rid, values)
        }

        fn canonical(&self, rid: RowId) -> RowId {
            self.rowing.canonical(rid, &self.dict)
        }
    }

    fn assert_inserted(observed: &Observed, rid: RowId) {
        assert!(
            matches!(observed.outcome, ObservedOutcome::Inserted { rid: got, .. } if got == rid),
            "expected insert of {rid}, got {observed:?}"
        );
        assert!(observed.fixups.is_empty(), "inserts carry no fixups");
    }

    fn assert_kept_old(observed: &Observed, rid: RowId) {
        assert!(
            matches!(observed.outcome, ObservedOutcome::KeptOld(got) if got == rid),
            "expected kept-old of {rid}, got {observed:?}"
        );
    }

    #[test]
    fn deduplicates_equal_hashcons_rows() {
        let table = int_table("Term", true);
        let mut rowing = Fixture::new();
        let first = row_id(1, 0);
        let second = row_id(2, 0);

        let first_outcome = rowing
            .observe(&table, first, &[CellValue::Int(7)])
            .expect("first row should observe");
        let second_outcome = rowing
            .observe(&table, second, &[CellValue::Int(7)])
            .expect("second row should observe");

        assert_inserted(&first_outcome, first);
        assert_kept_old(&second_outcome, first);
        assert!(second_outcome.fixups.is_empty());
        assert_eq!(rowing.canonical(first), first);
        assert_eq!(rowing.canonical(second), first);
    }

    #[test]
    fn keeps_equal_non_hashcons_rows_distinct() {
        let table = int_table("Term", false);
        let mut rowing = Fixture::new();
        let first = row_id(1, 0);
        let second = row_id(2, 0);

        let first_outcome = rowing
            .observe(&table, first, &[CellValue::Int(7)])
            .expect("first row should observe");
        let second_outcome = rowing
            .observe(&table, second, &[CellValue::Int(7)])
            .expect("second row should observe");

        assert_inserted(&first_outcome, first);
        assert_inserted(&second_outcome, second);
        assert_eq!(rowing.canonical(first), first);
        assert_eq!(rowing.canonical(second), second);
    }

    #[test]
    fn uses_canonical_child_ids_in_parent_key() {
        let leaf_table = int_table("Term", false);
        let plus_table = id_table("Plus", 2, true);
        let mut rowing = Fixture::new();

        let a = row_id(1, 0);
        let b = row_id(1, 1);
        let c = row_id(1, 2);
        rowing
            .observe(&leaf_table, a, &[CellValue::Int(1)])
            .expect("observe a");
        rowing
            .observe(&leaf_table, b, &[CellValue::Int(2)])
            .expect("observe b");
        rowing
            .observe(&leaf_table, c, &[CellValue::Int(3)])
            .expect("observe c");

        let first_child = row_id(2, 0);
        let second_child = row_id(3, 0);
        rowing
            .observe(
                &plus_table,
                first_child,
                &[CellValue::Id(a), CellValue::Id(b)],
            )
            .expect("observe first child");
        rowing
            .observe(
                &plus_table,
                second_child,
                &[CellValue::Id(a), CellValue::Id(b)],
            )
            .expect("observe equivalent child");

        let first_parent = row_id(4, 0);
        let second_parent = row_id(5, 0);
        let first_outcome = rowing
            .observe(
                &plus_table,
                first_parent,
                &[CellValue::Id(first_child), CellValue::Id(c)],
            )
            .expect("observe first parent");
        let second_outcome = rowing
            .observe(
                &plus_table,
                second_parent,
                &[CellValue::Id(second_child), CellValue::Id(c)],
            )
            .expect("observe equivalent parent");

        assert_inserted(&first_outcome, first_parent);
        assert_kept_old(&second_outcome, first_parent);
        assert_eq!(rowing.canonical(first_parent), first_parent);
        assert_eq!(rowing.canonical(second_parent), first_parent);
    }

    #[test]
    fn returns_error_for_missing_child() {
        let table = id_table("Plus", 2, true);
        let mut rowing = Fixture::new();
        let row = row_id(1, 0);
        let missing = row_id(2, 0);

        let err = rowing
            .observe(&table, row, &[CellValue::Id(missing), CellValue::Int(1)])
            .expect_err("missing child should be rejected");

        assert!(matches!(err, RowingError::MissingChild { rid } if rid == missing));
    }

    /// The canonical id is the smallest unpacked [`RowId`]. The larger id is
    /// observed (and interned) first, so its packed id is smaller: comparing
    /// packed ids would pick the wrong canonical here.
    #[test]
    fn smaller_later_row_becomes_canonical() {
        let table = int_table("Term", true);
        let mut rowing = Fixture::new();
        let larger = row_id(2, 0);
        let smaller = row_id(1, 0);

        rowing
            .observe(&table, larger, &[CellValue::Int(7)])
            .expect("larger row should observe");
        let observed = rowing
            .observe(&table, smaller, &[CellValue::Int(7)])
            .expect("smaller row should observe");

        assert!(
            matches!(observed.outcome, ObservedOutcome::Swap { old, new } if old == larger && new == smaller)
        );
        assert_eq!(rowing.canonical(larger), smaller);
        assert_eq!(rowing.canonical(smaller), smaller);
    }

    #[test]
    fn reobserving_row_with_different_values_errors() {
        let table = int_table("Term", true);
        let mut rowing = Fixture::new();
        let rid = row_id(1, 0);

        rowing
            .observe(&table, rid, &[CellValue::Int(7)])
            .expect("first observation should succeed");
        let err = rowing
            .observe(&table, rid, &[CellValue::Int(8)])
            .expect_err("same row id with different values should be rejected");

        assert!(matches!(err, RowingError::InconsistentRow { rid: got } if got == rid));
    }

    #[test]
    fn three_equal_rows_merge_to_smallest() {
        let table = int_table("Term", true);
        let mut rowing = Fixture::new();
        let high = row_id(3, 0);
        let mid = row_id(2, 0);
        let low = row_id(1, 0);

        let high_outcome = rowing
            .observe(&table, high, &[CellValue::Int(7)])
            .expect("observe high");
        let mid_outcome = rowing
            .observe(&table, mid, &[CellValue::Int(7)])
            .expect("observe mid");
        let low_outcome = rowing
            .observe(&table, low, &[CellValue::Int(7)])
            .expect("observe low");

        assert_inserted(&high_outcome, high);
        assert!(
            matches!(mid_outcome.outcome, ObservedOutcome::Swap { old, new } if old == high && new == mid)
        );
        assert!(
            matches!(low_outcome.outcome, ObservedOutcome::Swap { old, new } if old == mid && new == low)
        );
        assert_eq!(rowing.canonical(high), low);
        assert_eq!(rowing.canonical(mid), low);
        assert_eq!(rowing.canonical(low), low);
    }

    /// A parent's index entry embeds the child's canonical id as of
    /// observation time. When the child's class later merges with a smaller
    /// id (a swap), the entry is re-keyed, so a structurally equal parent
    /// observed after the swap still deduplicates.
    #[test]
    fn parent_key_survives_child_canonical_swap() {
        let term_table = int_table("Term", true);
        let plus_table = id_table("Plus", 2, true);
        let mut rowing = Fixture::new();

        let t_high = row_id(2, 0);
        rowing
            .observe(&term_table, t_high, &[CellValue::Int(7)])
            .expect("observe high term");

        // Parent entry embeds canonical(t_high) = t_high.
        let first_parent = row_id(3, 0);
        let first_outcome = rowing
            .observe(
                &plus_table,
                first_parent,
                &[CellValue::Id(t_high), CellValue::Id(t_high)],
            )
            .expect("observe first parent");
        assert_inserted(&first_outcome, first_parent);

        // A smaller equal term arrives, so the child class swaps its
        // canonical id from t_high to t_low.
        let t_low = row_id(1, 0);
        let term_outcome = rowing
            .observe(&term_table, t_low, &[CellValue::Int(7)])
            .expect("observe low term");
        assert!(
            matches!(term_outcome.outcome, ObservedOutcome::Swap { old, new } if old == t_high && new == t_low)
        );

        // Structurally the same parent: both children canonicalize to t_low.
        let second_parent = row_id(4, 0);
        let second_outcome = rowing
            .observe(
                &plus_table,
                second_parent,
                &[CellValue::Id(t_low), CellValue::Id(t_low)],
            )
            .expect("observe second parent");
        assert_kept_old(&second_outcome, first_parent);
        assert_eq!(rowing.canonical(second_parent), first_parent);
    }

    /// A child swap reports one fixup per referencing stored row, carrying
    /// the re-canonicalized cells for the store to write back.
    #[test]
    fn swap_emits_fixups_for_referencing_rows() {
        let term_table = int_table("Term", true);
        let plus_table = id_table("Plus", 2, true);
        let mut rowing = Fixture::new();

        let t_high = row_id(2, 0);
        rowing
            .observe(&term_table, t_high, &[CellValue::Int(7)])
            .expect("observe high term");

        let parent = row_id(3, 0);
        rowing
            .observe(
                &plus_table,
                parent,
                &[CellValue::Id(t_high), CellValue::Id(t_high)],
            )
            .expect("observe parent");

        let t_low = row_id(1, 0);
        let observed = rowing
            .observe(&term_table, t_low, &[CellValue::Int(7)])
            .expect("observe low term");

        assert!(matches!(observed.outcome, ObservedOutcome::Swap { .. }));
        assert_eq!(
            observed.fixups,
            vec![CellFixup {
                table: Path::from("Plus"),
                row: parent,
                values: vec![CellValue::Id(t_low), CellValue::Id(t_low)],
            }]
        );
    }

    /// Values reported for insertion embed canonical child ids, so a row
    /// referencing a deduplicated member never stores a dangling id.
    #[test]
    fn insert_values_use_canonical_child_ids() {
        let term_table = int_table("Term", true);
        let plus_table = id_table("Plus", 2, true);
        let mut rowing = Fixture::new();

        let t_a = row_id(1, 0);
        let t_b = row_id(2, 0);
        rowing
            .observe(&term_table, t_a, &[CellValue::Int(7)])
            .expect("observe first term");
        let dedup = rowing
            .observe(&term_table, t_b, &[CellValue::Int(7)])
            .expect("observe equal term");
        assert_kept_old(&dedup, t_a);

        // The parent references the deduplicated member t_b, which is never
        // stored in the Term table; the insert values must name t_a.
        let parent = row_id(3, 0);
        let observed = rowing
            .observe(
                &plus_table,
                parent,
                &[CellValue::Id(t_b), CellValue::Id(t_b)],
            )
            .expect("observe parent");
        match observed.outcome {
            ObservedOutcome::Inserted { rid, values } => {
                assert_eq!(rid, parent);
                assert_eq!(values, vec![CellValue::Id(t_a), CellValue::Id(t_a)]);
            }
            other => panic!("expected insert, got {other:?}"),
        }
    }

    /// Non-hashcons rows referencing a hashcons class are re-keyed on a
    /// swap too, so their table cells never dangle.
    #[test]
    fn non_hashcons_referrer_is_fixed_up_on_child_swap() {
        let term_table = int_table("Term", true);
        let note_table = id_table("Note", 1, false);
        let mut rowing = Fixture::new();

        let t_high = row_id(2, 0);
        rowing
            .observe(&term_table, t_high, &[CellValue::Int(7)])
            .expect("observe high term");

        let note = row_id(3, 0);
        let note_observed = rowing
            .observe(&note_table, note, &[CellValue::Id(t_high)])
            .expect("observe note");
        assert_inserted(&note_observed, note);

        let t_low = row_id(1, 0);
        let observed = rowing
            .observe(&term_table, t_low, &[CellValue::Int(7)])
            .expect("observe low term");

        assert!(matches!(observed.outcome, ObservedOutcome::Swap { .. }));
        assert_eq!(
            observed.fixups,
            vec![CellFixup {
                table: Path::from("Note"),
                row: note,
                values: vec![CellValue::Id(t_low)],
            }]
        );
    }

    /// A non-hashcons row may reference a row before it is observed. When
    /// that row later joins a class with a smaller canonical id, the class
    /// canonical does not change (kept-old), but the referrer's cell still
    /// went stale and must be fixed up.
    #[test]
    fn kept_old_join_emits_fixups_for_early_referrer() {
        let term_table = int_table("Term", true);
        let note_table = id_table("Note", 1, false);
        let mut rowing = Fixture::new();

        // The note references t2 before t2 is observed.
        let t2 = row_id(2, 0);
        let note = row_id(3, 0);
        rowing
            .observe(&note_table, note, &[CellValue::Id(t2)])
            .expect("observe note");

        let t1 = row_id(1, 0);
        rowing
            .observe(&term_table, t1, &[CellValue::Int(7)])
            .expect("observe t1");

        // t2 joins t1's class as a non-canonical member: kept-old for the
        // class, but the note's cell must move from t2 to t1.
        let observed = rowing
            .observe(&term_table, t2, &[CellValue::Int(7)])
            .expect("observe t2");
        assert_kept_old(&observed, t1);
        assert_eq!(
            observed.fixups,
            vec![CellFixup {
                table: Path::from("Note"),
                row: note,
                values: vec![CellValue::Id(t1)],
            }]
        );
        assert_eq!(rowing.canonical(t2), t1);
    }

    /// A swap re-keys direct parents only. A grandparent's entry embeds the
    /// parent's canonical id, which a re-key never changes, so it stays
    /// valid without a second level of rewriting.
    #[test]
    fn grandparent_keys_stay_valid_after_child_swap() {
        let term_table = int_table("Term", true);
        let plus_table = id_table("Plus", 2, true);
        let mut rowing = Fixture::new();

        let t_high = row_id(3, 0);
        rowing
            .observe(&term_table, t_high, &[CellValue::Int(7)])
            .expect("observe high term");

        let parent = row_id(4, 0);
        rowing
            .observe(
                &plus_table,
                parent,
                &[CellValue::Id(t_high), CellValue::Id(t_high)],
            )
            .expect("observe parent");

        let grandparent = row_id(5, 0);
        rowing
            .observe(
                &plus_table,
                grandparent,
                &[CellValue::Id(parent), CellValue::Id(parent)],
            )
            .expect("observe grandparent");

        let t_low = row_id(1, 0);
        let term_outcome = rowing
            .observe(&term_table, t_low, &[CellValue::Int(7)])
            .expect("observe low term");
        assert!(matches!(term_outcome.outcome, ObservedOutcome::Swap { .. }));
        // Only the parent's cells embed the term class; the grandparent's
        // cells embed the parent's canonical id, which did not change.
        assert_eq!(term_outcome.fixups.len(), 1);
        assert_eq!(term_outcome.fixups[0].row, parent);

        let parent_dup = row_id(6, 0);
        let parent_outcome = rowing
            .observe(
                &plus_table,
                parent_dup,
                &[CellValue::Id(t_low), CellValue::Id(t_low)],
            )
            .expect("observe duplicate parent");
        assert_kept_old(&parent_outcome, parent);

        let grandparent_dup = row_id(7, 0);
        let grandparent_outcome = rowing
            .observe(
                &plus_table,
                grandparent_dup,
                &[CellValue::Id(parent_dup), CellValue::Id(parent_dup)],
            )
            .expect("observe duplicate grandparent");
        assert_kept_old(&grandparent_outcome, grandparent);
    }

    /// The parent's entry follows the child class through several swaps, and
    /// the re-keyed entry still participates in later merges: a smaller
    /// structurally equal parent swaps the parent class itself.
    #[test]
    fn parent_key_follows_repeated_child_swaps() {
        let term_table = int_table("Term", true);
        let plus_table = id_table("Plus", 2, true);
        let mut rowing = Fixture::new();

        let t3 = row_id(3, 0);
        rowing
            .observe(&term_table, t3, &[CellValue::Int(7)])
            .expect("observe t3");

        let parent = row_id(4, 0);
        rowing
            .observe(&plus_table, parent, &[CellValue::Id(t3), CellValue::Id(t3)])
            .expect("observe parent");

        for commit_byte in [2u8, 1u8] {
            let term = row_id(commit_byte, 0);
            let observed = rowing
                .observe(&term_table, term, &[CellValue::Int(7)])
                .expect("observe smaller term");
            assert!(matches!(observed.outcome, ObservedOutcome::Swap { .. }));
            // Each swap re-keys the parent to the new canonical term id.
            let term_id = row_id(commit_byte, 0);
            assert_eq!(
                observed.fixups,
                vec![CellFixup {
                    table: Path::from("Plus"),
                    row: parent,
                    values: vec![CellValue::Id(term_id), CellValue::Id(term_id)],
                }]
            );
        }

        let t1 = row_id(1, 0);
        let parent_dup = row_id(5, 0);
        let dup_outcome = rowing
            .observe(
                &plus_table,
                parent_dup,
                &[CellValue::Id(t1), CellValue::Id(t1)],
            )
            .expect("observe duplicate parent");
        assert_kept_old(&dup_outcome, parent);

        let parent_small = row_id(2, 1);
        let small_outcome = rowing
            .observe(
                &plus_table,
                parent_small,
                &[CellValue::Id(t1), CellValue::Id(t1)],
            )
            .expect("observe smaller parent");
        assert!(
            matches!(small_outcome.outcome, ObservedOutcome::Swap { old, new }
                if old == parent && new == parent_small)
        );
        assert_eq!(rowing.canonical(parent), parent_small);
    }

    /// Re-observing a parent with its original raw values after a child swap
    /// must not report [`RowingError::InconsistentRow`]: the fresh cells use
    /// current canonical ids, and the re-key updated the stored ones to
    /// match.
    #[test]
    fn reobserving_parent_after_child_swap_is_consistent() {
        let term_table = int_table("Term", true);
        let plus_table = id_table("Plus", 2, true);
        let mut rowing = Fixture::new();

        let t_high = row_id(2, 0);
        rowing
            .observe(&term_table, t_high, &[CellValue::Int(7)])
            .expect("observe high term");

        let parent = row_id(3, 0);
        rowing
            .observe(
                &plus_table,
                parent,
                &[CellValue::Id(t_high), CellValue::Id(t_high)],
            )
            .expect("observe parent");

        let t_low = row_id(1, 0);
        rowing
            .observe(&term_table, t_low, &[CellValue::Int(7)])
            .expect("observe low term");

        let observed = rowing
            .observe(
                &plus_table,
                parent,
                &[CellValue::Id(t_high), CellValue::Id(t_high)],
            )
            .expect("re-observation with original values should stay consistent");
        assert_kept_old(&observed, parent);
        assert!(observed.fixups.is_empty());
    }
}
