// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir;

#[cfg(feature = "native")]
use crate::txn::ops::{TempRowId, TxnCellValue};
use crate::{
    commit::hash::CommitHash,
    store::{Store, error::StoreIntError},
    txn::ops::{RowHandle, TxnValue},
};

mod inner;
pub mod ops;
mod timestamp;

use inner::TxnInner;

pub struct Transaction<'a> {
    inner: TxnInner,
    store: &'a mut Store,
}

impl<'a> Transaction<'a> {
    pub fn new(store: &'a mut Store) -> Self {
        let deps = store.commits().heads().copied().collect();
        Self {
            inner: TxnInner::new(deps),
            store,
        }
    }

    // TODO this API is a bit awkward to use, clients have to call .into() all
    // the time on their values
    pub fn add(
        &mut self,
        table: &ir::Path,
        values: Vec<TxnValue>,
    ) -> Result<RowHandle, StoreIntError> {
        self.inner.add(self.store, table, values)
    }

    // Used by the REPL only
    #[cfg(feature = "native")]
    pub(crate) fn add_internal(
        &mut self,
        table: &ir::Path,
        values: Vec<TxnCellValue>,
    ) -> Result<TempRowId, StoreIntError> {
        self.inner.add_internal(self.store, table, values)
    }

    pub fn commit(self) -> Result<CommitHash, StoreIntError> {
        self.inner.commit(self.store)
    }
    // pub fn commit_with(mut self, opts: CommitOptions) -> Result<CommitHash, StoreIntError> { ... }
}

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
    ) -> Result<RowHandle, StoreIntError> {
        self.inner.add(&self.store, table, values)
    }

    // We need to return Store to the user for roll back purposes, so the Err variant must be large
    #[allow(clippy::result_large_err)]
    pub fn commit(mut self) -> Result<(CommitHash, Store), (StoreIntError, Store)> {
        match self.inner.commit(&mut self.store) {
            Ok(hash) => Ok((hash, self.store)),
            Err(err) => Err((err, self.store)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
    use crate::store::test_support::link_foreign_key_theory;
    use crate::table::{CellValue, ValidationError};

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

    fn row_id_col(name: &str, path: Path) -> ColumnEntry {
        ColumnEntry {
            path: Path::from(name),
            col_type: ColType::RowId { path },
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
            StoreIntError::Validation(ValidationError::UnknownTable { .. })
        ));

        let err = tx.add(&path, vec![1_i64.into(), 2_i64.into()]).unwrap_err();
        assert!(matches!(
            err,
            StoreIntError::Validation(ValidationError::ColumnCount { .. })
        ));
    }

    #[test]
    fn owned_transaction_commit_err_returns_original_store() {
        let theory = link_foreign_key_theory();
        let link = Path::from("Link");
        let store = Store::try_from_theory(theory).expect("theory");

        let mut tx = OwnedTransaction::new(store);
        tx.add(&link, vec![10_i64.into(), 20_i64.into()])
            .expect("add");

        let (err, recovered) = tx.commit().unwrap_err();
        assert!(matches!(err, StoreIntError::Rule(_)));
        assert_eq!(recovered.table_at(&link).expect("Link").row_count(), 0);
    }

    #[test]
    fn transaction_resolves_pending_row_references_with_commit_hash() {
        let nodes = Path::from("Nodes");
        let edges = Path::from("Edges");
        let mut store = Store::new();
        store
            .create_table(nodes.clone(), table_schema(vec![], None))
            .expect("create nodes table");
        store
            .create_table(
                edges.clone(),
                table_schema(vec![row_id_col("node", nodes.clone())], None),
            )
            .expect("create edges table");

        let mut tx = store.transaction();
        let node_temp = tx.add(&nodes, vec![]).expect("add node");
        tx.add(&edges, vec![node_temp.into()]).expect("add edge");
        let commit = tx.commit().expect("commit");

        let node_id = store
            .table_at(&nodes)
            .expect("Nodes")
            .row_id_at(0)
            .expect("node row id");
        let edge = store.table_at(&edges).expect("Edges");
        let edge_id = edge.row_id_at(0).expect("edge row id");

        assert_eq!(node_id.commit, commit);
        assert_eq!(node_id.counter, 0);
        assert_eq!(edge_id.commit, commit);
        assert_eq!(edge_id.counter, 1);
        assert_eq!(edge.cell_at(0, 0), Some(CellValue::Id(node_id)));
    }

    #[test]
    fn committed_row_handle_can_be_used_in_later_transaction() {
        let nodes = Path::from("Nodes");
        let edges = Path::from("Edges");
        let mut store = Store::new();
        store
            .create_table(nodes.clone(), table_schema(vec![], None))
            .expect("create nodes table");
        store
            .create_table(
                edges.clone(),
                table_schema(vec![row_id_col("node", nodes.clone())], None),
            )
            .expect("create edges table");

        let mut tx = store.transaction();
        let node = tx.add(&nodes, vec![]).expect("add node");
        let first_commit = tx.commit().expect("commit node");

        let node_id = node.row_id().expect("node handle finalized");
        assert_eq!(node_id.commit, first_commit);

        let mut tx = store.transaction();
        tx.add(&edges, vec![node.into()]).expect("add edge");
        tx.commit().expect("commit edge");

        let edge = store.table_at(&edges).expect("Edges");
        assert_eq!(edge.cell_at(0, 0), Some(CellValue::Id(node_id)));
    }

    #[test]
    fn failed_transaction_invalidates_returned_row_handles() {
        let nodes = Path::from("Nodes");
        let edges = Path::from("Edges");
        let mut store = Store::new();
        store
            .create_table(nodes.clone(), table_schema(vec![], Some(vec![])))
            .expect("create nodes table");
        store
            .create_table(
                edges.clone(),
                table_schema(vec![row_id_col("node", nodes.clone())], None),
            )
            .expect("create edges table");

        let mut tx = store.transaction();
        let node = tx.add(&nodes, vec![]).expect("add first node");
        tx.add(&nodes, vec![]).expect("add duplicate singleton row");
        let err = tx
            .commit()
            .expect_err("duplicate singleton row should fail");
        assert!(matches!(
            err,
            StoreIntError::Validation(ValidationError::DuplicatePrimaryKey)
        ));

        let err = node.row_id().expect_err("failed commit invalidates handle");
        assert!(matches!(
            err,
            StoreIntError::Validation(ValidationError::InvalidRowHandle { .. })
        ));

        let mut tx = store.transaction();
        let err = tx
            .add(&edges, vec![node.into()])
            .expect_err("invalid handle cannot be reused");
        assert!(matches!(
            err,
            StoreIntError::Validation(ValidationError::InvalidRowHandle { .. })
        ));
    }

    #[test]
    fn transaction_commit_updates_commit_graph_heads_and_deps() {
        let path = Path::from("T");
        let schema = table_schema(vec![int_col("c0")], None);
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");
        let root = store.commits().root_commit().expect("root commit").hash();

        let mut tx = store.transaction();
        tx.add(&path, vec![CellValue::Int(1).into()])
            .expect("add first row");
        let first = tx.commit().expect("first commit");

        assert!(store.commits().contains(&first));
        assert_eq!(store.commits().parents_of(&first), Some([root].as_slice()));
        assert_eq!(
            store.commits().heads().copied().collect::<Vec<_>>(),
            vec![first]
        );

        let mut tx = store.transaction();
        tx.add(&path, vec![CellValue::Int(2).into()])
            .expect("add second row");
        let second = tx.commit().expect("second commit");

        assert!(store.commits().contains(&second));
        assert_eq!(
            store.commits().parents_of(&second),
            Some([first].as_slice())
        );
        assert_eq!(
            store.commits().heads().copied().collect::<Vec<_>>(),
            vec![second]
        );
    }
}
