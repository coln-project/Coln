// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    engine::{
        delta::{Consolidated, PartialConsolidated, StoreDelta},
        packed::StoreTuple,
        query::QueryTuple,
        tx::{TxRowId, TxTuple},
    },
    hash::CommitHash,
    ir,
    public::PublicRowId,
};
use coln_query::api::{
    ColnQuery,
    transaction::{Committed, TryCommitErr, TryCommitOk, Tx},
    violations::ViolationsDelta,
};
use coln_store::store::{
    IdLookup, Store, WriteTxn as StoreWriteTxn,
    commit::{CommitBatch, Unprepared},
    txn::Prepare,
};

use super::ReadTxn;
use crate::{
    bouncer::Bouncer,
    error::{BouncerError, RuleViolation},
};

/// Staged local operations, committed together after validation.
pub struct Local<'a>(StoreWriteTxn<'a, Prepare<'a>>);

/// Imported commits, validated and accepted one at a time.
pub struct Import<'a>(&'a Store);

/// Exclusive access to bouncer writes. Dropping a local transaction discards it.
#[must_use = "a write transaction must be committed or explicitly discarded"]
pub struct WriteTxn<'a, S = Local<'a>> {
    state: S,
    query: &'a mut ColnQuery,
}

pub struct CommitResult {
    pub monitored_constraints: ViolationsDelta,
    pub cmt_hash: CommitHash,
}

impl Bouncer {
    pub fn begin_write(&mut self) -> WriteTxn<'_> {
        WriteTxn {
            state: Local(self.store.begin_write()),
            query: &mut self.query,
        }
    }

    pub fn begin_import(&mut self) -> WriteTxn<'_, Import<'_>> {
        WriteTxn {
            state: Import(&self.store),
            query: &mut self.query,
        }
    }
}

impl WriteTxn<'_> {
    /// Read committed data, excluding this transaction's queued operations.
    pub fn read(&self) -> ReadTxn<'_> {
        ReadTxn {
            store: self.state.0.read(),
        }
    }

    pub fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, BouncerError> {
        Ok(self.state.0.add(table, values)?)
    }

    pub fn commit(self) -> Result<CommitResult, BouncerError> {
        let mut pending = self.state.0.try_commit()?;
        let hash = pending.prepared_hash();
        let (committed, num_ops) = validate(self.query, pending.store_delta())?;
        let (derived, result) = finish_query(&pending.read(), committed, hash, num_ops);
        pending
            .apply_derived_view(derived)
            .expect("applying derived data from coln-query should not fail");
        pending.commit();
        Ok(result)
    }
}

impl<S> WriteTxn<'_, S> {
    /// Discard this transaction without applying its queued operations.
    pub fn abort(self) {}
}

impl WriteTxn<'_, Import<'_>> {
    /// Apply each ready commit independently. Earlier accepted commits remain
    /// applied if a later commit is rejected; blocked descendants remain pending.
    pub fn apply_chunk_bytes(
        self,
        bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<Vec<ViolationsDelta>, BouncerError> {
        let batch = self.state.0.prepare_commit_bytes(bytes)?;
        apply_batch(self.query, batch)
    }
}

pub(in crate::bouncer) fn apply_batch(
    query: &mut ColnQuery,
    mut batch: CommitBatch<'_, Unprepared>,
) -> Result<Vec<ViolationsDelta>, BouncerError> {
    let mut reports = Vec::new();
    while batch.has_ready() {
        let mut prepared = batch.prepare_next()?.expect("ready commit");
        let hash = prepared.prepared_hash();
        let (committed, num_ops) = match validate(query, prepared.store_delta()) {
            Ok(committed) => committed,
            Err(error) => {
                prepared.reject_prepared();
                return Err(error);
            }
        };
        let (derived, result) = finish_query(&prepared.read(), committed, hash, num_ops);
        prepared
            .apply_derived_view(derived)
            .expect("applying derived data from coln-query should not fail");
        batch = prepared.accept_prepared();
        reports.push(result.monitored_constraints);
    }
    Ok(reports)
}

fn validate(
    query: &mut ColnQuery,
    delta: StoreDelta<PartialConsolidated, StoreTuple>,
) -> Result<(Tx<Committed>, u32), BouncerError> {
    // Derived row ids follow every base operation, including multiple rows in
    // the same table delta.
    let num_ops = delta
        .deltas()
        .iter()
        .map(|td| td.delta().len())
        .sum::<usize>() as u32;
    let tx = Tx::new(delta.map(|td| td.map(QueryTuple::from)));
    match tx.try_commit(query) {
        Ok(TryCommitOk::Pending(pending)) => Ok((pending.commit()?, num_ops)),
        Ok(TryCommitOk::Rejected(mut rejected)) => {
            Err(RuleViolation::HardViolation(rejected.take_hard_violations()).into())
        }
        Err(TryCommitErr::TxApplyError(error) | TryCommitErr::RollbackError(error)) => {
            panic!("coln query encounters unrecoverable error: {error}")
        }
    }
}

// Query has committed at this point, so conversion and installation of its
// derived data must succeed while the store transaction still holds its lock.
fn finish_query(
    read: &coln_store::store::ReadTxn<'_>,
    mut committed: Tx<Committed>,
    cmt_hash: CommitHash,
    mut num_ops: u32,
) -> (StoreDelta<Consolidated, StoreTuple>, CommitResult) {
    let mut id_alloc = || {
        let row_id = PublicRowId {
            commit: cmt_hash,
            counter: num_ops,
        };
        num_ops += 1;
        read.id_lookup()
            .packed(&row_id)
            .expect("hash already packed")
    };
    let derived = committed.take_derived_data_delta().map(|td| {
        let table = read
            .table_at(td.for_entity())
            .expect("valid schema from coln-query");
        td.map(|qt| qt.into_store(table.schema(), &mut id_alloc))
    });
    let result = CommitResult {
        monitored_constraints: committed.take_soft_violations(),
        cmt_hash,
    };
    (derived, result)
}
