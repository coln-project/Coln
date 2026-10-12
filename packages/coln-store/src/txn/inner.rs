// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    engine::tx::{PendingRowId, TxRowId, TxTuple},
    hash::CommitHash,
    ir,
};
use tracing::info;

use crate::{
    commit::{Commit, author::Author, wire::CommitData},
    op::PendingOp,
    store::{
        Store,
        commit::{BatchInner, Prepared},
        error::{CommitApplyError, StoreError},
    },
    table::ValidationError,
    txn::timestamp::Timestamp,
};

pub(crate) struct TxnInner {
    deps: Vec<CommitHash>,
    author: Author,
    pending: Vec<PendingOp>,
    timestamp: Timestamp,
    message: Option<String>,
}

impl TxnInner {
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
        store: &Store,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<PendingRowId, StoreError> {
        let t = store
            .table_at(table)
            .map(|t| t.inner())
            .ok_or(ValidationError::UnknownTable {
                path: table.clone(),
            })?;
        let values = values.into();
        t.validate_column_count(values.len())?;
        let temp_id = self.next_id();
        self.pending.push(PendingOp::add(temp_id, t.oid(), values));
        Ok(temp_id)
    }

    pub(crate) fn add(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, StoreError> {
        let temp_id = self.add_cell_values(store, table, values)?;
        let handle = TxRowId::Pending(temp_id);
        Ok(handle)
    }

    // Used by the REPL only
    #[cfg(feature = "native")]
    pub(super) fn add_internal(
        &mut self,
        store: &Store,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<PendingRowId, StoreError> {
        self.add_cell_values(store, table, values)
    }

    pub(crate) fn try_commit(
        &mut self,
        store: &mut Store,
    ) -> Result<BatchInner<Prepared>, StoreError> {
        let TxnInner {
            deps,
            author,
            pending,
            timestamp,
            message,
            ..
        } = self;

        info!(op_count = pending.len(), "commit txn");

        // Reject empty commits.
        // TODO we could add an option to allow empty commit
        if pending.is_empty() {
            return Err(CommitApplyError::EmptyCommit.into());
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
        )?;
        let batch = store.prepare_commit(cmt)?;
        Ok(batch
            .prepare_next(store)?
            .expect("local commit dependencies are already in the store"))
    }

    pub(crate) fn commit(store: &mut Store, prepared: BatchInner<Prepared>) -> CommitHash {
        let hash = prepared.prepared_hash();
        prepared.accept_prepared(store);
        hash
    }

    pub(crate) fn abort(&mut self) {
        // do nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::single_int_store;
    use rstest::rstest;

    #[rstest]
    fn commit_accepts_previously_prepared_writes(#[from(single_int_store)] mut store: Store) {
        let path = ir::Path::from("T");
        let heads: Vec<_> = store.commits().heads().copied().collect();
        let mut txn = TxnInner::new(heads.clone());
        txn.add(&store, &path, vec![42i32]).expect("add");

        let prepared = txn.try_commit(&mut store).expect("prepare");
        let hash = prepared.prepared_hash();
        assert_eq!(store.table_at(&path).expect("T").row_count(), 1);
        assert!(!store.commits().contains(&hash));
        assert_eq!(store.commits().heads().copied().collect::<Vec<_>>(), heads);

        assert_eq!(TxnInner::commit(&mut store, prepared), hash);
        assert!(store.commits().contains(&hash));
        assert_eq!(
            store.commits().heads().copied().collect::<Vec<_>>(),
            vec![hash]
        );
        assert_eq!(store.table_at(&path).expect("T").row_count(), 1);
    }
}
