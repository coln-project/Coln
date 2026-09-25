// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    engine::txn_val::{TempRowId, TxnWireRowId, TxnWireTuple},
    hash::{self, CommitHash},
    ir,
};
use tracing::info;

use crate::{
    commit::{Commit, author::Author, wire::CommitData},
    store::{Store, error::StoreError},
    table::ValidationError,
    txn::{PendingOp, timestamp::Timestamp},
};

pub(crate) struct TxnInner {
    deps: Vec<CommitHash>,
    author: Author,
    pending: Vec<PendingOp>,
    timestamp: Timestamp,
    message: Option<String>,
}

impl TxnInner {
    pub(super) fn new(deps: Vec<CommitHash>) -> Self {
        Self {
            deps,
            author: Author::foo(),
            pending: Vec::new(),
            timestamp: Timestamp::now(),
            message: None,
        }
    }

    fn next_id(&self) -> TempRowId {
        TempRowId::from(self.pending.len() as u32)
    }

    fn add_cell_values(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: impl Into<TxnWireTuple>,
    ) -> Result<TempRowId, StoreError> {
        let t = store
            .table_at(table)
            .map(|t| t.inner())
            .ok_or(ValidationError::UnknownTable {
                path: table.clone(),
            })?;
        let values = values.into();
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
        values: impl Into<TxnWireTuple>,
    ) -> Result<TxnWireRowId, StoreError> {
        let temp_id = self.add_cell_values(store, table, values)?;
        let handle = TxnWireRowId::Pending(temp_id);
        Ok(handle)
    }

    // Used by the REPL only
    #[cfg(feature = "native")]
    pub(super) fn add_internal(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: impl Into<TxnWireTuple>,
    ) -> Result<TempRowId, StoreError> {
        self.add_cell_values(store, table, values)
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
                return Err(err.into());
            }
        };

        let h = cmt.hash();
        match store.apply_commit(cmt) {
            Ok(None) => {
                // Everything applied successfully
                Ok(h)
            }
            Ok(Some(_)) => {
                unreachable!("commit a local transaction should always succeed");
            }
            Err(err) => Err(err),
        }
    }

    pub(super) fn abort(&mut self) {
        // do nothing
    }
}
