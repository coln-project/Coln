// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dependency scheduling and tentative application of commit batches.

use std::collections::{BTreeSet, HashMap};
use std::marker::PhantomData;

use coln_flir_rs::engine::delta::{Consolidated, PartialConsolidated, StoreDelta};
use coln_flir_rs::engine::packed::StoreTuple;
use coln_flir_rs::hash::CommitHash;
use parking_lot::RwLockWriteGuard;

use crate::commit::DataCommit;
use crate::rollback::Rollback;
use crate::store::data::{CheckedOpCommit, DataSnapshot};
use crate::store::error::StoreError;
use crate::store::{Catalog, ReadTxn, StoreData};
use crate::table::ValidationError;

pub trait PrepareState {}

pub struct Unprepared;
pub struct Prepared;

impl PrepareState for Unprepared {}
impl PrepareState for Prepared {}

/// A `CommitBatch` presents a batch of commits that will be applied to the store.
///
/// The batch holds the store's write lock for its whole lifetime and moves
/// between two typestates, one commit at a time:
///
/// ```text
///                          CommitBatch::new
///                                 │
///                                 ▼
///                  ┌──────────────────────────────┐
///        ┌────────▶│          Unprepared          │◀────────┐
///        │         └──────────────────────────────┘         │
///        │                        │                         │
///        │                        │ prepare_next()          │
///        │                        │   -> Ok(Some(_))        │
///        │                        ▼                         │
///        │         ┌──────────────────────────────┐         │
///        │         │           Prepared           │         │
///        │         └──────────────────────────────┘         │
///        │                │                │                │
///        └────────────────┘                └────────────────┘
///         accept_prepared()                reject_prepared()
/// ```
///
/// - `prepare_next` pops the next ready commit, snapshots the store, and
///   tentatively applies the commit and rebuilds derived data. The snapshot
///   and the commit's [`StoreDelta`] are held until the commit is accepted or
///   rejected.
/// - `accept_prepared` keeps the tentative changes, records the commit in the
///   commit graph, and marks dependants whose dependencies are now all
///   applied as ready.
/// - `reject_prepared` rolls the store back to the snapshot and discards the
///   commit. Its dependants stay pending and never become ready in this batch.
///
/// Exits from the cycle:
///
/// - `Unprepared::prepare_next` returning `Ok(None)` means no commit is
///   ready. Returning `Err` discards the commit that failed to apply. Both
///   consume the batch.
/// - Dropping the batch in either state returns all remaining commits to
///   `StoreData::pending_commits`. In the `Prepared` state the store is first
///   rolled back and the prepared commit is returned to pending as well.
pub struct CommitBatch<'a, S: PrepareState> {
    guard: BatchGuard<'a>,
    state: PhantomData<S>,
}

impl<S: PrepareState> CommitBatch<'_, S> {
    /// Read the current data without acquiring another lock.
    pub fn read(&self) -> ReadTxn<'_> {
        ReadTxn::borrowed(self.guard.catalog, &self.guard.store_data)
    }
}

