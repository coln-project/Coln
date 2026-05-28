use std::collections::{HashMap, HashSet, VecDeque};

use tracing::{debug, info};

use crate::commit::Commit;
use crate::commit::error::CodecError;
use crate::commit::graph::CommitGraph;
use crate::commit::hash::CommitHash;
use crate::commit::wire::metadata::{RootCommitData, RootTableEntry};
use crate::ir::{self, FlatTheory, LawEntry};
use crate::solver::compile::{CompLaw, CompileError};
use crate::solver::validate::LawViolation;
use crate::solver::{self};
use crate::store::error::{CommitApplyError, StoreIntError};
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
    // Constructors and basic accessors
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

    /// Add commit to the commit graph. This is a low level API, typically you
    /// want to use `apply_commits`
    pub(crate) fn record_in_commit_graph(&mut self, commit: Commit<'static>) {
        self.commits.add_commit(commit);
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
}

impl Store {
    // create stores from theory and transactions on stores

    /// Create a store from a root commit. A root commit contains all the necessary
    /// information about schema and laws for the store to generate the right shape
    /// of tables.
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

    fn root_commit_graph(
        tables: &HashMap<TableOid, Table>,
        law_entries: &[LawEntry],
    ) -> Result<CommitGraph, CodecError> {
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

    #[cfg(test)]
    pub(crate) fn insert_table(&mut self, path: ir::Path, table: Table) -> TableOid {
        let oid = self.next_oid;
        self.next_oid = self.next_oid.saturating_add(1);
        self.path_to_oid.insert(path, oid);
        self.tables.insert(oid, table);
        oid
    }

    /// Validates and appends a batch to this store.
    ///
    /// This is a low level API that mutates `self` before law checking, so
    /// callers that need atomicity must call it on a preview clone and publish
    /// that clone only after success.
    fn apply_batch(&mut self, ops: Vec<Op>) -> Result<(), Box<StoreIntError>> {
        info!(op_count = ops.len(), "applying batch");
        self.validate_batch(&ops)?;

        for op in &ops {
            let Op::Add {
                table,
                values,
                row_id,
            } = op;
            let oid = self.resolve_table(table).expect("validated batch");
            let t = self.table_mut(oid).expect("validated batch");

            t.append_row(values.clone(), *row_id);
        }

        self.check_laws()?;
        Ok(())
    }

    fn validate_batch(&self, ops: &[Op]) -> Result<(), StoreIntError> {
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

    pub fn transaction(&mut self) -> Transaction<'_> {
        Transaction::new(self)
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
}

impl Store {
    // Dealing with laws
    fn compile_laws(laws: &[LawEntry]) -> Result<Vec<CompLaw>, CompileError> {
        debug!(law_count = laws.len(), "compiling laws");
        let comp = laws
            .iter()
            .map(solver::compile::compile_law)
            .collect::<Result<Vec<_>, CompileError>>()?;
        debug!(compiled_law_count = comp.len(), "compiled laws");
        Ok(comp)
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

impl Store {
    // Read and write commits

    pub fn heads(&self) -> Vec<CommitHash> {
        self.commits.heads().cloned().collect()
    }

    pub fn commit_by_hash(&self, hash: &CommitHash) -> Option<&Commit<'static>> {
        self.commits.get(hash)
    }

    /// return commits that are not ancestors of the heads
    pub fn commits_after(&self, have_heads: &[CommitHash]) -> Vec<Commit<'static>> {
        let mut seen = HashSet::new();
        let mut stack = have_heads.to_vec();

        while let Some(ch) = stack.pop() {
            if !seen.insert(ch) {
                continue;
            }

            if let Some(cm) = self.commit_by_hash(&ch) {
                stack.extend(cm.deps.iter());
            }
        }

        self.commits
            .iter_topological()
            .filter(|cm| !seen.contains(&cm.hash()))
            .cloned()
            .collect::<Vec<Commit>>()
    }

    /// Get commits in `other` that are not in `self`
    pub fn commits_added(&self, other: &Self) -> Vec<Commit<'static>> {
        // a depth first search from the heads of others backwards until hashes
        // are in self
        let mut stack = other.heads();
        let mut seen = HashSet::new();
        let mut added = Vec::new();

        while let Some(hash) = stack.pop() {
            if !seen.insert(hash) || self.commits.contains(&hash) {
                continue;
            }

            added.push(hash);
            if let Some(commit) = other.commit_by_hash(&hash) {
                stack.extend(commit.deps.iter());
            }
        }

        added.reverse();
        added
            .into_iter()
            .filter_map(|hash| other.commit_by_hash(&hash).cloned())
            .collect()
    }

    pub fn merge(&mut self, other: &Self) -> Result<Vec<CommitHash>, Box<StoreIntError>> {
        let commits = self.commits_added(other);
        self.apply_commits(commits)?;
        Ok(self.heads())
    }

    fn apply_commit_ready(&mut self, cmt: Commit<'static>) -> Result<(), Box<StoreIntError>> {
        self.apply_batch(cmt.resolved_ops())?;
        self.record_in_commit_graph(cmt);
        Ok(())
    }

    pub fn apply_commit(&mut self, commit: Commit<'static>) -> Result<(), Box<StoreIntError>> {
        self.apply_commits([commit])
    }

    pub fn apply_commits(
        &mut self,
        commits: impl IntoIterator<Item = Commit<'static>>,
    ) -> Result<(), Box<StoreIntError>> {
        let mut pending = HashMap::new();

        for commit in commits {
            let hash = commit.hash();
            if self.commits.contains(&hash) {
                continue;
            }

            if commit.is_root() {
                return Err(CommitApplyError::RootCommit.into());
            }
            if commit.deps.is_empty() {
                return Err(CommitApplyError::MissingDep.into());
            }

            if let Some(existing) = pending.get(&hash) {
                let existing: &Commit<'static> = existing;
                if existing.chunk_type() != commit.chunk_type()
                    || existing.payload() != commit.payload()
                {
                    return Err(CommitApplyError::ConflictPayload.into());
                }
                continue;
            }

            pending.insert(hash, commit);
        }

        let mut unsatisfied: HashMap<CommitHash, i32> = HashMap::new();
        let mut waiting_on: HashMap<CommitHash, Vec<CommitHash>> = HashMap::new();

        // commits that can be applied
        let mut ready: VecDeque<CommitHash> = VecDeque::new();

        for (hash, commit) in &pending {
            let mut count = 0;

            for dep in &commit.deps {
                if self.commits.contains(dep) {
                    continue;
                }

                if pending.contains_key(dep) {
                    count += 1;
                    waiting_on.entry(*dep).or_default().push(*hash);
                } else {
                    // deps is not in pending or applied commits
                    return Err(CommitApplyError::MissingDep.into());
                }
            }

            if count == 0 {
                ready.push_back(*hash);
            } else {
                unsatisfied.insert(*hash, count);
            }
        }

        let mut preview = self.clone();

        while let Some(hash) = ready.pop_front() {
            let commit = pending
                .remove(&hash)
                .ok_or(CommitApplyError::MissingCommit)?;
            preview.apply_commit_ready(commit)?;

            if let Some(waitings) = waiting_on.remove(&hash) {
                for wh in waitings {
                    let count = unsatisfied
                        .get_mut(&wh)
                        .ok_or(CommitApplyError::MissingCommit)?;
                    *count -= 1;
                    if *count == 0 {
                        unsatisfied.remove(&wh).unwrap();
                        ready.push_back(wh);
                    }
                }
            }
        }

        // pending is not empty, but there is no commit to apply
        if !pending.is_empty() {
            return Err(CommitApplyError::DisconnectedCommit.into());
        }

        *self = preview;
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

    fn single_int_store() -> Store {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path, schema));
        store
    }

