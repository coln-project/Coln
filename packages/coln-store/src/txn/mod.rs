// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub mod id;
mod inner;
mod owned;
pub mod rw;
mod timestamp;

use crate::{
    store::{Store, error::StoreError},
    txn::rw::{StoreRead, StoreWrite},
};
#[cfg(feature = "native")]
use coln_flir_rs::engine::txn_val::{TempRowId, TxnWireTuple};
use coln_flir_rs::{
    WireRowId, WireRowView, WireTuple, engine::txn_val::TxnWireRowId, hash::CommitHash, ir,
    query::WhereClause,
};

pub(crate) use id::PendingOp;
pub use id::empty_row;
use inner::TxnInner;
pub use owned::OwnedTransaction;

pub struct ReadOnly<'a> {
    store: &'a Store,
}
pub struct ReadWrite<'a> {
    store: &'a mut Store,
}

pub trait Mode {
    fn store(&self) -> &Store;
}

impl<'a> Mode for ReadOnly<'a> {
    fn store(&self) -> &Store {
        self.store
    }
}

impl<'a> Mode for ReadWrite<'a> {
    fn store(&self) -> &Store {
        self.store
    }
}

pub struct Transaction<M> {
    inner: TxnInner,
    mode: M,
    // Need to know if txn is still open to implement Drop
    // but not checking this in every txn because the type system ensures
    // that no method can be called on a closed txn
    open: bool,
}

impl<'a> Transaction<ReadOnly<'a>> {
    pub(crate) fn new(store: &'a Store) -> Self {
        let deps = store.commits().heads().copied().collect();
        Self {
            inner: TxnInner::new(deps),
            mode: ReadOnly { store },
            open: true,
        }
    }
}

impl<'a> Transaction<ReadWrite<'a>> {
    pub(crate) fn new(store: &'a mut Store) -> Self {
        let deps = store.commits().heads().copied().collect();
        Self {
            inner: TxnInner::new(deps),
            mode: ReadWrite { store },
            open: true,
        }
    }

    // Used by the REPL only
    #[cfg(feature = "native")]
    pub(crate) fn add_internal(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxnWireTuple>,
    ) -> Result<TempRowId, StoreError> {
        self.inner.add_internal(self.mode.store, table, values)
    }

    pub fn commit(mut self) -> Result<CommitHash, StoreError> {
        let h = self.inner.commit(self.mode.store);
        self.open = false;
        h
    }

    // pub fn commit_with(mut self, opts: CommitOptions) -> Result<CommitHash, StoreIntError> { ... }

    pub fn abort(mut self) {
        self.open = false;
        self.inner.abort()
    }
}

impl<M: Mode> StoreRead for Transaction<M> {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<WireRowView>> {
        self.mode
            .store()
            .scan_table_iter(table)
            .map(|rows| rows.collect())
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &WireRowId) -> Option<WireRowView> {
        self.mode.store().row_by_id_inner(table, row_id)
    }

    fn all_proj(&self, query: &WhereClause, select: &[u32]) -> Result<Vec<WireTuple>, StoreError> {
        self.mode.store().all_proj_inner(query, select)
    }

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<WireRowId>, StoreError> {
        self.mode.store().all_row_id_inner(query)
    }
}

impl StoreWrite for Transaction<ReadWrite<'_>> {
    fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxnWireTuple>,
    ) -> Result<TxnWireRowId, StoreError> {
        self.inner.add(self.mode.store(), table, values)
    }
}

impl<M> Drop for Transaction<M> {
    fn drop(&mut self) {
        if self.open {
            // This is fine for RO txn, because there will be no handles
            self.inner.abort();
        }
    }
}

#[cfg(test)]
mod tests {

    use coln_flir_rs::WireValue;
    use coln_flir_rs::engine::txn_val::TxnWireValue;
    use rstest::rstest;

    use super::*;
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
    use crate::table::ValidationError;
    use crate::test_utils::{nodes_edges_store, single_int_store, single_memoized_int_store};
    use crate::txn::id::{Promote, empty_row};

    #[rstest]
    fn validates_then_applies(#[from(single_int_store)] mut store: Store) {
        let path = Path::from("T");

        let mut txn = store.transaction();
        txn.add(&path, vec![1i32]).expect("first add");
        txn.add(&path, vec![2i32]).expect("second add");

        txn.commit().expect("commit");

        assert_eq!(store.table_at(&path).expect("T").row_count(), 2);
    }

