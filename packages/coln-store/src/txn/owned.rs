// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir;

use crate::{
    commit::hash::CommitHash,
    store::{Store, error::StoreError, read::StoreRead},
    table::{RowId, RowView},
};

use super::{RowHandle, TxnInner, TxnValue};

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

    pub fn add(
        &mut self,
        table: &ir::Path,
        values: Vec<TxnValue>,
    ) -> Result<RowHandle, StoreError> {
        self.inner.add(&self.store, table, values)
    }

    pub fn abort(self) -> Store {
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
    fn scan_table(&self, table: &ir::Path) -> Option<impl Iterator<Item = RowView> + '_> {
        self.store.scan_table(table)
    }

    fn row_by_handle(&self, table: &ir::Path, handle: &RowHandle) -> Option<RowView> {
        self.store.row_by_handle(table, handle)
    }

    fn row_by_id(&self, table: &ir::Path, row_id: RowId) -> Option<RowView> {
        self.store.row_by_id(table, row_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
    use crate::table::ValidationError;

    fn table_schema(columns: Vec<ColumnEntry>, primary_key: Option<Vec<Path>>) -> Schema {
        Schema {
            entity_variant: EntityVariant::Table,
            columns,
            primary_key,
        }
    }

    fn int_col(name: &str) -> ColumnEntry {
        ColumnEntry {
            path: Path::from(name),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            },
        }
    }

    #[test]
    fn owned_transaction_commits_and_returns_updated_store() {
        let path = Path::from("T");
        let schema = table_schema(vec![int_col("c0")], None);
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");

        let mut tx = OwnedTransaction::new(store);
        tx.add(&path, vec![42_i64.into()]).expect("add");

        let (_hash, committed) = tx.commit().expect("commit");
        assert_eq!(committed.table_at(&path).expect("T").row_count(), 1);
    }

    #[test]
    fn owned_transaction_add_validates_table_and_column_count() {
        let path = Path::from("T");
        let schema = table_schema(vec![int_col("c0")], None);
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");

        let mut tx = OwnedTransaction::new(store);
        let err = tx
            .add(&Path::from("missing"), vec![1_i64.into()])
            .unwrap_err();
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::UnknownTable { .. })
        ));

        let err = tx.add(&path, vec![1_i64.into(), 2_i64.into()]).unwrap_err();
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::ColumnCount { .. })
        ));
    }

    #[test]
    fn owned_transaction_store_read_sees_committed_rows_not_pending() {
        let path = Path::from("T");
        let schema = table_schema(vec![int_col("c0")], None);
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");

        let mut tx = OwnedTransaction::new(store);
        tx.add(&path, vec![1_i64.into()]).expect("add");
        let (_hash, store) = tx.commit().expect("commit");

        let mut tx = OwnedTransaction::new(store);
        let rows: Vec<_> = tx.scan_table(&path).expect("T").collect();
        assert_eq!(rows.len(), 1);
        assert!(tx.row_by_id(&path, rows[0].row_id).is_some());

        tx.add(&path, vec![2_i64.into()]).expect("add pending");
        assert_eq!(tx.scan_table(&path).expect("T").count(), 1);
        let _store = tx.abort();
    }
}
