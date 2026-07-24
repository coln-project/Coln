// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod cell;
mod col;
pub(crate) mod index;
pub mod table_ref;

pub use cell::{CellKind, CellValue, RowId};
pub use table_ref::TableRef;

use std::collections::HashMap;
use std::fmt::Write;

use crate::commit::hash_dict::HashMapper;
use crate::ir;
use crate::ir::Schema;
use crate::table::index::{IndexId, IndexMeta, TableIndex};
use crate::txn::ops::TxnId;

pub(crate) use self::cell::{PackedCell, PackedRowId};
use self::col::{Column, IdColumn};

pub type TableOid = usize;

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
    #[error("invalid index id passed {index}")]
    InvalidIndex { index: u64 },
    #[error("invalid index key for index {index}: expected {expected} values, got {got}")]
    InvalidIndexKey {
        index: IndexId,
        expected: usize,
        got: usize,
    },
    #[error("lookup column {column} is outside the table's {column_count} columns")]
    InvalidLookupColumn { column: usize, column_count: usize },
}

/// Public facing row value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowView {
    pub row_id: RowId,
    pub values: Vec<CellValue>,
}

type ColName = ir::Path;

/// How the primary key constraint is checked on insert. Resolved once at
/// table construction.
#[derive(Debug, Clone)]
enum PkConstraint {
    /// No primary key in the schema.
    None,
    /// An empty primary key: the table holds at most one row.
    Singleton,
    /// A non-empty primary key backed by the sorted index at this position
    /// in [`Table::indexes`].
    Indexed(usize),
}

/// Columnar store: `cols[i]` is all values for schema column `i` (same length per column).
///
/// Row ids are dictionary encoded: each distinct commit hash is stored once
/// in the store-wide [`HashMapper`] and rows refer to it by a `u32` index
/// (see [`PackedRowId`]). The dictionary is append-only, so packed ids stay
/// valid for the lifetime of the store. The [`Store`](crate::store::Store)
/// owns the dictionary and passes it into every table operation that packs
/// or unpacks ids; [`TableRef`] bundles the two for read-only callers.
#[derive(Debug, Clone)]
pub struct Table {
    path: ir::Path,
    schema: Schema,
    col_name_map: HashMap<ColName, usize>,
    /// Structural (all-columns) index used for hashcons lookup, when enabled.
    hashcons_index: Option<IndexId>,
    row_ids: IdColumn,
    cols: Vec<Column>,
    /// Sorted secondary indexes, maintained by [`Self::insert_row`] and
    /// [`Self::replace_row_id`]. Currently only the primary key index.
    indexes: Vec<TableIndex>,
    pk: PkConstraint,
}

impl Table {
    // Basic accessors

