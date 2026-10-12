// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::{BTreeSet, HashMap, HashSet};

use coln_flir_rs::engine::delta::{PartialConsolidated, StoreDelta};
use coln_flir_rs::engine::packed::StoreTuple;
use coln_flir_rs::hash::CommitHash;
use coln_flir_rs::public::PublicScalarValue;
use tracing::info;

use crate::commit::Commit;
use crate::op::PublicOp;
use crate::rollback::Rollback;
use crate::store::error::{CommitApplyError, StoreError};
use crate::store::{Store, StoreSnapshot};
use crate::table::{TableMeta, TableOid, ValidationError};

impl Store {
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

    /// This will try to merge the `other` store as much as possible into this store
    // TODO need to rethink `merge` more carefully
    pub fn merge(&mut self, other: &Self) -> Result<Vec<CommitHash>, StoreError> {
        let commits = self.commits_added(other);
        self.apply_commits_unchecked(commits)?;
        Ok(self.heads())
    }

    /// Apply a single commit, respect its dependency.
    /// Return the commit if it cannot be applied due to missing deps
    pub fn apply_commit_unchecked(
        &mut self,
        commit: Commit<'static>,
    ) -> Result<Option<Commit<'static>>, StoreError> {
        // This needs to call apply_commits because it needs to do dependency check
        self.apply_commits_unchecked([commit]).map(|mut h| h.pop())
    }

    // For single commit, we just need the BatchInner structure. The caller AutoStore
    // will pass a store when calling its method.
    pub(crate) fn prepare_commit(
        &mut self,
        commit: Commit<'static>,
    ) -> Result<BatchInner<Unprepared>, StoreError> {
        let batch = self.prepare_commits(std::iter::once(commit))?;
        Ok(batch.inner)
    }

    pub fn prepare_commits(
        &mut self,
        commits: impl IntoIterator<Item = Commit<'static>>,
    ) -> Result<CommitBatch<'_, Unprepared>, StoreError> {
        let mut pending = HashMap::new();

        for commit in commits {
            let hash = commit.hash();
            if self.commits.contains(&hash) {
                continue;
            }

            if commit.is_root() {
                return Err(CommitApplyError::RootCommit(hash).into());
            }
            if commit.deps.is_empty() {
                return Err(CommitApplyError::DanglingCommit(hash).into());
            }

            if let Some(existing) = pending.get(&hash) {
                let existing: &Commit<'static> = existing;
                if *existing != commit {
                    return Err(CommitApplyError::ConflictPayload(hash).into());
                }
                continue;
            }

            pending.insert(hash, commit);
        }

        let mut unsatisfied = HashMap::new();
        let mut waiting_on: HashMap<CommitHash, Vec<CommitHash>> = HashMap::new();
        let mut ready = BTreeSet::new();

        for (hash, commit) in &pending {
            let mut count = 0;
            for dependency in &commit.deps {
                if self.commits.contains(dependency) {
                    continue;
                }

                count += 1;
                if pending.contains_key(dependency) {
                    waiting_on.entry(*dependency).or_default().push(*hash);
                } else {
                    tracing::info!(
                        commit_hash = %hash,
                        missing_dep = %dependency,
                        "blocking commit with a dependency that is neither applied nor pending"
                    );
                }
            }

            if count == 0 {
                ready.insert(*hash);
            } else {
                unsatisfied.insert(*hash, count);
            }
        }

        let inner = BatchInner {
            pending,
            ready,
            unsatisfied,
            waiting_on,
            prepared: Unprepared,
        };

        Ok(CommitBatch { store: self, inner })
    }

    /// Apply as many commits as possible respecting their dependencies. Return the
    /// commit hashes that are NOT applied, so the caller knows which ones they
    /// need to retry.
    pub fn apply_commits_unchecked(
        &mut self,
        commits: impl IntoIterator<Item = Commit<'static>>,
    ) -> Result<Vec<Commit<'static>>, StoreError> {
        let mut batch = self.prepare_commits(commits)?;
        while !batch.inner.ready.is_empty() {
            let prepared = batch.prepare_next()?.unwrap();
            batch = prepared.accept_prepared();
        }
        Ok(batch.into_pending())
    }

    /// Rebuild until a pass displaces no further ids, so a commit that merged
    /// nothing does no rebuild work at all.
    pub(super) fn rebuild_to_fixpoint(&mut self) -> Result<(), StoreError> {
        while self.rowing.has_displaced() {
            self.rebuild_one()?;
            tracing::debug!("finished one iteration of rebuilding");
        }
        Ok(())
    }

    fn rebuild_one(&mut self) -> Result<(), StoreError> {
        let affected = self.rebuild_tables();
        self.apply_staged_ops(&affected)
    }

    fn rebuild_tables(&mut self) -> Vec<TableOid> {
        for tbl in self.tables.values_mut() {
            tbl.rebuild(&self.rowing, &self.id_packer);
        }

        // clear up the displaced table because the changes have all been staged.
        self.rowing.clear_displaced();
        self.tables.keys().copied().collect()
    }

    /// Applying the data, assuming that it has passed the format checker, i.e.
    /// the data conforms the the schema type definitions.
    /// But it might not follow all the rule definitions, it might also violate
    /// primary key constraints after hashconsing
    pub(super) fn apply_commit_ops(&mut self, ops: Vec<PublicOp>) -> Result<(), StoreError> {
        let op_count = ops.len();
        let affected = self.stage_commit_ops(ops);
        self.apply_staged_ops(&affected)?;

        info!(op_count, "applied batch");
        Ok(())
    }

    // Stage all the commit ops into the table's pending state.
    fn stage_commit_ops(&mut self, ops: Vec<PublicOp>) -> Vec<TableOid> {
        let mut affected = HashSet::new();
        for op in ops {
            let oid = op.table();
            let op = self.id_packer.table_op(op);
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
    fn validate_commit_ops(&self, ops: &[PublicOp]) -> Result<(), StoreError> {
        let mut pending_pk: HashMap<TableOid, Vec<Vec<PublicScalarValue>>> = HashMap::new();

        for op in ops {
            let PublicOp::Add { table, values, .. } = op;
            let t = self
                .table(*table)
                .ok_or(ValidationError::UnknownTableOid { oid: *table })?;
            t.inner().validate_insert(&values[1..], &self.id_packer)?;

            // Check primary key conflicts within ops batch
            if let Some(key) = t.inner().primary_key_values(&values[1..]) {
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
    ops: Vec<PublicOp>,
    original: Commit<'static>,
}

pub trait PrepareState {}

pub struct Unprepared;
pub struct Prepared(PreparedCommit);

impl PrepareState for Unprepared {}
impl PrepareState for Prepared {}

pub struct CommitBatch<'a, S: PrepareState> {
    store: &'a mut Store,
    inner: BatchInner<S>,
}

impl<'a> CommitBatch<'a, Unprepared> {
    pub fn prepare_next(self) -> Result<Option<CommitBatch<'a, Prepared>>, StoreError> {
        let Self { store, inner } = self;
        Ok(inner
            .prepare_next(store)?
            .map(|inner| CommitBatch { store, inner }))
    }
}

impl<'a> CommitBatch<'a, Prepared> {
    pub fn accept_prepared(self) -> CommitBatch<'a, Unprepared> {
        let Self { store, inner } = self;
        let inner = inner.accept_prepared(store);
        CommitBatch { store, inner }
    }

    pub fn reject_prepared(self) -> CommitBatch<'a, Unprepared> {
        let Self { store, inner } = self;
        let inner = inner.reject_prepared(store);
        CommitBatch { store, inner }
    }

    pub fn prepared_commit(self) -> PreparedCommit {
        self.inner.prepared_commit()
    }

    pub fn prepared_hash(&self) -> CommitHash {
        self.inner.prepared_hash()
    }

    pub fn store_delta(&mut self) -> StoreDelta<PartialConsolidated, StoreTuple> {
        self.inner.store_delta()
    }
}

