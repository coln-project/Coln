// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A Store that manages a transaction for the user.
//! But the user is still responsible for starting/finishing transactions
//! This module has lots of panic because AutoStore is used by the TS FFI, which
//! does not know anything about lifetime, and so cannot have a typesafe interface
//! Therefore the user is expected to do the right thing.

use coln_flir_rs::{
    engine::{
        op::TabPathOp,
        packed::{PackedRowId, StoreTuple},
        schema::ColnDef,
        tx::{TxRowId, TxTuple},
    },
    hash::CommitHash,
    ir::{self, FlatRealm},
    public::PublicRowId,
    query::WhereClause,
};

use crate::{
    IdLookup,
    rollback::Rollback,
    store::{
        Store,
        commit::{BatchInner, Prepared},
        error::StoreError,
        frag::{CommitChunk, FragmentSync},
    },
    txn::{
        id::Promote,
        inner::TxnInner,
        rw::{StoreRead, StoreWrite},
    },
};

enum AutoState {
    Idle,
    Transaction(TxnInner),
    Prepared(BatchInner<Prepared>),
}

pub struct AutoStore {
    store: Store,
    state: AutoState,
}

impl StoreRead for AutoStore {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<StoreTuple>> {
        self.store.scan_table(table)
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &PackedRowId) -> Option<StoreTuple> {
        self.store.row_by_id(table, row_id)
    }

    fn all_proj(&self, query: &WhereClause, select: &[u32]) -> Result<Vec<StoreTuple>, StoreError> {
        self.store.all_proj(query, select)
    }

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<PackedRowId>, StoreError> {
        self.store.all_row_id(query)
    }
}

impl StoreWrite for AutoStore {
    fn add(&mut self, table: &ir::Path, values: impl Into<TxTuple>) -> Result<TxRowId, StoreError> {
        let AutoState::Transaction(txn) = &mut self.state else {
            panic!("open txn")
        };
        txn.add(&mut self.store, table, values)
    }
}

impl FragmentSync for AutoStore {
    fn commit_chunks_after(&self, have_heads: &[CommitHash]) -> Vec<CommitChunk> {
        self.store.commit_chunks_after(have_heads)
    }

    // TODO allow this after we have a good concurrency control theory
    // A current open transaction should only read data from its deps backward,
    // But no a concurrent txn
    fn apply_chunk_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<(), StoreError> {
        self.store.apply_chunk_bytes(chunk_bytes)
    }
}

impl AutoStore {
    pub fn try_from_ir(ir: &FlatRealm, coln_def: ColnDef) -> Result<Self, StoreError> {
        let store = Store::try_from_ir(ir, coln_def)?;
        Ok(Self::new(store))
    }

    pub fn new(store: Store) -> Self {
        Self {
            store,
            state: AutoState::Idle,
        }
    }

    pub fn transaction(&mut self) {
        let deps = self.store.commits().heads().copied().collect();
        self.state = AutoState::Transaction(TxnInner::new(deps));
    }

    pub fn try_commit(&mut self) -> Result<Vec<TabPathOp>, StoreError> {
        let AutoState::Transaction(txn) = &mut self.state else {
            panic!("open txn")
        };
        let prepared = txn.try_commit(&mut self.store)?;
        let table_ops = prepared.table_ops().to_vec();
        self.state = AutoState::Prepared(prepared);
        Ok(table_ops)
    }

    pub fn commit(&mut self) -> Result<CommitHash, StoreError> {
        let state = std::mem::replace(&mut self.state, AutoState::Idle);
        let AutoState::Prepared(prepared) = state else {
            panic!("tried commit");
        };
        Ok(TxnInner::commit(&mut self.store, prepared))
    }

    pub fn abort(&mut self) {
        match std::mem::replace(&mut self.state, AutoState::Idle) {
            AutoState::Idle => {
                panic!("should not abort without starting a txn");
            }
            AutoState::Transaction(mut txn) => txn.abort(),
            AutoState::Prepared(prepared) => {
                self.store.rollback_to(prepared.prepared_commit().snapshot);
            }
        }
    }

    pub fn try_from_commit_bytes(
        chunk_bytes: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Result<Self, StoreError> {
        let store = Store::try_from_commit_bytes(chunk_bytes)?;
        Ok(store.auto())
    }

    pub fn id_lookup(&self) -> &impl IdLookup {
        self.store.id_lookup()
    }
}

impl Promote for AutoStore {
    fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxRowId>,
        hash: CommitHash,
    ) -> Vec<PublicRowId> {
        self.store.promote(pending_ids, hash)
    }
}

impl Drop for AutoStore {
    fn drop(&mut self) {
        if matches!(self.state, AutoState::Prepared(..)) {
            self.abort();
        };
    }
}

#[cfg(test)]
mod tests {

    use coln_flir_rs::engine::packed::StoreScalarValue;
    use coln_flir_rs::ir::Path;
    use rstest::rstest;

    use super::*;
    use crate::test_utils::single_int_autostore;

    #[rstest]
    fn owned_transaction_store_read_sees_committed_rows_not_pending(
        #[from(single_int_autostore)] mut store: AutoStore,
    ) {
        let path = Path::from("T");

        store.transaction();
        store.add(&path, vec![1i32]).expect("add");
        store.try_commit().expect("prepare");
        store.commit().expect("commit");

        // The commit hash is the first one interned, so it packs as index 0.
        let row_id = PackedRowId {
            commit_idx: 0,
            counter: 0,
        };

        store.transaction();
        assert_eq!(
            store.row_by_id(&path, &row_id),
            Some(StoreTuple::from(vec![
                StoreScalarValue::RowId(row_id),
                StoreScalarValue::I32(1),
            ]))
        );

        store.add(&path, vec![2i32]).expect("add pending");
        assert_eq!(store.scan_table(&path).expect("T").len(), 1);
    }
}