impl<'a> CommitBatch<'a, Unprepared> {
    /// Prepares a [`CommitBatch`] for applying those commits.
    /// To make sure that we don't accidentally drop the pending_commits from the store,
    /// this function cannot fail.
    ///
    /// Therefore we assume that the new_commits has already been checked
    /// Also we take `new_commits` and will append whatever is pending to it.
    pub(super) fn new(
        catalog: &'a Catalog,
        mut store_data: RwLockWriteGuard<'a, StoreData>,
        new_commits: impl IntoIterator<Item = DataCommit>,
        drain_sync: bool,
    ) -> Self {
        let pending: HashMap<CommitHash, DataCommit> = if drain_sync {
            new_commits
                .into_iter()
                .chain(std::mem::take(&mut store_data.pending_commits))
                .filter(|c| !store_data.commits.contains(&c.hash()))
                .map(|c| (c.hash(), c))
                .collect()
        } else {
            new_commits.into_iter().map(|c| (c.hash(), c)).collect()
        };

        let mut unsatisfied = HashMap::new();
        let mut waiting_on: HashMap<CommitHash, Vec<CommitHash>> = HashMap::new();
        let mut ready = BTreeSet::new();

        for (hash, commit) in &pending {
            let mut count = 0;
            for dependency in &commit.deps {
                if store_data.commits.contains(dependency) {
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

        let guard = BatchGuard {
            catalog,
            store_data,
            queue: BatchQueue {
                pending,
                ready,
                unsatisfied,
                waiting_on,
            },
            prepared: None,
        };
        Self {
            guard,
            state: PhantomData,
        }
    }

    pub fn prepare_next(mut self) -> Result<Option<CommitBatch<'a, Prepared>>, StoreError> {
        if !self.guard.prepare_next()? {
            return Ok(None);
        }

        Ok(Some(CommitBatch {
            guard: self.guard,
            state: PhantomData,
        }))
    }

    pub fn has_ready(&self) -> bool {
        !self.guard.queue.ready.is_empty()
    }

    pub fn size(&self) -> usize {
        self.guard.queue.pending_len()
    }

    pub(crate) fn apply_plain(mut self) -> Result<(), StoreError> {
        while let Some(prepared) = self.prepare_next()? {
            self = prepared.accept_prepared();
        }
        Ok(())
    }
}

impl<'a> CommitBatch<'a, Prepared> {
    pub fn accept_prepared(mut self) -> CommitBatch<'a, Unprepared> {
        self.guard.accept_prepared();
        CommitBatch {
            guard: self.guard,
            state: PhantomData,
        }
    }

    pub fn reject_prepared(mut self) -> CommitBatch<'a, Unprepared> {
        self.guard.reject_prepared();

        CommitBatch {
            guard: self.guard,
            state: PhantomData,
        }
    }

    pub fn prepared_hash(&self) -> CommitHash {
        self.guard.prepared_hash()
    }

    pub fn store_delta(&mut self) -> StoreDelta<PartialConsolidated, StoreTuple> {
        self.guard.store_delta()
    }

    /// Apply derived data under the prepared commit's write lock and snapshot.
    pub fn apply_derived_view(
        &mut self,
        derived: StoreDelta<Consolidated, StoreTuple>,
    ) -> Result<(), StoreError> {
        for td in derived.into_table_deltas() {
            let oid = self.guard.catalog.resolve_table(td.for_entity()).ok_or(
                ValidationError::UnknownTable {
                    path: td.for_entity().clone(),
                },
            )?;
            let data = &mut *self.guard.store_data;
            let table = data.tables.get_mut(&oid).expect("resolved correct table");

            table.stage_updates(td.into_parts().1);

            table.apply_staged_ops(&mut data.rowing)?;
        }
        Ok(())
    }
}

// A BatchGuard holds a prepared commit and when dropped, this commit would be
// put back into store's pending_commits
struct BatchGuard<'a> {
    catalog: &'a Catalog,
    store_data: RwLockWriteGuard<'a, StoreData>,
    queue: BatchQueue,
    prepared: Option<PreparedCommit>,
}

impl Drop for BatchGuard<'_> {
    fn drop(&mut self) {
        if let Some(prepared) = self.prepared.take() {
            self.store_data.rollback_to(prepared.snapshot);

            // The current commit will be added back to pending commits for next time
            self.queue
                .pending
                .insert(prepared.commit.hash(), prepared.commit);
        }

        self.store_data
            .pending_commits
            .extend(std::mem::take(&mut self.queue.pending).into_values());
    }
}

