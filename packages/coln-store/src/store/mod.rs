// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub mod error;

use std::collections::{BTreeSet, HashMap, HashSet};

use tracing::{debug, info};

use crate::commit::Commit;
use crate::commit::chunk::Chunk;
use crate::commit::error::CodecError;
use crate::commit::graph::CommitGraph;
use crate::commit::hash::CommitHash;
use crate::id_packer::{IdPacker, IdPackerSnapshot};
use crate::ir::{self, FlatRealm, RuleEntry};
use crate::rollback::Rollback;
use crate::rowing::{self, RowingSnapshot};
use crate::solver::compile::{CompRule, CompileError};
use crate::solver::validate::RuleViolation;
use crate::solver::{self};
use crate::store::error::{CommitApplyError, StoreError};
use crate::table::{
    CellValue, RowId, RowView, Table, TableMeta, TableOid, TableRef, TableSnapshot, ValidationError,
};
use crate::txn::{OwnedTransaction, Transaction};
use crate::{op::Op, txn::RowHandle};

#[derive(Debug)]
pub struct Store {
    pub(crate) next_oid: TableOid,
    path_to_oid: HashMap<ir::Path, TableOid>,
    tables: HashMap<TableOid, Table>,
    id_packer: IdPacker,
    /// Source rule entries retained for persistence. Compiled form lives in `rules`.
    rule_entries: Vec<ir::RuleEntry>,
    /// Compiled rule for this instance; table schemas live only on each [`Table`].
    rules: Vec<CompRule>,
    commits: CommitGraph,
    rowing: rowing::Rowing,
}

pub(crate) struct StoreSnapshot {
    tables: Vec<(TableOid, TableSnapshot)>,
    id_packer: IdPackerSnapshot,
    rowing: RowingSnapshot,
}

impl Rollback for Store {
    type Snapshot = StoreSnapshot;

    fn snapshot(&mut self) -> Self::Snapshot {
        let tables = self
            .tables
            .iter_mut()
            .map(|(&oid, table)| (oid, table.snapshot()))
            .collect();
        let id_packer = self.id_packer.snapshot();
        let rowing = self.rowing.snapshot();
        StoreSnapshot {
            tables,
            id_packer,
            rowing,
        }
    }

    fn commit_snapshot(&mut self, snapshot: Self::Snapshot) {
        let StoreSnapshot {
            tables,
            id_packer,
            rowing,
        } = snapshot;
        for (oid, snapshot) in tables {
            self.tables
                .get_mut(&oid)
                .expect("snapshotted table should still exist")
                .commit_snapshot(snapshot);
        }
        self.id_packer.commit_snapshot(id_packer);
        self.rowing.commit_snapshot(rowing);
    }

    fn rollback(&mut self, snapshot: Self::Snapshot) {
        let StoreSnapshot {
            tables,
            id_packer,
            rowing,
        } = snapshot;
        for (oid, snapshot) in tables {
            self.tables
                .get_mut(&oid)
                .expect("snapshotted table should still exist")
                .rollback(snapshot);
        }
        self.id_packer.rollback(id_packer);
        self.rowing.rollback(rowing);
    }
}

impl Store {
    // Constructors and basic accessors
    pub fn new() -> Self {
        let commits = Self::graph_with_root_commit(&FlatRealm {
            tables: Vec::new(),
            rules: Vec::new(),
        })
        .expect("empty root commit should build");
        Self {
            next_oid: 0,
            path_to_oid: HashMap::new(),
            tables: HashMap::new(),
            id_packer: IdPacker::new(),
            rule_entries: vec![],
            rules: vec![],
            commits,
            rowing: rowing::Rowing::new(),
        }
    }

    pub fn tables(&self) -> impl Iterator<Item = (&TableOid, TableRef<'_>)> {
        self.tables
            .iter()
            .map(|(oid, table)| (oid, TableRef::new(table, &self.id_packer)))
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

    pub fn table(&self, oid: TableOid) -> Option<TableRef<'_>> {
        self.tables
            .get(&oid)
            .map(|table| TableRef::new(table, &self.id_packer))
    }

