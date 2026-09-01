// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod inner;
mod owned;
mod row_handle;
mod timestamp;

use coln_flir_rs::ir;

use crate::{
    commit::hash::CommitHash,
    store::{Store, error::StoreError, read::StoreRead},
    table::{RowId, RowView},
};

use inner::TxnInner;
pub use owned::OwnedTransaction;
pub(crate) use row_handle::{PendingOp, RowRef, TempRowId, TxnCellValue};
pub use row_handle::{RowHandle, TxnId, TxnValue};

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
    ) -> Result<RowHandle, StoreError> {
        self.inner.add(self.store, table, values)
    }

    // Used by the REPL only
    #[cfg(feature = "native")]
    pub(crate) fn add_internal(
        &mut self,
        table: &ir::Path,
        values: Vec<TxnCellValue>,
    ) -> Result<TempRowId, StoreError> {
        self.inner.add_internal(self.store, table, values)
    }

    pub fn commit(self) -> Result<CommitHash, StoreError> {
        self.inner.commit(self.store)
    }
    // pub fn commit_with(mut self, opts: CommitOptions) -> Result<CommitHash, StoreIntError> { ... }

    pub fn abort(self) {
        self.inner.abort()
    }
}

impl StoreRead for Transaction<'_> {
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
    fn abort_invalidates_returned_row_handles() {
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
        tx.abort();
        let err = node.row_id().expect_err("abort invalidates handle");
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::InvalidRowHandle { .. })
        ));
        let mut tx = store.transaction();
        let err = tx
            .add(&edges, vec![node.into()])
            .expect_err("aborted handle cannot be reused");
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::InvalidRowHandle { .. })
        ));
        assert_eq!(store.table_at(&nodes).expect("Nodes").row_count(), 0);
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
            StoreError::Validation(ValidationError::DuplicatePrimaryKey)
        ));

        let err = node.row_id().expect_err("failed commit invalidates handle");
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::InvalidRowHandle { .. })
        ));

        let mut tx = store.transaction();
        let err = tx
            .add(&edges, vec![node.into()])
            .expect_err("invalid handle cannot be reused");
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::InvalidRowHandle { .. })
        ));
    }

    /// A handle whose row deduplicates into an existing structural class
    /// finalizes to the id the store actually kept, not to its raw
    /// `(commit, counter)` id, which names no stored row. The first handle
    /// may still go stale when the second commit wins the merge; reading
    /// through `row_by_handle` resolves and repairs it.
    #[test]
    fn deduplicated_row_handle_finalizes_to_canonical_id() {
        let term = Path::from("Term");
        let mut store = Store::new();
        store
            .create_table(term.clone(), table_schema(vec![int_col("value")], None))
            .expect("create term table");
        store.set_structural_index_for_test(&term, true);

        let mut tx = store.transaction();
        let first = tx.add(&term, vec![7_i64.into()]).expect("add first term");
        tx.commit().expect("commit first term");

        // Structurally equal row: deduplicates into the first row's class.
        // Which id wins the merge depends on the commit hash ordering.
        let mut tx = store.transaction();
        let second = tx.add(&term, vec![7_i64.into()]).expect("add equal term");
        tx.commit().expect("commit equal term");

        let stored = store
            .table_at(&term)
            .expect("Term")
            .row_id_at(0)
            .expect("one stored row");
        assert_eq!(store.table_at(&term).expect("Term").row_count(), 1);

        // The second handle is born canonical, whether its row was kept old
        // or won the merge.
        assert_eq!(second.row_id().expect("finalized"), stored);

        // The first handle resolves through the store even if its id went
        // stale, and the read writes the canonical id back into the handle.
        let view = store
            .row_by_handle(&term, &first)
            .expect("class row is stored");
        assert_eq!(view.row_id, stored);
        assert_eq!(first.row_id().expect("finalized"), stored);
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

    /// We provide read-committed isolation guarantee. So uncommitted data will
    /// not be seen.
    #[test]
    fn transaction_store_read_sees_committed_rows_not_pending() {
        let path = Path::from("T");
        let schema = table_schema(vec![int_col("c0")], None);
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");

        let mut tx = store.transaction();
        tx.add(&path, vec![1_i64.into()]).expect("add");
        tx.commit().expect("commit");

        let mut tx = store.transaction();
        let rows: Vec<_> = tx.scan_table(&path).expect("T").collect();
        assert_eq!(rows.len(), 1);
        assert!(tx.row_by_id(&path, rows[0].row_id).is_some());
        assert!(
            tx.row_by_handle(&path, &RowHandle::from_existing(rows[0].row_id))
                .is_some()
        );

        tx.add(&path, vec![2_i64.into()]).expect("add pending");
        assert_eq!(tx.scan_table(&path).expect("T").count(), 1);
        tx.abort();
    }

    /// Committing an empty transaction does not modify the commit graph
    #[test]
    fn txn_empty_commit_not_added() {
        let mut store = Store::new();
        let root = store.commits().root_commit().expect("root commit").hash();
        let heads: Vec<_> = store.commits().heads().copied().collect();
        let commits: Vec<_> = store
            .commits()
            .iter_topological()
            .map(|c| c.hash())
            .collect();

        let empty = store.transaction().commit().expect("empty commit");

        assert!(!store.commits().contains(&empty));
        assert_eq!(
            store.commits().root_commit().expect("root commit").hash(),
            root
        );
        assert_eq!(store.commits().heads().copied().collect::<Vec<_>>(), heads);
        assert_eq!(
            store
                .commits()
                .iter_topological()
                .map(|c| c.hash())
                .collect::<Vec<_>>(),
            commits
        );
    }
}
