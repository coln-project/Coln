// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    engine::tx::{PendingRowId, TxRowId, TxTuple},
    hash::CommitHash,
    ir,
};
use tracing::info;

use super::timestamp::Timestamp;

use crate::{
    commit::{Commit, DataCommit, author::Author, wire::CommitData},
    op::PendingOp,
    store::{
        catalog::Catalog,
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
        catalog: &Catalog,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<PendingRowId, StoreError> {
        let meta = catalog
            .resolve_table(table)
            .map(|oid| catalog.table_meta(oid).unwrap())
            .ok_or(ValidationError::UnknownTable {
                path: table.clone(),
            })?;
        let values = values.into();
        meta.validate_column_count(values.len())?;
        let temp_id = self.next_id();
        self.pending
            .push(PendingOp::add(temp_id, meta.oid(), values));
        Ok(temp_id)
    }

    pub(crate) fn add(
        &mut self,
        catalog: &Catalog,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, StoreError> {
        let temp_id = self.add_cell_values(catalog, table, values)?;
        let handle = TxRowId::Pending(temp_id);
        Ok(handle)
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