    pub fn new(path: ir::Path, schema: Schema) -> Self {
        let col_name_map: HashMap<ColName, usize> = schema
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

        let mut indexes = Vec::new();
        let pk = match &schema.primary_key {
            None => PkConstraint::None,
            Some(pk) if pk.is_empty() => PkConstraint::Singleton,
            Some(pk) => {
                // Schemas come from the compiler, so an unresolvable primary
                // key column is a construction bug, not a runtime condition.
                let key_cols: Vec<usize> = pk
                    .iter()
                    // we can expect the schema to contain right information
                    .map(|name| {
                        col_name_map
                            .get(name)
                            .copied()
                            .expect("schema pk spec is correct")
                    })
                    .collect();
                indexes.push(TableIndex::new(&key_cols, &schema));
                // ? Is referring to the index id the right thing to do?
                PkConstraint::Indexed(indexes.len() - 1)
            }
        };

        // TODO if hashcons, then create another index
        let hashcons_cols: Vec<usize> = (0..schema.columns.len()).collect();
        indexes.push(TableIndex::new(&hashcons_cols, &schema));

        Self {
            path,
            col_name_map,
            schema,
            hashcons_index: None,
            row_ids: IdColumn::new(),
            cols,
            indexes,
            pk,
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn path(&self) -> &ir::Path {
        &self.path
    }

    pub(crate) fn hashcons(&self) -> bool {
        self.hashcons_index.is_some()
    }

    pub fn row_count(&self) -> usize {
        // We need to return row_ids here, because cols might be empty for tables with only ids but nothing else
        self.row_ids.len()
    }

    /// Row id at a given physical row index.
    pub(crate) fn row_id_at(&self, row_idx: usize, dict: &HashMapper) -> Option<RowId> {
        self.row_ids.get(row_idx).map(|packed| packed.unpack(dict))
    }

    /// Cell at `(row_idx, col_idx)` in columnar storage.
    pub(crate) fn cell_at(
        &self,
        row_idx: usize,
        col_idx: usize,
        dict: &HashMapper,
    ) -> Option<CellValue> {
        self.cols
            .get(col_idx)
            .and_then(|col| col.get(row_idx, dict))
    }

    /// Find the index of the row given a `row_id`. Internal API only.
    fn row_idx(&self, row_id: RowId, dict: &HashMapper) -> Option<usize> {
        let packed = PackedRowId::lookup(row_id, dict)?;
        self.row_ids.position(packed).ok()
    }

    pub fn indexes_meta(&self) -> Vec<IndexMeta<'_>> {
        self.indexes
            .iter()
            .enumerate()
            .map(|(id, index)| IndexMeta {
                id,
                key_cols: index.key_cols(),
            })
            .collect()
    }

    pub fn primary_index(&self) -> Option<IndexId> {
        match self.pk {
            PkConstraint::Indexed(i) => Some(i),
            PkConstraint::None | PkConstraint::Singleton => None,
        }
    }

    pub(crate) fn hashcons_index(&self) -> Option<IndexId> {
        self.hashcons_index
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SeekKey {
    pub(crate) column: usize,
    pub(crate) value: CellValue,
}

impl Table {
    // public(ish) facing lookup APIs

    pub(crate) fn row_at(&self, row_idx: usize, id_packer: &HashMapper) -> Option<RowView> {
        let row_id = self.row_id_at(row_idx, id_packer)?;
        let values = (0..self.schema.columns.len())
            .map(|col_idx| self.cell_at(row_idx, col_idx, id_packer))
            .collect::<Option<Vec<_>>>()?;

        Some(RowView { row_id, values })
    }

    /// O(N * log S) as first find out the index from the row_id, and then do a
    /// lookup on each column
    pub(crate) fn packed_row_at(&self, row_id: PackedRowId) -> Option<Vec<PackedCell>> {
        let row_idx = self.row_ids.position(row_id).ok()?;
        (0..self.schema.columns.len())
            .map(|col_idx| {
                self.cols
                    .get(col_idx)
                    .and_then(|col| col.get_packed(row_idx))
            })
            .collect()
    }

    pub(crate) fn table_scan(&self, id_packer: &HashMapper) -> impl Iterator<Item = RowView> {
        (0..self.row_count()).filter_map(move |row_idx| self.row_at(row_idx, id_packer))
    }

    pub(crate) fn seek(
        &self,
        key: &[SeekKey],
        id_packer: &HashMapper,
    ) -> Result<impl Iterator<Item = RowId>, ValidationError> {
        if let Some(column) = key
            .iter()
            .map(|part| part.column)
            .find(|&column| column >= self.cols.len())
        {
            return Err(ValidationError::InvalidLookupColumn {
                column,
                column_count: self.cols.len(),
            });
        }

        Ok((0..self.row_count()).filter_map(move |row_idx| {
            key.iter()
                .all(|part| {
                    self.cell_at(row_idx, part.column, id_packer).as_ref() == Some(&part.value)
                })
                .then(|| {
                    self.row_id_at(row_idx, id_packer)
                        .expect("row index came from the table's row count")
                })
        }))
    }

    pub(crate) fn index_seek(
        &self,
        index: IndexId,
        key: &[CellValue],
        id_packer: &HashMapper,
    ) -> Result<impl Iterator<Item = RowId>, ValidationError> {
        let table_index = self
            .indexes
            .get(index)
            .ok_or(ValidationError::InvalidIndex {
                index: index as u64,
            })?;
        if key.len() != table_index.key_cols().len() {
            return Err(ValidationError::InvalidIndexKey {
                index,
                expected: table_index.key_cols().len(),
                got: key.len(),
            });
        }

        let rows = key
            .iter()
            .map(|value| PackedCell::try_pack_cell(value, id_packer))
            .collect::<Option<Vec<_>>>()
            .map(|key| {
                table_index
                    .get(&key)
                    .map(|row_id| row_id.unpack(id_packer))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Ok(rows.into_iter())
    }

    pub(crate) fn index_seek_packed(
        &self,
        index: IndexId,
        key: &[PackedCell],
    ) -> Result<impl Iterator<Item = PackedRowId>, ValidationError> {
        let table_index = self
            .indexes
            .get(index)
            .ok_or(ValidationError::InvalidIndex {
                index: index as u64,
            })?;
        if key.len() != table_index.key_cols().len() {
            return Err(ValidationError::InvalidIndexKey {
                index,
                expected: table_index.key_cols().len(),
                got: key.len(),
            });
        }
        Ok(table_index.get(key))
    }

    pub(crate) fn lookup(
        &self,
        key: &[SeekKey],
        id_packer: &HashMapper,
    ) -> Result<bool, ValidationError> {
        Ok(self.seek(key, id_packer)?.next().is_some())
    }

    pub(crate) fn index_lookup(
        &self,
        index: IndexId,
        key: &[CellValue],
        id_packer: &HashMapper,
    ) -> Result<bool, ValidationError> {
        Ok(self.index_seek(index, key, id_packer)?.next().is_some())
    }
}

impl Table {
    // Basic validation against schema

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
    pub(crate) fn validate_insert(
        &self,
        values: &[CellValue],
        dict: &HashMapper,
    ) -> Result<(), ValidationError> {
        // duplicated as txn::add(), but this is cheap enough we can afford to
        // do it here just in case.
        self.validate_column_count(values.len())?;

        for (i, (col_entry, value)) in self.schema.columns.iter().zip(values.iter()).enumerate() {
            value.matches_schema(&col_entry.col_type, i)?;
        }

        match &self.pk {
            PkConstraint::None => {}
            PkConstraint::Singleton => {
                // A primary key with empty columns only allows at most one
                // row, hence inserting any more rows would be an error
                if self.row_count() >= 1 {
                    return Err(ValidationError::DuplicatePrimaryKey);
                }
            }
            PkConstraint::Indexed(i) => {
                let index = &self.indexes[*i];
                let Some(key) = index
                    .key_cols()
                    .iter()
                    .map(|&ci| PackedCell::try_pack_cell(&values[ci], dict))
                    .collect::<Option<Vec<_>>>()
                else {
                    // If we cannot pack, then the primary key should be absent, so noneed to check
                    return Ok(());
                };
                if index.contains_key(&key) {
                    return Err(ValidationError::DuplicatePrimaryKey);
                }
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
                        let i = self.col_name_map.get(name).copied()?;
                        Some(values[i].clone())
                    })
                    .collect()
            }
        })
    }
}

impl Table {
    // Adding and changing contents of the table

    /// Insert a row into columnar storage at its sorted position.
    ///
    /// Does **not** validate. Used internally when the caller has already checked the row
    /// (e.g. batch validation).
    pub(crate) fn insert_row(
        &mut self,
        values: Vec<CellValue>,
        row_id: RowId,
        dict: &mut HashMapper,
    ) {
        debug_assert_eq!(values.len(), self.schema.columns.len());
        let values: Vec<PackedCell> = values
            .iter()
            .map(|v| PackedCell::pack_cell(v, dict))
            .collect();
        self.insert_packed(values, row_id, dict);
    }

    fn insert_packed(&mut self, values: Vec<PackedCell>, row_id: RowId, dict: &mut HashMapper) {
        debug_assert_eq!(values.len(), self.schema.columns.len());

        let row_id = PackedRowId::pack(row_id, dict);
        for index in &mut self.indexes {
            let key = Self::project_index_key(index, &values);
            index.insert(key, row_id);
        }
        let pos = match self.row_ids.position(row_id) {
            Ok(pos) | Err(pos) => pos,
        };
        self.row_ids.insert(pos, row_id);
        for (i, v) in values.into_iter().enumerate() {
            self.cols[i].insert(pos, v);
        }
    }

    fn project_index_key(index: &TableIndex, values: &[PackedCell]) -> Vec<PackedCell> {
        index
            .key_cols()
            .iter()
            .map(|&col_idx| values[col_idx].clone())
            .collect()
    }

    /// Used for canonicalising row_ids. The row moves to the sorted position
    /// of its new id, together with its cells. Panics if `old` is absent,
    /// because rowing only reports swaps for physical rows.
    pub(crate) fn replace_row_id(&mut self, old: &RowId, new: RowId, dict: &mut HashMapper) {
        let old_pos = self
            .row_idx(*old, dict)
            .expect("row id should exist, as produced by rowing");
        let packed_old = PackedRowId::lookup(*old, dict)
            .expect("row_position found the row, so its commit hash is interned");

        let values = self
            .packed_row_at(packed_old)
            .expect("row id should exist, as produced by rowing");

        for index in &mut self.indexes {
            let key = Self::project_index_key(index, &values);
            index.remove_rowid(&key, packed_old);
        }
        self.row_ids.remove(old_pos);
        for col in &mut self.cols {
            col.remove(old_pos);
        }
        self.insert_packed(values, new, dict);
    }

    // TODO this is potentially an expensive operation. If on the hot path, then
    // we need to reconsider...

    /// Replace the cells of a stored row, keeping its row id. Used when a
    /// referenced row's canonical id changes and cells embedding the old id
    /// must be rewritten. Panics if `row_id` is absent, because rowing only
    /// reports fixups for physical rows.
    pub(crate) fn rewrite_row_cells(
        &mut self,
        row_id: &RowId,
        values: Vec<PackedCell>,
        dict: &mut HashMapper,
    ) {
        debug_assert_eq!(values.len(), self.schema.columns.len());
        let pos = self
            .row_idx(*row_id, dict)
            .expect("row id should exist, otherwise rowing is wrong");
        let packed = PackedRowId::lookup(*row_id, dict)
            .expect("row_position found the row, so its commit hash is interned");
        let old_values = self
            .packed_row_at(packed)
            .expect("row id should exist, as produced by rowing");

        for index in &mut self.indexes {
            let old_key = Self::project_index_key(index, &old_values);
            let new_key = Self::project_index_key(index, &values);
            index.remove_rowid(&old_key, packed);
            index.insert(new_key, packed);
        }

        for (i, v) in values.into_iter().enumerate() {
            self.cols[i].remove(pos);
            self.cols[i].insert(pos, v);
        }
    }
}

impl Table {
    // For debugging for testing

    // TODO remove this when we have schema level hashcons
    #[cfg(test)]
    pub(crate) fn set_hashcons_for_test(&mut self, hashcons: bool) {
        // `Table::new` always appends the all-columns index last; enable
        // hashcons by pointing at that slot.
        self.hashcons_index = hashcons.then_some(self.indexes.len() - 1);
    }

    /// Dump table contents row by row for debugging.
    pub(crate) fn dump(&self, dict: &HashMapper) -> String {
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
            let row_id = self.row_ids.at(row_idx).unpack(dict);
            let _ = write!(out, "[{row_idx}] row_id={row_id}");
            for col_idx in 0..self.schema.columns.len() {
                let value = self.cols[col_idx]
                    .get(row_idx, dict)
                    .expect("columns have one cell per row");
                let _ = write!(out, " | c{col_idx}={value}");
            }
            let _ = writeln!(out);
        }

        out
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::commit::hash::CommitHash;
    use crate::ir::{self, Path};
    use crate::ir::{BuiltinTy, ColType};

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

    /// A [`Table`] paired with its own dictionary, forwarding every method
    /// that takes a dict, so tests read like production call sites.
    struct TestTable {
        table: Table,
        dict: HashMapper,
    }

    impl TestTable {
        fn new(path: Path, schema: ir::Schema) -> Self {
            Self {
                table: Table::new(path, schema),
                dict: HashMapper::new(),
            }
        }

        fn row_count(&self) -> usize {
            self.table.row_count()
        }

        fn row_id_at(&self, row_idx: usize) -> Option<RowId> {
            self.table.row_id_at(row_idx, &self.dict)
        }

        fn cell_at(&self, row_idx: usize, col_idx: usize) -> Option<CellValue> {
            self.table.cell_at(row_idx, col_idx, &self.dict)
        }

        fn row_at(&self, row_idx: usize) -> Option<RowView> {
            self.table.row_at(row_idx, &self.dict)
        }

        fn row_position(&self, row_id: RowId) -> Option<usize> {
            self.table.row_idx(row_id, &self.dict)
        }

        fn validate_insert(&self, values: &[CellValue]) -> Result<(), ValidationError> {
            self.table.validate_insert(values, &self.dict)
        }

        fn insert_row(&mut self, values: Vec<CellValue>, row_id: RowId) {
            self.table.insert_row(values, row_id, &mut self.dict)
        }

        fn replace_row_id(&mut self, old: &RowId, new: RowId) {
            self.table.replace_row_id(old, new, &mut self.dict)
        }

        fn rewrite_row_cells(&mut self, row_id: &RowId, values: Vec<PackedCell>) {
            self.table.rewrite_row_cells(row_id, values, &mut self.dict)
        }

        fn dump(&self) -> String {
            self.table.dump(&self.dict)
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
        let mut tbl = TestTable::new(Path::from("id_only"), schema);
        assert!(tbl.table.cols.is_empty());
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
        let mut tbl = TestTable::new(Path::from("singleton"), schema);

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
        let mut tbl = TestTable::new(Path::from("readable"), schema);

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
        let packed = PackedRowId::lookup(row_id, &tbl.dict).expect("insert packed the row id");
        assert_eq!(
            tbl.table.packed_row_at(packed),
            Some(vec![PackedCell::Int(7), PackedCell::Str("x".to_string())])
        );
        assert_eq!(tbl.row_at(1), None);
        assert_eq!(tbl.row_id_at(1), None);
        assert_eq!(tbl.cell_at(0, 2), None);
    }

    /// Row ids and id cells survive the pack/unpack round trip across rows
    /// from different commits.
    #[test]
    fn packed_row_ids_round_trip_across_commits() {
        let mut tbl = TestTable::new(Path::from("edges"), id_schema(&["src", "dst"]));

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
        assert_eq!(tbl.dict.hashes().len(), 4);
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
        let mut tbl = TestTable::new(Path::from("sorted"), schema);

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
        let mut tbl = TestTable::new(Path::from("moving"), schema);

        tbl.insert_row(vec![CellValue::Int(0)], row_id_from(1, 0));
        tbl.insert_row(vec![CellValue::Int(1)], row_id_from(1, 1));
        tbl.insert_row(vec![CellValue::Int(2)], row_id_from(1, 2));

        // (1, 1) -> (1, 9): the row moves from the middle to the end.
        tbl.replace_row_id(&row_id_from(1, 1), row_id_from(1, 9));

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
        let mut tbl = TestTable::new(Path::from("id_only"), schema);

        let old = row_id_from(1, 0);
        tbl.insert_row(vec![], old);

        let new = row_id_from(2, 5);
        tbl.replace_row_id(&old, new);
        assert_eq!(tbl.row_id_at(0), Some(new));
    }

    #[test]
    #[should_panic(expected = "row id should exist, as produced by rowing")]
    fn replace_row_id_panics_when_old_row_is_missing() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![],
            primary_key: None,
        };
        let mut tbl = TestTable::new(Path::from("id_only"), schema);
        let missing = row_id_from(9, 0);
        tbl.replace_row_id(&missing, row_id_from(1, 0));
    }

    /// Primary key comparison works on dictionary-encoded id columns, and an
    /// id with an unseen commit hash never collides.
    #[test]
    fn primary_key_detects_duplicates_in_id_columns() {
        let mut schema = id_schema(&["src", "dst"]);
        schema.primary_key = Some(vec![Path::from("src")]);
        let mut tbl = TestTable::new(Path::from("edges"), schema);

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

    fn int_schema(columns: &[&str], primary_key: Option<&[&str]>) -> ir::Schema {
        ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: columns
                .iter()
                .map(|name| ir::ColumnEntry {
                    path: Path::from(*name),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                })
                .collect(),
            primary_key: primary_key.map(|pk| pk.iter().map(|name| Path::from(*name)).collect()),
        }
    }

    /// Multi-column primary keys reject a duplicate pair but accept rows
    /// sharing only one key column, regardless of insert order.
    #[test]
    fn multi_column_primary_key_checks_all_columns() {
        let schema = int_schema(&["c0", "c1", "c2"], Some(&["c0", "c1"]));
        let mut tbl = TestTable::new(Path::from("pairs"), schema);

        let rows = [(3, 1), (1, 2), (1, 1), (2, 1), (2, 2)];
        for (i, (a, b)) in rows.into_iter().enumerate() {
            let values = vec![CellValue::Int(a), CellValue::Int(b), CellValue::Int(0)];
            tbl.validate_insert(&values).expect("unique pair");
            tbl.insert_row(values, test_row_id(i as u32));
        }

        for (a, b) in rows {
            let dup = vec![CellValue::Int(a), CellValue::Int(b), CellValue::Int(9)];
            assert_eq!(
                tbl.validate_insert(&dup),
                Err(ValidationError::DuplicatePrimaryKey)
            );
        }
        let fresh = vec![CellValue::Int(3), CellValue::Int(2), CellValue::Int(0)];
        assert!(tbl.validate_insert(&fresh).is_ok());
    }

    /// String primary keys go through the sorted index as well.
    #[test]
    fn string_primary_key_detects_duplicates() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![ir::ColumnEntry {
                path: Path::from("name"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinStr,
                },
            }],
            primary_key: Some(vec![Path::from("name")]),
        };
        let mut tbl = TestTable::new(Path::from("named"), schema);

