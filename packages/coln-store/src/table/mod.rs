// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub mod cell;
mod col;
pub mod handle;
pub(crate) mod index;
pub mod sorted;
mod undo;

pub use cell::{CellKind, WireRowId, WireValue};
pub use handle::TableHandle;

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use coln_query::api::deltas::{ScalarTypedValue, TableDelta, ZRow};

use crate::ir;
use crate::ir::Schema;
use crate::pack::{IdPacker, PackedOp, PackedRowId, PackedRowView, PackedTuple, PackedValue};
use crate::rollback::Rollback;
use crate::rowing::Rowing;
use crate::table::col::{Column, IdColumn};
use crate::table::index::{IndexMeta, TableIndex};
use crate::table::undo::UndoOp;
use crate::txn::TxnId;

pub type TableOid = usize;

/// Borrowed identity and schema for a registered table.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TableMeta<'a> {
    pub path: &'a ir::Path,
    pub oid: TableOid,
    pub schema: &'a Schema,
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
    /// No table registered for this store-local oid.
    #[error("unknown table oid: {oid}")]
    UnknownTableOid { oid: TableOid },
    #[error("table mismatch: expected: {expected:?}, actual: {actual:?}")]
    TableMismatch {
        expected: ir::Path,
        actual: ir::Path,
    },
    #[error("row handle belongs to a different transaction: current {current:?}, got {got:?}")]
    TxnIdMismatch { current: TxnId, got: TxnId },
    #[error("invalid row handle: {reason}")]
    InvalidTxnLiveRowId { reason: String },
    #[error("invalid index key for index: expected <= {expected} values, got {got}")]
    InvalidIndexKey { expected: usize, got: usize },
    #[error("lookup column {column} is outside the table's {column_count} columns")]
    InvalidLookupColumn { column: usize, column_count: usize },
    #[error("passed in row id is not valid {wire_id}")]
    InvalidRowId { wire_id: WireRowId },
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
    // metadata
    oid: TableOid,
    path: ir::Path,
    schema: Schema,

    // actual data
    row_ids: IdColumn,
    cols: Vec<Column>,

    // the first n
    pk: Option<usize>,
    // A single index on all columns
    index: TableIndex,

    // buffering rollback
    pending_updates: Vec<PackedOp>,
    undo_log: Option<Vec<UndoOp>>,

    // structural identification
    structural: bool,
    // Map each rowid to the rows that refer to them.
    rebuild_index: HashMap<PackedRowId, Vec<PackedRowId>>,
}

impl Table {
    // Basic accessors

    pub(crate) fn new(path: ir::Path, oid: TableOid, schema: Schema) -> Self {
        let cols = schema
            .columns
            .iter()
            .map(|column| Column::new(CellKind::from(&column.col_type)))
            .collect();

        let pk = match &schema.primary_key {
            None => None,
            Some(pk) if pk.is_empty() => Some(0),
            Some(pk) => Some(pk.len()),
        };

        let except_rowid: Vec<usize> = (0..schema.columns.len()).collect();
        let index = TableIndex::new(&except_rowid, &schema);
        // TODO if structural identity is enabled, then change this.
        let structural = false;

        Self {
            oid,
            path,
            schema,
            structural,
            row_ids: IdColumn::new(),
            cols,
            index,
            pk,
            pending_updates: Vec::new(),
            undo_log: None,
            rebuild_index: HashMap::new(),
        }
    }

    pub(crate) fn schema(&self) -> &Schema {
        &self.schema
    }

    pub(crate) fn path(&self) -> &ir::Path {
        &self.path
    }

    pub(crate) fn oid(&self) -> TableOid {
        self.oid
    }

    pub(crate) fn row_count(&self) -> usize {
        // We need to return row_ids here, because cols might be empty for tables with only ids but nothing else
        self.row_ids.len()
    }

