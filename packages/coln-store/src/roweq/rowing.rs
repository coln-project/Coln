// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

// Why is it called rowing? Because it is comparing row for equivalence by looking
// at each value of the row one by one. And it sounds cool.
// Anyway way it will probably be renamed in the future.

use coln_flir_rs::ir;
use petgraph::unionfind::UnionFind;
use std::collections::HashMap;

use crate::{
    roweq::ObservedOutcome::{self, KeptOld},
    table::{CellValue, RowId, Table},
};

type NodeId = u32;

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
struct RowKey {
    table: ir::Path,
    row_id: Option<RowId>,
    cells: Vec<CellValue>,
}

#[derive(Debug, thiserror::Error)]
pub enum RowingError {
    #[error("Child row missing {rid}")]
    MissingChild { rid: RowId },
    #[error("duplicate rowid {rid} with different row values")]
    InconsistentRow { rid: RowId },
}

#[derive(Debug, Clone)]
pub(crate) struct Index {
    row_to_node: HashMap<RowId, u32>,
    uf: UnionFind<u32>,
    // rootnode -> canonical row_id
    canonical_row: HashMap<NodeId, RowId>,

    // check whether a structurally identical row exists
    // return value not necessary canonical
    // this is the place hashcons is happening
    by_key: HashMap<RowKey, RowId>,

    by_row: HashMap<RowId, RowKey>,
}

impl Index {
    pub(crate) fn new() -> Self {
        Self {
            row_to_node: HashMap::new(),
            uf: UnionFind::new_empty(),
            canonical_row: HashMap::new(),
            by_key: HashMap::new(),
            by_row: HashMap::new(),
        }
    }

    pub(crate) fn canonical(&self, row_id: &RowId) -> RowId {
        *self
            .row_to_node
            .get(row_id)
            .map(|&k| self.uf.find(k))
            .and_then(|root| self.canonical_row.get(&root))
            .unwrap_or(row_id)
    }

    fn row_key(
        &self,
        table: &Table,
        rid: &RowId,
        values: &[CellValue],
    ) -> Result<RowKey, RowingError> {
        let cells = if table.hashcons() {
            // Hashcons rows must be observed in dependency order: every
            // referenced child row has already been observed, and its canonical
            // id is settled before it is embedded in a parent key.
            values
                .iter()
                .map(|cell| match cell {
                    CellValue::Id(child) => {
                        if !self.by_row.contains_key(child) {
                            return Err(RowingError::MissingChild { rid: *child });
                        }
                        Ok(CellValue::Id(self.canonical(child)))
                    }
                    CellValue::Int(i) => Ok(CellValue::Int(*i)),
                    CellValue::Str(s) => Ok(CellValue::Str(s.to_owned())),
                })
                .collect::<Result<Vec<_>, RowingError>>()?
        } else {
            values.to_vec()
        };
        Ok(RowKey {
            table: table.path().clone(),
            row_id: if table.hashcons() { None } else { Some(*rid) },
            cells,
        })
    }

    fn node_for(&mut self, rid: &RowId) -> NodeId {
        if let Some(nid) = self.row_to_node.get(rid) {
            *nid
        } else {
            let n = self.uf.new_set();
            self.row_to_node.insert(*rid, n);
            n
        }
    }