impl<S: PrepareState> CommitBatch<'_, S> {
    pub fn into_pending(self) -> Vec<Commit<'static>> {
        self.inner.into_pending()
    }
}

pub(crate) struct BatchInner<S: PrepareState> {
    pending: HashMap<CommitHash, Commit<'static>>,
    ready: BTreeSet<CommitHash>,
    unsatisfied: HashMap<CommitHash, usize>,
    waiting_on: HashMap<CommitHash, Vec<CommitHash>>,
    prepared: S,
}

impl BatchInner<Unprepared> {
    pub fn prepare_next(
        mut self,
        store: &mut Store,
    ) -> Result<Option<BatchInner<Prepared>>, StoreError> {
        let Some(hash) = self.ready.pop_first() else {
            return Ok(None);
        };
        let commit = self
            .pending
            .remove(&hash)
            .expect("a ready commit should also be pending");

        // Prechecking does not mutate the store. Keep the original commit so a
        // failed preparation can put it back into the batch for inspection or
        // retry.
        let PrecheckedCommit { ops, original } = match store.precheck_commit(commit.clone()) {
            Ok(prechecked) => prechecked,
            Err(error) => {
                self.pending.insert(hash, commit);
                self.ready.insert(hash);
                return Err(error);
            }
        };

        let sd: StoreDelta<PartialConsolidated, StoreTuple> = ops
            .iter()
            .map(|op| {
                let packed = store.id_packer.pack_op(op.clone());
                let meta = store
                    .table_meta(op.table())
                    .expect("prechecked commit with valid table oid");
                packed.map_oid(|_| meta.path.clone()).into()
            })
            .collect();

        let snapshot = store.snapshot();
        if let Err(error) = store
            .apply_commit_ops(ops)
            .and_then(|()| store.rebuild_to_fixpoint())
        {
            store.rollback_to(snapshot);
            self.pending.insert(hash, commit);
            self.ready.insert(hash);
            return Err(error);
        }

        Ok(Some(BatchInner {
            prepared: Prepared(PreparedCommit {
                commit: original,
                snapshot,
                sd,
            }),
            pending: self.pending,
            ready: self.ready,
            unsatisfied: self.unsatisfied,
            waiting_on: self.waiting_on,
        }))
    }
}

