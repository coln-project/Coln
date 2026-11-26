// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    engine::{
        packed::StoreTuple,
        tx::{PendingRowId, TxRowId, TxTuple},
    },
    hash::CommitHash,
    ir,
};
use tracing::info;

use super::timestamp::Timestamp;

use crate::{
    IdLookup,
    commit::{Commit, DataCommit, author::Author, wire::CommitData},
    op::PendingOp,
    store::{
        ReadView,
        error::{CommitApplyError, StoreError},
    },
    table::{TableMeta, TableOid, ValidationError},
};

pub(crate) struct TxnData {
    deps: Vec<CommitHash>,
    author: Author,
    pending: Vec<PendingOp>,
    timestamp: Timestamp,
    message: Option<String>,
}

impl TxnData {
    pub(crate) fn new(deps: Vec<CommitHash>) -> Self {
        Self {
            deps,
            author: Author::foo(),
            pending: Vec::new(),
            timestamp: Timestamp::now(),
            message: None,
        }
    }

    fn next_id(&self) -> PendingRowId {
        PendingRowId::from(self.pending.len() as u32)
    }

    fn add_cell_values(
        &mut self,
        view: ReadView,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, StoreError> {
        let table = view.table_at(table).ok_or(ValidationError::UnknownTable {
            path: table.clone(),
        })?;
        let values = values.into();
        let meta = table.meta();
        meta.validate_column_count(values.len())?;

        if table.inner().cons() {
            let key: Option<StoreTuple> = values
                .iter()
                .cloned()
                .map(|value| {
                    value
                        .try_map(|id| match id {
                            TxRowId::Existing(id) => {
                                let id = view.canonical_row_id(&id)?;
                                view.id_lookup().packed(&id)
                            }
                            TxRowId::Pending(_) => None,
                        })
                        .map(Into::into)
                })
                .collect();
            if let Some(key) = key
                && let Some(rid) = table.index_seek(&key)?.next()
            {
                let pid = view.id_lookup().unpacked(&rid).unwrap();
                return Ok(TxRowId::Existing(pid));
            }
        }

        let temp_id = self.next_id();
        self.pending
            .push(PendingOp::add(temp_id, meta.oid(), values));
        Ok(TxRowId::Pending(temp_id))
    }

    pub(crate) fn add(
        &mut self,
        view: ReadView,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, StoreError> {
        let tx_id = self.add_cell_values(view, table, values)?;
        Ok(tx_id)
    }

    pub(crate) fn into_commit<'a, F>(self, table_meta_for: F) -> Result<DataCommit, StoreError>
    where
        F: Fn(TableOid) -> Option<&'a TableMeta>,
    {
        let TxnData {
            deps,
            author,
            pending,
            timestamp,
            message,
            ..
        } = self;

        info!(op_count = pending.len(), "commit txn");

        // TODO we could add an option to allow empty commit
        if pending.is_empty() {
            return Err(CommitApplyError::EmptyCommit.into());
        }

        Ok(Commit::from_commit_data(
            CommitData::new(deps, author, *timestamp.as_ref(), message, pending),
            table_meta_for,
        )?)
    }
}
