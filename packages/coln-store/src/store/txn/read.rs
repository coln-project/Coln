// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Read transactions over committed or tentatively applied store data.

use std::sync::Arc;

use coln_flir_rs::engine::packed::{PackedRowId, StoreScalarValue, StoreTuple};
use coln_flir_rs::engine::table::{STReadTxn, SortedTable, StoreST};
use coln_flir_rs::engine::tx::TxRowId;
use coln_flir_rs::hash::CommitHash;
use coln_flir_rs::public::PublicRowId;
use parking_lot::{ArcRwLockReadGuard, RawRwLock};

use crate::IdLookup;
use crate::commit::DataCommit;
use crate::ir;
use crate::store::{Catalog, Store, StoreData};
use crate::table::{TableHandle, TableOid};

/// Borrow a read view without acquiring another lock.
pub trait ReadTxnable {
    fn read(&self) -> ReadView<'_>;
}

/// An owned read transaction that keeps store data alive and locked for reading.
///
/// The transaction remains usable after the originating store is dropped.
/// Writers wait until it is dropped, including when it has no active views.
pub struct ReadTxn {
    catalog: Arc<Catalog>,
    pub(in crate::store) data: ArcRwLockReadGuard<RawRwLock, StoreData>,
}

/// A read view borrowing data protected by an existing transaction's lock.
///
/// A preparing write's view excludes queued operations. A pending write's view
/// includes tentative data, but its commit is not yet in the commit graph.
/// Views and their table handles cannot outlive the transaction they borrow.
///
/// ```compile_fail
/// use coln_store::store::Store;
/// let txn = Store::new().begin_read();
/// let view = txn.read();
/// drop(txn);
/// view.heads();
/// ```
///
/// ```compile_fail
/// use coln_store::store::Store;
/// use coln_flir_rs::ir::Path;
/// let txn = Store::new().begin_read();
/// let table = txn.read().table_at(&Path::from("T")).unwrap();
/// drop(txn);
/// table.row_count();
/// ```
///
/// ```compile_fail
/// use coln_store::store::Store;
/// let store = Store::new();
/// let txn = store.begin_write();
/// let view = txn.read();
/// drop(txn);
/// view.heads();
/// ```
pub struct ReadView<'a> {
    pub(in crate::store) catalog: &'a Catalog,
    pub(in crate::store) data: &'a StoreData,
}

impl Store {
    pub fn begin_read(&self) -> ReadTxn {
        ReadTxn {
            catalog: Arc::clone(&self.catalog),
            data: self.data.read_arc(),
        }
    }
}

impl ReadTxnable for ReadTxn {
    fn read(&self) -> ReadView<'_> {
        self.read()
    }
}

impl ReadTxnable for ReadView<'_> {
    fn read(&self) -> ReadView<'_> {
        ReadView::borrowed(self.catalog, self.data)
    }
}

impl StoreST for Store {
    fn begin_read(&self) -> impl STReadTxn {
        Store::begin_read(self)
    }
}

impl STReadTxn for ReadTxn {
    fn sorted_table_at(
        &self,
        table: &ir::Path,
    ) -> Option<impl SortedTable<Value = StoreScalarValue>> {
        self.read().table_at(table)?.sorted_copies().pop()
    }
}

impl STReadTxn for ReadView<'_> {
    fn sorted_table_at(
        &self,
        table: &ir::Path,
    ) -> Option<impl SortedTable<Value = StoreScalarValue>> {
        self.table_at(table)?.sorted_copies().pop()
    }
}

