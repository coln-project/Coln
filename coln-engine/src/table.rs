use std::error::Error;
use std::fmt;

use crate::ir;
use crate::{
    ir::{ColType, PrimType, Schema},
    ops::Op,
};

pub type TableOid = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
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
    Id(u64),
    Int(i64),
    Str(String),
}

fn cell_matches_schema(
    col_type: &ColType,
    value: &CellValue,
    column: usize,
) -> Result<(), ValidationError> {
    match col_type {
        ColType::EntityType { .. } => match value {
            CellValue::Id(_) => Ok(()),
            _ => Err(ValidationError::TypeMismatch {
                column,
                expected: "entity id",
                got: cell_kind(value),
            }),
        },
        ColType::PrimType { prim } => match (prim, value) {
            (PrimType::PrimInt, CellValue::Int(_)) => Ok(()),
            (PrimType::PrimString, CellValue::Str(_)) => Ok(()),
            _ => Err(ValidationError::TypeMismatch {
                column,
                expected: match prim {
                    PrimType::PrimInt => "int",
                    PrimType::PrimString => "string",
                },
                got: cell_kind(value),
            }),
        },
        ColType::Tuple { .. } => Err(ValidationError::UnsupportedTuple { column }),
    }
}

// TODO perhaps do a display trait
fn cell_kind(v: &CellValue) -> &'static str {
    match v {
        CellValue::Id(_) => "id",
        CellValue::Int(_) => "int",
        CellValue::Str(_) => "string",
    }
}

/// Columnar store: `cols[i]` is all values for schema column `i` (same length per column).
pub struct Table {
    path: ir::Path,
    schema: Schema,
    cols: Vec<Vec<CellValue>>,
}

impl Table {
    pub fn new(path: ir::Path, schema: Schema) -> Self {
        let n = schema.columns.len();
        Self {
            path,
            schema,
            cols: vec![Vec::new(); n],
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn row_count(&self) -> usize {
        self.cols.first().map(|c| c.len()).unwrap_or(0)
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
            cell_matches_schema(col_type, value, i)?;
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
                Some(pk.iter().map(|&i| values[i as usize].clone()).collect())
            }
        })
    }

    /// Schema and primary-key check against rows already stored (does not mutate).
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

    pub fn apply(&mut self, op: Op) -> Result<(), ValidationError> {
        match op {
            Op::Add { values, .. } => {
                self.validate_new_row(&values)?;
                for (i, v) in values.iter().enumerate() {
                    self.cols[i].push(v.clone());
                }
                Ok(())
            }
        }
    }

    /// Append one row after validation and primary-key uniqueness check.
    pub fn add(&mut self, values: Vec<CellValue>) -> Op {
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
        let values = vec![CellValue::Id(1)];
        let op = tbl.add(values);
        assert_eq!(tbl.apply(op), Ok(()));
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

        let op0 = tbl.add(vec![CellValue::Int(0)]);
        assert_eq!(tbl.apply(op0), Ok(()));
        assert_eq!(tbl.row_count(), 1);

        let op1 = tbl.add(vec![CellValue::Int(1)]);
        let err = tbl.apply(op1).unwrap_err();
        assert_eq!(err, ValidationError::DuplicatePrimaryKey);
        assert_eq!(tbl.row_count(), 1);
    }
}