impl BatchGuard<'_> {
    fn prepare_next(&mut self) -> Result<bool, StoreError> {
        let Some(hash) = self.queue.ready.pop_first() else {
            return Ok(false);
        };

        let commit = self
            .queue
            .pending
            .remove(&hash)
            .expect("a ready commit must also be in pending");

        let CheckedOpCommit { ops, original } =
            self.store_data.check_commit_ops(commit, self.catalog)?;
        let sd: StoreDelta<PartialConsolidated, StoreTuple> = ops
            .iter()
            .map(|op| {
                let packed = self.store_data.id_packer.pack_op(op.clone());
                let meta = self
                    .catalog
                    .table_meta(op.table())
                    .expect("prechecked commit with valid table oid");
                packed.map_oid(|_| meta.path.clone()).into()
            })
            .collect();

        let snapshot = self.store_data.snapshot();
        if let Err(error) = self
            .store_data
            .apply_commit_ops(ops)
            .and_then(|()| self.store_data.rebuild_to_fixpoint())
        {
            self.store_data.rollback_to(snapshot);
            return Err(error);
        }

        let prepared = PreparedCommit {
            commit: original,
            snapshot,
            sd,
        };

        // Preparation succeeded. Transfer ownership from the queue
        // to the active prepared slot.
        self.prepared = Some(prepared);

        Ok(true)
    }

    fn accept_prepared(&mut self) {
        let prepared = self
            .prepared
            .take()
            .expect("have called prepare_next to preapare the commit");
        let hash = prepared.accept(&mut self.store_data);
        self.queue.mark_applied(&hash);
    }

    fn reject_prepared(&mut self) {
        let prepared = self
            .prepared
            .take()
            .expect("have called prepare_next to preapare the commit");
        let hash = prepared.commit.hash();
        prepared.reject(&mut self.store_data);
        self.queue.mark_rejected(&hash);
    }

    fn prepared_hash(&self) -> CommitHash {
        self.prepared
            .as_ref()
            .expect("have called prepare_next to prepare the commit")
            .prepared_hash()
    }

    /// Takes the store_delta out of the BatchInner, so can only be called once!
    fn store_delta(&mut self) -> StoreDelta<PartialConsolidated, StoreTuple> {
        let prepared = self.prepared.as_mut().expect("have called prepare_next");
        std::mem::take(&mut prepared.sd)
    }
}

struct BatchQueue {
    // All commits to be applied, ready or not
    pending: HashMap<CommitHash, DataCommit>,
    // commits with no pending deps
    ready: BTreeSet<CommitHash>,
    // number of deps not yet accepted
    unsatisfied: HashMap<CommitHash, usize>,
    // commits -> commits waiting for that commit, reverse map
    waiting_on: HashMap<CommitHash, Vec<CommitHash>>,
}

