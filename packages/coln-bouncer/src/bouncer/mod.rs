// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The top level APIs exposed by the coln backend database to the user and FFI

mod persist;

use coln_flir_rs::{
    engine::{
        packed::StoreTuple,
        query::QueryTuple,
        schema::ColnDef,
        tx::{TxRowId, TxTuple},
    },
    hash::CommitHash,
    ir::{self},
    public::{PublicRowId, PublicTuple},
    query::WhereClause,
};
use coln_query::api::{
    ColnQuery,
    transaction::{Committed, TryCommitErr, TryCommitOk, Tx},
    violations::ViolationsDelta,
};
use coln_store::{
    store::{
        IdLookup, Store,
        auto::AutoStore,
        commit::{CommitBatch, CommitChunk, Unprepared},
    },
    txn::rw::{StoreRead, StoreWrite},
};

use crate::{
    error::{BouncerError, RuleViolation},
    rw::{DbRead, DbWrite},
};

pub struct Bouncer {
    store: AutoStore,
    query: ColnQuery,
}

impl Bouncer {
    /// Creates a new [`Bouncer`] from the IR `ir` and coln source `coln_def`.
    pub fn try_new(ir: ir::FlatRealm, coln_def: ColnDef) -> Result<Self, BouncerError> {
        let store = AutoStore::try_from_ir(&ir, coln_def)?;
        let query = ColnQuery::init(&ir)?;
        Ok(Bouncer { store, query })
    }

    /// # Panics
    ///
    /// If the initialisation fails
    pub fn new(ir: ir::FlatRealm, coln_def: ColnDef) -> Self {
        Self::try_new(ir, coln_def).expect("init success")
    }

    fn new_from_store_query(store: Store, query: ColnQuery) -> Self {
        Bouncer {
            store: store.auto(),
            query,
        }
    }
}

impl Bouncer {
    pub fn coln_def(&self) -> Result<ColnDef, BouncerError> {
        Ok(self.store.inner().coln_def()?)
    }

}

impl Bouncer {
    /// Returns the list of commits that have no dependents
    pub fn heads(&self) -> Vec<CommitHash> {
        self.store.inner().heads()
    }

    /// Retrieve the commits that after the `have_heads`, i.e. commits that 
    /// (transitively dependent on them.
    pub fn commit_chunks_after(&self, have_heads: &[CommitHash]) -> Vec<CommitChunk> {
        self.store.inner().commit_chunks_after(have_heads)
    }

    /// Given some bytes that has already been separated by commit boundary
    /// attempt to apply them to the database.
    /// Returns a list of monitored constraint violations, if any.
    pub fn apply_chunk_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<Vec<ViolationsDelta>, BouncerError> {
        let batch = self.store.inner_mut().prepare_commit_bytes(chunk_bytes)?;

        Self::apply_batch(&mut self.query, batch)
    }

    /// Attempt to build a bouncer from commit chunks from scratch, assuming that
    /// the input `chunk_bytes` contains a valid root commit.
    pub fn try_from_commit_bytes(
        bytes: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Result<(Self, Vec<ViolationsDelta>), BouncerError> {
        let (store, commits) = Store::try_from_commits_bytes(bytes)?;
        // nothing to drain here
        let query = ColnQuery::init(store.ir())?;
        let mut bouncer = Bouncer::new_from_store_query(store, query);

        let batch = bouncer.store.inner_mut().prepare_commits(commits, false)?;
        let violations = Bouncer::apply_batch(&mut bouncer.query, batch)?;
        Ok((bouncer, violations))
    }

    fn apply_batch(
        query: &mut ColnQuery,
        mut batch: CommitBatch<'_, Unprepared>,
    ) -> Result<Vec<ViolationsDelta>, BouncerError> {
        let mut monitored_constraints = Vec::new();

        while batch.has_ready() {
            let mut prepared = batch.prepare_next()?.unwrap();

            let cmt_hash = prepared.prepared_hash();
            let store_delta = prepared.store_delta().map(|td| td.map(QueryTuple::from));
            let num_ops = store_delta.size() as u32;

            let tx = Tx::new(store_delta);

            match tx.try_commit(query) {
                Ok(TryCommitOk::Pending(pending)) => {
                    let committed = pending.commit()?;
                    batch = prepared.accept_prepared_with(|store| {
                        let monitored =
                            Self::commit_to_store(store, committed, (cmt_hash, num_ops))
                                .monitored_constraints;
                        monitored_constraints.push(monitored);
                    })
                }
                Ok(TryCommitOk::Rejected(mut rejected)) => {
                    // reject this commit, but restore whatever that has not been finished
                    prepared.reject_prepared();
                    let violations = rejected.take_hard_violations();
                    return Err(RuleViolation::HardViolation(violations).into());
                }
                Err(TryCommitErr::TxApplyError(_err)) | Err(TryCommitErr::RollbackError(_err)) => {
                    panic!("coln query encounters unrecoverable error")
                }
            }
        }

        Ok(monitored_constraints)
    }
}

impl Bouncer {
    /// Helper to unpack a tuple from store.
    /// This method lives here rather than coln-flir-rs because it is requires
    /// an API from coln-store
    fn unpack_store_tuple(&self, st: StoreTuple) -> PublicTuple {
        let id_lookup = self.store.id_lookup();
        PublicTuple::from_store(st, |packed| {
            id_lookup
                .unpacked(&packed)
                .expect("tuples from store can be unpacked")
        })
    }
}

impl DbRead for Bouncer {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<PublicTuple>> {
        Some(
            self.store
                .scan_table(table)?
                .into_iter()
                .map(|st| self.unpack_store_tuple(st))
                .collect(),
        )
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &PublicRowId) -> Option<PublicTuple> {
        // An unknown commit hash means the row cannot exist in this store.
        let packed = self.store.id_lookup().packed(row_id)?;
        self.store
            .row_by_id(table, &packed)
            .map(|st| self.unpack_store_tuple(st))
    }

