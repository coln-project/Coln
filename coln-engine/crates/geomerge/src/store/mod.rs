use std::collections::HashMap;

use tracing::{debug, info};

use crate::commit::Commit;
use crate::commit::error::PersistError;
use crate::commit::graph::CommitGraph;
use crate::commit::wire::metadata::{RootCommitData, RootTableEntry};
use crate::ir::{self, FlatTheory, LawEntry};
use crate::solver::compile::{CompLaw, CompileError};
use crate::solver::validate::LawViolation;
use crate::solver::{self};
use crate::store::error::StoreIntError;
use crate::table::{CellValue, Table, TableOid, ValidationError};
use crate::txn::Transaction;
use crate::txn::ops::Op;

pub mod error;

// TODO this should not be cloneable for efficiency reasons. In the future we should
// be able to teach the law validator to check the delta
#[derive(Debug, Clone)]
pub struct Store {
    pub(crate) next_oid: TableOid,
    path_to_oid: HashMap<ir::Path, TableOid>,
    tables: HashMap<TableOid, Table>,
    /// Source law entries retained for persistence. Compiled form lives in `laws`.
    law_entries: Vec<ir::LawEntry>,
    /// Compiled law for this instance; table schemas live only on each [`Table`].
    laws: Vec<CompLaw>,
    commits: CommitGraph,
}

impl Store {
    pub fn new() -> Self {
        let commits =
            Self::root_commit_graph(&HashMap::new(), &[]).expect("empty root commit should build");
        Self {
            next_oid: 0,
            path_to_oid: HashMap::new(),
            tables: HashMap::new(),
            law_entries: vec![],
            laws: vec![],
            commits,
        }
    }

    pub fn tables(&self) -> impl Iterator<Item = (&TableOid, &Table)> {
        self.tables.iter()
    }

    pub fn commits(&self) -> &CommitGraph {
        &self.commits
    }

    pub(crate) fn record_commit(&mut self, commit: Commit<'static>) {
        self.commits.add_commit(commit);
    }