impl BatchQueue {
    fn mark_applied(&mut self, hash: &CommitHash) {
        for dependent in self.waiting_on.remove(hash).unwrap_or_default() {
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
    }

    fn mark_rejected(&mut self, hash: &CommitHash) {
        self.waiting_on.remove(hash);
    }

    fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

pub(crate) struct PreparedCommit {
    commit: DataCommit,
    snapshot: DataSnapshot,
    sd: StoreDelta<PartialConsolidated, StoreTuple>,
}

impl PreparedCommit {
    pub(crate) fn prepared_hash(&self) -> CommitHash {
        self.commit.hash()
    }

    pub(super) fn accept(self, store_data: &mut RwLockWriteGuard<'_, StoreData>) -> CommitHash {
        let hash = self.commit.hash();
        store_data.commit(self.snapshot);
        store_data.record_in_commit_graph(self.commit);
        hash
    }

    pub(super) fn reject(self, store: &mut RwLockWriteGuard<'_, StoreData>) {
        store.rollback_to(self.snapshot);
    }
}

#[cfg(test)]
mod tests {
    use coln_flir_rs::engine::packed::StoreScalarValue;
    use coln_flir_rs::ir::Path;
    use rstest::rstest;

    use super::*;
    use crate::store::Store;
    use crate::table::ValidationError;
    use crate::test_utils::{
        commit_int, commit_int_store, expect_data_chunk, int_schema, single_int_store,
    };

    #[test]
    fn precheck_failure_discards_failed_commit_and_preserves_remaining_queue() {
        let schema = int_schema(vec!["value"], Some(vec![0]));
        let mut source = single_int_store(schema.clone());
        let mut target = single_int_store(schema);
        let first = commit_int(&mut source, 1);
        let conflicting = commit_int(&mut source, 2);
        let child = commit_int(&mut source, 3);
        commit_int(&mut target, 2);
        target
            .apply_commits_plain([source
                .begin_read()
                .commit_by_hash(&first)
                .expect("parent")
                .clone()])
            .expect("apply parent");
        let heads = target.begin_read().heads();
        let rows = target
            .begin_read()
            .scan_table(&Path::from("T"))
            .expect("table");

        let batch = target
            .prepare_commits(
                [
                    source
                        .begin_read()
                        .commit_by_hash(&conflicting)
                        .expect("conflicting commit")
                        .clone(),
                    source
                        .begin_read()
                        .commit_by_hash(&child)
                        .expect("child")
                        .clone(),
                ],
                true,
            )
            .expect("batch");
        assert!(matches!(
            batch.prepare_next(),
            Err(StoreError::Validation(ValidationError::DuplicatePrimaryKey))
        ));

        assert_eq!(target.begin_read().heads(), heads);
        assert_eq!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table"),
            rows
        );
        assert!(target.begin_read().commit_by_hash(&conflicting).is_none());
        target
            .apply_commits_plain([])
            .expect("failed commit must not be retried");
        let batch = target.prepare_commits([], true).expect("remaining queue");
        assert_eq!(batch.size(), 1);
        assert!(!batch.has_ready());
    }

    #[rstest]
    fn dropping_unprepared_batch_preserves_new_and_previously_buffered_commits(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, first) = source;
        let second = commit_int(&mut source, 2);
        let heads = target.begin_read().heads();
        drop(
            target
                .prepare_commits(
                    [source
                        .begin_read()
                        .commit_by_hash(&second)
                        .expect("child")
                        .clone()],
                    true,
                )
                .expect("buffer child"),
        );
        assert!(target.begin_read().data.has_pending());

        let batch = target
            .prepare_commits(
                [source
                    .begin_read()
                    .commit_by_hash(&first)
                    .expect("parent")
                    .clone()],
                true,
            )
            .expect("combine parent with buffered child");
        assert!(batch.has_ready());
        assert_eq!(batch.size(), 2);
        drop(batch);

        assert_eq!(target.begin_read().heads(), heads);
        assert!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table")
                .is_empty()
        );
        target
            .apply_commits_plain([])
            .expect("retry restored queue");
        assert_eq!(target.begin_read().heads(), vec![second]);
        assert_eq!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table")
                .len(),
            2
        );
        assert!(!target.begin_read().data.has_pending());
    }

    #[rstest]
    fn dropping_prepared_batch_after_taking_delta_rolls_back_and_preserves_commits(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, first) = source;
        let second = commit_int(&mut source, 2);
        let heads = target.begin_read().heads();
        let batch = target
            .prepare_commits(
                source
                    .begin_read()
                    .chunks_after(&heads)
                    .into_iter()
                    .map(expect_data_chunk),
                true,
            )
            .expect("batch");
        let mut prepared = batch
            .prepare_next()
            .expect("prepare parent")
            .expect("ready parent");
        assert_eq!(prepared.prepared_hash(), first);
        assert_eq!(prepared.store_delta().size(), 1);
        drop(prepared);

        assert_eq!(target.begin_read().heads(), heads);
        assert!(target.begin_read().commit_by_hash(&first).is_none());
        assert!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table")
                .is_empty()
        );
        assert!(target.begin_read().data.has_pending());
        target
            .apply_commits_plain([])
            .expect("retry parent and child");
        assert_eq!(target.begin_read().heads(), vec![second]);
        assert_eq!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table")
                .len(),
            2
        );
        assert!(!target.begin_read().data.has_pending());
    }

    #[rstest]
    fn prepare_next_tentatively_applies_the_next_ready_commit(
        #[from(commit_int_store)]
        #[with(42)]
        source: (Store, CommitHash),
        #[from(single_int_store)] target: Store,
    ) {
        let (source, hash) = source;
        let commit = source
            .begin_read()
            .commit_by_hash(&hash)
            .expect("source commit")
            .clone();
        let batch = target
            .prepare_commits([commit], false)
            .expect("prepare batch");

        let mut prepared = batch
            .prepare_next()
            .expect("prepare succeeds")
            .expect("ready commit");

        assert_eq!(prepared.prepared_hash(), hash);
        let deltas = prepared.store_delta().into_table_deltas();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].for_entity(), &Path::from("T"));
        assert_eq!(deltas[0].delta().len(), 1);
        let row = &deltas[0].delta()[0];
        assert_eq!(row.zweight(), 1);
        assert_eq!(
            row.clone().into_tuple().values(),
            [StoreScalarValue::I32(42)]
        );
        assert_eq!(
            prepared
                .read()
                .table_at(&Path::from("T"))
                .expect("table")
                .row_count(),
            1
        );
        assert!(!prepared.guard.store_data.commits.contains(&hash));

        let batch = prepared.reject_prepared();
        assert!(!batch.has_ready());
        assert_eq!(batch.size(), 0);
        assert!(batch.prepare_next().expect("batch exhausted").is_none());
        assert!(target.begin_read().data.pending_commits.is_empty());
        assert_eq!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table"),
            vec![]
        );
    }

    #[rstest]
    fn accepting_a_commit_makes_its_dependant_ready(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] target: Store,
    ) {
        let (mut source, first) = source;
        let second = crate::test_utils::commit_int(&mut source, 2);
        let commits = vec![
            source
                .begin_read()
                .commit_by_hash(&second)
                .expect("second commit")
                .clone(),
            source
                .begin_read()
                .commit_by_hash(&first)
                .expect("first commit")
                .clone(),
        ];
        let batch = target
            .prepare_commits(commits, false)
            .expect("prepare batch");

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

        assert!(!batch.has_ready());
        assert_eq!(batch.size(), 0);
        assert!(batch.prepare_next().expect("batch exhausted").is_none());
        assert!(target.begin_read().data.pending_commits.is_empty());
        assert_eq!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table")
                .len(),
            2
        );
        assert_eq!(target.begin_read().heads(), vec![second]);
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
        let commits = source
            .begin_read()
            .chunks_after(&target.begin_read().heads())
            .into_iter()
            .map(expect_data_chunk);
        let batch = target
            .prepare_commits(commits, false)
            .expect("prepare batch");

        let prepared_first = batch
            .prepare_next()
            .expect("prepare first")
            .expect("first is ready");
        assert_eq!(prepared_first.prepared_hash(), first);
        let batch = prepared_first.reject_prepared();

        assert!(!batch.has_ready());
        assert_eq!(batch.size(), 1);
        drop(batch);
        let pending_hashes = target
            .begin_read()
            .data
            .pending_commits
            .iter()
            .map(|commit| commit.hash())
            .collect::<Vec<_>>();
        assert_eq!(pending_hashes, vec![second]);
        let pending = std::mem::take(&mut target.data.get_mut().pending_commits);
        assert!(
            target
                .prepare_commits(pending, false)
                .expect("prepare blocked batch")
                .prepare_next()
                .expect("dependent remains blocked")
                .is_none()
        );
        assert_eq!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table"),
            vec![]
        );
    }

    #[rstest]
    fn prepared_commit_can_be_explicitly_rolled_back_after_taking_delta(
        #[from(commit_int_store)]
        #[with(7)]
        source: (Store, CommitHash),
        #[from(single_int_store)] target: Store,
    ) {
        let (source, hash) = source;
        let commit = source
            .begin_read()
            .commit_by_hash(&hash)
            .expect("source commit")
            .clone();
        let batch = target
            .prepare_commits([commit], false)
            .expect("prepare batch");

        let mut prepared = batch
            .prepare_next()
            .expect("prepare succeeds")
            .expect("commit is ready");

        assert_eq!(prepared.prepared_hash(), hash);
        let deltas = prepared.store_delta().into_table_deltas();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].for_entity(), &Path::from("T"));
        assert_eq!(deltas[0].delta().len(), 1);
        let row = &deltas[0].delta()[0];
        assert_eq!(row.zweight(), 1);
        assert_eq!(
            row.clone().into_tuple().values(),
            [StoreScalarValue::I32(7)]
        );
        assert_eq!(
            prepared
                .read()
                .table_at(&Path::from("T"))
                .expect("table")
                .row_count(),
            1
        );
        assert!(!prepared.guard.store_data.commits.contains(&hash));

        let batch = prepared.reject_prepared();
        assert!(!batch.has_ready());
        assert_eq!(batch.size(), 0);
        drop(batch);

        assert_eq!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table"),
            vec![]
        );
        assert!(target.begin_read().commit_by_hash(&hash).is_none());
    }
}
