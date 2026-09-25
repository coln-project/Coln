// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    WireRowId, WireRowView, WireTuple, engine::txn_val::TxnWireTuple, hash::CommitHash, ir,
    query::WhereClause,
};

use crate::{
    store::{Store, error::StoreError},
    txn::{
        TxnWireRowId,
        rw::{StoreRead, StoreWrite},
    },
};

use super::TxnInner;

pub struct OwnedTransaction {
    inner: TxnInner,
    store: Store,
}

impl OwnedTransaction {
    pub fn new(store: Store) -> Self {
        let deps = store.commits().heads().copied().collect();
        Self {
            inner: TxnInner::new(deps),
            store,
        }
    }

    pub fn abort(mut self) -> Store {
        self.inner.abort();
        self.store
    }

    // We need to return Store to the user for roll back purposes, so the Err variant must be large
    #[allow(clippy::result_large_err)]
    pub fn commit(mut self) -> Result<(CommitHash, Store), (StoreError, Store)> {
        match self.inner.commit(&mut self.store) {
            Ok(hash) => Ok((hash, self.store)),
            Err(err) => Err((err, self.store)),
        }
    }
}

impl StoreRead for OwnedTransaction {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<WireRowView>> {
        self.store.scan_table_iter(table).map(|rows| rows.collect())
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &WireRowId) -> Option<WireRowView> {
        self.store.row_by_id_inner(table, row_id)
    }

    fn all_proj(&self, query: &WhereClause, select: &[u32]) -> Result<Vec<WireTuple>, StoreError> {
        self.store.all_proj_inner(query, select)
    }

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<WireRowId>, StoreError> {
        self.store.all_row_id_inner(query)
    }
}

impl StoreWrite for OwnedTransaction {
    fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxnWireTuple>,
    ) -> Result<TxnWireRowId, StoreError> {
        self.inner.add(&self.store, table, values)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::ir::Path;
    use crate::table::ValidationError;
    use crate::test_utils::single_int_store;
    use crate::txn::id::Promote;

    #[rstest]
    fn owned_transaction_commits_and_returns_updated_store(#[from(single_int_store)] store: Store) {
        let path = Path::from("T");

        let mut tx = OwnedTransaction::new(store);
        let pending_id = tx.add(&path, vec![42i32]).expect("add");

        let (h, committed) = tx.commit().expect("commit");
        let row_id = committed.promote_one(pending_id, h);
        assert_eq!(
            committed.row_by_id(&path, &row_id),
            Some(WireRowView {
                row_id,
                values: vec![42i32.into()],
            })
        );
    }

    #[rstest]
    fn owned_transaction_add_validates_table_and_column_count(
        #[from(single_int_store)] store: Store,
    ) {
        let path = Path::from("T");

        let mut tx = OwnedTransaction::new(store);
        let err = tx.add(&Path::from("missing"), vec![1i32]).unwrap_err();
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::UnknownTable { .. })
        ));

        let err = tx.add(&path, vec![1i32, 2i32]).unwrap_err();
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::ColumnCount { .. })
        ));
    }

    #[rstest]
    fn owned_transaction_store_read_sees_committed_rows_not_pending(
        #[from(single_int_store)] store: Store,
    ) {
        let path = Path::from("T");

        let mut tx = OwnedTransaction::new(store);
        let pending_id = tx.add(&path, vec![1i32]).expect("add");
        let (h, store) = tx.commit().expect("commit");
        let row_id = store.promote_one(pending_id, h);

        let mut tx = OwnedTransaction::new(store);
        assert_eq!(
            tx.row_by_id(&path, &row_id),
            Some(WireRowView {
                row_id,
                values: vec![1i32.into()],
            })
        );

        tx.add(&path, vec![2i32]).expect("add pending");
        assert_eq!(tx.scan_table(&path).expect("T").len(), 1);
        let _store = tx.abort();
    }
}
