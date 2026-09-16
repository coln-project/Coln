// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::{self, FlatRealm};

use crate::{
    commit::hash::CommitHash,
    store::{ColnDef, Store, error::StoreError},
    table::{WireRowId, cell::WireTuple, handle::WireRowView},
    txn::{
        OwnedTransaction, TxnWireRowId,
        id::{Promote, TxnWireTuple},
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

    fn all_proj(&self, query: &WhereClause, select: &[u32]) -> Result<Vec<WireTuple>, StoreError> {
        self.txn.as_ref().expect("open txn").all_proj(query, select)
    }

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<crate::table::WireRowId>, StoreError> {
        self.txn.as_ref().expect("open txn").all_row_id(query)
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &crate::table::WireRowId) -> Option<WireRowView> {
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

impl Promote for AutoStore {
    fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxnWireRowId>,
        hash: CommitHash,
    ) -> Vec<WireRowId> {
        self.txn
            .as_ref()
            .expect("open txn")
            .store()
            .promote(pending_ids, hash)
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

        let pending_id = store.add(&path, vec![1i32]).expect("add");
        let h = store.commit().expect("commit");
        let row_id = store.promote_one(pending_id, h);

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