impl ReadTxn {
    pub fn read(&self) -> ReadView<'_> {
        ReadView::borrowed(&self.catalog, &self.data)
    }

    pub fn has_pending(&self) -> bool {
        self.read().has_pending()
    }

    pub fn table(&self, oid: TableOid) -> Option<TableHandle<'_>> {
        self.read().table(oid)
    }

    pub fn table_at(&self, path: &ir::Path) -> Option<TableHandle<'_>> {
        self.read().table_at(path)
    }

    pub fn tables(&self) -> impl Iterator<Item = (TableOid, TableHandle<'_>)> + '_ {
        self.read().tables()
    }

    pub fn heads(&self) -> Vec<CommitHash> {
        self.read().heads()
    }

    pub fn commit_by_hash(&self, hash: &CommitHash) -> Option<&DataCommit> {
        self.read().commit_by_hash(hash)
    }

    pub fn id_lookup(&self) -> &impl IdLookup {
        self.read().id_lookup()
    }

    pub fn canonical_row_id(&self, row_id: &PublicRowId) -> Option<PublicRowId> {
        self.read().canonical_row_id(row_id)
    }

    pub fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxRowId>,
        hash: CommitHash,
    ) -> Vec<PublicRowId> {
        self.read().promote(pending_ids, hash)
    }

    pub fn promote_one(&self, pending_id: impl Into<TxRowId>, hash: CommitHash) -> PublicRowId {
        self.read().promote_one(pending_id, hash)
    }

    pub fn scan_table(&self, table: &ir::Path) -> Option<Vec<StoreTuple>> {
        self.read().scan_table(table)
    }

    pub fn row_by_id(&self, table: &ir::Path, row_id: &PackedRowId) -> Option<StoreTuple> {
        self.read().row_by_id(table, row_id)
    }
}

impl<'a> ReadView<'a> {
    pub(in crate::store) fn borrowed(catalog: &'a Catalog, data: &'a StoreData) -> Self {
        Self { catalog, data }
    }

    pub fn has_pending(&self) -> bool {
        self.data.has_pending()
    }