impl BatchInner<Prepared> {
    pub(crate) fn accept_prepared(mut self, store: &mut Store) -> BatchInner<Unprepared> {
        let Prepared(prepared) = self.prepared;
        let hash = prepared.commit.hash();
        store.commit(prepared.snapshot);
        store.record_in_commit_graph(prepared.commit);

        for dependent in self.waiting_on.remove(&hash).unwrap_or_default() {
            let is_ready = {
                let count = self
                    .unsatisfied
                    .get_mut(&dependent)
                    .expect("a waiting commit should have unsatisfied dependencies");
                *count -= 1;
                *count == 0
            };
            if is_ready {
                self.unsatisfied.remove(&dependent);
                self.ready.insert(dependent);
            }
        }

        BatchInner {
            prepared: Unprepared,
            pending: self.pending,
            ready: self.ready,
            unsatisfied: self.unsatisfied,
            waiting_on: self.waiting_on,
        }
    }

    pub(crate) fn reject_prepared(mut self, store: &mut Store) -> BatchInner<Unprepared> {
        let Prepared(prepared) = self.prepared;
        store.rollback_to(prepared.snapshot);
        self.waiting_on.remove(&prepared.commit.hash());

        BatchInner {
            prepared: Unprepared,
            pending: self.pending,
            ready: self.ready,
            unsatisfied: self.unsatisfied,
            waiting_on: self.waiting_on,
        }
    }