    pub fn table_at(&self, path: &ir::Path) -> Option<TableRef<'_>> {
        self.resolve_table(path).and_then(|oid| self.table(oid))
    }

    pub fn rules(&self) -> &[CompRule] {
        &self.rules
    }

    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    pub fn rule_entries(&self) -> &[ir::RuleEntry] {
        &self.rule_entries
    }

    pub fn scan_table(&self, table_path: &ir::Path) -> Option<impl Iterator<Item = RowView> + '_> {
        self.table_at(table_path).map(|table| table.table_scan())
    }

    pub fn json_ir(&self) -> Result<String, StoreError> {
        let realm = self.commits.root_commit()?.root_payload()?;
        Ok(serde_json::to_string(&realm).map_err(CodecError::from)?)
    }

    pub(crate) fn canonical_row_id(&self, row_id: RowId) -> Option<RowId> {
        let packed = self.id_packer.lookup_row_id(row_id)?;
        let canonical = self.rowing.canonical_id(&packed, &self.id_packer);
        Some(self.id_packer.unpack_row_id(canonical))
    }

    pub fn row_by_handle(&self, table: &ir::Path, row_handle: RowHandle) -> Option<RowView> {
        let row_id = row_handle.row_id().ok()?;
        let con_rowid = self.canonical_row_id(row_id)?;
        // replace the rowid in the row_handle so it stays canonical
        if row_id != con_rowid {
            row_handle.canonicalise(con_rowid).ok()?
        }
        self.row_by_id(table, con_rowid)
    }

    // This function will canonicalise the row_id on read, but will not change it
    // See `row_by_handle` which will actually canonicalise the handle.
    // We need both because the TS FFI does not deal with handles.
    pub fn row_by_id(&self, table: &ir::Path, row_id: RowId) -> Option<RowView> {
        let row_id = self.canonical_row_id(row_id)?;
        self.table_at(table)
            .and_then(|table| table.row_at(table.row_position(row_id)?))
    }
}

impl Store {
    // create stores from theory and transactions on stores

    fn graph_with_root_commit(ir: &FlatRealm) -> Result<CommitGraph, CodecError> {
        let mut graph = CommitGraph::new();
        graph.add_commit(Commit::from_root_data(ir)?);
        Ok(graph)
    }

    #[cfg(feature = "native")]
    // used in SQL mode only
    pub(crate) fn create_table(
        &mut self,
        path: ir::Path,
        schema: ir::Schema,
    ) -> Result<TableOid, StoreError> {
        let oid = self.next_oid;
        self.next_oid = self.next_oid.saturating_add(1);
        self.path_to_oid.insert(path.clone(), oid);
        self.tables.insert(oid, Table::new(path, oid, schema));

        let mut tables: Vec<_> = self
            .tables
            .values()
            .map(|table| {
                (
                    table.oid(),
                    ir::TableEntry {
                        path: table.path().clone(),
                        table: table.schema().clone(),
                    },
                )
            })
            .collect();
        tables.sort_by_key(|(oid, _)| *oid);
        let ir = FlatRealm {
            tables: tables.into_iter().map(|(_, entry)| entry).collect(),
            rules: self.rule_entries.clone(),
        };
        self.commits = Self::graph_with_root_commit(&ir)?;
        Ok(oid)
    }

    pub fn transaction(&mut self) -> Transaction<'_> {
        Transaction::new(self)
    }

    pub fn into_transaction(self) -> OwnedTransaction {
        OwnedTransaction::new(self)
    }

    /// Builds an empty column store per `theory.tables` and keeps only `theory.rules`
    /// (schemas are stored on each [`Table`]).
    pub fn try_from_ir(ir: FlatRealm) -> Result<Self, StoreError> {
        info!(
            table_count = ir.tables.len(),
            rule_count = ir.rules.len(),
            "building store from theory"
        );

        let mut next_oid: TableOid = 0;
        let mut path_to_oid = HashMap::new();
        let mut tables_map = HashMap::new();

        for entry in &ir.tables {
            let oid = next_oid;
            next_oid = next_oid.saturating_add(1);
            path_to_oid.insert(entry.path.clone(), oid);
            tables_map.insert(
                oid,
                Table::new(entry.path.clone(), oid, entry.table.clone()),
            );
        }

        let comp_rules = Store::compile_rules(&ir.rules)?;
        let commits = Self::graph_with_root_commit(&ir)?;

        Ok(Self {
            next_oid,
            path_to_oid,
            tables: tables_map,
            id_packer: IdPacker::new(),
            rule_entries: ir.rules,
            rules: comp_rules,
            commits,
            rowing: rowing::Rowing::new(),
        })
    }
}

impl Store {
    // Dealing with rules
    fn compile_rules(rules: &[RuleEntry]) -> Result<Vec<CompRule>, CompileError> {
        debug!(rule_count = rules.len(), "compiling rules");
        let comp = rules
            .iter()
            .map(solver::compile::compile_rule)
            .collect::<Result<Vec<_>, CompileError>>()?;
        debug!(compiled_rule_count = comp.len(), "compiled rules");
        Ok(comp)
    }