    /// Covers the same rollback guarantee as the old `transact` test: if validation fails,
    /// no rows from the batch are committed (here the second op references an unregistered table).
    #[rstest]
    fn unknown_table_leaves_store_unchanged(#[from(single_int_store)] mut store: Store) {
        let path = Path::from("T");

        let err = {
            let mut txn = store.transaction();
            txn.add(&path, vec![1i32]).expect("first add");
            txn.add(&Path::from("missing"), vec![2i32]).unwrap_err()
        };

        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::UnknownTable { .. })
        ));
        assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
    }

    #[test]
    fn duplicate_primary_key_within_batch() {
        let path = Path::from("T");
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: Some(vec![0u64]),
        };
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");

        let mut txn = store.transaction();
        txn.add(&path, vec![1i32]).expect("first add");
        txn.add(&path, vec![1i32]).expect("second add");
        let err = txn.commit().unwrap_err();

        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::DuplicatePrimaryKey)
        ));
        assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
    }

    #[rstest]
    fn single_insert_commits(#[from(single_int_store)] mut store: Store) {
        let path = Path::from("T");

        let mut txn = store.transaction();
        let row_id = txn.add(&path, vec![42i32]).expect("add");
        let h = txn.commit().expect("commit");
        let row_id = store.promote_one(row_id, h);

        assert_eq!(
            store.row_by_id(&path, &row_id),
            Some(WireRowView {
                row_id,
                values: vec![42i32.into()],
            })
        );
    }

    #[rstest]
    fn transaction_resolves_pending_row_references_with_commit_hash(
        #[from(nodes_edges_store)] mut store: Store,
    ) {
        let nodes = Path::from("Nodes");
        let edges = Path::from("Edges");

        let mut tx = store.transaction();
        let node_temp = tx.add(&nodes, empty_row()).expect("add node");
        let edge_temp = tx
            .add(&edges, vec![TxnWireValue::Id(node_temp.clone())])
            .expect("add edge");
        let h = tx.commit().expect("commit");
        let ids = store.promote([node_temp, edge_temp], h);
        let [node_id, edge_id] = ids.try_into().unwrap();

        assert_eq!(node_id.commit, h);
        assert_eq!(node_id.counter, 0);
        assert_eq!(edge_id.commit, h);
        assert_eq!(edge_id.counter, 1);
        assert_eq!(
            store.row_by_id(&edges, &edge_id),
            Some(WireRowView {
                row_id: edge_id,
                values: vec![WireValue::Id(node_id)],
            })
        );
    }

    #[rstest]
    fn committed_row_handle_can_be_used_in_later_transaction(
        #[from(nodes_edges_store)] mut store: Store,
    ) {
        let nodes = Path::from("Nodes");
        let edges = Path::from("Edges");

        let mut tx = store.transaction();
        let node = tx.add(&nodes, empty_row()).expect("add node");
        let first_commit = tx.commit().expect("commit node");

        let node_id = store.promote_one(node, first_commit);
        assert_eq!(node_id.commit, first_commit);

        let mut tx = store.transaction();
        let edge = tx
            .add(&edges, vec![TxnWireValue::Id(node_id.clone().into())])
            .expect("add edge");
        let h = tx.commit().expect("commit edge");

        let edge_id = store.promote_one(edge, h);

        assert_eq!(
            store.row_by_id(&edges, &edge_id),
            Some(WireRowView {
                row_id: edge_id,
                values: vec![WireValue::Id(node_id)],
            })
        );
    }

    /// A pending id whose row deduplicates into an existing structural class
    /// promotes to the id the store actually kept, not to its raw
    /// `(commit, counter)` id, which names no stored row. The first id
    /// may still go stale when the second commit wins the merge; promoting
    /// the existing id again resolves it to the canonical id.
    #[rstest]
    fn deduplicated_row_handle_finalizes_to_canonical_id(
        #[from(single_memoized_int_store)] mut store: Store,
    ) {
        let term = Path::from("T");

        let mut tx = store.transaction();
        let first = tx.add(&term, vec![7_i32]).expect("add first term");
        let h1 = tx.commit().expect("commit first term");
        let first = store.promote_one(first, h1);

        // Structurally equal row: deduplicates into the first row's class.
        // Which id wins the merge depends on the commit hash ordering.
        let mut tx = store.transaction();
        let second = tx.add(&term, vec![7_i32]).expect("add equal term");
        let h2 = tx.commit().expect("commit equal term");

        let stored = store.promote_one(second, h2);
        assert_eq!(store.scan_table(&term).expect("Term").len(), 1);

        // Reads return the canonical id. Promoting an already-existing id
        // also canonicalises, so a stale first id catches up.
        let view = store.row_by_id(&term, &first).expect("class row is stored");
        assert_eq!(view.row_id, stored);
        let first = store.promote_one(first, h1);
        assert_eq!(first, stored);
    }

    #[rstest]
    fn transaction_commit_updates_commit_graph_heads_and_deps(
        #[from(single_int_store)] mut store: Store,
    ) {
        let path = Path::from("T");
        let root = store.commits().root_commit().expect("root commit").hash();

        let mut tx = store.transaction();
        tx.add(&path, vec![1i32]).expect("add first row");
        let first = tx.commit().expect("first commit");

        assert!(store.commits().contains(&first));
        assert_eq!(store.commits().parents_of(&first), Some([root].as_slice()));
        assert_eq!(
            store.commits().heads().copied().collect::<Vec<_>>(),
            vec![first]
        );

        let mut tx = store.transaction();
        tx.add(&path, vec![2i32]).expect("add second row");
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
    #[rstest]
    fn transaction_store_read_sees_committed_rows_not_pending(
        #[from(single_int_store)] mut store: Store,
    ) {
        let path = Path::from("T");

        let mut tx = store.transaction();
        let pending_id = tx.add(&path, vec![1i32]).expect("add");
        let h = tx.commit().expect("commit");
        let row_id = store.promote_one(pending_id, h);

        let mut tx = store.transaction();
        assert_eq!(
            tx.row_by_id(&path, &row_id),
            Some(WireRowView {
                row_id,
                values: vec![1i32.into()],
            })
        );

        tx.add(&path, vec![2i32]).expect("add pending");
        assert_eq!(tx.scan_table(&path).expect("T").len(), 1);
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
