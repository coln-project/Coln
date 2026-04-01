use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use tracing::{debug, info};

use crate::ir::{self, FlatTheory, LawEntry};
use crate::ops::Op;
use crate::solver::compile::{CompLaw, CompileError};
use crate::solver::validate::LawViolation;
use crate::solver::{self};
use crate::table::{CellValue, RowId, Table, TableOid, ValidationError};

// TODO this should not be cloneable for efficiency reasons. In the future we should
// be able to teach the law validator to check the delta
#[derive(Debug, Clone)]
pub struct Store {
    next_oid: TableOid,
    path_to_oid: HashMap<ir::Path, TableOid>,
    tables: HashMap<TableOid, Table>,
    /// Compiled law for this instance; table schemas live only on each [`Table`].
    laws: Vec<CompLaw>,
}

/// Store integrity error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreIntError {
    Validation(ValidationError),
    Law(LawViolation),
    Compile(CompileError),
}

impl fmt::Display for StoreIntError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreIntError::Validation(err) => write!(f, "{err}"),
            StoreIntError::Law(err) => write!(f, "{err}"),
            StoreIntError::Compile(err) => write!(f, "{err:?}"),
        }
    }
}

impl Error for StoreIntError {}

impl From<ValidationError> for StoreIntError {
    fn from(value: ValidationError) -> Self {
        Self::Validation(value)
    }
}

impl From<LawViolation> for StoreIntError {
    fn from(value: LawViolation) -> Self {
        Self::Law(value)
    }
}

impl From<CompileError> for StoreIntError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

impl Store {
    pub fn new() -> Self {
        Self {
            next_oid: 0,
            path_to_oid: HashMap::new(),
            tables: HashMap::new(),
            laws: vec![],
        }
    }

    fn compile_laws(laws: &[LawEntry]) -> Result<Vec<CompLaw>, CompileError> {
        debug!(law_count = laws.len(), "compiling laws");
        let comp = laws
            .iter()
            .map(|law_entry| Ok(solver::compile::compile_law(law_entry)?))
            .collect::<Result<Vec<_>, CompileError>>()?;
        debug!(compiled_law_count = comp.len(), "compiled laws");
        Ok(comp)
    }

    /// Builds an empty column store per `theory.tables` and keeps only `theory.laws`
    /// (schemas are stored on each [`Table`]).
    pub fn try_from_theory(theory: FlatTheory) -> Result<Self, StoreIntError> {
        let FlatTheory { tables, laws } = theory;
        info!(
            table_count = tables.len(),
            law_count = laws.len(),
            "building store from theory"
        );

        let mut next_oid: TableOid = 0;
        let mut path_to_oid = HashMap::new();
        let mut tables_map = HashMap::new();

        for entry in tables {
            let oid = next_oid;
            next_oid = next_oid.saturating_add(1);
            path_to_oid.insert(entry.path.clone(), oid);
            tables_map.insert(oid, Table::new(entry.path, entry.table));
        }

        let comp_laws = Store::compile_laws(&laws)?;
        info!(
            table_count = tables_map.len(),
            compiled_law_count = comp_laws.len(),
            "store initialized"
        );
        Ok(Self {
            next_oid,
            path_to_oid,
            tables: tables_map,
            laws: comp_laws,
        })
    }

    pub fn laws(&self) -> &[CompLaw] {
        &self.laws
    }

    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    pub fn insert_table(&mut self, path: ir::Path, table: Table) -> TableOid {
        let oid = self.next_oid;
        self.next_oid = self.next_oid.saturating_add(1);
        self.path_to_oid.insert(path, oid);
        self.tables.insert(oid, table);
        oid
    }

    pub fn resolve_table(&self, path: &ir::Path) -> Option<TableOid> {
        self.path_to_oid.get(path).copied()
    }

    pub fn table(&self, oid: TableOid) -> Option<&Table> {
        self.tables.get(&oid)
    }

    pub fn table_mut(&mut self, oid: TableOid) -> Option<&mut Table> {
        self.tables.get_mut(&oid)
    }

    pub fn table_at(&self, path: &ir::Path) -> Option<&Table> {
        self.resolve_table(path).and_then(|oid| self.table(oid))
    }

    pub fn table_at_mut(&mut self, path: &ir::Path) -> Option<&mut Table> {
        self.resolve_table(path).and_then(|oid| self.table_mut(oid))
    }

    /// Validates the full batch against current store state (including PK clashes **within** the
    /// batch), then applies each op in order. On validation failure, the store is unchanged.
    /// Returns a vector of row_ids, in the same order as ops
    pub fn apply_batch(&mut self, ops: Vec<Op>) -> Result<Vec<RowId>, StoreIntError> {
        info!(op_count = ops.len(), "applying batch");
        self.validate_batch(&ops)?;
        let mut preview_store = self.clone();

        let mut row_ids = vec![];
        for op in &ops {
            let Op::Add { table, values } = op;
            let oid = preview_store
                .resolve_table(&table)
                .expect("validated batch");
            let t = preview_store.table_mut(oid).expect("validated batch");
            row_ids.push(t.append_row(values.clone()));
        }

        preview_store.check_laws()?;
        *self = preview_store;

        info!(row_count = row_ids.len(), "applied batch");
        Ok(row_ids)
    }