    pub(crate) fn replace_commit_graph(&mut self, commits: CommitGraph) {
        self.commits = commits;
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

    pub fn laws(&self) -> &[CompLaw] {
        &self.laws
    }

    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    pub(crate) fn law_entries(&self) -> &[ir::LawEntry] {
        &self.law_entries
    }

    pub(crate) fn from_root_commit_data(
        next_oid: TableOid,
        root: RootCommitData,
    ) -> Result<Self, CompileError> {
        let mut path_to_oid = HashMap::new();
        let mut tables_map = HashMap::new();
        for entry in root.tables {
            let path = ir::Path::from(entry.path.as_str());
            path_to_oid.insert(path.clone(), entry.oid);
            tables_map.insert(entry.oid, Table::new(path, entry.schema));
        }

        let laws = Store::compile_laws(&root.laws)?;
        Ok(Self {
            next_oid,
            path_to_oid,
            tables: tables_map,
            law_entries: root.laws,
            laws,
            commits: CommitGraph::new(),
        })
    }

    fn compile_laws(laws: &[LawEntry]) -> Result<Vec<CompLaw>, CompileError> {
        debug!(law_count = laws.len(), "compiling laws");
        let comp = laws
            .iter()
            .map(solver::compile::compile_law)
            .collect::<Result<Vec<_>, CompileError>>()?;
        debug!(compiled_law_count = comp.len(), "compiled laws");
        Ok(comp)
    }

    fn root_commit_graph(
        tables: &HashMap<TableOid, Table>,
        law_entries: &[LawEntry],
    ) -> Result<CommitGraph, PersistError> {
        let mut table_entries = tables
            .iter()
            .map(|(&oid, table)| RootTableEntry {
                path: table.path().to_string(),
                oid,
                schema: table.schema().clone(),
            })
            .collect::<Vec<_>>();
        table_entries.sort_by_key(|entry| entry.oid);

        let root = RootCommitData {
            tables: table_entries,
            laws: law_entries.to_vec(),
        };

        let mut graph = CommitGraph::new();
        graph.add_commit(Commit::from_root_data(&root)?);
        Ok(graph)
    }

    /// Builds an empty column store per `theory.tables` and keeps only `theory.laws`
    /// (schemas are stored on each [`Table`]).
    pub fn try_from_theory(theory: FlatTheory) -> Result<Self, Box<StoreIntError>> {
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
        let commits = Self::root_commit_graph(&tables_map, &laws)?;
        info!(
            table_count = tables_map.len(),
            compiled_law_count = comp_laws.len(),
            "store initialized"
        );
        Ok(Self {
            next_oid,
            path_to_oid,
            tables: tables_map,
            law_entries: laws,
            laws: comp_laws,
            commits,
        })
    }

    /// Dump every table in the store for debugging, in ascending [`TableOid`] order,
    /// separated by a blank line.
    pub fn dump(&self) -> String {
        let mut oids: Vec<TableOid> = self.tables.keys().copied().collect();
        oids.sort_unstable();
        oids.into_iter()
            .map(|oid| self.tables[&oid].dump())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn insert_table(&mut self, path: ir::Path, table: Table) -> TableOid {
        let oid = self.next_oid;
        self.next_oid = self.next_oid.saturating_add(1);
        self.path_to_oid.insert(path, oid);
        self.tables.insert(oid, table);
        oid
    }

    /// Validates the full batch against current store state (including PK clashes **within** the
    /// batch), then applies each op in order. On validation failure, the store is unchanged.
    /// Returns a vector of row_ids, in the same order as ops
    pub(crate) fn apply_batch(&mut self, ops: Vec<Op>) -> Result<(), Box<StoreIntError>> {
        info!(op_count = ops.len(), "applying batch");
        self.validate_batch(&ops)?;
        let mut preview_store = self.clone();

        for op in &ops {
            let Op::Add {
                table,
                values,
                row_id,
            } = op;
            let oid = preview_store.resolve_table(table).expect("validated batch");
            let t = preview_store.table_mut(oid).expect("validated batch");

            t.append_row(values.clone(), *row_id);
        }

        preview_store.check_laws()?;
        *self = preview_store;
        Ok(())
    }

    pub fn transaction(&mut self) -> Transaction<'_> {
        Transaction::new(self)
    }

    pub(crate) fn validate_batch(&self, ops: &[Op]) -> Result<(), StoreIntError> {
        debug!(op_count = ops.len(), "validating batch");
        let mut pending_pk: HashMap<TableOid, Vec<Vec<CellValue>>> = HashMap::new();

        for op in ops {
            let Op::Add { table, values, .. } = op;
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
            t.validate_insert(values)?;

            // Check primary key conflicts within ops batch
            if let Some(key) = t.primary_key_values(values) {
                let keys = pending_pk.entry(oid).or_default();
                if keys.iter().any(|k| k == &key) {
                    return Err(ValidationError::DuplicatePrimaryKey.into());
                }
                keys.push(key);
            }
        }
        Ok(())
    }

    pub fn check_laws(&self) -> Result<(), Box<StoreIntError>> {
        debug!(law_count = self.laws.len(), "checking laws");
        self.laws()
            .iter()
            .map(|law_entry| solver::validate::check_law(self, law_entry))
            .collect::<Result<Vec<_>, Box<LawViolation>>>()?;
        debug!(law_count = self.laws.len(), "all laws satisfied");
        Ok(())
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared theory fixtures for unit tests (`store`, `transaction`, etc.).
#[cfg(test)]
pub(crate) mod test_support {
    use crate::ir::{
        Atom, ColType, FlatTheory, Law, LawEntry, Path, PrimType, Prop, Schema, TableEntry, Term,
        ValueEntry,
    };

    pub fn link_foreign_key_theory() -> FlatTheory {
        let left = Path::from("Left");
        let right = Path::from("Right");
        let link = Path::from("Link");
        let int_col = || ColType::PrimType {
            prim: PrimType::PrimInt,
        };
        FlatTheory {
            tables: vec![
                TableEntry {
                    path: left.clone(),
                    table: Schema {
                        columns: vec![int_col()],
                        primary_key: None,
                    },
                },
                TableEntry {
                    path: right.clone(),
                    table: Schema {
                        columns: vec![int_col()],
                        primary_key: None,
                    },
                },
                TableEntry {
                    path: link.clone(),
                    table: Schema {
                        columns: vec![int_col(), int_col()],
                        primary_key: None,
                    },
                },
            ],
            laws: vec![LawEntry {
                path: Path::from("Link.foreignKeys"),
                law: Law {
                    variables: vec![
                        ColType::PrimType {
                            prim: PrimType::PrimInt,
                        },
                        ColType::PrimType {
                            prim: PrimType::PrimInt,
                        },
                    ],
                    antecedent: Prop::Atom {
                        atom: Atom {
                            table: link.clone(),
                            row_id: None,
                            values: vec![
                                ValueEntry {
                                    column: 0,
                                    term: Term::Var { index: 0 },
                                },
                                ValueEntry {
                                    column: 1,
                                    term: Term::Var { index: 1 },
                                },
                            ],
                        },
                    },
                    consequent: Prop::And {
                        props: vec![
                            Prop::Atom {
                                atom: Atom {
                                    table: left.clone(),
                                    row_id: None,
                                    values: vec![ValueEntry {
                                        column: 0,
                                        term: Term::Var { index: 0 },
                                    }],
                                },
                            },
                            Prop::Atom {
                                atom: Atom {
                                    table: right.clone(),
                                    row_id: None,
                                    values: vec![ValueEntry {
                                        column: 0,
                                        term: Term::Var { index: 1 },
                                    }],
                                },
                            },
                        ],
                    },
                },
            }],
        }
    }
}

#[cfg(test)]
mod tests {

    use super::test_support::link_foreign_key_theory;
    use super::*;
    use crate::ir::{ColType, Path, PrimType, Schema};

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
    fn transaction_validates_then_applies() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let mut txn = store.transaction();
        txn.add(&path, vec![CellValue::Int(1).into()])
            .expect("first add");
        txn.add(&path, vec![CellValue::Int(2).into()])
            .expect("second add");

        txn.commit().expect("commit");

        assert_eq!(store.table_at(&path).expect("T").row_count(), 2);
    }

    /// Covers the same rollback guarantee as the old `transact` test: if validation fails,
    /// no rows from the batch are committed (here the second op references an unregistered table).
    #[test]
    fn transaction_unknown_table_leaves_store_unchanged() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let err = {
            let mut txn = store.transaction();
            txn.add(&path, vec![CellValue::Int(1).into()])
                .expect("first add");
            txn.add(&Path::from("missing"), vec![CellValue::Int(2).into()])
                .unwrap_err()
        };

        assert!(matches!(
            *err,
            StoreIntError::Validation(ValidationError::UnknownTable { .. })
        ));
        assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
    }

    #[test]
    fn transaction_duplicate_primary_key_within_batch() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: Some(vec![0]),
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let mut txn = store.transaction();
        txn.add(&path, vec![CellValue::Int(1).into()])
            .expect("first add");
        txn.add(&path, vec![CellValue::Int(1).into()])
            .expect("second add");
        let err = txn.commit().unwrap_err();

        assert!(matches!(
            *err,
            StoreIntError::Validation(ValidationError::DuplicatePrimaryKey)
        ));
        assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
    }