    pub(crate) fn prepared_commit(self) -> PreparedCommit {
        let Prepared(prepared) = self.prepared;
        prepared
    }

    pub(crate) fn prepared_hash(&self) -> CommitHash {
        let Prepared(prepared) = &self.prepared;
        prepared.commit.hash()
    }

    /// Takes the store_delta out of the BatchInner, so can only be called once!
    pub(super) fn store_delta(&mut self) -> StoreDelta<PartialConsolidated, StoreTuple> {
        let Prepared(prepared) = &mut self.prepared;
        std::mem::take(&mut prepared.sd)
    }
}

impl<S: PrepareState> BatchInner<S> {
    pub fn into_pending(self) -> Vec<Commit<'static>> {
        self.pending.into_values().collect()
    }
}

// We cannot impl Drop for CommitBatch, so the user would have to do the cleanup

pub struct PreparedCommit {
    pub(crate) commit: Commit<'static>,
    pub(crate) snapshot: StoreSnapshot,
    pub(crate) sd: StoreDelta<PartialConsolidated, StoreTuple>,
}

#[cfg(test)]
mod tests {
    use coln_flir_rs::engine::packed::{StoreScalarValue, StoreTuple};
    use coln_flir_rs::ir::Path;
    use rstest::rstest;

    use super::*;
    use crate::IdLookup;
    use crate::test_utils::{commit_int_store, single_int_store};
    use crate::txn::rw::StoreRead;

    #[test]
    fn resolved_op_validates_and_stores_schema_values_after_the_row_id() {
        use crate::op::PendingOp;
        use crate::test_utils::{int_schema, row_id_from};
        use coln_flir_rs::engine::tx::PendingRowId;

        let mut store = Store::new();
        let path = Path::from("T");
        let table = store
            .create_table(path.clone(), int_schema(vec!["value"], Some(vec![0])))
            .expect("create table");
        let row_id = row_id_from(1, 0);
        let op = PendingOp::add(PendingRowId(0), table, vec![42i32].into()).resolve(row_id.commit);
        assert_eq!(op.id(), row_id);
        assert_eq!(op.table(), table);
        store
            .validate_commit_ops(std::slice::from_ref(&op))
            .expect("valid row");

        let duplicate =
            PendingOp::add(PendingRowId(1), table, vec![42i32].into()).resolve(row_id.commit);
        assert!(matches!(
            store.validate_commit_ops(&[op.clone(), duplicate]),
            Err(StoreError::Validation(ValidationError::DuplicatePrimaryKey))
        ));

        store.apply_commit_ops(vec![op]).expect("apply row");
        let packed_id = store.id_lookup().packed(&row_id).expect("packed row id");
        assert_eq!(
            store.scan_table(&path).expect("table"),
            vec![StoreTuple::from_id_values(
                packed_id,
                vec![StoreScalarValue::I32(42)]
            )]
        );
    }

