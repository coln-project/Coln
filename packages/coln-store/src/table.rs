// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::fmt::Write;

use crate::commit::hash::CommitHash;
use crate::commit::hash_dict::HashMapper;
use crate::ir;
use crate::ir::{BuiltinTy, ColType, Schema};
use crate::txn::ops::TxnId;

pub type TableOid = u64;

/// The unique id that identifies each row in a table
/// It is managed by the database, read-only for the user
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct RowId {
    pub commit: CommitHash,
    pub counter: u32,
}

impl fmt::Display for RowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.commit.0[..6] {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ":{}", self.counter)
    }
}

/// This is a RowId representation used internally by dictionary encoding the
/// hashes. So now we have 8 bytes instead of 36 bytes per row id.
///
/// Only meaningful together with the [`HashMapper`] that produced it, so it
/// never crosses the table boundary: public APIs still speak [`RowId`].
/// It is also only meaningful in memory, which is exactly the use case we want
/// to optimise here.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
struct PackedRowId {
    commit_idx: u32,
    counter: u32,
}

impl PackedRowId {
    /// Pack `id`, interning its commit hash in `dict` if it is new.
    fn pack(id: RowId, dict: &mut HashMapper) -> Self {
        PackedRowId {
            commit_idx: dict.insert(id.commit),
            counter: id.counter,
        }
    }

    /// Pack without interning: `None` when the commit hash is not in `dict`,
    /// which means no stored row can carry `id`.
    fn lookup(id: RowId, dict: &HashMapper) -> Option<Self> {
        Some(PackedRowId {
            commit_idx: dict.index(id.commit)?,
            counter: id.counter,
        })
    }

    fn unpack(self, dict: &HashMapper) -> RowId {
        RowId {
            commit: dict
                .hash_at(self.commit_idx)
                .expect("packed row id commit hash was interned on insert"),
            counter: self.counter,
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("column count mismatch: expected {expected}, got {got}")]
    ColumnCount { expected: usize, got: usize },
    #[error("type mismatch at column {column}: expected {expected}, got {got}")]
    TypeMismatch {
        column: usize,
        expected: CellKind,
        got: CellKind,
    },
    #[error("primary key references unknown column: {name}")]
    InvalidPrimaryKeyName { name: ColName },
    #[error("duplicate primary key")]
    DuplicatePrimaryKey,
    /// No table registered for this path (e.g. batch apply).
    #[error("unknown table: {path:?}")]
    UnknownTable { path: ir::Path },
    #[error("table mismatch: expected: {expected:?}, actual: {actual:?}")]
    TableMismatch {
        expected: ir::Path,
        actual: ir::Path,
    },
    #[error("row handle belongs to a different transaction: current {current:?}, got {got:?}")]
    TxnIdMismatch { current: TxnId, got: TxnId },
    #[error("invalid row handle: {reason}")]
    InvalidRowHandle { reason: String },
}

/// One cell in columnar storage: entity id, or primitive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CellValue {
    Id(RowId),
    Int(i64),
    Str(String),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CellKind {
    RowId,
    Int,
    Str,
}

impl From<&ColType> for CellKind {
    fn from(col_type: &ColType) -> Self {
        match col_type {
            ColType::RowId { .. } => CellKind::RowId,
            ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            } => CellKind::Int,
            ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinStr,
            } => CellKind::Str,
        }
    }
}

impl From<&CellValue> for CellKind {
    fn from(value: &CellValue) -> Self {
        match value {
            CellValue::Id(_) => CellKind::RowId,
            CellValue::Int(_) => CellKind::Int,
            CellValue::Str(_) => CellKind::Str,
        }
    }
}

impl fmt::Display for CellKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CellKind::RowId => "entity id",
            CellKind::Int => "int",
            CellKind::Str => "string",
        })
    }
}

impl CellValue {
    fn matches_schema(&self, col_type: &ColType, column: usize) -> Result<(), ValidationError> {
        let expected = CellKind::from(col_type);
        let got = CellKind::from(self);
        if expected == got {
            Ok(())
        } else {
            Err(ValidationError::TypeMismatch {
                column,
                expected,
                got,
            })
        }
    }
}

impl fmt::Display for CellValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellValue::Id(id) => write!(f, "#{id}"),
            CellValue::Int(value) => write!(f, "{value}"),
            CellValue::Str(value) => write!(f, "{value:?}"),
        }
    }
}