    pub fn check_rules(&self) -> Result<(), StoreError> {
        debug!(rule_count = self.rules.len(), "checking rules");
        self.rules()
            .iter()
            .map(|rule| solver::validate::check_rule(self, rule))
            .collect::<Result<Vec<_>, Box<RuleViolation>>>()?;
        debug!(rule_count = self.rules.len(), "all rules satisfied");
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

    /// Path, oid, and schema for a registered table.
    pub(crate) fn table_meta(&self, oid: TableOid) -> Option<TableMeta<'_>> {
        self.table(oid).map(|table| TableMeta {
            path: table.path(),
            oid,
            schema: table.schema(),
        })
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

    pub fn merge(&mut self, other: &Self) -> Result<Vec<CommitHash>, StoreError> {
        let commits = self.commits_added(other);
        self.apply_commits(commits)?;
        Ok(self.heads())
    }

    pub fn apply_commit(&mut self, commit: Commit<'static>) -> Result<(), StoreError> {
        // This needs to call apply_commits because it needs to do dependency check
        self.apply_commits([commit])
    }

    pub fn apply_commits(
        &mut self,
        commits: impl IntoIterator<Item = Commit<'static>>,
    ) -> Result<(), StoreError> {
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
                if *existing != commit {
                    return Err(CommitApplyError::ConflictPayload.into());
                }
                continue;
            }

            pending.insert(hash, commit);
        }

        let mut unsatisfied: HashMap<CommitHash, i32> = HashMap::new();
        let mut waiting_on: HashMap<CommitHash, Vec<CommitHash>> = HashMap::new();

        // commits that can be applied
        // use BTreeSet to ensure concurrent commit ordering is deterministic
        let mut ready: BTreeSet<CommitHash> = BTreeSet::new();

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
                ready.insert(*hash);
            } else {
                unsatisfied.insert(*hash, count);
            }
        }

        while let Some(hash) = ready.pop_first() {
            let commit = pending
                .remove(&hash)
                .ok_or(CommitApplyError::MissingCommit)?;

            self.apply_commit_atomic(commit)?;
            if let Some(waitings) = waiting_on.remove(&hash) {
                for wh in waitings {
                    let count = unsatisfied
                        .get_mut(&wh)
                        .ok_or(CommitApplyError::MissingCommit)?;
                    *count -= 1;
                    if *count == 0 {
                        unsatisfied.remove(&wh).unwrap();
                        ready.insert(wh);
                    }
                }
            }
        }

        // pending is not empty, but there is no commit to apply
        if !pending.is_empty() {
            return Err(CommitApplyError::DisconnectedCommit.into());
        }

        Ok(())
    }

    fn apply_commit_atomic(&mut self, commit: Commit<'static>) -> Result<(), StoreError> {
        let snapshot = self.snapshot();
        match self.apply_atomic_inner(commit) {
            Ok(()) => {
                self.commit_snapshot(snapshot);
                Ok(())
            }
            Err(e) => {
                self.rollback(snapshot);
                Err(e)
            }
        }
    }

    // Apply a commit + and fixpoint rebuilding + rule checking
    // This function is doing the actual work, after a dozen levels of indirection.
    fn apply_atomic_inner(&mut self, commit: Commit<'static>) -> Result<(), StoreError> {
        let commit = self.apply_commit_ready(commit)?;
        self.rebuild_to_fixpoint()?;
        self.check_rules()?;
        self.record_in_commit_graph(commit);
        Ok(())
    }

    /// Rebuild until a pass displaces no further ids, so a commit that merged
    /// nothing does no rebuild work at all.
    fn rebuild_to_fixpoint(&mut self) -> Result<(), StoreError> {
        while self.rowing.has_displaced() {
            self.rebuild_one()?;
        }
        Ok(())
    }

    fn rebuild_one(&mut self) -> Result<(), StoreError> {
        for tbl in self.tables.values_mut() {
            tbl.rebuild(&self.rowing, &self.id_packer);
        }

        // clear up the displaced table because the changes have all been staged.
        self.rowing.clear_displaced();
        let affected: Vec<TableOid> = self.tables.keys().copied().collect();
        self.apply_staged_ops(&affected)?;
        Ok(())
    }

    // Apply a commit with its deps checked to be satisfied
    // The commit data itself might still violate rules, primary key constraints, etc
    fn apply_commit_ready(&mut self, cmt: Commit<'static>) -> Result<Commit<'static>, StoreError> {
        // TODO resolved_ops need to decode data, there is code path which decodes
        // to get ops immediately after a commit has been encoded. Consider optimise this.

        // Here we check the commit and then apply it without worrying about the
        // store changing after checking and before applying.
        // This is ok because the model we have is that the store should only
        // materialise up to one particular commit, if violations are caused by
        // concurrent commits, then this would be resolved at merge time, not when
        // applying one of the concurrent commits.

        let PrecheckedCommit { ops, original } = self.precheck_commit(cmt)?;
        self.apply_commit_ops(ops)?;
        Ok(original)
    }

    /// Applying the data, assuming that it has passed the format checker, i.e.
    /// the data conforms the the schema type definitions.
    /// But it might not follow all the rule definitions, it might also violate
    /// primary key constraints after hashconsing
    fn apply_commit_ops(&mut self, ops: Vec<Op>) -> Result<(), StoreError> {
        let op_count = ops.len();
        let affected = self.stage_commit_ops(ops);
        self.apply_staged_ops(&affected)?;

        info!(op_count, "applied batch");
        Ok(())
    }

    // Stage all the commit ops into the table's pending state.
    fn stage_commit_ops(&mut self, ops: Vec<Op>) -> Vec<TableOid> {
        let mut affected = HashSet::new();
        for op in ops {
            let oid = op.table();
            let op = self.id_packer.pack_op(op);
            self.tables
                .get_mut(&oid)
                .expect("validated batch")
                .stage_update(op);
            affected.insert(oid);
        }
        affected.into_iter().collect()
    }

    fn apply_staged_ops(&mut self, tables: &[TableOid]) -> Result<(), StoreError> {
        for oid in tables {
            self.tables
                .get_mut(oid)
                .expect("staged table exists")
                .apply_staged_ops(&mut self.rowing)?;
        }
        self.rowing.apply_unions(&self.id_packer);
        Ok(())
    }

    // We do as much check as possible without making changes to the tables
    // including checks like:
    //  - data following schema format
    //  - no duplication of primary keys before hashconsing
    fn precheck_commit(&self, cmt: Commit<'static>) -> Result<PrecheckedCommit, StoreError> {
        // TODO perhaps use late resolution, i.e. not resolving any ids, and when
        // we resolve, immediately make them packed.
        let ops = cmt.resolved_ops(|path| {
            self.resolve_table(path)
                .and_then(|oid| self.table_meta(oid))
        })?;
        self.validate_commit_ops(&ops)?;
        Ok(PrecheckedCommit { ops, original: cmt })
    }

    // TODO also need to validate that ids in op is referring to an existing id
    fn validate_commit_ops(&self, ops: &[Op]) -> Result<(), StoreError> {
        let mut pending_pk: HashMap<TableOid, Vec<Vec<CellValue>>> = HashMap::new();

        for op in ops {
            let Op::Add { table, values, .. } = op;
            let t = self
                .table(*table)
                .ok_or(ValidationError::UnknownTableOid { oid: *table })?;
            t.validate_insert(values)?;

            // Check primary key conflicts within ops batch
            if let Some(key) = t.primary_key_values(values) {
                let keys = pending_pk.entry(*table).or_default();
                if keys.iter().any(|k| k == &key) {
                    return Err(ValidationError::DuplicatePrimaryKey.into());
                }
                keys.push(key);
            }
        }
        Ok(())
    }
}

