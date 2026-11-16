// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Write transactions: pending operations that are prepared against the
//! store and then committed or aborted.

use std::sync::Arc;

use coln_flir_rs::engine::delta::{Consolidated, PartialConsolidated, StoreDelta};
use coln_flir_rs::engine::packed::StoreTuple;
use coln_flir_rs::engine::tx::{TxRowId, TxTuple};
use coln_flir_rs::hash::CommitHash;
use parking_lot::{ArcRwLockUpgradableReadGuard, RawRwLock};

use crate::ir;
use crate::store::catalog::Catalog;
use crate::store::commit::{CommitBatch, Prepared};
use crate::store::error::StoreError;
use crate::store::txn::ReadTxnable;
use crate::store::txn::inner::TxnData;
use crate::store::{ReadView, Store, StoreData};

impl Store {
    pub fn begin_write(&self) -> WriteTxn<Prepare> {
        let guard = self.data.upgradable_read_arc();
        let deps = guard.heads();
        WriteTxn {
            state: Prepare {
                catalog: Arc::clone(&self.catalog),
                guard,
                data: TxnData::new(deps),
            },
        }
    }
}

// Txn states
pub trait TxnState {}
pub struct Prepare {
    catalog: Arc<Catalog>,
    guard: ArcRwLockUpgradableReadGuard<RawRwLock, StoreData>,
    data: TxnData,
}
pub struct Pending(Option<CommitBatch<Prepared>>);
pub struct Committed(CommitHash);
pub struct Aborted;

impl TxnState for Prepare {}
impl TxnState for Pending {}
impl TxnState for Committed {}
impl TxnState for Aborted {}

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(batch) = self.0.take() {
            batch.reject_prepared();
        }
    }
}

/// A write transaction that stages operations, applies them tentatively, and
/// then commits or aborts them.
///
/// ```text
///                         Store::begin_write()
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
/// - `Prepare` holds a read lock. `add` queues operations, and `read` sees
///   committed data without those operations.
/// - `try_commit` releases the read lock, acquires the write lock, and
///   tentatively applies the operations. Returning `Err` consumes the
///   transaction without keeping its changes.
/// - `Pending` holds the write lock and a rollback snapshot. `read` sees
///   tentative data, and `apply_derived_view` adds changes to the same
///   transaction. `commit` accepts these changes and releases the lock;
///   `abort` rolls them back and releases the lock.
/// - Dropping `Prepare` discards queued operations. Dropping `Pending` rolls
///   back tentative changes, including derived data, just like `abort`.
pub struct WriteTxn<S: TxnState> {
    pub(super) state: S,
}

impl ReadTxnable for WriteTxn<Prepare> {
    // TODO this should read my own writes
    fn read(&self) -> ReadView<'_> {
        ReadView::borrowed(&self.state.catalog, &self.state.guard)
    }
}

impl WriteTxn<Prepare> {
    pub fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, StoreError> {
        self.state.data.add(&self.state.catalog, table, values)
    }

    pub fn try_commit(self) -> Result<WriteTxn<Pending>, StoreError> {
        let Prepare {
            catalog,
            guard,
            data,
        } = self.state;

        let commit = data.into_commit(|oid| catalog.table_meta(oid))?;
        let guard = ArcRwLockUpgradableReadGuard::upgrade(guard);

        let batch = CommitBatch::new(catalog, guard, [commit], false)
            .prepare_next()?
            .expect("local commit must be ready to apply");

        Ok(WriteTxn {
            state: Pending(Some(batch)),
        })
    }

    // Could just commit, if don't care about intermeidate states
    pub(crate) fn commit(self) -> Result<WriteTxn<Committed>, StoreError> {
        let t = self.try_commit()?;
        Ok(t.commit())
    }
}

impl WriteTxn<Pending> {
    pub fn read(&self) -> ReadView<'_> {
        self.state.0.as_ref().unwrap().read()
    }

    pub fn prepared_hash(&self) -> CommitHash {
        self.state.0.as_ref().unwrap().prepared_hash()
    }

    /// Apply derived data before accepting the commit and releasing its lock.
    /// Aborting or dropping this transaction rolls these changes back too.
    pub fn apply_derived_view(
        &mut self,
        derived: StoreDelta<Consolidated, StoreTuple>,
    ) -> Result<(), StoreError> {
        self.state.0.as_mut().unwrap().apply_derived_view(derived)
    }

    /// Takes the store_delta out of the batch
    pub fn store_delta(&mut self) -> StoreDelta<PartialConsolidated, StoreTuple> {
        self.state.0.as_mut().unwrap().store_delta()
    }

    pub fn commit(mut self) -> WriteTxn<Committed> {
        let batch = self.state.0.take().unwrap();
        let hash = batch.prepared_hash();
        batch.accept_prepared();
        WriteTxn {
            state: Committed(hash),
        }
    }

    pub fn abort(self) -> WriteTxn<Aborted> {
        // Drop it will rejec it
        WriteTxn { state: Aborted }
    }
}

impl WriteTxn<Committed> {
    pub fn commit_hash(&self) -> CommitHash {
        self.state.0
    }
}
