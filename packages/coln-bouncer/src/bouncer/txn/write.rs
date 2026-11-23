// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Write transactions: local operations are staged, validated by coln-query and
//! tentatively applied to coln-store, then committed or aborted in both.

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
    transaction::{self as query_tx, DerivedDataDelta, TryCommitErr, TryCommitOk, Tx},
    violations::ViolationsDelta,
};
use coln_store::store::{
    IdLookup, ReadView, Store, WriteTxn as StoreWriteTxn,
    commit::{CommitBatch, Unprepared},
    txn::{self as store_txn, ReadTxnable},
};

use super::read::sealed::StoreView;
use crate::{
    bouncer::Bouncer,
    error::{BouncerError, RuleViolation},
};

// Txn states
pub trait TxnState {}

/// Operations are queued; neither the store nor the query engine has seen them.
pub struct Prepare<'a> {
    store: StoreWriteTxn<store_txn::Prepare>,
    query: &'a mut ColnQuery,
}

/// Hard constraints are met and both engines hold tentative state that is
/// rolled back unless committed.
pub struct Pending<'a> {
    // Each field rolls back its own engine when dropped, so dropping `Pending`
    // undoes the transaction without a `Drop` impl here.
    store: StoreWriteTxn<store_txn::Pending>,
    query: Tx<query_tx::Pending<'a, ColnQuery>>,
    monitored_constraints: ViolationsDelta,
}

/// Accepted by both engines.
pub struct Committed {
    cmt_hash: CommitHash,
    monitored_constraints: ViolationsDelta,
}

/// Discarded; neither engine retains any of its changes.
pub struct Aborted;

impl TxnState for Prepare<'_> {}
impl TxnState for Pending<'_> {}
impl TxnState for Committed {}
impl TxnState for Aborted {}

/// coln-bouncer implements the following state machine for (write) transactions
/// (as are coln-store and coln-query):
///
/// ```text
///                        Bouncer::begin_write()
///                                  │
///                                  ▼
///                   ┌──────────────────────────────┐
///                   │           Prepare            │
///                   └──────────────────────────────┘
///                                  │
///                                  │ try_commit() -> Ok(_)
///                                  ▼
///                   ┌──────────────────────────────┐
///                   │           Pending            │
///                   └──────────────────────────────┘
///                          │                │
///                 commit() │                │ abort()
///                          ▼                ▼
///                   ┌─────────────┐  ┌─────────────┐
///                   │  Committed  │  │   Aborted   │
///                   └─────────────┘  └─────────────┘
/// ```
///
/// - `Prepare` queues operations with `add`; `read` sees committed data only.
///   at this stage no data is applied, they are just buffered, and cannot be read
///   through the read APIs
/// - `try_commit` applies the operations tentatively to the store and runs them
///   through the query engine. A hard constraint violation, or any store
///   error, returns `Err` and leaves both engines unchanged.
/// - `Pending` holds the store's write lock and the query engine's rollback.
///   `read` sees the tentative base and derived data.
/// - `commit` commits the query engine and the store. `abort` rolls both back.
/// - Dropping `Prepare` discards queued operations. Dropping `Pending` rolls
///   both engines back, just like `abort`.
pub struct WriteTxn<S: TxnState> {
    state: S,
}

/// Applies incoming commits, validating and accepting each one independently.
pub struct ImportTxn<'a> {
    store: &'a Store,
    query: &'a mut ColnQuery,
}

impl Bouncer {
    #[must_use = "a write transaction must be committed or explicitly discarded"]
    pub fn begin_write(&mut self) -> WriteTxn<Prepare<'_>> {
        WriteTxn {
            state: Prepare {
                store: self.store.begin_write(),
                query: &mut self.query,
            },
        }
    }

    pub fn begin_import(&mut self) -> ImportTxn<'_> {
        ImportTxn {
            store: &self.store,
            query: &mut self.query,
        }
    }
}

/// Reads see committed data, excluding this transaction's queued operations.
impl StoreView for WriteTxn<Prepare<'_>> {
    fn store_view(&self) -> ReadView<'_> {
        self.state.store.read()
    }
}

/// Reads see tentative base and derived data, including this transaction's operations.
impl StoreView for WriteTxn<Pending<'_>> {
    fn store_view(&self) -> ReadView<'_> {
        self.state.store.read()
    }
}

impl<'a> WriteTxn<Prepare<'a>> {
    pub fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, BouncerError> {
        Ok(self.state.store.add(table, values)?)
    }

    #[must_use = "a pending transaction rolls back unless committed"]
    pub fn try_commit(self) -> Result<WriteTxn<Pending<'a>>, BouncerError> {
        let Prepare { store, query } = self.state;
        let mut store = store.try_commit()?;
        // On `Err`, `store` drops here and rolls back its tentative changes.
        let (mut query, num_ops) = validate(query, store.store_delta())?;
        let hash = store.prepared_hash();
        let derived = derived_to_store(&store, query.take_derived_data_delta(), hash, num_ops);
        let monitored_constraints = query.take_soft_violations();
        store
            .apply_derived_view(derived)
            .expect("applying derived data from coln-query should not fail");
        Ok(WriteTxn {
            state: Pending {
                store,
                query,
                monitored_constraints,
            },
        })
    }

    /// Convenience for `try_commit()?.commit()`.
    pub fn commit(self) -> Result<WriteTxn<Committed>, BouncerError> {
        self.try_commit()?.commit()
    }

    /// Discard this transaction without applying its queued operations.
    pub fn abort(self) -> WriteTxn<Aborted> {
        WriteTxn { state: Aborted }
    }
}