    pub fn table(&self, oid: TableOid) -> Option<TableHandle<'a>> {
        self.data
            .tables
            .get(&oid)
            .map(|t| TableHandle::new(t, &self.data.id_packer, &self.data.rowing))
    }

    pub fn table_at(&self, path: &ir::Path) -> Option<TableHandle<'a>> {
        let oid = self.catalog.resolve_table(path)?;
        self.table(oid)
    }

    pub fn tables(&self) -> impl Iterator<Item = (TableOid, TableHandle<'a>)> + use<'a> {
        let data = self.data;
        data.tables
            .iter()
            .map(move |(&oid, t)| (oid, TableHandle::new(t, &data.id_packer, &data.rowing)))
    }

    pub fn heads(&self) -> Vec<CommitHash> {
        self.data.heads()
    }

    /// Look up a data commit. Access the root through
    /// [`CommitGraph::root_commit`](crate::commit::graph::CommitGraph::root_commit).
    pub fn commit_by_hash(&self, hash: &CommitHash) -> Option<&'a DataCommit> {
        self.data.commit_by_hash(hash)
    }

    /// Read-only access to the id dictionary, for converting between public
    /// and packed row ids.
    pub fn id_lookup(&self) -> &'a (impl IdLookup + use<'a>) {
        &self.data.id_packer
    }

    /// Resolve a row id to the id its structural class is stored under.
    pub fn canonical_row_id(&self, row_id: &PublicRowId) -> Option<PublicRowId> {
        let packed = self.data.id_packer.packed(row_id)?;
        let canonical = self.data.rowing.canonical_id(&packed, &self.data.id_packer);
        Some(self.data.id_packer.unpack_row_id(canonical))
    }

    /// Resolve pending ids against the commit `hash` they were committed in,
    /// and canonicalise every id.
    pub fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxRowId>,
        hash: CommitHash,
    ) -> Vec<PublicRowId> {
        pending_ids
            .into_iter()
            .map(|pending| {
                let wire_id = match pending {
                    TxRowId::Pending(pending) => pending.resolve(hash),
                    TxRowId::Existing(row_id) => row_id,
                };
                self.canonical_row_id(&wire_id).unwrap_or(wire_id)
            })
            .collect()
    }

    pub fn promote_one(&self, pending_id: impl Into<TxRowId>, hash: CommitHash) -> PublicRowId {
        self.promote(std::iter::once(pending_id.into()), hash)
            .pop()
            .expect("one id to promote")
    }

    // The methods below return owned values for the external world.

    pub fn scan_table(&self, table: &ir::Path) -> Option<Vec<StoreTuple>> {
        self.table_at(table).map(|table| table.scan().collect())
    }

    /// Canonicalises `row_id` on read, but does not change it.
    pub fn row_by_id(&self, table: &ir::Path, row_id: &PackedRowId) -> Option<StoreTuple> {
        self.table_at(table)?.row_by_id(row_id)
    }
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::store::error::StoreError;
    use crate::test_utils::{commit_int_store, int_schema, single_int_store};

    #[rstest]
    fn owned_read_survives_store_drop(#[from(commit_int_store)] committed: (Store, CommitHash)) {
        let (store, hash) = committed;
        let txn = store.begin_read();
        let encoded = txn.encode().expect("encode before store drop");
        drop(store);

        let view = txn.read();
        let path = ir::Path::from("T");
        assert_eq!(view.heads(), vec![hash]);
        assert_eq!(view.table_at(&path).expect("T").row_count(), 1);
        assert!(view.commit_by_hash(&hash).is_some());
        assert_eq!(view.encode().expect("encode after store drop"), encoded);
        let sorted = view.sorted_table_at(&path).expect("sorted T");
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted.value(0, 1), Some(StoreScalarValue::I32(42)));
    }

    #[test]
    fn read_lock_is_held_until_all_read_transactions_are_dropped() {
        let store = Store::new();
        let first = store.begin_read();
        let second = store.begin_read();

        assert!(store.data.try_write().is_none());
        assert!(store.data.try_upgradable_read().is_some());
        drop(first);
        assert!(store.data.try_write().is_none());
        drop(second);
        assert!(store.data.try_write().is_some());
    }

    #[test]
    fn dropping_the_last_read_releases_store_resources() {
        let store = Store::new();
        let catalog = Arc::downgrade(&store.catalog);
        let data = Arc::downgrade(&store.data);
        let txn = store.begin_read();
        drop(store);

        assert!(catalog.upgrade().is_some());
        assert!(data.upgrade().is_some());
        drop(txn);
        assert!(catalog.upgrade().is_none());
        assert!(data.upgrade().is_none());
    }

    #[rstest]
    fn handles_and_iterators_borrow_the_transaction_not_the_temporary_view(
        #[from(commit_int_store)] committed: (Store, CommitHash),
    ) {
        let (store, hash) = committed;
        let txn = store.begin_read();
        let table = txn.read().table_at(&ir::Path::from("T")).expect("T");
        let tables = txn.read().tables();
        let commit = txn.read().commit_by_hash(&hash).expect("commit");
        let lookup = txn.read().id_lookup();
        drop(store);

        assert_eq!(table.scan().count(), 1);
        assert_eq!(tables.count(), 1);
        assert_eq!(commit.hash(), hash);
        let tuple = table.scan().next().expect("row");
        let StoreScalarValue::RowId(id) = tuple.first().expect("row id") else {
            panic!("first column is a row id");
        };
        assert_eq!(lookup.unpacked(id).expect("public row id").commit, hash);
    }

    #[rstest]
    fn schema_changes_require_read_transactions_to_finish(
        #[from(single_int_store)] mut store: Store,
    ) {
        let txn = store.begin_read();
        let heads = txn.heads();
        let path = ir::Path::from("Other");
        let schema = int_schema(vec!["value"], None);

        assert!(matches!(
            store.create_table(path.clone(), schema.clone()),
            Err(StoreError::StoreInUse)
        ));
        assert_eq!(store.table_count(), 1);
        assert!(store.resolve_table(&path).is_none());
        assert_eq!(txn.heads(), heads);
        assert!(txn.read().table_at(&path).is_none());

        drop(txn);
        let oid = store
            .create_table(path.clone(), schema)
            .expect("create table");
        let read = store.begin_read();
        assert_eq!(read.table_at(&path).expect("Other").oid(), oid);
    }
}