    fn commit_int(store: &mut Store, value: i64) -> CommitHash {
        let path = Path::from("T");
        let mut tx = store.transaction();
        tx.add(&path, vec![value.into()]).expect("add row");
        tx.commit().expect("commit row")
    }

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
    fn heads_and_commit_by_hash_track_current_frontier() {
        let mut store = single_int_store();
        let root = store.heads();
        assert_eq!(root.len(), 1);
        assert_eq!(
            store.commit_by_hash(&root[0]).expect("root").hash(),
            root[0]
        );

        let commit = commit_int(&mut store, 42);

        assert_eq!(store.heads(), vec![commit]);
        assert_eq!(
            store.commit_by_hash(&commit).expect("data commit").hash(),
            commit
        );
    }

    #[test]
    fn commits_after_returns_descendants_in_topological_order() {
        let mut store = single_int_store();
        let root = store.heads();
        let first = commit_int(&mut store, 1);
        let second = commit_int(&mut store, 2);

        let commits = store.commits_after(&root);
        let hashes = commits.iter().map(Commit::hash).collect::<Vec<_>>();

        assert_eq!(hashes, vec![first, second]);
        assert!(store.commits_after(&store.heads()).is_empty());
    }

    #[test]
    fn commits_added_returns_commits_in_other_store() {
        let base = single_int_store();
        let mut other = base.clone();
        let commit = commit_int(&mut other, 7);

        let commits = base.commits_added(&other);
        let hashes = commits.iter().map(Commit::hash).collect::<Vec<_>>();

        assert_eq!(hashes, vec![commit]);
    }

    #[test]
    fn apply_commits_applies_rows_and_updates_heads() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        let commit = commit_int(&mut source, 99);

        let commits = source.commits_after(&target.heads());
        target.apply_commits(commits).expect("apply commits");

        let table = target.table_at(&Path::from("T")).expect("table");
        assert_eq!(table.row_count(), 1);
        assert_eq!(table.cell_at(0, 0), Some(&CellValue::Int(99)));
        assert_eq!(table.row_id_at(0).expect("row id").commit, commit);
        assert_eq!(target.heads(), source.heads());
    }

    #[test]
    fn apply_commits_accepts_out_of_order_input() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        commit_int(&mut source, 1);
        commit_int(&mut source, 2);

        let mut commits = source.commits_after(&target.heads());
        commits.reverse();
        target.apply_commits(commits).expect("apply commits");

        let table = target.table_at(&Path::from("T")).expect("table");
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.cell_at(0, 0), Some(&CellValue::Int(1)));
        assert_eq!(table.cell_at(1, 0), Some(&CellValue::Int(2)));
        assert_eq!(target.heads(), source.heads());
    }

    #[test]
    fn apply_commits_ignores_known_commits() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        commit_int(&mut source, 5);

        let commits = source.commits_after(&target.heads());
        target
            .apply_commits(commits.clone())
            .expect("first apply commits");
        target.apply_commits(commits).expect("second apply commits");

        assert_eq!(
            target
                .table_at(&Path::from("T"))
                .expect("table")
                .row_count(),
            1
        );
    }

    #[test]
    fn apply_commits_rejects_missing_dependency_without_changing_store() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        commit_int(&mut source, 1);
        let second = commit_int(&mut source, 2);
        let second_commit = source
            .commit_by_hash(&second)
            .expect("second commit")
            .clone();

        let err = target.apply_commits([second_commit]).unwrap_err();

        assert!(matches!(
            *err,
            StoreIntError::Commit(CommitApplyError::MissingDep)
        ));
        assert_eq!(
            target
                .table_at(&Path::from("T"))
                .expect("table")
                .row_count(),
            0
        );
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