/// Public facing row value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowView {
    pub row_id: RowId,
    pub values: Vec<CellValue>,
}

type ColName = ir::Path;

/// Columnar storage for [`PackedRowId`]s, split into two parallel columns.
///
/// Commit indexes are RLE encoded: rows created by the same commit form long
/// runs of the same `u32`. Counters are delta encoded: they ascend within a
/// commit, so consecutive deltas are mostly 1.
///
/// When used as [`Table::row_ids`] the ids are kept sorted by
/// `(commit_idx, counter)`, which makes [`position`](Self::position) the row
/// lookup mechanism.
#[derive(Debug, Clone)]
struct IdColumn {
    commit_idxs: hexane::Column<u32>,
    counters: hexane::DeltaColumn<u32>,
}

impl IdColumn {
    fn new() -> Self {
        IdColumn {
            commit_idxs: hexane::Column::new(),
            counters: hexane::DeltaColumn::new(),
        }
    }

    fn get(&self, row: usize) -> Option<PackedRowId> {
        Some(PackedRowId {
            commit_idx: self.commit_idxs.get(row)?,
            counter: self.counters.get(row)?,
        })
    }

    fn insert(&mut self, row: usize, id: PackedRowId) {
        self.commit_idxs.insert(row, id.commit_idx);
        self.counters.insert(row, id.counter);
    }

    fn remove(&mut self, row: usize) {
        self.commit_idxs.remove(row);
        self.counters.remove(row);
    }

    /// Sorted position of `id`: `Ok(row)` when present, `Err(row)` with the
    /// insertion point otherwise. Requires ids sorted by
    /// `(commit_idx, counter)`.
    fn position(&self, id: PackedRowId) -> Result<usize, usize> {
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

        // Binary search the rest of the run. Each `get` probe jumps to the
        // containing slab through the column index instead of decoding the
        // run from the start.
        let mut lo = run.start;
        let mut hi = run.end - 1;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let counter = self.counters.get(mid).expect("run is in bounds");
            match counter.cmp(&id.counter) {
                Ordering::Less => lo = mid + 1,
                Ordering::Equal => return Ok(mid),
                Ordering::Greater => hi = mid,
            }
        }
        Err(lo)
    }

    fn len(&self) -> usize {
        self.counters.len()
    }
}

/// One column of typed storage. The variant is fixed by the schema column type.
/// Each id is now 8 bytes instead of a 40-byte [`CellValue`].
#[derive(Debug, Clone)]
enum Column {
    Id(IdColumn),
    Int(hexane::Column<i64>),
    Str(hexane::Column<String>),
}

impl Column {
    fn new(kind: CellKind) -> Self {
        match kind {
            CellKind::RowId => Column::Id(IdColumn::new()),
            CellKind::Int => Column::Int(hexane::Column::<i64>::new()),
            CellKind::Str => Column::Str(hexane::Column::<String>::new()),
        }
    }

    /// Insert a schema-validated cell at `row`. Panics on a type mismatch,
    /// which [`Table::validate_insert`] rules out before rows reach storage.
    fn insert(&mut self, row: usize, value: CellValue, dict: &mut HashMapper) {
        match (self, value) {
            (Column::Id(cells), CellValue::Id(id)) => {
                cells.insert(row, PackedRowId::pack(id, dict))
            }
            (Column::Int(cells), CellValue::Int(value)) => cells.insert(row, value),
            (Column::Str(cells), CellValue::Str(value)) => cells.insert(row, value),
            (column, value) => panic!(
                "cell type mismatch: column stores {:?}, got {value}",
                CellKind::from(&*column)
            ),
        }
    }

    fn remove(&mut self, row: usize) {
        match self {
            Column::Id(cells) => cells.remove(row),
            Column::Int(cells) => cells.remove(row),
            Column::Str(cells) => cells.remove(row),
        }
    }

    fn get(&self, row: usize, dict: &HashMapper) -> Option<CellValue> {
        match self {
            Column::Id(cells) => cells.get(row).map(|id| CellValue::Id(id.unpack(dict))),
            Column::Int(cells) => cells.get(row).map(CellValue::Int),
            Column::Str(cells) => cells.get(row).map(|s| CellValue::Str(s.to_owned())),
        }
    }

