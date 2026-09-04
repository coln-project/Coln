// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::sync::atomic::{AtomicU64, Ordering};

use coln_flir_rs::ir;
use tracing::info;

use crate::{
    commit::{
        Commit,
        author::Author,
        hash::{self, CommitHash},
        wire::CommitData,
    },
    store::{Store, error::StoreError},
    table::ValidationError,
    txn::{PendingOp, RowHandle, TempRowId, TxnCellValue, TxnId, TxnValue, timestamp::Timestamp},
};

static NEXT_TX_ID: AtomicU64 = AtomicU64::new(1);

fn next_tx_id() -> TxnId {
    TxnId::new(NEXT_TX_ID.fetch_add(1, Ordering::Relaxed))
}

pub(crate) struct TxnInner {
    deps: Vec<CommitHash>,
    author: Author,
    pending: Vec<PendingOp>,
    timestamp: Timestamp,
    message: Option<String>,
    tx_id: TxnId,
    pending_handles: Vec<RowHandle>,
}

impl TxnInner {
    pub(super) fn new(deps: Vec<CommitHash>) -> Self {
        Self {
            deps,
            author: Author::foo(),
            pending: Vec::new(),
            timestamp: Timestamp::now(),
            message: None,
            tx_id: next_tx_id(),
            pending_handles: Vec::new(),
        }
    }

    fn next_id(&self) -> TempRowId {
        TempRowId::from(self.pending.len() as u32)
    }

    fn add_cell_values(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: Vec<TxnCellValue>,
    ) -> Result<TempRowId, StoreError> {
        let t = store.table_at(table).ok_or(ValidationError::UnknownTable {
            path: table.clone(),
        })?;
        t.validate_column_count(values.len())?;
        let temp_id = self.next_id();
        self.pending.push(PendingOp::Add {
            row_id: temp_id,
            table: t.oid(),
            values,
        });
        Ok(temp_id)
    }

    pub(super) fn add(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: Vec<TxnValue>,
    ) -> Result<RowHandle, StoreError> {
        let txn_values = values
            .into_iter()
            .map(|v| v.to_txn_cell_value(self.tx_id))
            .collect::<Result<Vec<TxnCellValue>, _>>()?;
        let temp_id = self.add_cell_values(store, table, txn_values)?;
        let handle = RowHandle::from_pending(self.tx_id, temp_id.0);
        self.pending_handles.push(handle.clone());
        Ok(handle)
    }

    // Used by the REPL only
    #[cfg(feature = "native")]
    pub(crate) fn add_internal(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: Vec<TxnCellValue>,
    ) -> Result<TempRowId, StoreError> {
        self.add_cell_values(store, table, values)
    }

    fn invalidate_handles(&mut self, reason: &str) {
        self.pending_handles
            .iter()
            .for_each(|h| h.invalidate(reason));
    }

    /// Finalize handles to the id the store actually kept: a row that was
    /// deduplicated against an existing class finalizes to that class's
    /// canonical id, not to the never-stored raw id.
    fn finalize_handles(&mut self, h: CommitHash, store: &Store) {
        self.pending_handles.iter().for_each(|handle| {
            handle.finalize(h, |rid| store.canonical_row_id(rid).unwrap_or(rid))
        });
    }

    pub(super) fn commit(&mut self, store: &mut Store) -> Result<CommitHash, StoreError> {
        let TxnInner {
            deps,
            author,
            pending,
            timestamp,
            message,
            ..
        } = self;

        info!(op_count = pending.len(), "commit txn");

        // If we received an empty commit, then do nothing, return a all-zero hash
        // TODO we could add an option to allow empty commit
        if pending.is_empty() {
            return Ok(hash::ALL_ZERO_HASH);
        }

        let cmt = Commit::from_commit_data(
            CommitData::new(
                std::mem::take(deps),
                std::mem::take(author),
                *timestamp.as_ref(),
                message.take(),
                std::mem::take(pending),
            ),
            |oid| store.table_meta(oid),
        );
        let cmt = match cmt {
            Ok(cmt) => cmt,
            Err(err) => {
                self.invalidate_handles("txn commit encoding failed");
                return Err(err.into());
            }
        };

        let h = cmt.hash();
        match store.apply_commit(cmt) {
            Ok(None) => {
                // Everything applied successfully
                self.finalize_handles(h, store);
                Ok(h)
            }
            Ok(Some(_)) => {
                unreachable!("commit a local transaction should always succeed");
            }
            Err(err) => {
                self.invalidate_handles("txn commit failed");
                Err(err)
            }
        }
    }

    pub(super) fn abort(&mut self) {
        self.invalidate_handles("txn abort");
    }
}
