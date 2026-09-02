// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Sorted secondary indexes over table columns.
//!
//! An index stores a copy of its key columns plus a row id locator, ordered
//! lexicographically by (key cells..., row id). The row id tiebreak makes
//! entries unique, so duplicate keys are representable and removal is exact.
//!
//! Indexes are derived data: they are rebuilt by replaying commits. They might
//! be persisted for performance reasons in the future.

use std::ops::Range;

use crate::ir::Schema;

use super::{CellKind, Column, IdColumn, PackedValue, PackedRowId};

pub(crate) type IndexId = usize;

pub struct IndexMeta<'a> {
    pub id: IndexId,
    pub key_cols: &'a [usize],
}

/// Index for tables, with support for dynamic sizing and types.
/// But the dynamic types are restricted to what types each table support.
/// Use hexane.
#[derive(Debug, Clone)]
pub(super) struct TableIndex {
    key_cols: Vec<usize>,
    keys: Vec<Column>,
    values: IdColumn,
}

impl TableIndex {
    pub(super) fn key_cols(&self) -> &[usize] {
        &self.key_cols
    }

    pub(super) fn new(key_cols: &[usize], schema: &Schema) -> Self {
        let keys = key_cols
            .iter()
            .map(|&column_idx| {
                let column = schema.columns.get(column_idx).unwrap_or_else(|| {
                    panic!("index references missing schema column {column_idx}")
                });
                Column::new(CellKind::from(&column.col_type))
            })
            .collect();

        Self {
            key_cols: key_cols.to_vec(),
            keys,
            values: IdColumn::new(),
        }
    }

    pub(super) fn insert(&mut self, key: Vec<PackedValue>, value: PackedRowId) {
        let key_range = self.scope_key(&key);
        let value_range = self.values.scope_to_value(value, key_range);
        let position = value_range.end;

        for (column, cell) in self.keys.iter_mut().zip(key) {
            column.insert(position, cell);
        }
        self.values.insert(position, value);
    }

    pub(super) fn remove(&mut self, key: &[PackedValue], value: PackedRowId) {
        let key_range = self.scope_key(key);
        let value_range = self.values.scope_to_value(value, key_range);
        for position in value_range.rev() {
            for column in &mut self.keys {
                column.remove(position);
            }
            self.values.remove(position);
        }
    }

    pub(super) fn get(&self, key: &[PackedValue]) -> impl Iterator<Item = PackedRowId> {
        self.scope_key(key).map(|position| self.values.at(position))
    }

    pub(super) fn contains_key(&self, key: &[PackedValue]) -> bool {
        self.get(key).next().is_some()
    }