    pub(crate) fn observe(
        &mut self,
        table: &Table,
        rid: RowId,
        values: &[CellValue],
    ) -> Result<ObservedOutcome, RowingError> {
        let row_key = self.row_key(table, &rid, values)?;
        let node = self.node_for(&rid);

        if let Some(old_key) = self.by_row.get(&rid) {
            // If we already saw this exact row id, it should have the same key.
            if old_key != &row_key {
                return Err(RowingError::InconsistentRow { rid });
            }
        } else {
            self.by_row.insert(rid, row_key.clone());
        }

        if !table.hashcons() {
            return Ok(ObservedOutcome::Inserted(rid));
        }

        match self.by_key.get(&row_key).copied() {
            None => {
                self.by_key.insert(row_key, rid);
                Ok(ObservedOutcome::Inserted(rid))
            }
            Some(existing_rid) => {
                let old_canonical = self.canonical(&existing_rid);
                let new_canonical = std::cmp::min(old_canonical, self.canonical(&rid));

                let existing_node = self.node_for(&existing_rid);

                // Roots before the union; the survivor is one of these two, so
                // the loser becomes a stale key in `canonical_row`.
                let root_a = self.uf.find(node);
                let root_b = self.uf.find(existing_node);
                self.uf.union(node, existing_node);

                let root = self.uf.find(node);
                if root_a != root {
                    self.canonical_row.remove(&root_a);
                }
                if root_b != root {
                    self.canonical_row.remove(&root_b);
                }
                self.canonical_row.insert(root, new_canonical);

                self.by_key.insert(row_key, new_canonical);

                if new_canonical == old_canonical {
                    Ok(KeptOld(old_canonical))
                } else {
                    Ok(ObservedOutcome::Swap {
                        old: old_canonical,
                        new: new_canonical,
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::hash::CommitHash;
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
    use crate::roweq::ObservedOutcome::{Inserted, Swap};

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

    #[test]
    fn deduplicates_equal_hashcons_rows() {
        let table = int_table("Term", true);
        let mut index = Index::new();
        let first = row_id(1, 0);
        let second = row_id(2, 0);

        let first_outcome = index
            .observe(&table, first, &[CellValue::Int(7)])
            .expect("first row should observe");
        let second_outcome = index
            .observe(&table, second, &[CellValue::Int(7)])
            .expect("second row should observe");

        assert!(matches!(first_outcome, Inserted(row) if row == first));
        assert!(matches!(second_outcome, KeptOld(row) if row == first));
        assert_eq!(index.canonical(&first), first);
        assert_eq!(index.canonical(&second), first);
    }

    #[test]
    fn keeps_equal_non_hashcons_rows_distinct() {
        let table = int_table("Term", false);
        let mut index = Index::new();
        let first = row_id(1, 0);
        let second = row_id(2, 0);

        let first_outcome = index
            .observe(&table, first, &[CellValue::Int(7)])
            .expect("first row should observe");
        let second_outcome = index
            .observe(&table, second, &[CellValue::Int(7)])
            .expect("second row should observe");

        assert!(matches!(first_outcome, Inserted(row) if row == first));
        assert!(matches!(second_outcome, Inserted(row) if row == second));
        assert_eq!(index.canonical(&first), first);
        assert_eq!(index.canonical(&second), second);
    }

    #[test]
    fn uses_canonical_child_ids_in_parent_key() {
        let leaf_table = int_table("Term", false);
        let plus_table = id_table("Plus", 2, true);
        let mut index = Index::new();

        let a = row_id(1, 0);
        let b = row_id(1, 1);
        let c = row_id(1, 2);
        index
            .observe(&leaf_table, a, &[CellValue::Int(1)])
            .expect("observe a");
        index
            .observe(&leaf_table, b, &[CellValue::Int(2)])
            .expect("observe b");
        index
            .observe(&leaf_table, c, &[CellValue::Int(3)])
            .expect("observe c");

        let first_child = row_id(2, 0);
        let second_child = row_id(3, 0);
        index
            .observe(
                &plus_table,
                first_child,
                &[CellValue::Id(a), CellValue::Id(b)],
            )
            .expect("observe first child");
        index
            .observe(
                &plus_table,
                second_child,
                &[CellValue::Id(a), CellValue::Id(b)],
            )
            .expect("observe equivalent child");

        let first_parent = row_id(4, 0);
        let second_parent = row_id(5, 0);
        let first_outcome = index
            .observe(
                &plus_table,
                first_parent,
                &[CellValue::Id(first_child), CellValue::Id(c)],
            )
            .expect("observe first parent");
        let second_outcome = index
            .observe(
                &plus_table,
                second_parent,
                &[CellValue::Id(second_child), CellValue::Id(c)],
            )
            .expect("observe equivalent parent");

        assert!(matches!(first_outcome, Inserted(row) if row == first_parent));
        assert!(matches!(second_outcome, KeptOld(row) if row == first_parent));
        assert_eq!(index.canonical(&first_parent), first_parent);
        assert_eq!(index.canonical(&second_parent), first_parent);
    }

    #[test]
    fn returns_error_for_missing_child() {
        let table = id_table("Plus", 2, true);
        let mut index = Index::new();
        let row = row_id(1, 0);
        let missing = row_id(2, 0);

        let err = index
            .observe(&table, row, &[CellValue::Id(missing), CellValue::Int(1)])
            .expect_err("missing child should be rejected");

        assert!(matches!(err, RowingError::MissingChild { rid } if rid == missing));
    }

    #[test]
    fn smaller_later_row_becomes_canonical() {
        let table = int_table("Term", true);
        let mut index = Index::new();
        let larger = row_id(2, 0);
        let smaller = row_id(1, 0);

        index
            .observe(&table, larger, &[CellValue::Int(7)])
            .expect("larger row should observe");
        let outcome = index
            .observe(&table, smaller, &[CellValue::Int(7)])
            .expect("smaller row should observe");

        assert!(matches!(outcome, Swap { old, new } if old == larger && new == smaller));
        assert_eq!(index.canonical(&larger), smaller);
        assert_eq!(index.canonical(&smaller), smaller);
    }

    #[test]
    fn reobserving_row_with_different_values_errors() {
        let table = int_table("Term", true);
        let mut index = Index::new();
        let rid = row_id(1, 0);

        index
            .observe(&table, rid, &[CellValue::Int(7)])
            .expect("first observation should succeed");
        let err = index
            .observe(&table, rid, &[CellValue::Int(8)])
            .expect_err("same row id with different values should be rejected");

        assert!(matches!(err, RowingError::InconsistentRow { rid: got } if got == rid));
    }

    #[test]
    fn three_equal_rows_merge_to_smallest() {
        let table = int_table("Term", true);
        let mut index = Index::new();
        let high = row_id(3, 0);
        let mid = row_id(2, 0);
        let low = row_id(1, 0);

        let high_outcome = index
            .observe(&table, high, &[CellValue::Int(7)])
            .expect("observe high");
        let mid_outcome = index
            .observe(&table, mid, &[CellValue::Int(7)])
            .expect("observe mid");
        let low_outcome = index
            .observe(&table, low, &[CellValue::Int(7)])
            .expect("observe low");

        assert!(matches!(high_outcome, Inserted(row) if row == high));
        assert!(matches!(mid_outcome, Swap { old, new } if old == high && new == mid));
        assert!(matches!(low_outcome, Swap { old, new } if old == mid && new == low));
        assert_eq!(index.canonical(&high), low);
        assert_eq!(index.canonical(&mid), low);
        assert_eq!(index.canonical(&low), low);
    }
}