struct PrecheckedCommit {
    ops: Vec<Op>,
    original: Commit<'static>,
}

/// For consumption by subduction
pub struct CommitChunk {
    pub hash: CommitHash,
    pub parents: Vec<CommitHash>,
    pub bytes: Vec<u8>,
}

impl Store {
    // for interfacing with subduction
    // TODO add fragments API

    pub fn commit_chunks_after(&self, have_heads: &[CommitHash]) -> Vec<CommitChunk> {
        self.commits_after(have_heads)
            .into_iter()
            .map(|commit| {
                let head = commit.hash();
                let parents = commit.deps.clone();
                let bytes = Chunk::from(commit).encoded();
                CommitChunk {
                    hash: head,
                    parents,
                    bytes,
                }
            })
            .collect()
    }

    /// Apply the bytes received by interpreting them as chunks, for syncing purposes
    pub fn apply_chunk_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<(), StoreError> {
        let commits = chunk_bytes
            .into_iter()
            .map(|bytes| Chunk::decode(&bytes))
            .map(|chunk| {
                chunk.and_then(|chunk| {
                    Commit::from_chunk(chunk, |path| {
                        self.resolve_table(path)
                            .and_then(|oid| self.table_meta(oid))
                    })
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        self.apply_commits(commits)
    }
}

impl Store {
    // for debugging and testing

    /// Dump every table in the store for debugging, in ascending [`TableOid`] order,
    /// separated by a blank line.
    pub fn dump(&self) -> String {
        let mut oids: Vec<TableOid> = self.tables.keys().copied().collect();
        oids.sort_unstable();
        oids.into_iter()
            .map(|oid| self.tables[&oid].dump(&self.id_packer))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[cfg(test)]
    fn apply_ops_and_rebuild(&mut self, ops: Vec<Op>) -> Result<(), StoreError> {
        self.apply_commit_ops(ops)?;
        self.rebuild_to_fixpoint()
    }

    // TODO remove this when we have schema level hashcons
    #[cfg(test)]
    pub(crate) fn set_hashcons_for_test(&mut self, path: &ir::Path, hashcons: bool) {
        let oid = self.resolve_table(path).expect("table exists");
        self.tables
            .get_mut(&oid)
            .expect("resolved tables are registered")
            .set_hashcons_for_test(hashcons);
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
