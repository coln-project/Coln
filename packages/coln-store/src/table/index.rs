// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Sorted secondary indexes over table columns.
//!
//! An index stores a copy of its key columns plus a row id locator, ordered
//! lexicographically by (key cells..., row id). The row id tiebreak makes
//! entries unique, so duplicate keys are representable and removal is exact.
//!
//! Indexes are derived data: they are rebuilt by replaying commits on load
//! and are never persisted.

use std::cmp::Ordering;

use crate::commit::hash_dict::HashMapper;
use crate::ir::Schema;

use super::{CellKind, CellValue, Column, IdColumn, PackedRowId, RowId};

/// Comparable form of one key cell. Id cells are packed up front, so search
/// and insert never touch the hash dictionary while comparing.
#[derive(Debug)]
enum KeyCell<'a> {
    Id(PackedRowId),
    Int(i64),
    Str(&'a str),
}

impl<'a> KeyCell<'a> {
    /// Comparable form of `value`, without interning. `None` when an id cell
    /// carries a commit hash absent from `dict`.
    fn try_new(value: &'a CellValue, dict: &HashMapper) -> Option<Self> {
        Some(match value {
            CellValue::Id(id) => KeyCell::Id(PackedRowId::lookup(*id, dict)?),
            CellValue::Int(value) => KeyCell::Int(*value),
            CellValue::Str(value) => KeyCell::Str(value.as_str()),
        })
    }

    /// Comparable form of `value` for inserts, interning unseen commit
    /// hashes.
    fn new_interning(value: &'a CellValue, dict: &mut HashMapper) -> Self {
        match value {
            CellValue::Id(id) => KeyCell::Id(PackedRowId::pack(*id, dict)),
            CellValue::Int(value) => KeyCell::Int(*value),
            CellValue::Str(value) => KeyCell::Str(value.as_str()),
        }
    }
}

impl Column {
    /// Ordering of the stored cell at `row` against a key cell. Ids order by
    /// `(commit_idx, counter)`, which is stable for the lifetime of the table
    /// but not meaningful across tables or reloads. Panics on a kind
    /// mismatch, which schema validation rules out before keys reach an
    /// index.
    fn cmp_key(&self, row: usize, key: &KeyCell<'_>) -> Ordering {
        match (self, key) {
            (Column::Id(cells), KeyCell::Id(id)) => cells.at(row).cmp(id),
            (Column::Int(cells), KeyCell::Int(value)) => {
                cells.get(row).expect("row is in bounds").cmp(value)
            }
            (Column::Str(cells), KeyCell::Str(value)) => {
                cells.get(row).expect("row is in bounds").cmp(value)
            }
            (column, key) => panic!(
                "key type mismatch: column stores {:?}, got {key:?}",
                CellKind::from(column)
            ),
        }
    }
}

/// Sorted index over a subset of table columns.
///
/// For packing and unpacking row ids, it needs callers passing a dict.
#[derive(Debug, Clone)]
pub(crate) struct HexaneIndex {
    /// Column indices into `Table::cols`, in key order.
    key_cols: Vec<usize>,
    /// One sorted copy per key column, same encodings as the base table.
    keys: Vec<Column>,
    /// Locator back into the base table.
    row_ids: IdColumn,
}

impl HexaneIndex {
    pub(crate) fn new(key_cols: Vec<usize>, schema: &Schema) -> Self {
        let keys = key_cols
            .iter()
            .map(|&ci| Column::new(CellKind::from(&schema.columns[ci].col_type)))
            .collect();
        HexaneIndex {
            key_cols,
            keys,
            row_ids: IdColumn::new(),
        }
    }

    pub(crate) fn key_cols(&self) -> &[usize] {
        &self.key_cols
    }

    fn len(&self) -> usize {
        self.row_ids.len()
    }