    fn validate_batch(&self, ops: &[Op]) -> Result<(), StoreIntError> {
        debug!(op_count = ops.len(), "validating batch");
        let mut pending_pk: HashMap<TableOid, Vec<Vec<CellValue>>> = HashMap::new();

        for op in ops {
            let Op::Add { table, values } = op;
            // Check ops have the right table path
            let oid = self
                .resolve_table(table)
                .ok_or_else(|| ValidationError::UnknownTable {
                    path: table.clone(),
                })?;
            let t = self
                .table(oid)
                .ok_or_else(|| ValidationError::UnknownTable {
                    path: table.clone(),
                })?;
            t.validate_new_row(values)?;

            // Check primary key conflicts within ops batch
            if let Some(key) = t.primary_key_values(values) {
                let keys = pending_pk.entry(oid).or_default();
                if keys.iter().any(|k| k == &key) {
                    return Err(ValidationError::DuplicatePrimaryKey)?;
                }
                keys.push(key);
            }
        }
        Ok(())
    }

    pub fn check_laws(&self) -> Result<(), StoreIntError> {
        debug!(law_count = self.laws.len(), "checking laws");
        self.laws()
            .iter()
            .map(|law_entry| Ok(solver::validate::check_law(self, law_entry)?))
            .collect::<Result<Vec<_>, LawViolation>>()?;
        debug!(law_count = self.laws.len(), "all laws satisfied");
        Ok(())
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::ir::{ColType, Path, PrimType, Schema};
    use crate::ops::Op;

    #[test]
    fn test_store_create_table() {
        let path = Path::from("table1");
        let schema = Schema {
            columns: vec![ColType::EntityType { path: path.clone() }],
            primary_key: None,
        };
        let table = Table::new(path.clone(), schema);

        let mut store = Store::new();
        let oid0 = store.insert_table(path.clone(), table);
        assert_eq!(oid0, 0);

        let t = store.table(oid0).expect("table at oid 0");
        assert_eq!(t.schema().columns.len(), 1);
        assert_eq!(t.row_count(), 0);

        // Second registration gets the next oid.
        let schema2 = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let oid1 = store.insert_table(
            Path::from("Other"),
            Table::new(Path::from("table2"), schema2),
        );
        assert_eq!(oid1, 1);
    }

    #[test]
    fn test_store_resolve_table_oid() {
        let path = Path::from("G.E");
        let schema = Schema {
            columns: vec![ColType::EntityType { path: path.clone() }],
            primary_key: None,
        };

        let mut store = Store::new();
        let oid = store.insert_table(path.clone(), Table::new(path.clone(), schema));

        assert_eq!(store.resolve_table(&path), Some(oid));
        assert_eq!(store.resolve_table(&Path::from("missing")), None);
    }

    #[test]
    fn apply_batch_validates_then_applies() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        store
            .apply_batch(vec![
                Op::Add {
                    table: path.clone(),
                    values: vec![CellValue::Int(1)],
                },
                Op::Add {
                    table: path.clone(),
                    values: vec![CellValue::Int(2)],
                },
            ])
            .expect("batch");

        assert_eq!(store.table_at(&path).expect("T").row_count(), 2);
    }

    #[test]
    fn apply_batch_unknown_table_leaves_store_unchanged() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let err = store
            .apply_batch(vec![
                Op::Add {
                    table: path.clone(),
                    values: vec![CellValue::Int(1)],
                },
                Op::Add {
                    table: Path::from("missing"),
                    values: vec![CellValue::Int(2)],
                },
            ])
            .unwrap_err();

        assert!(matches!(
            err,
            StoreIntError::Validation(ValidationError::UnknownTable { .. })
        ));
        assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
    }

    #[test]
    fn apply_batch_duplicate_primary_key_within_batch() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: Some(vec![0]),
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let err = store
            .apply_batch(vec![
                Op::Add {
                    table: path.clone(),
                    values: vec![CellValue::Int(1)],
                },
                Op::Add {
                    table: path.clone(),
                    values: vec![CellValue::Int(1)],
                },
            ])
            .unwrap_err();

        assert_eq!(
            err,
            StoreIntError::Validation(ValidationError::DuplicatePrimaryKey)
        );
        assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
    }

    #[test]
    fn apply_error_from_inner_errors() {
        let validation = StoreIntError::from(ValidationError::DuplicatePrimaryKey);
        assert_eq!(
            validation,
            StoreIntError::Validation(ValidationError::DuplicatePrimaryKey)
        );

        let compile = StoreIntError::from(CompileError::UnsupportedTerm);
        assert_eq!(
            compile,
            StoreIntError::Compile(CompileError::UnsupportedTerm)
        );

        let compiled_law = solver::compile::CompLaw {
            path: Path::from("T.total"),
            vars: vec![],
            antecedent: vec![],
            consequent: vec![],
            tables: vec![Path::from("T")],
        };
        let violation = LawViolation {
            law: compiled_law,
            atom: solver::compile::CompAtom {
                table: Path::from("T"),
                row_id: None,
                values: vec![],
            },
            binding: vec![],
        };
        let law = StoreIntError::from(violation.clone());
        assert_eq!(law, StoreIntError::Law(violation));
    }
}