    #[rstest]
    fn prepare_next_tentatively_applies_the_next_ready_commit(
        #[from(commit_int_store)]
        #[with(42)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (source, hash) = source;
        let commit = source.commit_by_hash(&hash).expect("source commit").clone();
        let batch = target.prepare_commits([commit]).expect("prepare batch");

        let mut prepared = batch
            .prepare_next()
            .expect("prepare succeeds")
            .expect("ready commit");

        assert_eq!(prepared.prepared_hash(), hash);
        let deltas = prepared.store_delta().clone().into_table_deltas();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].for_entity(), &Path::from("T"));
        assert_eq!(deltas[0].delta().len(), 1);
        let row = &deltas[0].delta()[0];
        assert_eq!(row.zweight(), 1);
        assert_eq!(
            row.clone().into_tuple().values(),
            [StoreScalarValue::I32(42)]
        );
        assert!(!prepared.store.commits.contains(&hash));

        let batch = prepared.reject_prepared();
        assert!(batch.inner.ready.is_empty());
        let pending = batch.into_pending();
        assert!(pending.is_empty());
        assert!(
            target
                .prepare_commits(pending)
                .expect("prepare empty batch")
                .prepare_next()
                .expect("batch exhausted")
                .is_none()
        );
        assert_eq!(target.scan_table(&Path::from("T")).expect("table"), vec![]);
    }

    #[rstest]
    fn accepting_a_commit_makes_its_dependant_ready(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, first) = source;
        let second = crate::test_utils::commit_int(&mut source, 2);
        let commits = vec![
            source
                .commit_by_hash(&second)
                .expect("second commit")
                .clone(),
            source.commit_by_hash(&first).expect("first commit").clone(),
        ];
        let batch = target.prepare_commits(commits).expect("prepare batch");

        let prepared_first = batch
            .prepare_next()
            .expect("prepare first")
            .expect("first is ready");
        assert_eq!(prepared_first.prepared_hash(), first);
        let batch = prepared_first.accept_prepared();

        let prepared_second = batch
            .prepare_next()
            .expect("prepare second")
            .expect("second became ready");
        assert_eq!(prepared_second.prepared_hash(), second);
        let batch = prepared_second.accept_prepared();

        assert!(batch.inner.ready.is_empty());
        let pending = batch.into_pending();
        assert!(pending.is_empty());
        assert!(
            target
                .prepare_commits(pending)
                .expect("prepare empty batch")
                .prepare_next()
                .expect("batch exhausted")
                .is_none()
        );
        assert_eq!(target.scan_table(&Path::from("T")).expect("table").len(), 2);
        assert_eq!(target.heads(), vec![second]);
    }

    #[rstest]
    fn rejecting_a_commit_keeps_its_dependant_blocked(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, first) = source;
        let second = crate::test_utils::commit_int(&mut source, 2);
        let commits = source.commits_after(&target.heads());
        let batch = target.prepare_commits(commits).expect("prepare batch");

        let prepared_first = batch
            .prepare_next()
            .expect("prepare first")
            .expect("first is ready");
        assert_eq!(prepared_first.prepared_hash(), first);
        let batch = prepared_first.reject_prepared();

        assert!(batch.inner.ready.is_empty());
        let pending = batch.into_pending();
        let pending_hashes = pending
            .iter()
            .map(|commit| commit.hash())
            .collect::<Vec<_>>();
        assert_eq!(pending_hashes, vec![second]);
        assert!(
            target
                .prepare_commits(pending)
                .expect("prepare blocked batch")
                .prepare_next()
                .expect("dependent remains blocked")
                .is_none()
        );
        assert_eq!(target.scan_table(&Path::from("T")).expect("table"), vec![]);
    }

    #[rstest]
    fn extracted_prepared_commit_can_be_explicitly_rolled_back(
        #[from(commit_int_store)]
        #[with(7)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (source, hash) = source;
        let commit = source.commit_by_hash(&hash).expect("source commit").clone();
        let batch = target.prepare_commits([commit]).expect("prepare batch");

        let prepared = batch
            .prepare_next()
            .expect("prepare succeeds")
            .expect("commit is ready")
            .prepared_commit();

        assert_eq!(prepared.commit.hash(), hash);
        let deltas = prepared.sd.into_table_deltas();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].for_entity(), &Path::from("T"));
        assert_eq!(deltas[0].delta().len(), 1);
        let row = &deltas[0].delta()[0];
        assert_eq!(row.zweight(), 1);
        assert_eq!(
            row.clone().into_tuple().values(),
            [StoreScalarValue::I32(7)]
        );
        assert_eq!(target.scan_table(&Path::from("T")).expect("table").len(), 1);
        assert!(target.commit_by_hash(&hash).is_none());

        target.rollback_to(prepared.snapshot);

        assert_eq!(target.scan_table(&Path::from("T")).expect("table"), vec![]);
        assert!(target.commit_by_hash(&hash).is_none());
    }
}