    #[test]
    fn transaction_single_insert_commits() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let mut txn = store.transaction();
        txn.add(&path, vec![CellValue::Int(42).into()])
            .expect("add");
        txn.commit().expect("commit");

        let t = store.table_at(&path).expect("T");
        assert_eq!(t.row_count(), 1);
        assert_eq!(t.cell_at(0, 0), Some(&CellValue::Int(42)));
    }

    #[test]
    fn transaction_leaves_store_unchanged_when_laws_fail() {
        let theory = link_foreign_key_theory();
        let link = Path::from("Link");
        let mut store = Store::try_from_theory(theory).expect("theory");

        let mut txn = store.transaction();
        txn.add(
            &link,
            vec![CellValue::Int(10).into(), CellValue::Int(20).into()],
        )
        .expect("add");
        let err = txn.commit().unwrap_err();

        assert!(matches!(*err, StoreIntError::Law(_)));
        assert_eq!(store.table_at(&link).expect("Link").row_count(), 0);
    }

    #[test]
    fn apply_error_from_inner_errors() {
        let validation = StoreIntError::from(ValidationError::DuplicatePrimaryKey);
        assert!(matches!(
            validation,
            StoreIntError::Validation(ValidationError::DuplicatePrimaryKey)
        ));

        let compile = StoreIntError::from(CompileError::UnsupportedTerm);
        assert!(matches!(
            compile,
            StoreIntError::Compile(CompileError::UnsupportedTerm)
        ));

        let compiled_law = solver::compile::CompLaw {
            path: Path::from("T.total"),
            vars: vec![],
            antecedent: solver::compile::CompProp::And(vec![]),
            consequent: solver::compile::CompProp::And(vec![]),
            tables: vec![Path::from("T")],
        };
        let violation = LawViolation {
            law: compiled_law,
            cause: solver::validate::ViolationCause::MissingAtom(solver::compile::CompAtom {
                table: Path::from("T"),
                row_id: None,
                values: vec![],
            }),
            binding: vec![],
        };
        let law = StoreIntError::from(violation);
        assert!(matches!(law, StoreIntError::Law(_)));
    }
}