    fn scope_key(&self, key: &[PackedValue]) -> Range<usize> {
        debug_assert_eq!(
            key.len(),
            self.keys.len(),
            "index key has the wrong column count"
        );

        self.keys
            .iter()
            .zip(key)
            .fold(0..self.values.len(), |range, (column, value)| {
                column.scope_to_value(value, range)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{self, BuiltinTy, ColType, Path};

    fn one_int_index() -> TableIndex {
        let schema = Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![ir::ColumnEntry {
                path: Path::from("key"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        };
        TableIndex::new(&[0], &schema)
    }

    fn packed(counter: u32) -> PackedRowId {
        PackedRowId {
            commit_idx: 0,
            counter,
        }
    }

    /// Create an index on just one column and check look up works fine.
    #[test]
    fn basic_lookup() {
        let mut index = one_int_index();

        let rows = [(5, packed(1)), (1, packed(2)), (9, packed(3))];
        for (key, row_id) in rows {
            index.insert(vec![PackedValue::Int(key)], row_id);
        }

        for (key, row_id) in rows {
            assert_eq!(
                index.get(&[PackedValue::Int(key)]).collect::<Vec<_>>(),
                vec![row_id]
            );
        }
        assert!(!index.contains_key(&[PackedValue::Int(3)]));
    }

    /// If there are multiple keys of the same value, then `get` returns an iterator
    /// to all of them
    #[test]
    fn duplicate_keys_return_all_values() {
        let mut index = one_int_index();
        let first = packed(1);
        let second = packed(2);
        let third = packed(3);
        index.insert(vec![PackedValue::Int(5)], third);
        index.insert(vec![PackedValue::Int(7)], packed(4));
        index.insert(vec![PackedValue::Int(5)], first);
        index.insert(vec![PackedValue::Int(5)], second);

        assert_eq!(
            index.get(&[PackedValue::Int(5)]).collect::<Vec<_>>(),
            vec![first, second, third]
        );
    }

    /// Removing each duplicate-key entry by row id clears that key and leaves
    /// other keys untouched.
    #[test]
    fn duplicate_key_removal() {
        let mut index = one_int_index();
        let first = packed(1);
        let second = packed(2);
        let other = packed(3);
        index.insert(vec![PackedValue::Int(5)], second);
        index.insert(vec![PackedValue::Int(7)], other);
        index.insert(vec![PackedValue::Int(5)], first);

        index.remove(&[PackedValue::Int(5)], first);
        index.remove(&[PackedValue::Int(5)], second);

        assert_eq!(index.get(&[PackedValue::Int(5)]).next(), None);
        assert_eq!(
            index.get(&[PackedValue::Int(7)]).collect::<Vec<_>>(),
            vec![other]
        );
    }

    /// Missing key, missing row id, or mismatched key/row id pairs are no-ops.
    #[test]
    fn remove_non_existing_key_does_nothing() {
        let mut index = one_int_index();
        let first = packed(1);
        let second = packed(2);
        let other = packed(3);
        index.insert(vec![PackedValue::Int(5)], second);
        index.insert(vec![PackedValue::Int(7)], other);
        index.insert(vec![PackedValue::Int(5)], first);

        index.remove(&[PackedValue::Int(4)], first);
        index.remove(&[PackedValue::Int(5)], packed(9));
        index.remove(&[PackedValue::Int(4)], first);

        assert_eq!(
            index.get(&[PackedValue::Int(5)]).collect::<Vec<_>>(),
            vec![first, second]
        );
        assert_eq!(
            index.get(&[PackedValue::Int(7)]).collect::<Vec<_>>(),
            vec![other]
        );
        assert_eq!(index.values.len(), 3);
    }

    /// Entries stay sorted by (c1, c0, row id) under adversarial insert
    /// order, with the second key column deciding ties.
    #[test]
    fn entries_stay_sorted_with_multi_column_keys() {
        let schema = Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: ["c0", "c1"]
                .into_iter()
                .map(|name| ir::ColumnEntry {
                    path: Path::from(name),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                })
                .collect(),
            primary_key: None,
        };
        let mut index = TableIndex::new(&[1, 0], &schema);
        let entries = [
            ((2, 0), packed(3)),
            ((1, 2), packed(4)),
            ((1, 1), packed(2)),
            ((0, 9), packed(5)),
            ((1, 1), packed(1)),
        ];

        for ((c1, c0), row_id) in entries {
            index.insert(vec![PackedValue::Int(c1), PackedValue::Int(c0)], row_id);
        }

        let stored = (0..index.values.len())
            .map(|position| {
                (
                    index.keys[0].get_packed(position).unwrap(),
                    index.keys[1].get_packed(position).unwrap(),
                    index.values.at(position),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            stored,
            vec![
                (PackedValue::Int(0), PackedValue::Int(9), packed(5)),
                (PackedValue::Int(1), PackedValue::Int(1), packed(1)),
                (PackedValue::Int(1), PackedValue::Int(1), packed(2)),
                (PackedValue::Int(1), PackedValue::Int(2), packed(4)),
                (PackedValue::Int(2), PackedValue::Int(0), packed(3)),
            ]
        );
    }
}