    fn all_proj(
        &self,
        query: &WhereClause,
        select: &[u32],
    ) -> Result<Vec<PublicTuple>, BouncerError> {
        Ok(self
            .store
            .all_proj(query, select)?
            .into_iter()
            .map(|st| self.unpack_store_tuple(st))
            .collect())
    }

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<PublicRowId>, BouncerError> {
        let id_lookup = self.store.id_lookup();
        Ok(self
            .store
            .all_row_id(query)?
            .into_iter()
            .map(|packed| {
                id_lookup
                    .unpacked(&packed)
                    .expect("row ids from store can be unpacked")
            })
            .collect())
    }
}

pub struct CommitResult {
    pub monitored_constraints: ViolationsDelta,
    pub cmt_hash: CommitHash,
}

impl Bouncer {
    // This helper should not fail (return an error) because at this point we
    // are committed to add the data.
    fn commit_to_store(
        store: &mut Store,
        mut committed: Tx<Committed>,
        (cmt_hash, mut num_ops): (CommitHash, u32),
    ) -> CommitResult {
        let derived_data_delta = committed.take_derived_data_delta();

        let mut id_alloc = || {
            let row_id = PublicRowId {
                commit: cmt_hash,
                counter: num_ops,
            };
            num_ops += 1;
            store
                .id_lookup()
                .packed(&row_id)
                .expect("hash already packed")
        };

        let derived_data_delta_store = derived_data_delta.map(|td| {
            let table_schema = store
                .table_at(td.for_entity())
                .expect("valid schema from conl-query")
                .schema();
            td.map(|qt| qt.into_store(table_schema, &mut id_alloc))
        });

        let monitored_constraints = committed.take_soft_violations();
        store
            .apply_derived_view(derived_data_delta_store)
            .expect("Applying derived data from coln-query should not fail");

        CommitResult {
            monitored_constraints,
            cmt_hash,
        }
    }
}

impl DbWrite for Bouncer {
    fn transaction(&mut self) {
        self.store.transaction();
    }

    fn commit(&mut self) -> Result<CommitResult, BouncerError> {
        let store_delta = self.store.try_commit()?.map(|td| td.map(QueryTuple::from));

        let num_ops = store_delta.size() as u32;
        let tx = Tx::new(store_delta);

        match tx.try_commit(&mut self.query) {
            Ok(TryCommitOk::Pending(pending)) => {
                let committed = pending.commit()?;
                let cmt_hash = self.store.commit()?;
                Ok(Self::commit_to_store(
                    self.store.inner_mut(),
                    committed,
                    (cmt_hash, num_ops),
                ))
            }
            Ok(TryCommitOk::Rejected(mut rejected)) => {
                self.store.abort();
                let violations = rejected.take_hard_violations();
                Err(RuleViolation::HardViolation(violations).into())
            }
            // The next two can also be combined, as they share the same error
            // type and should be considered a bug. If it's a bug, then it should panic.
            // Err(TryCommitErr::TxApplyError(err)) | Err(TryCommitErr::RollbackError(err)) => {}
            Err(TryCommitErr::TxApplyError(_err)) | Err(TryCommitErr::RollbackError(_err)) => {
                panic!("coln query encounters unrecoverable error")
            }
        }
    }

    fn abort(&mut self) {
        self.store.abort();
    }

    fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, BouncerError> {
        Ok(self.store.add(table, values)?)
    }
}

impl Bouncer {
    /// Promote the id from temporary `[TxRowId]` to permanent `[PublicRowId]`.
    /// It is the caller's responsiblity to do it after they call `[bouncer::commit]`
    /// on a transaction.
    pub fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxRowId>,
        hash: CommitHash,
    ) -> Vec<PublicRowId> {
        self.store.inner().promote(pending_ids, hash)
    }
}
