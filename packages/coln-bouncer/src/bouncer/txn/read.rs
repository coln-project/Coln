// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    engine::{packed::StoreTuple, tx::TxRowId},
    hash::CommitHash,
    ir,
    public::{PublicRowId, PublicTuple},
    query::WhereClause,
};
use coln_store::store::{
    IdLookup, ReadView, commit::Fragment, error::StoreError, txn::ReadTxnable as _,
};

use crate::{bouncer::Bouncer, error::BouncerError};

pub(in crate::bouncer) mod sealed {
    use coln_store::store::ReadView;

    /// Access to the underlying store view, hidden from users of the bouncer so
    /// that store-level (packed) ids and tuples do not leak through its API.
    pub trait StoreView {
        fn store_view(&self) -> ReadView<'_>;
    }
}

/// A consistent view of bouncer data, with public row ids and tuples.
///
/// Implemented by [`ReadTxn`] and by write transactions; what a write
/// transaction sees depends on its state.
pub trait ReadTxnable: sealed::StoreView {
    fn heads(&self) -> Vec<CommitHash> {
        self.store_view().heads()
    }

    fn has_heads(&self) -> bool {
        todo!()
    }

    fn fragments_after(&self, have_heads: &[CommitHash]) -> Vec<Fragment> {
        self.store_view().fragments_after(have_heads)
    }

    fn encode_store(&self) -> Result<Vec<u8>, BouncerError> {
        Ok(self.store_view().encode().map_err(StoreError::from)?)
    }

    fn scan_table(&self, table: &ir::Path) -> Option<Vec<PublicTuple>> {
        let view = self.store_view();
        Some(
            view.scan_table(table)?
                .into_iter()
                .map(|tuple| unpack_tuple(&view, tuple))
                .collect(),
        )
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &PublicRowId) -> Option<PublicTuple> {
        let view = self.store_view();
        let packed = view.id_lookup().packed(row_id)?;
        view.row_by_id(table, &packed)
            .map(|tuple| unpack_tuple(&view, tuple))
    }

    fn canonical_row_id(&self, row_id: &PublicRowId) -> Option<PublicRowId> {
        self.store_view().canonical_row_id(row_id)
    }

    /// Promote ids after the transaction that produced them has committed.
    fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxRowId>,
        hash: CommitHash,
    ) -> Vec<PublicRowId> {
        self.store_view().promote(pending_ids, hash)
    }

    fn promote_one(&self, pending_id: impl Into<TxRowId>, hash: CommitHash) -> PublicRowId {
        self.store_view().promote_one(pending_id, hash)
    }

    // TODO: delegate filtering and projection to coln-query.
    fn all_proj(
        &self,
        _query: &WhereClause,
        _select: &[u32],
    ) -> Result<Vec<PublicTuple>, BouncerError> {
        todo!()
    }

    fn all_row_id(&self, _query: &WhereClause) -> Result<Vec<PublicRowId>, BouncerError> {
        todo!()
    }

    fn one_proj(&self, _query: &WhereClause, _select: &[u32]) -> Result<PublicTuple, BouncerError> {
        todo!()
    }

    fn exists(&self, _query: &WhereClause) -> Result<bool, BouncerError> {
        todo!()
    }
}

impl<T: sealed::StoreView + ?Sized> ReadTxnable for T {}

pub(super) fn unpack_tuple(view: &ReadView<'_>, tuple: StoreTuple) -> PublicTuple {
    PublicTuple::from_store(tuple, |id| {
        view.id_lookup()
            .unpacked(&id)
            .expect("stored id can be unpacked")
    })
}

/// A read-only transaction over committed bouncer data.
pub struct ReadTxn {
    store: coln_store::store::ReadTxn,
}

impl sealed::StoreView for ReadTxn {
    fn store_view(&self) -> ReadView<'_> {
        self.store.read()
    }
}

impl Bouncer {
    pub fn begin_read(&self) -> ReadTxn {
        ReadTxn {
            store: self.store.begin_read(),
        }
    }
}
