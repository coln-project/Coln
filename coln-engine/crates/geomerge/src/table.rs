use std::error::Error;
use std::fmt;
use std::fmt::Write;

use crate::ir;
use crate::persist::ptbl::TableEntry;
use crate::{
    ir::{ColType, PrimType, Schema},
    ops::Op,
};

pub type TableOid = u64;

/// The unique id that identifies each row in a table
/// It is managed by the database, read-only for the user
pub type RowId = u64;

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
    DuplicatePrimaryKey,
    /// No table registered for this path (e.g. batch apply).
    UnknownTable {
        path: ir::Path,
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
            ValidationError::DuplicatePrimaryKey => {
                write!(f, "duplicate primary key")
            }
            ValidationError::UnknownTable { path } => {
                write!(f, "unknown table: {path:?}")
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
            ColType::EntityType { .. } => match self {
                CellValue::Id(_) => Ok(()),
                _ => Err(ValidationError::TypeMismatch {
                    column,
                    expected: "entity id",
                    got: self.kind(),
                }),
            },
            ColType::PrimType { prim } => match (prim, self) {
                (PrimType::PrimInt, CellValue::Int(_)) => Ok(()),
                (PrimType::PrimString, CellValue::Str(_)) => Ok(()),
                _ => Err(ValidationError::TypeMismatch {
                    column,
                    expected: match prim {
                        PrimType::PrimInt => "int",
                        PrimType::PrimString => "string",
                    },
                    got: self.kind(),
                }),
            },
            ColType::Tuple { .. } => Err(ValidationError::UnsupportedTuple { column }),
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

/// Columnar store: `cols[i]` is all values for schema column `i` (same length per column).
#[derive(Debug, Clone)]
pub struct Table {
    path: ir::Path,
    schema: Schema,
    pub(crate) next_rowid: u64,
    pub(crate) row_ids: Vec<RowId>,
    pub(crate) cols: Vec<Vec<CellValue>>,
}

impl Table {
    pub fn new(path: ir::Path, schema: Schema) -> Self {
        let n = schema.columns.len();
        Self {
            path,
            schema,
            next_rowid: 0,
            row_ids: vec![],
            cols: vec![Vec::new(); n],
        }
    }

    pub(crate) fn new_from_persist(
        entry: &TableEntry,
        row_ids: Vec<RowId>,
        cols: Vec<Vec<CellValue>>,
    ) -> Self {
        Self {
            path: ir::Path::from(entry.path.as_str()),
            schema: entry.schema.clone(),
            next_rowid: entry.next_rowid,
            row_ids,
            cols,
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
    pub fn row_id_at(&self, row_idx: usize) -> Option<RowId> {
        self.row_ids.get(row_idx).copied()
    }

    /// Cell at `(row_idx, col_idx)` in columnar storage.
    pub fn cell_at(&self, row_idx: usize, col_idx: usize) -> Option<&CellValue> {
        self.cols.get(col_idx).and_then(|col| col.get(row_idx))
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

    /// Checks that the values to be inserted matches the schema definition
    pub fn validate(&self, values: &[CellValue]) -> Result<(), ValidationError> {
        let expected = self.schema.columns.len();
        if values.len() != expected {
            return Err(ValidationError::ColumnCount {
                expected,
                got: values.len(),
            });
        }
        for (i, (col_type, value)) in self.schema.columns.iter().zip(values.iter()).enumerate() {
            value.matches_schema(col_type, i)?;
        }
        Ok(())
    }

    /// Values at primary-key columns for this row.
    /// A primary key definition would occur in tables that do not end up in Query
    /// An empty primary key means the table would have at most one row.
    pub fn primary_key_values(&self, values: &[CellValue]) -> Option<Vec<CellValue>> {
        self.schema.primary_key.as_ref().map(|pk| {
            if pk.is_empty() {
                Vec::new()
            } else {
                pk.iter().map(|&i| values[i as usize].clone()).collect()
            }
        })
    }

    /// Schema and primary-key check against rows already stored.
    pub fn validate_new_row(&self, values: &[CellValue]) -> Result<(), ValidationError> {
        self.validate(values)?;

        if let Some(pk) = &self.schema.primary_key {
            if !pk.is_empty() {
                let n = self.row_count();
                for row in 0..n {
                    let same = pk.iter().all(|&col_idx| {
                        let ci = col_idx as usize;
                        self.cols[ci][row] == values[ci]
                    });
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

    /// Validates `values` ([`validate_new_row`]), then appends and returns the new [`RowId`].
    pub fn append_row_validated(
        &mut self,
        values: Vec<CellValue>,
    ) -> Result<RowId, ValidationError> {
        self.validate_new_row(&values)?;
        Ok(self.append_row(values))
    }

    /// Append a row to columnar storage and assign a new [`RowId`].
    ///
    /// Does **not** validate. Used internally when the caller has already checked the row
    /// (e.g. batch validation); otherwise use [`try_append_row`].
    pub(crate) fn append_row(&mut self, values: Vec<CellValue>) -> RowId {
        debug_assert_eq!(values.len(), self.schema.columns.len());

        let row_id = self.next_rowid;
        self.row_ids.push(row_id);
        self.next_rowid += 1;
        for (i, v) in values.into_iter().enumerate() {
            self.cols[i].push(v);
        }
        row_id
    }

    /// Append one row after validation and primary-key uniqueness check.
    pub fn add(&self, values: Vec<CellValue>) -> Op {
        Op::Add {
            table: self.path.clone(),
            values,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::ir::{self, Path};

    /// Tables with no data columns still allocate row ids on insert; `row_count` must reflect
    /// those rows (it cannot use column length when `cols` is empty).
    #[test]
    fn row_count_matches_inserts_when_schema_has_no_columns() {
        let schema = ir::Schema {
            columns: vec![],
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("id_only"), schema);
        assert!(tbl.cols.is_empty());
        assert_eq!(tbl.row_count(), 0);

        let r0 = tbl.append_row_validated(vec![]).expect("first row");
        assert_eq!(tbl.row_count(), 1);
        assert_eq!(tbl.row_id_at(0), Some(r0));

        let r1 = tbl.append_row_validated(vec![]).expect("second row");
        assert_eq!(tbl.row_count(), 2);
        assert_eq!(tbl.row_id_at(1), Some(r1));
    }

    #[test]
    fn test_table_add() {
        let columns = vec![ColType::EntityType {
            path: Path::from("G.E"),
        }];
        let gv_schema = ir::Schema {
            columns,
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("test.table"), gv_schema);
        tbl.append_row_validated(vec![CellValue::Id(1)])
            .expect("row");
        assert_eq!(tbl.row_count(), 1);
    }

    /// `primary_key: Some([])` marks a singleton table (at most one row).
    #[test]
    fn empty_primary_key_rejects_second_row() {
        let schema = ir::Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: Some(vec![]),
        };
        let mut tbl = Table::new(Path::from("singleton"), schema);

        tbl.append_row_validated(vec![CellValue::Int(0)])
            .expect("first row");
        assert_eq!(tbl.row_count(), 1);

        let values1 = vec![CellValue::Int(1)];
        let err = tbl.validate_new_row(&values1).unwrap_err();
        assert_eq!(err, ValidationError::DuplicatePrimaryKey);
        assert_eq!(tbl.row_count(), 1);
    }

    #[test]
    fn row_read_helpers_return_row_id_and_cells() {
        let schema = ir::Schema {
            columns: vec![
                ColType::PrimType {
                    prim: PrimType::PrimInt,
                },
                ColType::PrimType {
                    prim: PrimType::PrimString,
                },
            ],
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("readable"), schema);

        let row_id = tbl
            .append_row_validated(vec![CellValue::Int(7), CellValue::Str("x".to_string())])
            .expect("row");

        assert_eq!(tbl.row_id_at(0), Some(row_id));
        assert_eq!(tbl.cell_at(0, 0), Some(&CellValue::Int(7)));
        assert_eq!(tbl.cell_at(0, 1), Some(&CellValue::Str("x".to_string())));
        assert_eq!(tbl.row_id_at(1), None);
        assert_eq!(tbl.cell_at(0, 2), None);
    }

    #[test]
    fn debug_dumps_rows() {
        let schema = ir::Schema {
            columns: vec![
                ColType::PrimType {
                    prim: PrimType::PrimInt,
                },
                ColType::PrimType {
                    prim: PrimType::PrimString,
                },
            ],
            primary_key: None,
        };
        let mut tbl = Table::new(Path::from("debug.table"), schema);

        tbl.append_row_validated(vec![CellValue::Int(7), CellValue::Str("x".to_string())])
            .expect("first");
        tbl.append_row_validated(vec![CellValue::Int(8), CellValue::Str("y".to_string())])
            .expect("second");

        assert_eq!(
            tbl.dump(),
            concat!(
                "table debug.table (rows: 2, cols: 2)\n",
                "[0] row_id=0 | c0=7 | c1=\"x\"\n",
                "[1] row_id=1 | c0=8 | c1=\"y\"\n"
            )
        );
    }
}
