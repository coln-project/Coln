// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    engine::{packed::StoreTuple, table::StoreRead, tx::TxRowId}, hash::CommitHash, ir, public::{PublicRowId, PublicTuple}, query::WhereClause,
};
use coln_store::store::{IdLookup, commit::CommitChunk, error::StoreError};

use crate::{bouncer::Bouncer, error::BouncerError};

/// A consistent view of bouncer data, with public row ids and tuples.
pub struct ReadTxn<'a> {
    pub(super) store: coln_store::store::ReadTxn<'a>,
}

impl Bouncer {
    pub fn begin_read(&self) -> ReadTxn<'_> {
        ReadTxn {
            store: self.store.begin_read(),
        }
    }
}

impl ReadTxn<'_> {
    pub fn heads(&self) -> Vec<CommitHash> {
        self.store.heads()
    }

    pub fn commit_chunks_after(&self, have_heads: &[CommitHash]) -> Vec<CommitChunk> {
        self.store.commit_chunks_after(have_heads)
    }

    pub fn encode_store(&self) -> Result<Vec<u8>, BouncerError> {
        Ok(self.store.encode().map_err(StoreError::from)?)
    }

    fn unpack_tuple(&self, tuple: StoreTuple) -> PublicTuple {
        PublicTuple::from_store(tuple, |id| {
            self.store
                .id_lookup()
                .unpacked(&id)
                .expect("stored id can be unpacked")
        })
    }

    pub fn scan_table(&self, table: &ir::Path) -> Option<Vec<PublicTuple>> {
        Some(
            self.store
                .scan_table(table)?
                .into_iter()
                .map(|tuple| self.unpack_tuple(tuple))
                .collect(),
        )
    }

    pub fn row_by_id(&self, table: &ir::Path, row_id: &PublicRowId) -> Option<PublicTuple> {
        let packed = self.store.id_lookup().packed(row_id)?;
        self.store
            .row_by_id(table, &packed)
            .map(|tuple| self.unpack_tuple(tuple))
    }

    pub fn canonical_row_id(&self, row_id: &PublicRowId) -> Option<PublicRowId> {
        self.store.canonical_row_id(row_id)
    }

    /// Promote ids after the transaction that produced them has committed.
    pub fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxRowId>,
        hash: CommitHash,
    ) -> Vec<PublicRowId> {
        self.store.promote(pending_ids, hash)
    }

    pub fn promote_one(&self, pending_id: impl Into<TxRowId>, hash: CommitHash) -> PublicRowId {
        self.store.promote_one(pending_id, hash)
    }

    // TODO: delegate filtering and projection to coln-query.
    pub fn all_proj(
        &self,
        _query: &WhereClause,
        _select: &[u32],
    ) -> Result<Vec<PublicTuple>, BouncerError> {
        todo!()
    }

    pub fn all_row_id(&self, _query: &WhereClause) -> Result<Vec<PublicRowId>, BouncerError> {
        todo!()
    }

    pub fn one_proj(
        &self,
        _query: &WhereClause,
        _select: &[u32],
    ) -> Result<PublicTuple, BouncerError> {
        todo!()
    }

    pub fn exists(&self, _query: &WhereClause) -> Result<bool, BouncerError> {
        todo!()
    }
}
