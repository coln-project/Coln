// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A Store that manages a transaction for the user.
//! But the user is still responsible for starting/finishing transactions
//!

use coln_flir_rs::{
    WireRowId, WireRowView, WireTuple,
    engine::{
        schema::ColnDef,
        txn_val::{TxnWireRowId, TxnWireTuple},
    },
    hash::CommitHash,
    ir::{self, FlatRealm},
    query::WhereClause,
};

use crate::{
    store::{Store, error::StoreError, frag::FragmentSync},
    txn::{
        OwnedTransaction,
        id::Promote,
        rw::{StoreRead, StoreWrite},
    },
};

pub struct AutoStore {
    txn: Option<OwnedTransaction>,
    store: Option<Store>,
}

impl StoreRead for AutoStore {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<WireRowView>> {
        self.txn.as_ref().expect("open txn").scan_table(table)
    }

    fn all_proj(&self, query: &WhereClause, select: &[u32]) -> Result<Vec<WireTuple>, StoreError> {
        self.txn.as_ref().expect("open txn").all_proj(query, select)
    }

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<WireRowId>, StoreError> {
        self.txn.as_ref().expect("open txn").all_row_id(query)
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &WireRowId) -> Option<WireRowView> {
        self.txn
            .as_ref()
            .expect("open txn")
            .row_by_id(table, row_id)
    }
}

impl StoreWrite for AutoStore {
    fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxnWireTuple>,
    ) -> Result<TxnWireRowId, StoreError> {
        self.txn.as_mut().expect("open_txn").add(table, values)
    }
}

impl FragmentSync for AutoStore {
    fn commit_chunks_after(&self, have_heads: &[CommitHash]) -> Vec<super::frag::CommitChunk> {
        self.store
            .as_ref()
            .expect("closed txn")
            .commit_chunks_after(have_heads)
    }

    // TODO allow this after we have a good concurrency control theory
    // A current open transaction should only read data from its deps backward,
    // But no a concurrent txn
    fn apply_chunk_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<(), StoreError> {
        self.store
            .as_mut()
            .expect("closed txn")
            .apply_chunk_bytes(chunk_bytes)
    }
}

impl AutoStore {
    pub fn try_from_ir(ir: FlatRealm, coln_def: ColnDef) -> Result<Self, StoreError> {
        let store = Store::try_from_ir(ir, coln_def)?;
        Ok(Self::new(store))
    }

    pub fn new(store: Store) -> Self {
        Self {
            txn: None,
            store: Some(store),
        }
    }

    pub fn transaction(&mut self) {
        self.txn = Some(self.store.take().expect("closed txn").into_transaction());
    }

    pub fn commit(&mut self) -> Result<CommitHash, StoreError> {
        let (res, store) = match self.txn.take().expect("open txn").commit() {
            Ok((hash, store)) => (Ok(hash), store),
            Err((err, store)) => (Err(err), store),
        };
        self.store = Some(store);
        self.txn = None;
        res
    }

    pub fn abort(&mut self) {
        let store = self.txn.take().expect("open txn").abort();
        self.store = Some(store);
        self.txn = None;
    }

    pub fn try_from_commit_bytes(
        chunk_bytes: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Result<Self, StoreError> {
        let store = Store::try_from_commit_bytes(chunk_bytes)?;
        Ok(store.auto())
    }
}

impl Promote for AutoStore {
    fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxnWireRowId>,
        hash: CommitHash,
    ) -> Vec<WireRowId> {
        self.store
            .as_ref()
            .expect("closed txn")
            .promote(pending_ids, hash)
    }
}

impl Drop for AutoStore {
    fn drop(&mut self) {
        if self.txn.is_some() {
            self.abort();
        }
    }
}

#[cfg(test)]
mod tests {

    use coln_flir_rs::ir::Path;
    use rstest::rstest;

    use super::*;
    use crate::test_utils::single_int_autostore;

    #[rstest]
    fn owned_transaction_store_read_sees_committed_rows_not_pending(
        #[from(single_int_autostore)] mut store: AutoStore,
    ) {
        let path = Path::from("T");

        store.transaction();
        let pending_id = store.add(&path, vec![1i32]).expect("add");
        let h = store.commit().expect("commit");
        let row_id = store.promote_one(pending_id, h);

        store.transaction();
        assert_eq!(
            store.row_by_id(&path, &row_id),
            Some(WireRowView {
                row_id,
                values: vec![1i32.into()],
            })
        );

        store.add(&path, vec![2i32]).expect("add pending");
        assert_eq!(store.scan_table(&path).expect("T").len(), 1);
    }
}
