// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Read transactions over committed or tentatively applied store data.

use std::ops::Deref;

use coln_flir_rs::engine::packed::{PackedRowId, StoreScalarValue, StoreTuple};
use coln_flir_rs::engine::table::{ReadTxnable, SortedTable, StoreRead};
use coln_flir_rs::engine::tx::TxRowId;
use coln_flir_rs::hash::CommitHash;
use coln_flir_rs::public::PublicRowId;
use parking_lot::RwLockReadGuard;

use crate::IdLookup;
use crate::commit::DataCommit;
use crate::ir;
use crate::store::{Catalog, Store, StoreData};
use crate::table::{TableHandle, TableOid};

/// A consistent read view backed by a held store lock.
///
/// Standalone transactions own a read lock. Views borrowed from a write
/// transaction reuse its lock and cannot outlive that borrow. A preparing
/// write's view excludes queued operations; a pending write's view includes
/// tentatively applied data, but its commit is not yet in the commit graph.
pub struct ReadTxn<'a> {
    pub(in crate::store) catalog: &'a Catalog,
    pub(in crate::store) data: ReadData<'a>,
}

pub(in crate::store) enum ReadData<'a> {
    Locked(RwLockReadGuard<'a, StoreData>),
    Borrowed(&'a StoreData),
}

impl Deref for ReadData<'_> {
    type Target = StoreData;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Locked(guard) => guard,
            Self::Borrowed(data) => data,
        }
    }
}

impl Store {
    pub fn begin_read(&self) -> ReadTxn<'_> {
        ReadTxn {
            catalog: &self.catalog,
            data: ReadData::Locked(self.data.read()),
        }
    }
}

impl ReadTxnable for Store {
    fn begin_read(&self) -> impl StoreRead {
        Store::begin_read(self)
    }
}

impl StoreRead for ReadTxn<'_> {
    fn sorted_table_at(
        &self,
        table: &ir::Path,
    ) -> Option<impl SortedTable<Value = StoreScalarValue>> {
        self.table_at(table)?.sorted_copies().pop()
    }
}

impl<'a> ReadTxn<'a> {
    pub(in crate::store) fn borrowed(catalog: &'a Catalog, data: &'a StoreData) -> Self {
        Self {
            catalog,
            data: ReadData::Borrowed(data),
        }
    }

    pub fn has_pending(&self) -> bool {
        self.data.has_pending()
    }

    pub fn table(&self, oid: TableOid) -> Option<TableHandle<'_>> {
        self.data
            .tables
            .get(&oid)
            .map(|t| TableHandle::new(t, &self.data.id_packer, &self.data.rowing))
    }

    pub fn table_at(&self, path: &ir::Path) -> Option<TableHandle<'_>> {
        let oid = self.catalog.resolve_table(path)?;
        self.table(oid)
    }

    pub fn tables(&self) -> impl Iterator<Item = (TableOid, TableHandle<'_>)> + '_ {
        self.data.tables.iter().map(move |(&oid, t)| {
            (
                oid,
                TableHandle::new(t, &self.data.id_packer, &self.data.rowing),
            )
        })
    }

    pub fn heads(&self) -> Vec<CommitHash> {
        self.data.commits.heads().cloned().collect()
    }

    /// Look up a data commit. Access the root through
    /// [`CommitGraph::root_commit`](crate::commit::graph::CommitGraph::root_commit).
    pub fn commit_by_hash(&self, hash: &CommitHash) -> Option<&DataCommit> {
        self.data.commits.get_data(hash)
    }

    /// Read-only access to the id dictionary, for converting between public
    /// and packed row ids.
    pub fn id_lookup(&self) -> &impl IdLookup {
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
