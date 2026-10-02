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

use core::panic;
use std::ops::Range;

use coln_flir_rs::engine::packed::{PackedRowId, StoreScalarValue, StoreTuple};

use crate::ir::Schema;

use super::{Column, IdColumn};

pub struct IndexMeta<'a> {
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
                Column::new((&column.col_type).into())
            })
            .collect();

        Self {
            key_cols: key_cols.to_vec(),
            keys,
            values: IdColumn::new(),
        }
    }

    /// insert assumes that the key as the same number of columns as the
    pub(super) fn insert(&mut self, key: impl Into<StoreTuple>, value: PackedRowId) {
        let key = key.into();
        if key.len() != self.key_cols.len() {
            panic!("insertion key length must be the same as the index length");
        }

        let key_range = self.scope_key(&key);
        let value_range = self.values.scope_to_value(value, key_range);
        let position = value_range.end;

        for (column, cell) in self.keys.iter_mut().zip(key) {
            column.insert(position, cell);
        }
        self.values.insert(position, value);
    }

    pub(super) fn remove(&mut self, key: &[StoreScalarValue], value: PackedRowId) {
        if key.len() != self.key_cols.len() {
            panic!("removing key length must be the same as the index length");
        }

        let key_range = self.scope_key(key);
        let value_range = self.values.scope_to_value(value, key_range);
        for position in value_range.rev() {
            for column in &mut self.keys {
                column.remove(position);
            }
            self.values.remove(position);
        }
    }

    /// Get does not require the key to be the same length as declared by the index
    /// if the key is shorter, we just return all the rows that matches the key
    pub(super) fn get<'s>(
        &'s self,
        key: &[StoreScalarValue],
    ) -> impl Iterator<Item = PackedRowId> + use<'s> {
        if key.len() > self.key_cols().len() {
            panic!("get must be on a key that is smaller/equal to the actual index column length");
        }

        self.scope_key(key).map(|position| self.values.at(position))
    }

    pub(super) fn contains_key(&self, key: &[StoreScalarValue]) -> bool {
        self.get(key).next().is_some()
    }

    fn scope_key(&self, key: &[StoreScalarValue]) -> Range<usize> {
        if key.len() > self.key_cols().len() {
            panic!(
                "scope_key must be on a key that is smaller/equal to the actual index column length"
            );
        }

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
    use coln_flir_rs::engine::schema::StoreScalarType;
    use rstest::{fixture, rstest};

    use super::*;

    /// Index over `i32` key columns, bypassing the schema so tests can pick
    /// the cell kind directly. Defaults to a single key column.
    #[fixture]
    fn i32_index(#[default(&[0])] key_cols: &[usize]) -> TableIndex {
        TableIndex {
            key_cols: key_cols.to_vec(),
            keys: key_cols
                .iter()
                .map(|_| Column::new(StoreScalarType::I32(())))
                .collect(),
            values: IdColumn::new(),
        }
    }

    fn packed(counter: u32) -> PackedRowId {
        PackedRowId {
            commit_idx: 0,
            counter,
        }
    }

    /// Create an index on just one column and check look up works fine.
    #[rstest]
    fn basic_lookup(#[from(i32_index)] mut index: TableIndex) {
        let rows = [(5, packed(1)), (1, packed(2)), (9, packed(3))];
        for (key, row_id) in rows {
            index.insert(vec![StoreScalarValue::I32(key)], row_id);
        }

        for (key, row_id) in rows {
            assert_eq!(
                index.get(&[StoreScalarValue::I32(key)]).collect::<Vec<_>>(),
                vec![row_id]
            );
        }
        assert!(!index.contains_key(&[StoreScalarValue::I32(3)]));
    }

    /// If there are multiple keys of the same value, then `get` returns an iterator
    /// to all of them
    #[rstest]
    fn duplicate_keys_return_all_values(#[from(i32_index)] mut index: TableIndex) {
        let first = packed(1);
        let second = packed(2);
        let third = packed(3);
        index.insert(vec![StoreScalarValue::I32(5)], third);
        index.insert(vec![StoreScalarValue::I32(7)], packed(4));
        index.insert(vec![StoreScalarValue::I32(5)], first);
        index.insert(vec![StoreScalarValue::I32(5)], second);

        assert_eq!(
            index.get(&[StoreScalarValue::I32(5)]).collect::<Vec<_>>(),
            vec![first, second, third]
        );
    }

    /// Removing each duplicate-key entry by row id clears that key and leaves
    /// other keys untouched.
    #[rstest]
    fn duplicate_key_removal(#[from(i32_index)] mut index: TableIndex) {
        let first = packed(1);
        let second = packed(2);
        let other = packed(3);
        index.insert(vec![StoreScalarValue::I32(5)], second);
        index.insert(vec![StoreScalarValue::I32(7)], other);
        index.insert(vec![StoreScalarValue::I32(5)], first);

        index.remove(&[StoreScalarValue::I32(5)], first);
        index.remove(&[StoreScalarValue::I32(5)], second);

        assert_eq!(index.get(&[StoreScalarValue::I32(5)]).next(), None);
        assert_eq!(
            index.get(&[StoreScalarValue::I32(7)]).collect::<Vec<_>>(),
            vec![other]
        );
    }

    /// Missing key, missing row id, or mismatched key/row id pairs are no-ops.
    #[rstest]
    fn remove_non_existing_key_does_nothing(#[from(i32_index)] mut index: TableIndex) {
        let first = packed(1);
        let second = packed(2);
        let other = packed(3);
        index.insert(vec![StoreScalarValue::I32(5)], second);
        index.insert(vec![StoreScalarValue::I32(7)], other);
        index.insert(vec![StoreScalarValue::I32(5)], first);

        index.remove(&[StoreScalarValue::I32(4)], first);
        index.remove(&[StoreScalarValue::I32(5)], packed(9));
        index.remove(&[StoreScalarValue::I32(4)], first);

        assert_eq!(
            index.get(&[StoreScalarValue::I32(5)]).collect::<Vec<_>>(),
            vec![first, second]
        );
        assert_eq!(
            index.get(&[StoreScalarValue::I32(7)]).collect::<Vec<_>>(),
            vec![other]
        );
        assert_eq!(index.values.len(), 3);
    }

    /// Entries stay sorted by (c1, c0, row id) under adversarial insert
    /// order, with the second key column deciding ties.
    #[rstest]
    fn entries_stay_sorted_with_multi_column_keys(
        #[from(i32_index)]
        #[with(&[1, 0])]
        mut index: TableIndex,
    ) {
        let entries = [
            ((2, 0), packed(3)),
            ((1, 2), packed(4)),
            ((1, 1), packed(2)),
            ((0, 9), packed(5)),
            ((1, 1), packed(1)),
        ];

        for ((c1, c0), row_id) in entries {
            index.insert(
                vec![StoreScalarValue::I32(c1), StoreScalarValue::I32(c0)],
                row_id,
            );
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
                (
                    StoreScalarValue::I32(0),
                    StoreScalarValue::I32(9),
                    packed(5)
                ),
                (
                    StoreScalarValue::I32(1),
                    StoreScalarValue::I32(1),
                    packed(1)
                ),
                (
                    StoreScalarValue::I32(1),
                    StoreScalarValue::I32(1),
                    packed(2)
                ),
                (
                    StoreScalarValue::I32(1),
                    StoreScalarValue::I32(2),
                    packed(4)
                ),
                (
                    StoreScalarValue::I32(2),
                    StoreScalarValue::I32(0),
                    packed(3)
                ),
            ]
        );
    }
}
