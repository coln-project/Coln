use geolog_lang::ir;

use crate::{
    commit::hash::CommitHash,
    store::{Store, StoreIntError},
    txn::ops::{TempRowId, TxnCellValue},
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

    pub fn add(
        &mut self,
        table: &ir::Path,
        values: Vec<TxnCellValue>,
    ) -> Result<TempRowId, Box<StoreIntError>> {
        self.inner.add(self.store, table, values)
    }

    pub fn commit(self) -> Result<CommitHash, Box<StoreIntError>> {
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
        values: Vec<TxnCellValue>,
    ) -> Result<TempRowId, Box<StoreIntError>> {
        self.inner.add(&self.store, table, values)
    }

    pub fn commit(mut self) -> Result<(CommitHash, Store), (Box<StoreIntError>, Store)> {
        match self.inner.commit(&mut self.store) {
            Ok(hash) => Ok((hash, self.store)),
            Err(err) => Err((err, self.store)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OwnedTransaction;
    use crate::ir::{ColType, Path, PrimType, Schema};
    use crate::store::test_support::link_foreign_key_theory;
    use crate::store::{Store, StoreIntError};
    use crate::table::{CellValue, Table, ValidationError};

    #[test]
    fn owned_transaction_commits_and_returns_updated_store() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let mut tx = OwnedTransaction::new(store);
        tx.add(&path, vec![42_i64.into()]).expect("add");

        let (_hash, committed) = tx.commit().expect("commit");
        assert_eq!(committed.table_at(&path).expect("T").row_count(), 1);
    }

    #[test]
    fn owned_transaction_add_validates_table_and_column_count() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let mut tx = OwnedTransaction::new(store);
        let err = tx
            .add(&Path::from("missing"), vec![1_i64.into()])
            .unwrap_err();
        assert!(matches!(
            *err,
            StoreIntError::Validation(ValidationError::UnknownTable { .. })
        ));

        let err = tx.add(&path, vec![1_i64.into(), 2_i64.into()]).unwrap_err();
        assert!(matches!(
            *err,
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
        assert!(matches!(*err, StoreIntError::Law(_)));
        assert_eq!(recovered.table_at(&link).expect("Link").row_count(), 0);
    }

    #[test]
    fn transaction_resolves_pending_row_references_with_commit_hash() {
        let nodes = Path::from("Nodes");
        let edges = Path::from("Edges");
        let mut store = Store::new();
        store.insert_table(
            nodes.clone(),
            Table::new(
                nodes.clone(),
                Schema {
                    columns: vec![],
                    primary_key: None,
                },
            ),
        );
        store.insert_table(
            edges.clone(),
            Table::new(
                edges.clone(),
                Schema {
                    columns: vec![ColType::EntityType {
                        path: nodes.clone(),
                    }],
                    primary_key: None,
                },
            ),
        );

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
        assert_eq!(edge.cell_at(0, 0), Some(&CellValue::Id(node_id)));
    }

    #[test]
    fn transaction_commit_updates_commit_graph_heads_and_deps() {
        let path = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        let mut tx = store.transaction();
        tx.add(&path, vec![CellValue::Int(1).into()])
            .expect("add first row");
        let first = tx.commit().expect("first commit");

        assert!(store.commits().contains(&first));
        assert_eq!(store.commits().parents_of(&first), Some([].as_slice()));
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