        for (i, name) in ["b", "a", "c"].into_iter().enumerate() {
            let values = vec![CellValue::Str(name.to_string())];
            tbl.validate_insert(&values).expect("unique name");
            tbl.insert_row(values, test_row_id(i as u32));
        }

        assert_eq!(
            tbl.validate_insert(&[CellValue::Str("a".to_string())]),
            Err(ValidationError::DuplicatePrimaryKey)
        );
        assert!(
            tbl.validate_insert(&[CellValue::Str("d".to_string())])
                .is_ok()
        );
    }

    /// Schemas are compiler-generated, so a primary key referencing an
    /// unknown column is a bug and fails table construction.
    #[test]
    #[should_panic(expected = "schema pk spec is correct")]
    fn invalid_primary_key_name_panics_at_construction() {
        let schema = int_schema(&["c0"], Some(&["missing"]));
        Table::new(Path::from("broken"), schema);
    }

    /// The primary key index follows `replace_row_id`, so duplicate
    /// detection still works after a row changes its id.
    #[test]
    fn primary_key_index_follows_replace_row_id() {
        let schema = int_schema(&["c0"], Some(&["c0"]));
        let mut tbl = TestTable::new(Path::from("moving"), schema);

        tbl.insert_row(vec![CellValue::Int(7)], row_id_from(1, 0));
        tbl.insert_row(vec![CellValue::Int(8)], row_id_from(1, 1));
        tbl.replace_row_id(&row_id_from(1, 0), row_id_from(2, 3));

        assert_eq!(
            tbl.validate_insert(&[CellValue::Int(7)]),
            Err(ValidationError::DuplicatePrimaryKey)
        );
        assert_eq!(
            tbl.validate_insert(&[CellValue::Int(8)]),
            Err(ValidationError::DuplicatePrimaryKey)
        );
        assert!(tbl.validate_insert(&[CellValue::Int(9)]).is_ok());
    }

    /// `rewrite_row_cells` replaces cells in place: the row keeps its id and
    /// position, and secondary indexes follow the new values.
    #[test]
    fn rewrite_row_cells_updates_cells_and_indexes() {
        let schema = int_schema(&["c0"], Some(&["c0"]));
        let mut tbl = TestTable::new(Path::from("rewritten"), schema);

        tbl.insert_row(vec![CellValue::Int(7)], row_id_from(1, 0));
        tbl.insert_row(vec![CellValue::Int(8)], row_id_from(1, 1));

        tbl.rewrite_row_cells(&row_id_from(1, 0), vec![PackedCell::Int(9)]);

        // Same row id and position, new cell value.
        assert_eq!(tbl.row_position(row_id_from(1, 0)), Some(0));
        assert_eq!(tbl.cell_at(0, 0), Some(CellValue::Int(9)));

        // The primary key index dropped the old key and holds the new one.
        assert!(tbl.validate_insert(&[CellValue::Int(7)]).is_ok());
        assert_eq!(
            tbl.validate_insert(&[CellValue::Int(9)]),
            Err(ValidationError::DuplicatePrimaryKey)
        );
    }

    #[test]
    #[should_panic(expected = "row id should exist, otherwise rowing is wrong")]
    fn rewrite_row_cells_panics_when_row_is_missing() {
        let schema = int_schema(&["c0"], Some(&["c0"]));
        let mut tbl = TestTable::new(Path::from("rewritten"), schema);
        tbl.rewrite_row_cells(&row_id_from(9, 0), vec![PackedCell::Int(1)]);
    }

    /// Manual benchmark for the primary key duplicate check on insert.
    /// Run with:
    /// cargo test -p coln-store --release pk_insert_benchmark -- --ignored --nocapture
    #[test]
    #[ignore = "manual benchmark"]
    fn pk_insert_benchmark() {
        let schema = ir::Schema {
            entity_variant: ir::EntityVariant::Table,
            columns: vec![ir::ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: Some(vec![Path::from("c0")]),
        };
        let mut tbl = TestTable::new(Path::from("bench"), schema);
        let n = 50_000;
        let start = std::time::Instant::now();
        for i in 0..n {
            let values = vec![CellValue::Int(i)];
            tbl.validate_insert(&values).expect("keys are unique");
            tbl.insert_row(values, test_row_id(i as u32));
        }
        println!("inserted {n} rows with pk check in {:?}", start.elapsed());
    }

    /// Creates a table with an index, and does an indexed lookup as well
    /// a non-indexed lookup and both should work.
    #[test]
    fn table_performs_index_lookup() {
        let schema = int_schema(&["indexed", "plain"], Some(&["indexed"]));
        let mut tbl = TestTable::new(Path::from("lookup"), schema);
        tbl.insert_row(vec![CellValue::Int(7), CellValue::Int(70)], test_row_id(0));
        tbl.insert_row(vec![CellValue::Int(8), CellValue::Int(80)], test_row_id(1));

        let index = tbl.table.primary_index().expect("primary-key index");
        assert_eq!(
            tbl.table
                .index_lookup(index, &[CellValue::Int(7)], &tbl.dict),
            Ok(true)
        );
        assert_eq!(
            tbl.table
                .index_lookup(index, &[CellValue::Int(9)], &tbl.dict),
            Ok(false)
        );
        assert_eq!(
            tbl.table.lookup(
                &[SeekKey {
                    column: 1,
                    value: CellValue::Int(80),
                }],
                &tbl.dict,
            ),
            Ok(true)
        );
        assert_eq!(
            tbl.table.lookup(
                &[SeekKey {
                    column: 1,
                    value: CellValue::Int(90),
                }],
                &tbl.dict,
            ),
            Ok(false)
        );
    }

    /// Creates a table with an index, but passes an index id that does not exist
    /// which is then rejected by the table.
    #[test]
    fn table_index_lookup_non_existing_index() {
        let schema = int_schema(&["indexed"], Some(&["indexed"]));
        let tbl = TestTable::new(Path::from("lookup"), schema);

        assert_eq!(
            tbl.table.index_lookup(99, &[CellValue::Int(7)], &tbl.dict),
            Err(ValidationError::InvalidIndex { index: 99 })
        );
    }

    /// Creates a table with an index, but gives a key that does not match the
    /// index shape, which should be rejected as an error.
    #[test]
    fn table_index_lookup_incorrect_key() {
        let schema = int_schema(&["indexed", "plain"], Some(&["indexed"]));
        let tbl = TestTable::new(Path::from("lookup"), schema);
        let index = tbl.table.primary_index().expect("primary-key index");

        assert_eq!(
            tbl.table
                .index_lookup(index, &[CellValue::Int(7), CellValue::Int(8)], &tbl.dict,),
            Err(ValidationError::InvalidIndexKey {
                index,
                expected: 1,
                got: 2,
            })
        );
    }

    /// Creates a table with index, and requests a index lookup. Also do a
    /// non-index lookup on a different column but looks for the same row(s)
    /// Indexed and non-index lookup should return the same results (positive and
    /// negative)
    #[test]
    fn table_index_non_index_give_same_results() {
        let schema = int_schema(&["indexed", "plain"], Some(&["indexed"]));
        let mut tbl = TestTable::new(Path::from("lookup"), schema);
        for value in [7, 8, 7] {
            let row_id = test_row_id(tbl.row_count() as u32);
            tbl.insert_row(vec![CellValue::Int(value), CellValue::Int(value)], row_id);
        }
        let index = tbl.table.primary_index().expect("primary-key index");

        for value in [7, 9] {
            let indexed = tbl
                .table
                .index_seek(index, &[CellValue::Int(value)], &tbl.dict)
                .expect("valid index lookup")
                .collect::<Vec<_>>();
            let scanned = tbl
                .table
                .seek(
                    &[SeekKey {
                        column: 1,
                        value: CellValue::Int(value),
                    }],
                    &tbl.dict,
                )
                .expect("valid table scan")
                .collect::<Vec<_>>();
            assert_eq!(indexed, scanned);
        }
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
        let mut tbl = TestTable::new(Path::from("debug.table"), schema);

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