    /// Equality against a candidate cell without materialising the stored
    /// value. An id whose commit hash is absent from `dict` cannot be stored
    /// in this table, so it does not match.
    fn matches(&self, row: usize, value: &CellValue, dict: &HashMapper) -> bool {
        match (self, value) {
            (Column::Id(cells), CellValue::Id(id)) => {
                PackedRowId::lookup(*id, dict).is_some_and(|packed| cells.get(row) == Some(packed))
            }
            (Column::Int(cells), CellValue::Int(value)) => cells.get(row) == Some(*value),
            (Column::Str(cells), CellValue::Str(value)) => cells.get(row) == Some(value),
            _ => false,
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

/// Columnar store: `cols[i]` is all values for schema column `i` (same length per column).
///
/// Row ids are dictionary encoded: each distinct commit hash is stored once in
/// `hash_dict` and rows refer to it by a `u32` index (see [`PackedRowId`]).
/// The dictionary is append-only, so packed ids stay valid for the lifetime of
/// the table.
#[derive(Debug, Clone)]
pub struct Table {
    path: ir::Path,
    schema: Schema,
    col_index: HashMap<ColName, usize>,
    hashcons: bool,
    hash_dict: HashMapper,
    row_ids: IdColumn,
    cols: Vec<Column>,
}

impl Table {
    pub fn new(path: ir::Path, schema: Schema) -> Self {
        let col_index = schema
            .columns
            .iter()
            .enumerate()
            .map(|(i, column)| (column.path.clone(), i))
            .collect();
        let cols = schema
            .columns
            .iter()
            .map(|column| Column::new(CellKind::from(&column.col_type)))
            .collect();
        Self {
            path,
            col_index,
            schema,
            hashcons: false,
            hash_dict: HashMapper::new(),
            row_ids: IdColumn::new(),
            cols,
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn path(&self) -> &ir::Path {
        &self.path
    }

    pub(crate) fn hashcons(&self) -> bool {
        self.hashcons
    }

    #[cfg(test)]
    pub(crate) fn set_hashcons_for_test(&mut self, hashcons: bool) {
        self.hashcons = hashcons;
    }

    fn column_index(&self, name: &ir::Path) -> Option<usize> {
        self.col_index.get(name).copied()
    }

    pub fn row_count(&self) -> usize {
        // We need to return row_ids here, because cols might be empty for tables with only ids but nothing else
        self.row_ids.len()
    }

    /// Row id at a given physical row index.
    pub fn row_id_at(&self, row_idx: usize) -> Option<RowId> {
        self.row_ids
            .get(row_idx)
            .map(|packed| packed.unpack(&self.hash_dict))
    }

    /// Cell at `(row_idx, col_idx)` in columnar storage.
    pub fn cell_at(&self, row_idx: usize, col_idx: usize) -> Option<CellValue> {
        self.cols
            .get(col_idx)
            .and_then(|col| col.get(row_idx, &self.hash_dict))
    }

    pub(crate) fn row_at(&self, row_idx: usize) -> Option<RowView> {
        let row_id = self.row_id_at(row_idx)?;
        let values = (0..self.schema.columns.len())
            .map(|col_idx| self.cell_at(row_idx, col_idx))
            .collect::<Option<Vec<_>>>()?;

        Some(RowView { row_id, values })
    }

    /// Dump table contents row by row for debugging.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "table {} (rows: {}, cols: {})",
            self.path,
            self.row_count(),
            self.schema.columns.len()
        );

        for row_idx in 0..self.row_count() {
            // should be fine here as
            let row_id = self.row_ids.get(row_idx).unwrap().unpack(&self.hash_dict);
            let _ = write!(out, "[{row_idx}] row_id={row_id}");
            for col_idx in 0..self.schema.columns.len() {
                let value = self.cols[col_idx]
                    .get(row_idx, &self.hash_dict)
                    .expect("columns have one cell per row");
                let _ = write!(out, " | c{col_idx}={value}");
            }
            let _ = writeln!(out);
        }

        out
    }

    /// Checks that a row has the right number of values for this table. This is
    /// a preliminary check that is done as soon as an operation is added. More
    /// complex check is in validate_insert and deferred at commit time
    pub fn validate_column_count(&self, got: usize) -> Result<(), ValidationError> {
        let expected = self.schema.columns.len();
        if got != expected {
            return Err(ValidationError::ColumnCount { expected, got });
        }
        Ok(())
    }

    /// Checks schema and primary-key constraints against rows already stored.
    pub fn validate_insert(&self, values: &[CellValue]) -> Result<(), ValidationError> {
        // duplicated as txn::add(), but this is cheap enough we can afford to
        // do it here just in case.
        self.validate_column_count(values.len())?;

        for (i, (col_entry, value)) in self.schema.columns.iter().zip(values.iter()).enumerate() {
            value.matches_schema(&col_entry.col_type, i)?;
        }

        if let Some(pk) = &self.schema.primary_key {
            if !pk.is_empty() {
                let n = self.row_count();
                let pk_indexes = pk
                    .iter()
                    .map(|col_name| {
                        self.column_index(col_name).ok_or_else(|| {
                            ValidationError::InvalidPrimaryKeyName {
                                name: col_name.clone(),
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for row in 0..n {
                    let same = pk_indexes
                        .iter()
                        .all(|&ci| self.cols[ci].matches(row, &values[ci], &self.hash_dict));
                    if same {
                        return Err(ValidationError::DuplicatePrimaryKey);
                    }
                }
            } else if self.row_count() >= 1 {
                // A primary key with empty column only allows at most one row,
                // hence inserting any more rows would be an error
                return Err(ValidationError::DuplicatePrimaryKey);
            }
        }
        Ok(())
    }

    /// Values at primary-key columns for this row.
    /// A primary key definition would occur in tables that do not end up in Query
    /// An empty primary key means the table would have at most one row.
    pub fn primary_key_values(&self, values: &[CellValue]) -> Option<Vec<CellValue>> {
        self.schema.primary_key.as_ref().and_then(|pk| {
            if pk.is_empty() {
                Some(Vec::new())
            } else {
                pk.iter()
                    .map(|name| {
                        let i = self.column_index(name)?;
                        Some(values[i].clone())
                    })
                    .collect()
            }
        })
    }

    /// Physical row index of `row_id`, found by sorted search on the id
    /// columns. `None` when the row is not stored.
    pub fn row_position(&self, row_id: RowId) -> Option<usize> {
        let packed = PackedRowId::lookup(row_id, &self.hash_dict)?;
        self.row_ids.position(packed).ok()
    }

    /// Insert a row into columnar storage at its sorted position.
    ///
    /// Does **not** validate. Used internally when the caller has already checked the row
    /// (e.g. batch validation).
    pub(crate) fn insert_row(&mut self, values: Vec<CellValue>, row_id: RowId) {
        debug_assert_eq!(values.len(), self.schema.columns.len());

        let packed = PackedRowId::pack(row_id, &mut self.hash_dict);
        let pos = match self.row_ids.position(packed) {
            Ok(pos) | Err(pos) => pos,
        };
        self.row_ids.insert(pos, packed);
        for (i, v) in values.into_iter().enumerate() {
            self.cols[i].insert(pos, v, &mut self.hash_dict);
        }
    }

    /// Used for canonicalising row_ids. The row moves to the sorted position
    /// of its new id, together with its cells.
    pub(crate) fn replace_row_id(
        &mut self,
        old: &RowId,
        new: RowId,
    ) -> Result<(), ValidationError> {
        let old_pos = self
            .row_position(*old)
            .ok_or_else(|| ValidationError::InvalidRowHandle {
                reason: format!("attempting to replace non-existing row_id {old}"),
            })?;
        let values = (0..self.cols.len())
            .map(|col_idx| {
                self.cell_at(old_pos, col_idx)
                    .expect("columns have one cell per row")
            })
            .collect();
        self.row_ids.remove(old_pos);
        for col in &mut self.cols {
            col.remove(old_pos);
        }
        self.insert_row(values, new);
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::ir::{self, Path};

    fn test_row_id(counter: u32) -> RowId {
        RowId {
            commit: CommitHash([0; 32]),
            counter,
        }
    }

    fn row_id_from(commit_byte: u8, counter: u32) -> RowId {
        RowId {
            commit: CommitHash([commit_byte; 32]),
            counter,
        }
    }

    fn id_schema(columns: &[&str]) -> ir::Schema {
        ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: columns
                .iter()
                .map(|name| ir::ColumnEntry {
                    path: Path::from(*name),
                    col_type: ColType::RowId {
                        path: Path::from("T"),
                    },
                })
                .collect(),
            primary_key: None,
        }
    }

    /// Tables with no data columns still allocate row ids on insert; `row_count` must reflect
    /// those rows (it cannot use column length when `cols` is empty).
    #[test]
    fn row_count_matches_inserts_when_schema_has_no_columns() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![],
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("id_only"), schema);
        assert!(tbl.cols.is_empty());
        assert_eq!(tbl.row_count(), 0);

        let r0 = test_row_id(0);
        tbl.insert_row(vec![], r0);
        assert_eq!(tbl.row_count(), 1);
        assert_eq!(tbl.row_id_at(0), Some(r0));

        let r1 = test_row_id(1);
        tbl.insert_row(vec![], r1);
        assert_eq!(tbl.row_count(), 2);
        assert_eq!(tbl.row_id_at(1), Some(r1));
    }

    /// `primary_key: Some([])` marks a singleton table (at most one row).
    #[test]
    fn empty_primary_key_rejects_second_row() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![ir::ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: Some(vec![]),
        };
        let mut tbl = Table::new(Path::from("singleton"), schema);

        tbl.insert_row(vec![CellValue::Int(0)], test_row_id(0));
        assert_eq!(tbl.row_count(), 1);

        let values1 = vec![CellValue::Int(1)];
        let err = tbl.validate_insert(&values1).unwrap_err();
        assert_eq!(err, ValidationError::DuplicatePrimaryKey);
        assert_eq!(tbl.row_count(), 1);
    }

    #[test]
    fn row_read_helpers_return_row_id_and_cells() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![
                ir::ColumnEntry {
                    path: Path::from("c0"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                },
                ir::ColumnEntry {
                    path: Path::from("c1"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinStr,
                    },
                },
            ],
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("readable"), schema);

        let row_id = test_row_id(0);
        tbl.insert_row(
            vec![CellValue::Int(7), CellValue::Str("x".to_string())],
            row_id,
        );

        assert_eq!(
            tbl.row_at(0),
            Some(RowView {
                row_id,
                values: vec![CellValue::Int(7), CellValue::Str("x".to_string())],
            })
        );
        assert_eq!(tbl.row_id_at(0), Some(row_id));
        assert_eq!(tbl.cell_at(0, 0), Some(CellValue::Int(7)));
        assert_eq!(tbl.cell_at(0, 1), Some(CellValue::Str("x".to_string())));
        assert_eq!(tbl.row_at(1), None);
        assert_eq!(tbl.row_id_at(1), None);
        assert_eq!(tbl.cell_at(0, 2), None);
    }

    /// Row ids and id cells survive the pack/unpack round trip across rows
    /// from different commits.
    #[test]
    fn packed_row_ids_round_trip_across_commits() {
        let mut tbl = Table::new(Path::from("edges"), id_schema(&["src", "dst"]));

        let rows = [
            (row_id_from(1, 0), row_id_from(3, 7), row_id_from(4, 8)),
            (row_id_from(2, 1), row_id_from(3, 9), row_id_from(1, 0)),
            (row_id_from(1, 2), row_id_from(2, 1), row_id_from(3, 7)),
        ];
        for (rid, src, dst) in rows {
            tbl.insert_row(vec![CellValue::Id(src), CellValue::Id(dst)], rid);
        }

        for (rid, src, dst) in rows {
            let idx = tbl.row_position(rid).expect("row is stored");
            assert_eq!(tbl.row_id_at(idx), Some(rid));
            assert_eq!(tbl.cell_at(idx, 0), Some(CellValue::Id(src)));
            assert_eq!(tbl.cell_at(idx, 1), Some(CellValue::Id(dst)));
        }

        // Four distinct commit hashes, each interned exactly once.
        assert_eq!(tbl.hash_dict.hashes().len(), 4);
    }

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

    /// Rows are stored sorted by packed row id regardless of insertion order,
    /// and `row_position` reports presence and absence accordingly.
    #[test]
    fn rows_stay_sorted_by_row_id() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![ir::ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("sorted"), schema);

        // Commit A is interned first, so its rows sort before commit B's, and
        // counters order rows within a commit.
        let rows = [
            (row_id_from(1, 5), 0),
            (row_id_from(2, 0), 1),
            (row_id_from(1, 0), 2),
            (row_id_from(2, 7), 3),
            (row_id_from(1, 2), 4),
        ];
        for (rid, v) in rows {
            tbl.insert_row(vec![CellValue::Int(v)], rid);
        }

        let stored: Vec<RowId> = (0..tbl.row_count())
            .map(|idx| tbl.row_id_at(idx).expect("row id"))
            .collect();
        assert_eq!(
            stored,
            vec![
                row_id_from(1, 0),
                row_id_from(1, 2),
                row_id_from(1, 5),
                row_id_from(2, 0),
                row_id_from(2, 7),
            ]
        );

        // Cells moved together with their row ids.
        for (rid, v) in rows {
            let idx = tbl.row_position(rid).expect("row is stored");
            assert_eq!(tbl.cell_at(idx, 0), Some(CellValue::Int(v)));
        }

        // Absent ids: known commit with unused counter, and unknown commit.
        assert_eq!(tbl.row_position(row_id_from(1, 3)), None);
        assert_eq!(tbl.row_position(row_id_from(9, 0)), None);
    }

    /// `replace_row_id` moves the row and its cells to the sorted position of
    /// the new id.
    #[test]
    fn replace_row_id_moves_row_to_sorted_position() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![ir::ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("moving"), schema);

        tbl.insert_row(vec![CellValue::Int(0)], row_id_from(1, 0));
        tbl.insert_row(vec![CellValue::Int(1)], row_id_from(1, 1));
        tbl.insert_row(vec![CellValue::Int(2)], row_id_from(1, 2));

        // (1, 1) -> (1, 9): the row moves from the middle to the end.
        tbl.replace_row_id(&row_id_from(1, 1), row_id_from(1, 9))
            .expect("replace row id");

        assert_eq!(tbl.row_position(row_id_from(1, 1)), None);
        let idx = tbl.row_position(row_id_from(1, 9)).expect("row is stored");
        assert_eq!(idx, 2);
        assert_eq!(tbl.cell_at(idx, 0), Some(CellValue::Int(1)));
        assert_eq!(tbl.row_id_at(0), Some(row_id_from(1, 0)));
        assert_eq!(tbl.row_id_at(1), Some(row_id_from(1, 2)));
    }

    /// `replace_row_id` re-indexes the row under a hash the dictionary has
    /// not seen before.
    #[test]
    fn replace_row_id_interns_new_commit_hash() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![],
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("id_only"), schema);

        let old = row_id_from(1, 0);
        tbl.insert_row(vec![], old);

        let new = row_id_from(2, 5);
        tbl.replace_row_id(&old, new).expect("replace row id");
        assert_eq!(tbl.row_id_at(0), Some(new));

        // Replacing an id that was never stored fails, including ids whose
        // commit hash is unknown to the dictionary.
        let missing = row_id_from(9, 0);
        assert!(matches!(
            tbl.replace_row_id(&missing, old),
            Err(ValidationError::InvalidRowHandle { .. })
        ));
        assert!(matches!(
            tbl.replace_row_id(&old, new),
            Err(ValidationError::InvalidRowHandle { .. })
        ));
    }