    pub(crate) fn index_meta(&self) -> IndexMeta<'_> {
        IndexMeta {
            key_cols: self.index.key_cols(),
        }
    }

    // Returns the first n columns that are required to be unique in this table
    pub(crate) fn unique_columns(&self) -> Option<usize> {
        self.pk
    }

    pub(crate) fn table_variant(&self) -> &ir::EntityVariant {
        &self.schema.entity_variant
    }
}

impl Table {
    // Accessing a row or a cell

    /// O(N * log S) as first find out the index from the row_id, and then do a
    /// lookup on each column
    pub(crate) fn row_by_id(&self, row_id: PackedRowId) -> Option<PackedTuple> {
        let row_idx = self.row_ids.position(row_id).ok()?;
        (0..self.schema.columns.len())
            .map(|col_idx| {
                self.cols
                    .get(col_idx)
                    .and_then(|col| col.get_packed(row_idx))
            })
            .collect()
    }

    pub(crate) fn row_by_idx(&self, row_idx: usize) -> Option<PackedRowView> {
        let row_id = self.row_id_by_idx(row_idx)?;
        let values = (0..self.schema.columns.len())
            .map(|col_idx| self.cell_by_idx(row_idx, col_idx))
            .collect::<Option<PackedTuple>>()?;

        Some(PackedRowView { row_id, values })
    }

    /// Row id at a given physical row index.
    pub(crate) fn row_id_by_idx(&self, row_idx: usize) -> Option<PackedRowId> {
        self.row_ids.get(row_idx)
    }

    /// Cell at `(row_idx, col_idx)` in columnar storage.
    /// O(1) to locate the column, roughly O(log S) to find by index in a slab.
    pub(crate) fn cell_by_idx(&self, row_idx: usize, col_idx: usize) -> Option<PackedValue> {
        self.cols
            .get(col_idx)
            .and_then(|col| col.get_packed(row_idx))
    }

    /// Find the index of the row given a `row_id`. Internal API only.
    fn packed_rowid_idx(&self, row_id: PackedRowId) -> Option<usize> {
        self.row_ids.position(row_id).ok()
    }

    pub(crate) fn scan(&self) -> impl Iterator<Item = PackedRowView> {
        (0..self.row_count()).filter_map(move |row_idx| self.row_by_idx(row_idx))
    }

    pub(crate) fn index_seek<'s>(
        &'s self,
        key: &PackedTuple,
    ) -> Result<impl Iterator<Item = PackedRowId> + use<'s>, ValidationError> {
        if key.len() > self.index.key_cols().len() {
            return Err(ValidationError::InvalidIndexKey {
                expected: self.index.key_cols().len(),
                got: key.len(),
            });
        }
        Ok(self.index.get(key))
    }
}

impl Table {
    // Basic validation against schema

    /// Checks that a row has the right number of values for this table. This is
    /// a preliminary check that is done as soon as an operation is added. More
    /// complex check is in validate_insert and deferred at commit time
    pub(crate) fn validate_column_count(&self, got: usize) -> Result<(), ValidationError> {
        let expected = self.schema.columns.len();
        if got != expected {
            return Err(ValidationError::ColumnCount { expected, got });
        }
        Ok(())
    }

    /// Checks schema and primary-key constraints against rows already stored.
    pub(crate) fn validate_insert(
        &self,
        values: &[WireValue],
        dict: &IdPacker,
    ) -> Result<(), ValidationError> {
        // duplicated as txn::add(), but this is cheap enough we can afford to
        // do it here just in case.
        self.validate_column_count(values.len())?;

        for (i, (col_entry, value)) in self.schema.columns.iter().zip(values.iter()).enumerate() {
            value.matches_schema(&col_entry.col_type, i)?;
        }

        if let Some(cols) = &self.pk {
            let Some(key) = (0..*cols)
                .map(|ci| dict.try_pack_value(&values[ci]))
                .collect::<Option<PackedTuple>>()
            else {
                // If we cannot pack, then the primary key should be absent, so no need to check
                return Ok(());
            };
            if self.index.contains_key(&key) {
                return Err(ValidationError::DuplicatePrimaryKey);
            }
        };
        Ok(())
    }

