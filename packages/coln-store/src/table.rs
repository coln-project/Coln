// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fmt::Write;

use crate::commit::hash::CommitHash;
use crate::ir;
use crate::ir::{BuiltinTy, ColType, Schema};
use crate::txn::ops::TxnId;

pub type TableOid = u64;

/// The unique id that identifies each row in a table
/// It is managed by the database, read-only for the user
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
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

#[derive(Debug, PartialEq, Eq)]
pub enum ValidationError {
    ColumnCount {
        expected: usize,
        got: usize,
    },
    TypeMismatch {
        column: usize,
        expected: &'static str,
        got: &'static str,
    },
    UnsupportedTuple {
        column: usize,
    },
    InvalidPrimaryKeyName {
        name: ColName,
    },
    DuplicatePrimaryKey,
    /// No table registered for this path (e.g. batch apply).
    UnknownTable {
        path: ir::Path,
    },
    TableMismatch {
        expected: ir::Path,
        actual: ir::Path,
    },
    TxnIdMismatch {
        current: TxnId,
        got: TxnId,
    },
    InvalidRowHandle {
        reason: String,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::ColumnCount { expected, got } => {
                write!(f, "column count mismatch: expected {expected}, got {got}")
            }
            ValidationError::TypeMismatch {
                column,
                expected,
                got,
            } => write!(
                f,
                "type mismatch at column {column}: expected {expected}, got {got}"
            ),
            ValidationError::UnsupportedTuple { column } => {
                write!(f, "tuple columns are not supported yet (column {column})")
            }
            ValidationError::InvalidPrimaryKeyName { name } => {
                write!(f, "primary key references unknown column: {name}")
            }
            ValidationError::DuplicatePrimaryKey => {
                write!(f, "duplicate primary key")
            }
            ValidationError::UnknownTable { path } => {
                write!(f, "unknown table: {path:?}")
            }
            ValidationError::TableMismatch { expected, actual } => {
                write!(
                    f,
                    "table mismatch: expected: {expected:?}, actual: {actual:?}"
                )
            }
            ValidationError::TxnIdMismatch { current, got } => {
                write!(
                    f,
                    "row handle belongs to a different transaction: current {current:?}, got {got:?}"
                )
            }
            ValidationError::InvalidRowHandle { reason } => {
                write!(f, "invalid row handle: {reason}")
            }
        }
    }
}

impl Error for ValidationError {}

/// One cell in columnar storage: entity id, or primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellValue {
    Id(RowId),
    Int(i64),
    Str(String),
}

impl CellValue {
    pub fn kind(&self) -> &'static str {
        match self {
            CellValue::Id(_) => "id",
            CellValue::Int(_) => "int",
            CellValue::Str(_) => "string",
        }
    }

    fn matches_schema(&self, col_type: &ColType, column: usize) -> Result<(), ValidationError> {
        match col_type {
            ColType::RowId { .. } => match self {
                CellValue::Id(_) => Ok(()),
                _ => Err(ValidationError::TypeMismatch {
                    column,
                    expected: "entity id",
                    got: self.kind(),
                }),
            },
            ColType::BuiltinTy { builtin_ty } => match (builtin_ty, self) {
                (BuiltinTy::BuiltinInt, CellValue::Int(_)) => Ok(()),
                (BuiltinTy::BuiltinStr, CellValue::Str(_)) => Ok(()),
                _ => Err(ValidationError::TypeMismatch {
                    column,
                    expected: match *builtin_ty {
                        BuiltinTy::BuiltinInt => "int",
                        BuiltinTy::BuiltinStr => "string",
                    },
                    got: self.kind(),
                }),
            },
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

/// Columnar store: `cols[i]` is all values for schema column `i` (same length per column).
#[derive(Debug, Clone)]
pub struct Table {
    path: ir::Path,
    schema: Schema,
    col_index: HashMap<ColName, usize>,
    pub(crate) row_ids: Vec<RowId>,
    pub(crate) cols: Vec<Vec<CellValue>>,
}

impl Table {
    pub fn new(path: ir::Path, schema: Schema) -> Self {
        let n = schema.columns.len();
        let col_index = schema
            .columns
            .iter()
            .enumerate()
            .map(|(i, column)| (column.path.clone(), i))
            .collect();
        Self {
            path,
            col_index,
            schema,
            row_ids: vec![],
            cols: vec![Vec::new(); n],
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn path(&self) -> &ir::Path {
        &self.path
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
        self.row_ids.get(row_idx).copied()
    }

    /// Cell at `(row_idx, col_idx)` in columnar storage.
    pub fn cell_at(&self, row_idx: usize, col_idx: usize) -> Option<&CellValue> {
        self.cols.get(col_idx).and_then(|col| col.get(row_idx))
    }

    pub(crate) fn row_at(&self, row_idx: usize) -> Option<RowView> {
        let row_id = self.row_id_at(row_idx)?;
        let values = (0..self.schema.columns.len())
            .map(|col_idx| self.cell_at(row_idx, col_idx).cloned())
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
            let row_id = self.row_ids[row_idx];
            let _ = write!(out, "[{row_idx}] row_id={row_id}");
            for col_idx in 0..self.schema.columns.len() {
                let value = &self.cols[col_idx][row_idx];
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
                        .all(|&ci| self.cols[ci][row] == values[ci]);
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

    /// Append a row to columnar storage and assign a new [`RowId`].
    ///
    /// Does **not** validate. Used internally when the caller has already checked the row
    /// (e.g. batch validation); otherwise use [`try_append_row`].
    pub(crate) fn append_row(&mut self, values: Vec<CellValue>, row_id: RowId) {
        debug_assert_eq!(values.len(), self.schema.columns.len());

        self.row_ids.push(row_id);
        for (i, v) in values.into_iter().enumerate() {
            self.cols[i].push(v);
        }
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
        tbl.append_row(vec![], r0);
        assert_eq!(tbl.row_count(), 1);
        assert_eq!(tbl.row_id_at(0), Some(r0));

        let r1 = test_row_id(1);
        tbl.append_row(vec![], r1);
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

        tbl.append_row(vec![CellValue::Int(0)], test_row_id(0));
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
        tbl.append_row(
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
        assert_eq!(tbl.cell_at(0, 0), Some(&CellValue::Int(7)));
        assert_eq!(tbl.cell_at(0, 1), Some(&CellValue::Str("x".to_string())));
        assert_eq!(tbl.row_at(1), None);
        assert_eq!(tbl.row_id_at(1), None);
        assert_eq!(tbl.cell_at(0, 2), None);
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

        tbl.append_row(
            vec![CellValue::Int(7), CellValue::Str("x".to_string())],
            test_row_id(0),
        );
        tbl.append_row(
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