    /// Primary key comparison works on dictionary-encoded id columns, and an
    /// id with an unseen commit hash never collides.
    #[test]
    fn primary_key_detects_duplicates_in_id_columns() {
        let mut schema = id_schema(&["src", "dst"]);
        schema.primary_key = Some(vec![Path::from("src")]);
        let mut tbl = Table::new(Path::from("edges"), schema);

        let src = row_id_from(3, 7);
        tbl.insert_row(
            vec![CellValue::Id(src), CellValue::Id(row_id_from(4, 8))],
            row_id_from(1, 0),
        );

        let duplicate = vec![CellValue::Id(src), CellValue::Id(row_id_from(4, 9))];
        assert_eq!(
            tbl.validate_insert(&duplicate),
            Err(ValidationError::DuplicatePrimaryKey)
        );

        let unseen_commit = vec![
            CellValue::Id(row_id_from(9, 7)),
            CellValue::Id(row_id_from(4, 8)),
        ];
        assert!(tbl.validate_insert(&unseen_commit).is_ok());
    }

    #[test]
    fn debug_dumps_rows() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![
                ir::ColumnEntry {
                    path: Path::from("c0"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                },
                ir::ColumnEntry {
                    path: Path::from("c1"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinStr,
                    },
                },
            ],
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("debug.table"), schema);

        tbl.insert_row(
            vec![CellValue::Int(7), CellValue::Str("x".to_string())],
            test_row_id(0),
        );
        tbl.insert_row(
            vec![CellValue::Int(8), CellValue::Str("y".to_string())],
            test_row_id(1),
        );

        assert_eq!(
            tbl.dump(),
            format!(
                concat!(
                    "table debug.table (rows: 2, cols: 2)\n",
                    "[0] row_id={} | c0=7 | c1=\"x\"\n",
                    "[1] row_id={} | c0=8 | c1=\"y\"\n",
                ),
                test_row_id(0),
                test_row_id(1),
            )
        );
    }
}