    /// Values at primary-key columns for this row.
    /// A primary key definition would occur in tables that do not end up in Query
    /// An empty primary key means the table would have at most one row.
    pub(crate) fn primary_key_values(&self, values: &[WireValue]) -> Option<Vec<WireValue>> {
        self.schema.primary_key.as_ref().and_then(|pk| {
            if pk.is_empty() {
                Some(Vec::new())
            } else {
                pk.iter()
                    .map(|i| Some(values[*i as usize].clone()))
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

    fn commit(&mut self, _snapshot: Self::Snapshot) {
        assert!(
            self.pending_updates.is_empty(),
            "cannot commit a snapshot with staged updates"
        );
        self.undo_log.take().expect("table has no active snapshot");
    }

    fn rollback_to(&mut self, _snapshot: Self::Snapshot) {
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
    pub(crate) fn apply_staged_ops(
        &mut self,
        rowing: &mut Rowing,
    ) -> Result<TableDelta, ValidationError> {
        let ops = std::mem::take(&mut self.pending_updates);
        let delta = self.table_delta_from_ops(ops.clone());
        for op in ops {
            let undo_op = self.apply_op(op, rowing)?;
            if let Some(undo_log) = &mut self.undo_log {
                undo_log.push(undo_op);
            }
        }
        Ok(delta)
    }

    fn table_delta_from_ops(&self, ops: impl IntoIterator<Item = PackedOp>) -> TableDelta {
        let zrows: Vec<ZRow> = ops
            .into_iter()
            .map(|op| match op {
                PackedOp::Add { row_id, values } => {
                    let tuple = PackedRowView { row_id, values }.into();
                    ZRow::new(1, tuple).unwrap()
                }
                PackedOp::Delete { row_id } => {
                    let values = self
                        .row_by_id(row_id)
                        .expect("element to delete should exist");
                    let tuple = PackedRowView { row_id, values }.into();
                    ZRow::new(-1, tuple).unwrap()
                }
            })
            .collect();

        TableDelta::new(self.path().to_string(), zrows)
    }

    // This conversion needs table schema, therefore cannot be done with From trait
    pub(crate) fn ops_from_table_delta<F>(
        &self,
        td: TableDelta,
        mut id_allocate: F,
    ) -> Vec<PackedOp>
    where
        F: FnMut() -> PackedRowId,
    {
        td.into_iter()
            .map(|zrow| {
                let row_id = id_allocate();
                if zrow.zweight() > 0 {
                    let mut val_iter = zrow.into_row().data.into_iter();
                    let mut packed_val = Vec::new();

                    for col in &self.schema().columns {
                        match col.col_type {
                            ir::ColType::RowId { .. } => {
                                let ScalarTypedValue::Uint(commit_idx) =
                                    val_iter.next().expect("coln-query returns valid data")
                                else {
                                    panic!("invalid data from coln-query");
                                };
                                let ScalarTypedValue::Uint(counter) =
                                    val_iter.next().expect("coln-query returns valid data")
                                else {
                                    panic!("invalid data from coln-query");
                                };
                                packed_val.push(PackedValue::Id(PackedRowId {
                                    commit_idx: commit_idx as u32,
                                    counter: counter as u32,
                                }));
                            }
                            ir::ColType::BuiltinTy {
                                builtin_ty: ir::BuiltinTy::BuiltinInt,
                            } => {
                                let ScalarTypedValue::String(s) = val_iter.next().unwrap() else {
                                    panic!("invalid data from coln-query");
                                };
                                packed_val.push(PackedValue::Str(s));
                            }
                            ir::ColType::BuiltinTy {
                                builtin_ty: ir::BuiltinTy::BuiltinStr,
                            } => {
                                let ScalarTypedValue::Iint(i) = val_iter.next().unwrap() else {
                                    panic!("invalid data from coln-query");
                                };
                                packed_val.push(PackedValue::Int(i as i32));
                            }
                        }
                    }

                    PackedOp::Add {
                        row_id,
                        values: packed_val.into(),
                    }
                } else if zrow.zweight() < 0 {
                    // TODO don't know how to remove yet
                    todo!()
                } else {
                    unreachable!("zero zweight impossible")
                }
            })
            .collect()
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

    #[expect(dead_code)]
    // Rebuild using rebuild_index
    fn rebuild_incremental(&mut self, rowing: &Rowing, id_packer: &IdPacker) {
        for old in rowing.displaced() {
            // A row whose own id was displaced is rebuilt here, including any
            // stale ids in its cells. Referring-row handling below skips it.
            if let Some(old_cells) = self.row_by_id(old) {
                let new_rid = rowing.canonical_id(&old, id_packer);
                let new_cells = Self::canonicalise_cells(&old_cells, rowing, id_packer);

                // Displacement onto an id that the table already has. Do not stage
                // an addition in this case.
                let collapses = self.row_ids.position(new_rid).is_ok();
                debug_assert!(
                    !collapses
                        || self.row_by_id(new_rid).is_some_and(|stored| {
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
                    .row_by_id(row_id)
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
            let old_cells: PackedTuple = self
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
                    || self.row_by_id(new_row_id).is_some_and(|stored| {
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
        values: &PackedTuple,
        rowing: &Rowing,
        id_packer: &IdPacker,
    ) -> PackedTuple {
        values
            .iter()
            .map(|cell| match cell {
                PackedValue::Id(id) => PackedValue::Id(rowing.canonical_id(id, id_packer)),
                other => other.clone(),
            })
            .collect()
    }

    /// ids referred by this row.
    #[expect(dead_code)]
    fn referenced_ids(values: &PackedTuple) -> impl Iterator<Item = PackedRowId> {
        values
            .iter()
            .enumerate()
            .filter_map(|(i, cell)| match cell {
                PackedValue::Id(id) if !values[..i].contains(cell) => Some(*id),
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
        values: PackedTuple,
        row_id: PackedRowId,
        rowing: &mut Rowing,
    ) -> Result<(), ValidationError> {
        // Checked before anything is recorded, so a rejected row leaves behind
        // neither an index entry nor a staged union.
        if let Some(unique_cols) = self.pk
            && self.index.contains_key(&values[..unique_cols])
        {
            return Err(ValidationError::DuplicatePrimaryKey);
        };

        // A structurally identical row is stored anyway: rowing unions the two
        // ids and a later rebuild pass collapses them.
        if self.structural
            && let Some(old) = self
                .index_seek(&values)
                .expect("valid structural index and key structure")
                .next()
        {
            rowing.stage_union(self.oid, old, row_id);
        }

        // Checks for existing row ids
        if let Ok(_pos) = self.row_ids.position(row_id) {
            panic!("should never insert a rowid that already exists");
        }

        self.insert_packed(values, row_id);
        Ok(())
    }

    /// Place a row in columnar storage and every index, with no validation
    fn insert_packed(&mut self, values: PackedTuple, row_id: PackedRowId) {
        debug_assert_eq!(values.len(), self.schema.columns.len());

        self.index.insert(values.clone(), row_id);

        // TODO this should only be maintained when a table needs rebuild, i.e. a structural table.
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
    fn remove_packed(&mut self, row_id: PackedRowId) -> PackedTuple {
        let row_idx = self
            .row_ids
            .position(row_id)
            .expect("removal target should be present");
        let values = self
            .row_by_id(row_id)
            .expect("removal target should have a complete row");

        self.index.remove(&values, row_id);

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
}

impl Table {
    // For debugging for testing

    // TODO remove this when we have schema level structural identity
    #[cfg(test)]
    pub(crate) fn set_structural_index_for_test(&mut self, enabled: bool) {
        // `Table::new` always appends the all-columns index last; enable
        // structural identity by pointing at that slot.
        self.structural = enabled;
    }

    /// Dump table contents row by row for debugging.
    pub(crate) fn dump(&self, dict: &IdPacker) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{} {} (rows: {}, cols: {})",
            self.schema.entity_variant,
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
