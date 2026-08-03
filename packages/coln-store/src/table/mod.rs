// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod cell;
mod col;
pub(crate) mod index;
pub mod table_ref;
mod undo;

pub use cell::{CellKind, CellValue, RowId};
pub use table_ref::TableRef;

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::id_packer::IdPacker;
use crate::ir;
use crate::ir::Schema;
use crate::rollback::Rollback;
use crate::rowing::Rowing;
use crate::table::index::{IndexId, IndexMeta, TableIndex};
use crate::table::undo::UndoOp;
use crate::txn::TxnId;

pub(crate) use self::cell::{PackedCell, PackedRowId};
use self::col::{Column, IdColumn};

pub type TableOid = usize;

/// Packed representation of an operation staged for a table.
#[derive(Debug)]
pub(crate) enum PackedOp {
    Add {
        row_id: PackedRowId,
        values: Vec<PackedCell>,
    },
    Delete {
        row_id: PackedRowId,
    },
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
/// in the store-wide [`IdPacker`] and rows refer to it by a `u32` index
/// (see [`PackedRowId`]). The dictionary is append-only, so packed ids stay
/// valid for the lifetime of the store. The [`Store`](crate::store::Store)
/// owns the dictionary and packs mutations before staging them in a table;
/// [`TableRef`] bundles the table and dictionary for decoded reads.
#[derive(Debug)]
pub struct Table {
    oid: TableOid,
    path: ir::Path,
    schema: Schema,
    col_name_map: HashMap<ColName, usize>,
    /// Structural (all-columns) index used for hashcons lookup, when enabled.
    hashcons_index: Option<IndexId>,
    indexes: Vec<TableIndex>,
    row_ids: IdColumn,
    cols: Vec<Column>,
    pk: PkConstraint,
    pending_updates: Vec<PackedOp>,
    undo_log: Option<Vec<UndoOp>>,

    // Map each rowid to the rows that refer to them.
    rebuild_index: HashMap<PackedRowId, Vec<PackedRowId>>,
}

impl Table {
    // Basic accessors

