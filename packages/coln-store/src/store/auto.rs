// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::{self, FlatRealm};

use crate::{
    commit::hash::CommitHash,
    store::{ColnDef, Store, error::StoreError},
    table::{cell::WireTuple, handle::WireRowView},
    txn::{
        OwnedTransaction, TxnLiveRowId, TxnLiveValue,
        rw::{StoreRead, StoreWrite, WhereClause},
    },
};

pub struct AutoStore {
    txn: Option<OwnedTransaction>,
}

impl StoreRead for AutoStore {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<WireRowView>> {
        self.txn.as_ref().expect("open txn").scan_table(table)
    }

    fn row_by_liveid(&self, table: &ir::Path, live_id: &TxnLiveRowId) -> Option<WireRowView> {
        self.txn
            .as_ref()
            .expect("open txn")
            .row_by_liveid(table, live_id)
    }

    fn all(&self, query: &WhereClause, select: &[u32]) -> Option<Vec<WireTuple>> {
        self.txn.as_ref().expect("open txn").all(query, select)
    }
}

impl StoreWrite for AutoStore {
    fn add<V: Into<TxnLiveValue>>(
        &mut self,
        table: &ir::Path,
        values: Vec<V>,
    ) -> Result<TxnLiveRowId, StoreError> {
        self.txn.as_mut().expect("open_txn").add(table, values)
    }
}

impl AutoStore {
    pub fn try_from_ir(ir: FlatRealm, coln_def: ColnDef) -> Result<Self, StoreError> {
        let store = Store::try_from_ir(ir, coln_def)?;
        Ok(Self::new(store))
    }

    pub fn new(store: Store) -> Self {
        Self {
            txn: Some(store.into_transaction()),
        }
    }

    pub fn commit(&mut self) -> Result<CommitHash, StoreError> {
        let (res, store) = match self.txn.take().expect("open txn").commit() {
            Ok((hash, store)) => (Ok(hash), store),
            Err((err, store)) => (Err(err), store),
        };
        self.txn.replace(store.into_transaction());
        res
    }

    pub fn abort(&mut self) {
        let store = self.txn.take().expect("open txn").abort();
        self.txn.replace(store.into_transaction());
    }
}

impl Drop for AutoStore {
    fn drop(&mut self) {
        self.abort();
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

        let live_id = store.add(&path, vec![1i32]).expect("add");
        let _hash = store.commit().expect("commit");
        let row_id = live_id.row_id().expect("finalized");

        assert_eq!(
            store.row_by_id(&path, row_id),
            Some(WireRowView {
                row_id,
                values: vec![1i32.into()],
            })
        );

        store.add(&path, vec![2i32]).expect("add pending");
        assert_eq!(store.scan_table(&path).expect("T").len(), 1);
    }
}