impl WriteTxn<Pending<'_>> {
    /// The hash this transaction will be committed under, e.g. for promoting
    /// its row ids before committing.
    pub fn prepared_hash(&self) -> CommitHash {
        self.state.store.prepared_hash()
    }

    pub fn monitored_constraints(&self) -> &ViolationsDelta {
        &self.state.monitored_constraints
    }

    pub fn commit(self) -> Result<WriteTxn<Committed>, BouncerError> {
        let Pending {
            store,
            query,
            monitored_constraints,
        } = self.state;
        // coln-query commits first because it is the only fallible step. If it
        // fails, it rolls itself back and `store` rolls back when dropped.
        query.commit()?;
        let cmt_hash = store.commit().commit_hash();
        Ok(WriteTxn {
            state: Committed {
                cmt_hash,
                monitored_constraints,
            },
        })
    }

    pub fn abort(self) -> WriteTxn<Aborted> {
        let Pending { store, query, .. } = self.state;
        if let Err(error) = query.abort() {
            panic!("coln query encounters unrecoverable error: {error}")
        }
        store.abort();
        WriteTxn { state: Aborted }
    }
}

impl WriteTxn<Committed> {
    pub fn cmt_hash(&self) -> CommitHash {
        self.state.cmt_hash
    }

    pub fn monitored_constraints(&self) -> &ViolationsDelta {
        &self.state.monitored_constraints
    }

    // TODO all monitored constraints, maybe?
}

impl ImportTxn<'_> {
    /// Apply each ready commit independently. Earlier accepted commits remain
    /// applied if a later commit is rejected; blocked descendants remain pending.
    pub fn apply_chunk_bytes(
        self,
        bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<Vec<ViolationsDelta>, BouncerError> {
        let batch = self.store.prepare_commit_bytes(bytes)?;
        self.apply_batch(batch)
    }

    pub(crate) fn apply_batch(
        self,
        mut batch: CommitBatch<Unprepared>,
    ) -> Result<Vec<ViolationsDelta>, BouncerError> {
        let mut reports = Vec::new();
        while batch.has_ready() {
            let mut prepared = batch.prepare_next()?.expect("ready commit");
            let hash = prepared.prepared_hash();
            let validated = validate(&mut *self.query, prepared.store_delta())
                .and_then(|(pending, num_ops)| Ok((pending.commit()?, num_ops)));
            let (mut committed, num_ops) = match validated {
                Ok(validated) => validated,
                Err(error) => {
                    prepared.reject_prepared();
                    return Err(error);
                }
            };
            let derived = derived_to_store(
                &prepared,
                committed.take_derived_data_delta(),
                hash,
                num_ops,
            );
            prepared
                .apply_derived_view(derived)
                .expect("applying derived data from coln-query should not fail");
            batch = prepared.accept_prepared();
            reports.push(committed.take_soft_violations());
        }
        Ok(reports)
    }
}

/// Run base operations through coln-query, leaving it pending on success.
/// Also returns the number of base operations, used to number derived rows.
fn validate(
    query: &mut ColnQuery,
    delta: StoreDelta<PartialConsolidated, StoreTuple>,
) -> Result<(Tx<query_tx::Pending<'_, ColnQuery>>, u32), BouncerError> {
    // Derived row ids follow every base operation, including multiple rows in
    // the same table delta.
    let num_ops = delta
        .deltas()
        .iter()
        .map(|td| td.delta().len())
        .sum::<usize>() as u32;
    let tx = Tx::new(delta.map(|td| td.map(QueryTuple::from)));
    match tx.try_commit(query) {
        Ok(TryCommitOk::Pending(pending)) => Ok((pending, num_ops)),
        Ok(TryCommitOk::Rejected(mut rejected)) => {
            Err(RuleViolation::HardViolation(rejected.take_hard_violations()).into())
        }
        Err(TryCommitErr::TxApplyError(error) | TryCommitErr::RollbackError(error)) => {
            panic!("coln query encounters unrecoverable error: {error}")
        }
    }
}

/// Convert coln-query's derived data into store tuples, allocating row ids
/// for new derived rows after the commit's base operations.
fn derived_to_store(
    read: &impl ReadTxnable,
    derived: DerivedDataDelta<Consolidated>,
    cmt_hash: CommitHash,
    mut num_ops: u32,
) -> StoreDelta<Consolidated, StoreTuple> {
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
    derived.map(|td| {
        let table = read
            .table_at(td.for_entity())
            .expect("valid schema from coln-query");
        td.map(|qt| qt.into_store(table.schema(), &mut id_alloc))
    })
}