    /// Ordering of the stored entry at `row` against `(key, row_id)`. A
    /// `row_id` of `None` sorts before every stored entry with equal keys, so
    /// binary search converges on the lower bound of the key's run.
    fn cmp_entry(&self, row: usize, key: &[KeyCell<'_>], row_id: Option<PackedRowId>) -> Ordering {
        for (col, cell) in self.keys.iter().zip(key) {
            match col.cmp_key(row, cell) {
                Ordering::Equal => {}
                ord => return ord,
            }
        }
        let stored = self.row_ids.at(row);
        match row_id {
            Some(id) => stored.cmp(&id),
            None => Ordering::Greater,
        }
    }

    // TODO replace with scope_to_id when delta column supports it
    /// Sorted position of `(key, row_id)`: `Ok(row)` when present, `Err(row)`
    /// with the insertion point otherwise.
    fn position(&self, key: &[KeyCell<'_>], row_id: Option<PackedRowId>) -> Result<usize, usize> {
        debug_assert_eq!(key.len(), self.keys.len());
        let mut lo = 0;
        let mut hi = self.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.cmp_entry(mid, key, row_id) {
                Ordering::Less => lo = mid + 1,
                Ordering::Equal => return Ok(mid),
                Ordering::Greater => hi = mid,
            }
        }
        Err(lo)
    }

    /// Whether the entry at `row` has exactly these key cells.
    fn key_matches(&self, row: usize, key: &[KeyCell<'_>]) -> bool {
        row < self.len()
            && self
                .keys
                .iter()
                .zip(key)
                .all(|(col, cell)| col.cmp_key(row, cell) == Ordering::Equal)
    }

    /// Packed row ids of every stored row whose key cells equal
    /// `key_values`, in row id order. Empty when the key is absent.
    pub(crate) fn packed_row_ids_for(
        &self,
        key_values: &[&CellValue],
        dict: &HashMapper,
    ) -> impl Iterator<Item = PackedRowId> {
        let key: Option<Vec<KeyCell<'_>>> = key_values
            .iter()
            .map(|value| KeyCell::try_new(value, dict))
            .collect();
        let start = match &key {
            Some(key) => self
                .position(key, None)
                .expect_err("a key without a row id never matches an entry"),
            // An id cell with an unseen commit hash matches no stored row.
            None => self.len(),
        };
        (start..self.len())
            .take_while(move |&row| key.as_ref().is_some_and(|key| self.key_matches(row, key)))
            .map(|row| self.row_ids.at(row))
    }

    /// Base table row ids of every stored row whose key cells equal
    /// `key_values`, in row id order. Empty when the key is absent.
    pub(crate) fn row_ids_for(
        &self,
        key_values: &[&CellValue],
        dict: &HashMapper,
    ) -> impl Iterator<Item = RowId> {
        self.packed_row_ids_for(key_values, dict)
            .map(|id| id.unpack(dict))
    }

    /// Whether any stored row has exactly these key cell values.
    pub(crate) fn contains_key(&self, key_values: &[&CellValue], dict: &HashMapper) -> bool {
        self.row_ids_for(key_values, dict).next().is_some()
    }

    /// Add the entry for a row entering the base table.
    pub(crate) fn insert(
        &mut self,
        row_values: &[CellValue],
        row_id: PackedRowId,
        dict: &mut HashMapper,
    ) {
        let key: Vec<KeyCell<'_>> = self
            .key_cols
            .iter()
            .map(|&ci| KeyCell::new_interning(&row_values[ci], dict))
            .collect();
        let pos = match self.position(&key, Some(row_id)) {
            Ok(pos) | Err(pos) => pos,
        };
        for (col, &ci) in self.keys.iter_mut().zip(&self.key_cols) {
            col.insert(pos, row_values[ci].clone(), dict);
        }
        self.row_ids.insert(pos, row_id);
    }

    /// Drop the entry for a row leaving the base table.
    pub(crate) fn remove(
        &mut self,
        row_values: &[CellValue],
        row_id: PackedRowId,
        dict: &HashMapper,
    ) {
        let key: Vec<KeyCell<'_>> = self
            .key_cols
            .iter()
            .map(|&ci| {
                KeyCell::try_new(&row_values[ci], dict)
                    .expect("stored key cells were interned on insert")
            })
            .collect();
        let pos = self
            .position(&key, Some(row_id))
            .expect("stored rows have an index entry");
        for col in &mut self.keys {
            col.remove(pos);
        }
        self.row_ids.remove(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::hash::CommitHash;
    use crate::ir::{self, BuiltinTy, ColType, Path};
    use crate::table::RowId;

    fn packed(commit_idx: u32, counter: u32) -> PackedRowId {
        PackedRowId {
            commit_idx,
            counter,
        }
    }

    /// Dictionary with `n` interned commit hashes, so fabricated packed row
    /// ids with `commit_idx < n` can be unpacked.
    fn dict_with_hashes(n: u8) -> HashMapper {
        let mut dict = HashMapper::new();
        for i in 0..n {
            dict.insert(CommitHash([i; 32]));
        }
        dict
    }

    fn two_int_schema() -> ir::Schema {
        ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: ["c0", "c1"]
                .iter()
                .map(|name| ir::ColumnEntry {
                    path: Path::from(*name),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                })
                .collect(),
            primary_key: None,
        }
    }

    fn int_at(index: &HexaneIndex, key: usize, row: usize) -> i64 {
        match &index.keys[key] {
            Column::Int(cells) => cells.get(row).unwrap(),
            other => panic!("expected int key column, got {other:?}"),
        }
    }

    fn entries(index: &HexaneIndex) -> Vec<(i64, i64, PackedRowId)> {
        (0..index.len())
            .map(|row| {
                (
                    int_at(index, 0, row),
                    int_at(index, 1, row),
                    index.row_ids.get(row).unwrap(),
                )
            })
            .collect()
    }

    /// Entries stay sorted by (c1, c0, row id) under adversarial insert
    /// order, with the second key column deciding ties.
    #[test]
    fn entries_stay_sorted_with_multi_column_keys() {
        let schema = two_int_schema();
        // Key order (c1, c0) differs from column order on purpose.
        let mut index = HexaneIndex::new(vec![1, 0], &schema);
        let mut dict = HashMapper::new();

        let rows = [
            (vec![CellValue::Int(2), CellValue::Int(1)], packed(0, 0)),
            (vec![CellValue::Int(1), CellValue::Int(1)], packed(0, 1)),
            (vec![CellValue::Int(3), CellValue::Int(0)], packed(0, 2)),
            (vec![CellValue::Int(0), CellValue::Int(1)], packed(0, 3)),
        ];
        for (values, rid) in &rows {
            index.insert(values, *rid, &mut dict);
        }

        // Sorted by c1 first, then c0.
        assert_eq!(
            entries(&index),
            vec![
                (0, 3, packed(0, 2)),
                (1, 0, packed(0, 3)),
                (1, 1, packed(0, 1)),
                (1, 2, packed(0, 0)),
            ]
        );
    }

    /// Duplicate keys are allowed and tie-broken by row id; removal takes out
    /// exactly the matching entry.
    #[test]
    fn duplicate_keys_tie_break_by_row_id() {
        let schema = two_int_schema();
        let mut index = HexaneIndex::new(vec![0], &schema);
        let mut dict = dict_with_hashes(2);

        let values = vec![CellValue::Int(7), CellValue::Int(0)];
        index.insert(&values, packed(0, 1), &mut dict);
        index.insert(&values, packed(0, 0), &mut dict);
        index.insert(&values, packed(1, 0), &mut dict);

        assert_eq!(
            (0..index.len())
                .map(|row| index.row_ids.get(row).unwrap())
                .collect::<Vec<_>>(),
            vec![packed(0, 0), packed(0, 1), packed(1, 0)]
        );

        index.remove(&values, packed(0, 1), &dict);
        assert!(index.contains_key(&[&CellValue::Int(7)], &dict));
        assert_eq!(
            (0..index.len())
                .map(|row| index.row_ids.get(row).unwrap())
                .collect::<Vec<_>>(),
            vec![packed(0, 0), packed(1, 0)]
        );
    }

    /// `row_ids_for` returns the base table row ids of every row matching a
    /// key, and nothing for absent keys.
    #[test]
    fn row_ids_for_returns_matching_rows() {
        let schema = two_int_schema();
        let mut index = HexaneIndex::new(vec![0], &schema);
        let mut dict = HashMapper::new();

        let rid = |commit_byte: u8, counter: u32| RowId {
            commit: CommitHash([commit_byte; 32]),
            counter,
        };
        let rows = [
            (7, rid(1, 0)),
            (5, rid(1, 1)),
            (7, rid(2, 0)),
            (9, rid(2, 1)),
        ];
        for (key, row_id) in rows {
            let packed = PackedRowId::pack(row_id, &mut dict);
            index.insert(&[CellValue::Int(key), CellValue::Int(0)], packed, &mut dict);
        }

        let ids_for = |key: i64| {
            index
                .row_ids_for(&[&CellValue::Int(key)], &dict)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids_for(7), vec![rid(1, 0), rid(2, 0)]);
        assert_eq!(ids_for(5), vec![rid(1, 1)]);
        assert_eq!(ids_for(9), vec![rid(2, 1)]);
        assert_eq!(ids_for(6), vec![]);
        assert_eq!(ids_for(10), vec![]);
    }

    /// A key holding an id whose commit hash was never interned cannot match.
    #[test]
    fn unseen_commit_hash_key_never_matches() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![ir::ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::RowId {
                    path: Path::from("T"),
                },
            }],
            primary_key: None,
        };
        let mut index = HexaneIndex::new(vec![0], &schema);
        let mut dict = HashMapper::new();

        let stored = RowId {
            commit: CommitHash([1; 32]),
            counter: 0,
        };
        index.insert(&[CellValue::Id(stored)], packed(0, 0), &mut dict);

        assert!(index.contains_key(&[&CellValue::Id(stored)], &dict));
        let unseen = RowId {
            commit: CommitHash([9; 32]),
            counter: 0,
        };
        assert!(!index.contains_key(&[&CellValue::Id(unseen)], &dict));
    }

    /// `contains_key` on an empty index and past-the-end insertion points.
    #[test]
    fn contains_key_handles_empty_and_boundary_positions() {
        let schema = two_int_schema();
        let mut index = HexaneIndex::new(vec![0], &schema);
        let mut dict = dict_with_hashes(1);

        assert!(!index.contains_key(&[&CellValue::Int(1)], &dict));

        index.insert(
            &[CellValue::Int(5), CellValue::Int(0)],
            packed(0, 0),
            &mut dict,
        );
        // Below, at, and above the only stored key.
        assert!(!index.contains_key(&[&CellValue::Int(4)], &dict));
        assert!(index.contains_key(&[&CellValue::Int(5)], &dict));
        assert!(!index.contains_key(&[&CellValue::Int(6)], &dict));
    }
}
