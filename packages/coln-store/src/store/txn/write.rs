// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Write transactions: pending operations that are prepared against the
//! store and then committed or aborted.

use coln_flir_rs::engine::delta::{PartialConsolidated, StoreDelta};
use coln_flir_rs::engine::packed::StoreTuple;
use coln_flir_rs::engine::tx::{TxRowId, TxTuple};
use coln_flir_rs::hash::CommitHash;

use crate::ir;
use crate::store::commit::{CommitBatch, Prepared};
use crate::store::error::StoreError;
use crate::store::txn::inner::TxnData;
use crate::store::{ReadTxn, Store};

impl Store {
    pub fn begin_write(&self) -> WriteTxn<'_, Prepare<'_>> {
        let deps = self.begin_read().heads();
        WriteTxn {
            store: self,
            state: Prepare {
                read: self.begin_read(),
                data: TxnData::new(deps),
            },
        }
    }
}

// Txn states
pub struct Prepare<'a> {
    read: ReadTxn<'a>,
    data: TxnData,
}
pub struct Pending<'a>(Option<CommitBatch<'a, Prepared>>);
pub struct Committed(CommitHash);
pub struct Aborted;

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        if let Some(batch) = self.0.take() {
            batch.reject_prepared();
        }
    }
}

pub struct WriteTxn<'a, S> {
    store: &'a Store,
    pub(super) state: S,
}

impl<'a> WriteTxn<'a, Prepare<'a>> {
    pub fn read(&self) -> &ReadTxn<'a> {
        &self.state.read
    }

    pub fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, StoreError> {
        self.state.data.add(&self.store, table, values)
    }

    pub fn try_commit(self) -> Result<WriteTxn<'a, Pending<'a>>, StoreError> {
        let cmt = self
            .state
            .data
            .into_commit(|oid| self.store.table_meta(oid))?;

        // drop the read guard before acquiring the write guard
        drop(self.state.read);

        let batch = self
            .store
            .prepare_commit(cmt)?
            .prepare_next()?
            .expect("local commit must be ready to apply");

        Ok(WriteTxn {
            store: self.store,
            state: Pending(Some(batch)),
        })
    }

    // Could just commit, if don't care about intermeidate states
    pub(crate) fn commit(self) -> Result<WriteTxn<'a, Committed>, StoreError> {
        let t = self.try_commit()?;
        Ok(t.commit())
    }
}

impl<'a> WriteTxn<'a, Pending<'a>> {
    /// Takes the store_delta out of the batch
    pub fn store_delta(&mut self) -> StoreDelta<PartialConsolidated, StoreTuple> {
        self.state.0.as_mut().unwrap().store_delta()
    }

    pub fn commit(mut self) -> WriteTxn<'a, Committed> {
        let batch = self.state.0.take().unwrap();
        let hash = batch.prepared_hash();
        batch.accept_prepared();
        WriteTxn {
            store: self.store,
            state: Committed(hash),
        }
    }

    pub fn abort(self) -> WriteTxn<'a, Aborted> {
        // Drop it will rejec it
        WriteTxn {
            store: self.store,
            state: Aborted,
        }
    }
}

impl WriteTxn<'_, Committed> {
    pub fn commit_hash(&self) -> CommitHash {
        self.state.0
    }
}

// TODO read methods on WriteTxn