    pub fn new(path: ir::Path, oid: TableOid, schema: Schema) -> Self {
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

        // TODO if hashcons, then create another index.
        let hashcons_cols: Vec<usize> = (0..schema.columns.len()).collect();
        indexes.push(TableIndex::new(&hashcons_cols, &schema));
        // let hashcons_index = Some(indexes.len() - 1);
        let hashcons_index = None;

        Self {
            oid,
            path,
            col_name_map,
            schema,
            hashcons_index,
            row_ids: IdColumn::new(),
            cols,
            indexes,
            pk,
            pending_updates: Vec::new(),
            undo_log: None,
            rebuild_index: HashMap::new(),
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn path(&self) -> &ir::Path {
        &self.path
    }

    pub fn row_count(&self) -> usize {
        // We need to return row_ids here, because cols might be empty for tables with only ids but nothing else
        self.row_ids.len()
    }

    /// Row id at a given physical row index.
    pub(crate) fn row_id_at(&self, row_idx: usize, packer: &IdPacker) -> Option<RowId> {
        self.row_ids
            .get(row_idx)
            .map(|packed| packer.unpack_row_id(packed))
    }

    /// Cell at `(row_idx, col_idx)` in columnar storage.
    pub(crate) fn cell_at(
        &self,
        row_idx: usize,
        col_idx: usize,
        packer: &IdPacker,
    ) -> Option<CellValue> {
        self.cols
            .get(col_idx)
            .and_then(|col| col.get(row_idx, packer))
    }

    /// Find the index of the row given a `row_id`. Internal API only.
    fn row_idx(&self, row_id: PackedRowId) -> Option<usize> {
        self.row_ids.position(row_id).ok()
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
}

#[derive(Debug, PartialEq, Eq)]
pub struct SeekKey {
    pub(crate) column: usize,
    pub(crate) value: CellValue,
}

impl Table {
    // public facing read and indexing APIs

    pub(crate) fn row_at(&self, row_idx: usize, id_packer: &IdPacker) -> Option<RowView> {
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

    pub(crate) fn table_scan(&self, id_packer: &IdPacker) -> impl Iterator<Item = RowView> {
        (0..self.row_count()).filter_map(move |row_idx| self.row_at(row_idx, id_packer))
    }

    pub(crate) fn seek(
        &self,
        key: &[SeekKey],
        id_packer: &IdPacker,
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

        Ok((0..self.row_count())
            .filter(move |&row_idx| {
                key.iter().all(|part| {
                    self.cell_at(row_idx, part.column, id_packer).as_ref() == Some(&part.value)
                })
            })
            .map(move |row_idx| {
                self.row_id_at(row_idx, id_packer)
                    .expect("row index came from the table's row count")
            }))
    }

    pub(crate) fn index_seek(
        &self,
        index: IndexId,
        key: &[CellValue],
        id_packer: &IdPacker,
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
            .map(|value| id_packer.try_pack_cell(value))
            .collect::<Option<Vec<_>>>()
            .map(|key| {
                table_index
                    .get(&key)
                    .map(|row_id| id_packer.unpack_row_id(row_id))
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
        id_packer: &IdPacker,
    ) -> Result<bool, ValidationError> {
        Ok(self.seek(key, id_packer)?.next().is_some())
    }

    pub(crate) fn index_lookup(
        &self,
        index: IndexId,
        key: &[CellValue],
        id_packer: &IdPacker,
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
        dict: &IdPacker,
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
                    .map(|&ci| dict.try_pack_cell(&values[ci]))
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

#[must_use]
pub(crate) struct TableSnapshot;

impl Rollback for Table {
    type Snapshot = TableSnapshot;

    // Take a snapshot of the table, which should then start recording all of the
    // operations that are recorded in the table.
    // Returns a handle to the user so they can roll back the changes applied
    fn snapshot(&mut self) -> Self::Snapshot {
        assert!(
            self.undo_log.is_none(),
            "nested table snapshots are not supported"
        );
        assert!(
            self.pending_updates.is_empty(),
            "cannot snapshot a table with staged updates"
        );

        self.undo_log = Some(Vec::new());
        TableSnapshot
    }

    fn commit_snapshot(&mut self, _snapshot: Self::Snapshot) {
        assert!(
            self.pending_updates.is_empty(),
            "cannot commit a snapshot with staged updates"
        );
        self.undo_log.take().expect("table has no active snapshot");
    }

    fn rollback(&mut self, _snapshot: Self::Snapshot) {
        self.pending_updates.clear();

        let undo_ops = self.undo_log.take().expect("table has no active snapshot");
        for undo_op in undo_ops.into_iter().rev() {
            self.apply_undo(undo_op);
        }
    }
}

impl Table {
    /// Stage an already packed operation without changing the materialised table.
    pub(crate) fn stage_update(&mut self, op: PackedOp) {
        self.pending_updates.push(op);
    }

    // Apply the staged updates to the table. Rollback support will record
    // inverse operations separately before these operations are consumed.
    pub(crate) fn apply_staged_ops(&mut self, rowing: &mut Rowing) -> Result<(), ValidationError> {
        let ops = std::mem::take(&mut self.pending_updates);
        for op in ops {
            let undo_op = self.apply_op(op, rowing)?;
            if let Some(undo_log) = &mut self.undo_log {
                undo_log.push(undo_op);
            }
        }
        Ok(())
    }

    fn apply_op(&mut self, op: PackedOp, rowing: &mut Rowing) -> Result<UndoOp, ValidationError> {
        match op {
            PackedOp::Add { row_id, values } => {
                self.insert_row(values, row_id, rowing)?;
                Ok(UndoOp::UndoAdd { row_id })
            }
            PackedOp::Delete { row_id } => {
                let values = self.remove_packed(row_id);
                Ok(UndoOp::UndoDelete { row_id, values })
            }
        }
    }

    fn apply_undo(&mut self, undo_op: UndoOp) {
        match undo_op {
            UndoOp::UndoAdd { row_id } => {
                self.remove_packed(row_id);
            }
            // inserting when undo cannot fail, and does not need rowing, primary key check, etc.
            UndoOp::UndoDelete { row_id, values } => self.insert_packed(values, row_id),
        }
    }
}

impl Table {
    // Rebuilding

    #[allow(unused)]
    // Rebuild using rebuild_index
    fn rebuild_incremental(&mut self, rowing: &Rowing, id_packer: &IdPacker) {
        for old in rowing.displaced() {
            // A row whose own id was displaced is rebuilt here, including any
            // stale ids in its cells. Referring-row handling below skips it.
            if let Some(old_cells) = self.packed_row_at(old) {
                let new_rid = rowing.canonical_id(&old, id_packer);
                let new_cells = Self::canonicalise_cells(&old_cells, rowing, id_packer);

                // Displacement onto an id that the table already has. Do not stage
                // an addition in this case.
                let collapses = self.row_ids.position(new_rid).is_ok();
                debug_assert!(
                    !collapses
                        || self.packed_row_at(new_rid).is_some_and(|stored| {
                            Self::canonicalise_cells(&stored, rowing, id_packer) == new_cells
                        }),
                    "collapsing {old:?} onto {new_rid:?} would discard differing cells"
                );

                self.stage_update(PackedOp::Delete { row_id: old });
                if !collapses {
                    self.stage_update(PackedOp::Add {
                        row_id: new_rid,
                        values: new_cells,
                    });
                }
            }

            // Clone the small referring-row list so staging can mutably borrow
            // the table. Rows with stale identities are owned by the branch
            // above and must not be staged a second time here.
            let referring = self.rebuild_index.get(&old).cloned().unwrap_or_default();
            for row_id in referring {
                if rowing.canonical_id(&row_id, id_packer) != row_id {
                    continue;
                }
                let old_cells = self
                    .packed_row_at(row_id)
                    .expect("a referring row is present in the table");
                let new_cells = Self::canonicalise_cells(&old_cells, rowing, id_packer);
                if new_cells == old_cells {
                    continue;
                }

                self.stage_update(PackedOp::Delete { row_id });
                self.stage_update(PackedOp::Add {
                    row_id,
                    values: new_cells,
                });
            }
        }
    }

    fn rebuild_full(&mut self, rowing: &Rowing, id_packer: &IdPacker) {
        let stale: HashSet<PackedRowId> = rowing.displaced().collect();

        for row_idx in 0..self.row_count() {
            let old_row_id = self.row_ids.at(row_idx);
            let row_stale = stale.contains(&old_row_id);
            let cells_stale = self.cols.iter().any(|column| match column {
                Column::Id(ids) => stale.contains(&ids.at(row_idx)),
                Column::Int(_) | Column::Str(_) => false,
            });
            if !row_stale && !cells_stale {
                continue;
            }

            let new_row_id = rowing.canonical_id(&old_row_id, id_packer);
            let old_cells: Vec<PackedCell> = self
                .cols
                .iter()
                .map(|column| {
                    column
                        .get_packed(row_idx)
                        .expect("all table columns have the same row count")
                })
                .collect();
            let new_cells = Self::canonicalise_cells(&old_cells, rowing, id_packer);
            let collapses = row_stale && self.row_ids.position(new_row_id).is_ok();

            debug_assert!(
                !collapses
                    || self.packed_row_at(new_row_id).is_some_and(|stored| {
                        Self::canonicalise_cells(&stored, rowing, id_packer) == new_cells
                    }),
                "collapsing {old_row_id:?} onto {new_row_id:?} would discard differing cells"
            );

            self.stage_update(PackedOp::Delete { row_id: old_row_id });
            if !collapses {
                self.stage_update(PackedOp::Add {
                    row_id: new_row_id,
                    values: new_cells,
                });
            }
        }
    }

    pub(crate) fn rebuild(&mut self, rowing: &Rowing, id_packer: &IdPacker) {
        self.rebuild_full(rowing, id_packer)
    }

    /// Rewrite every id cell to its canonical id, leaving other cells alone.
    fn canonicalise_cells(
        values: &[PackedCell],
        rowing: &Rowing,
        id_packer: &IdPacker,
    ) -> Vec<PackedCell> {
        values
            .iter()
            .map(|cell| match cell {
                PackedCell::Id(id) => PackedCell::Id(rowing.canonical_id(id, id_packer)),
                other => other.clone(),
            })
            .collect()
    }

    /// ids referred by this row.
    fn referenced_ids(values: &[PackedCell]) -> impl Iterator<Item = PackedRowId> {
        values
            .iter()
            .enumerate()
            .filter_map(|(i, cell)| match cell {
                PackedCell::Id(id) if !values[..i].contains(cell) => Some(*id),
                _ => None,
            })
    }
}

impl Table {
    // Actually modifying the table content

    /// Insert a row into columnar storage at its sorted position.
    ///
    /// Only does primary key check, but no other validation.
    pub(super) fn insert_row(
        &mut self,
        values: Vec<PackedCell>,
        row_id: PackedRowId,
        rowing: &mut Rowing,
    ) -> Result<(), ValidationError> {
        // Checked before anything is recorded, so a rejected row leaves behind
        // neither an index entry nor a staged union.
        match self.pk {
            PkConstraint::None => {}
            PkConstraint::Singleton => {
                if self.row_count() >= 1 {
                    return Err(ValidationError::DuplicatePrimaryKey);
                }
            }
            PkConstraint::Indexed(pk_index) => {
                let key = Self::project_index_key(&self.indexes[pk_index], &values);
                if self.indexes[pk_index].contains_key(&key) {
                    return Err(ValidationError::DuplicatePrimaryKey);
                }
            }
        }

        // A structurally identical row is stored anyway: rowing unions the two
        // ids and a later rebuild pass collapses them.
        if let Some(index) = self.hashcons_index {
            let key = Self::project_index_key(&self.indexes[index], &values);
            if let Some(old) = self
                .index_seek_packed(index, &key)
                .expect("valid hashcons index and key structure")
                .next()
            {
                rowing.stage_union(self.oid, old, row_id);
            }
        }

        // Checks for existing row ids
        if let Ok(_pos) = self.row_ids.position(row_id) {
            panic!("should never insert a rowid that already exists");
        }

        self.insert_packed(values, row_id);
        Ok(())
    }

    /// Place a row in columnar storage and every index, with no validation
    fn insert_packed(&mut self, values: Vec<PackedCell>, row_id: PackedRowId) {
        debug_assert_eq!(values.len(), self.schema.columns.len());

        for index in &mut self.indexes {
            let key = Self::project_index_key(index, &values);
            index.insert(key, row_id);
        }
        // TODO this should only be maintained when a table needs rebuild, i.e. a hahscons table.
        // for child in Self::referenced_ids(&values) {
        //     self.rebuild_index.entry(child).or_default().push(row_id);
        // }

        let pos: usize = match self.row_ids.position(row_id) {
            Ok(pos) | Err(pos) => pos,
        };
        self.row_ids.insert(pos, row_id);
        for (i, v) in values.into_iter().enumerate() {
            self.cols[i].insert(pos, v);
        }
    }

    /// Take a row out of columnar storage and every index, returning its cells
    fn remove_packed(&mut self, row_id: PackedRowId) -> Vec<PackedCell> {
        let row_idx = self
            .row_ids
            .position(row_id)
            .expect("removal target should be present");
        let values = self
            .packed_row_at(row_id)
            .expect("removal target should have a complete row");

        for index in &mut self.indexes {
            let key = Self::project_index_key(index, &values);
            index.remove_rowid(&key, row_id);
        }
        // for child in Self::referenced_ids(&values) {
        //     let referring = self
        //         .rebuild_index
        //         .get_mut(&child)
        //         .expect("a stored row is recorded against every id it refers to");
        //     let pos = referring
        //         .iter()
        //         .position(|rid| *rid == row_id)
        //         .expect("a stored row is recorded against every id it refers to");
        //     referring.swap_remove(pos);
        //     if referring.is_empty() {
        //         self.rebuild_index.remove(&child);
        //     }
        // }
        self.row_ids.remove(row_idx);
        for column in &mut self.cols {
            column.remove(row_idx);
        }
        values
    }

    fn project_index_key(index: &TableIndex, values: &[PackedCell]) -> Vec<PackedCell> {
        index
            .key_cols()
            .iter()
            .map(|&col_idx| values[col_idx].clone())
            .collect()
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
    pub(crate) fn dump(&self, dict: &IdPacker) -> String {
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
            let row_id = dict.unpack_row_id(self.row_ids.at(row_idx));
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
mod tests;
