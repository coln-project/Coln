// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::cmp::Ordering;
use std::ops::Range;

use crate::id_packer::IdPacker;

use super::{CellKind, WireValue, PackedValue, PackedRowId};

/// Columnar storage for [`PackedRowId`]s, split into two parallel columns.
///
/// Commit indexes are RLE encoded: rows created by the same commit form long
/// runs of the same `u32`. Counters are delta encoded: they ascend within a
/// commit, so consecutive deltas are mostly 1.
#[derive(Debug, Clone)]
pub(super) struct IdColumn {
    commit_idxs: hexane::Column<u32>,
    counters: hexane::DeltaColumn<u32>,
}

impl IdColumn {
    pub(super) fn new() -> Self {
        IdColumn {
            commit_idxs: hexane::Column::new(),
            counters: hexane::DeltaColumn::new(),
        }
    }

    pub(super) fn get(&self, row: usize) -> Option<PackedRowId> {
        Some(PackedRowId {
            commit_idx: self.commit_idxs.get(row)?,
            counter: self.counters.get(row)?,
        })
    }

    /// Returns the id at `row`.
    ///
    /// Panics when `row` is out of bounds.
    pub(super) fn at(&self, row: usize) -> PackedRowId {
        self.get(row).unwrap()
    }

    pub(super) fn insert(&mut self, row: usize, id: PackedRowId) {
        self.commit_idxs.insert(row, id.commit_idx);
        self.counters.insert(row, id.counter);
    }

    pub(super) fn remove(&mut self, row: usize) {
        self.commit_idxs.remove(row);
        self.counters.remove(row);
    }

    /// Sorted position of `id`: `Ok(row)` when present, `Err(row)` with the
    /// insertion point otherwise. Requires ids sorted by
    /// `(commit_idx, counter)`.
    pub(super) fn position(&self, id: PackedRowId) -> Result<usize, usize> {
        let run = self.commit_idxs.scope_to_value(id.commit_idx, ..);
        if run.is_empty() {
            return Err(run.start);
        }

        // Counters within a commit usually arrive ascending, so new ids
        // mostly land at the end of their commit run; check it first to keep
        // commit-order inserts constant time.
        let last = self.counters.get(run.end - 1).expect("run is in bounds");
        match last.cmp(&id.counter) {
            Ordering::Less => return Err(run.end),
            Ordering::Equal => return Ok(run.end - 1),
            Ordering::Greater => {}
        }

        let found = self.counters.scope_to_value(id.counter, run);
        if found.is_empty() {
            Err(found.start)
        } else {
            Ok(found.start)
        }
    }

    pub(super) fn len(&self) -> usize {
        self.counters.len()
    }

    pub(super) fn scope_to_value(&self, value: PackedRowId, range: Range<usize>) -> Range<usize> {
        let range = self.commit_idxs.scope_to_value(value.commit_idx, range);
        self.counters.scope_to_value(value.counter, range)
    }
}


/// One column of typed storage. The variant is fixed by the schema column type.
/// Each id is 8 bytes instead of a 40-byte [`WireValue`].
#[derive(Debug, Clone)]
pub(super) enum Column {
    Id(IdColumn),
    Int(hexane::Column<i64>), // TODO: change to i32
    Str(hexane::Column<String>),
}

impl Column {
    pub(super) fn new(kind: CellKind) -> Self {
        match kind {
            CellKind::RowId => Column::Id(IdColumn::new()),
            CellKind::Int => Column::Int(hexane::Column::<i64>::new()),
            CellKind::Str => Column::Str(hexane::Column::<String>::new()),
        }
    }

    /// Insert a schema-validated cell at `row`.
    ///
    /// Panics on a type mismatch, which `Table::validate_insert` rules out
    /// before rows reach storage.
    pub(super) fn insert(&mut self, row: usize, value: PackedValue) {
        match (self, value) {
            (Column::Id(cells), PackedValue::Id(id)) => cells.insert(row, id),
            (Column::Int(cells), PackedValue::Int(value)) => cells.insert(row, value as i64),
            (Column::Str(cells), PackedValue::Str(value)) => cells.insert(row, value),
            (column, value) => panic!(
                "cell type mismatch: column stores {:?}, got {value:?}",
                CellKind::from(&*column)
            ),
        }
    }

    pub(super) fn remove(&mut self, row: usize) {
        match self {
            Column::Id(cells) => cells.remove(row),
            Column::Int(cells) => cells.remove(row),
            Column::Str(cells) => cells.remove(row),
        }
    }

    pub(super) fn get(&self, row: usize, packer: &IdPacker) -> Option<WireValue> {
        match self {
            Column::Id(cells) => cells
                .get(row)
                .map(|id| WireValue::Id(packer.unpack_row_id(id))),
            Column::Int(cells) => cells.get(row).map(|i| WireValue::Int(i as i32)),
            Column::Str(cells) => cells.get(row).map(|s| WireValue::Str(s.to_owned())),
        }
    }

    pub(super) fn get_packed(&self, row: usize) -> Option<PackedValue> {
        match self {
            Column::Id(cells) => cells.get(row).map(PackedValue::Id),
            Column::Int(cells) => cells.get(row).map(|i| PackedValue::Int(i as i32)),
            Column::Str(cells) => cells.get(row).map(|s| PackedValue::Str(s.to_owned())),
        }
    }

    pub(super) fn scope_to_value(&self, value: &PackedValue, range: Range<usize>) -> Range<usize> {
        match (self, value) {
            (Column::Id(column), PackedValue::Id(value)) => column.scope_to_value(*value, range),
            (Column::Int(column), PackedValue::Int(value)) => column.scope_to_value(*value as i64, range),
            (Column::Str(column), PackedValue::Str(value)) => {
                column.scope_to_value(value.as_str(), range)
            }
            (column, value) => panic!(
                "index key type mismatch: column stores {:?}, got {value:?}",
                CellKind::from(column)
            ),
        }
    }
}

impl From<&Column> for CellKind {
    fn from(column: &Column) -> Self {
        match column {
            Column::Id(_) => CellKind::RowId,
            Column::Int(_) => CellKind::Int,
            Column::Str(_) => CellKind::Str,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `IdColumn::position` finds stored rows and insertion points through
    /// the end-of-run fast path, the binary search, and the empty-run case.
    #[test]
    fn id_column_position_finds_rows_and_insertion_points() {
        let mut ids = IdColumn::new();
        // Commit 0 with even counters 0, 2, ..., 126, then commit 1 with one row.
        for row in 0..64 {
            ids.insert(
                row,
                PackedRowId {
                    commit_idx: 0,
                    counter: row as u32 * 2,
                },
            );
        }
        ids.insert(
            64,
            PackedRowId {
                commit_idx: 1,
                counter: 5,
            },
        );

        for row in 0..64 {
            let id = PackedRowId {
                commit_idx: 0,
                counter: row as u32 * 2,
            };
            assert_eq!(ids.position(id), Ok(row), "counter {}", row * 2);
        }

        let absent = |commit_idx, counter| PackedRowId {
            commit_idx,
            counter,
        };
        // Odd counters fall between stored rows.
        assert_eq!(ids.position(absent(0, 7)), Err(4));
        // Before the first row of the run.
        assert_eq!(ids.position(absent(1, 0)), Err(64));
        // Past the end of the run (fast path).
        assert_eq!(ids.position(absent(0, 999)), Err(64));
        // Commit without any rows sorts after everything.
        assert_eq!(ids.position(absent(2, 0)), Err(65));
    }
}
