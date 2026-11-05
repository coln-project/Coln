// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The top level APIs exposed by the coln backend database to the user and FFI
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
    transaction::{TryCommitErr, TryCommitOk, Tx},
    violations::ViolationsDelta,
};
use coln_store::{
    store::{IdLookup, auto::AutoStore},
    txn::{
        id::Promote,
        rw::{StoreRead, StoreWrite},
    },
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
}

impl coln_store::store::frag::FragmentSync for Bouncer {
    fn commit_chunks_after(
        &self,
        have_heads: &[CommitHash],
    ) -> Vec<coln_store::store::frag::CommitChunk> {
        self.store.commit_chunks_after(have_heads)
    }

    fn apply_chunk_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<(), coln_store::store::error::StoreError> {
        // TODO need to change this workflow to something like
        // decode the bytes into commit
        // Apply what can be applied and return the tuples that can be applied
        // while making the store hold in a pending state
        // feed those into coln-query for rule checking
        // If ok, ask the store to apply the changes.
        self.store.apply_chunk_bytes(chunk_bytes)
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

impl DbWrite for Bouncer {
    fn transaction(&mut self) {
        self.store.transaction();
    }

    fn commit(&mut self) -> Result<CommitResult, BouncerError> {
        let store_delta = self.store.try_commit()?.map(|td| td.map(QueryTuple::from));

        let mut num_ops = store_delta.size() as u32;
        let tx = Tx::new(store_delta);

        match tx.try_commit(&mut self.query) {
            Ok(TryCommitOk::Pending(pending)) => {
                let mut committed = pending.commit()?;
                let cmt_hash = self.store.commit()?;
                let derived_data_delta = committed.take_derived_data_delta();

                let mut id_alloc = || {
                    let row_id = PublicRowId {
                        commit: cmt_hash,
                        counter: num_ops,
                    };
                    num_ops += 1;
                    self.store
                        .id_lookup()
                        .packed(&row_id)
                        .expect("hash already packed")
                };

                let derived_data_delta_store = derived_data_delta.map(|td| {
                    let table_schema = self
                        .store
                        .inner()
                        .table_at(td.for_entity())
                        .expect("valid schema from conl-query")
                        .schema();
                    td.map(|qt| qt.into_store(table_schema, &mut id_alloc))
                });

                let monitored_constraints = committed.take_soft_violations();
                self.store
                    .inner_mut()
                    .apply_derived_view(derived_data_delta_store)?;
                Ok(CommitResult {
                    monitored_constraints,
                    cmt_hash,
                })
            }
            Ok(TryCommitOk::Rejected(mut rejected)) => {
                // A hard constraint has been violated.
                // At this point in time coln-query has already rolled back
                // its internal state.
                // In this arm, coln-store must roll back to keep the two in
                // sync and report back the violations:
                let violations = rejected.take_hard_violations();
                Err(RuleViolation::HardViolation(violations).into())
            }
            // The next two can also be combined, as they share the same error
            // type and should be considered a bug, I guess.
            // Err(TryCommitErr::TxApplyError(err)) | Err(TryCommitErr::RollbackError(err)) => {}
            Err(TryCommitErr::TxApplyError(err)) | Err(TryCommitErr::RollbackError(err)) => {
                Err(BouncerError::QueryError(err))
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

impl Promote for Bouncer {
    fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxRowId>,
        hash: CommitHash,
    ) -> Vec<PublicRowId> {
        self.store.promote(pending_ids, hash)
    }
}
