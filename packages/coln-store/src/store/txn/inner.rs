// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

use coln_flir_rs::{
    engine::{
        packed::StoreTuple,
        tx::{PendingRowId, TxRowId, TxTuple},
    },
    hash::CommitHash,
    ir,
};
use tracing::info;

use super::timestamp::Timestamp;

use crate::{
    IdLookup,
    commit::{Commit, DataCommit, author::Author, wire::CommitData},
    op::PendingOp,
    store::{
        ReadView,
        error::{CommitApplyError, StoreError},
    },
    table::{TableMeta, TableOid, ValidationError},
};

pub(crate) struct TxnData {
    deps: Vec<CommitHash>,
    author: Author,
    pending: Vec<PendingOp>,
    /// Rows staged in constructor tables, keyed by table and column hash.
    /// Hash collisions are checked against the values retained in `pending`.
    pending_cons: HashMap<TableOid, HashMap<u64, Vec<PendingRowId>>>,
    timestamp: Timestamp,
    message: Option<String>,
}

impl TxnData {
    pub(crate) fn new(deps: Vec<CommitHash>) -> Self {
        Self {
            deps,
            author: Author::foo(),
            pending: Vec::new(),
            pending_cons: HashMap::new(),
            timestamp: Timestamp::now(),
            message: None,
        }
    }

    fn next_id(&self) -> PendingRowId {
        PendingRowId::from(self.pending.len() as u32)
    }

    fn add_cell_values(
        &mut self,
        view: ReadView,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, StoreError> {
        let table = view.table_at(table).ok_or(ValidationError::UnknownTable {
            path: table.clone(),
        })?;
        let values: TxTuple = values.into();
        let meta = table.meta();
        meta.validate_column_count(values.len())?;

        let cons = table.inner().cons();
        // Existing ids are canonicalised so that adds naming different
        // members of one id class are recognised as the same row, both
        // against the table and against rows staged in this transaction.
        let values: TxTuple = if cons {
            values
                .iter()
                .cloned()
                .map(|value| {
                    value.map(|id| match id {
                        TxRowId::Existing(id) => {
                            TxRowId::Existing(view.canonical_row_id(&id).unwrap_or(id))
                        }
                        pending @ TxRowId::Pending(_) => pending,
                    })
                })
                .collect()
        } else {
            values
        };

        let column_hash = cons.then(|| {
            let mut hasher = DefaultHasher::new();
            values.as_slice().hash(&mut hasher);
            hasher.finish()
        });
        if let Some(column_hash) = column_hash {
            if let Some(id) = self
                .pending_cons
                .get(&meta.oid())
                .and_then(|rows| rows.get(&column_hash))
                .and_then(|ids| {
                    ids.iter().copied().find(|id| {
                        self.pending[id.counter() as usize].column_values() == values.as_slice()
                    })
                })
            {
                return Ok(TxRowId::Pending(id));
            }

            let key: Option<StoreTuple> = values
                .iter()
                .cloned()
                .map(|value| {
                    value
                        .try_map(|id| match id {
                            TxRowId::Existing(id) => view.id_lookup().packed(&id),
                            TxRowId::Pending(_) => None,
                        })
                        .map(Into::into)
                })
                .collect();
            if let Some(key) = key
                && let Some(rid) = table.index_seek(&key)?.next()
            {
                let pid = view.id_lookup().unpacked(&rid).unwrap();
                return Ok(TxRowId::Existing(pid));
            }
        }

        let temp_id = self.next_id();
        if let Some(column_hash) = column_hash {
            self.pending_cons
                .entry(meta.oid())
                .or_default()
                .entry(column_hash)
                .or_default()
                .push(temp_id);
        }
        self.pending
            .push(PendingOp::add(temp_id, meta.oid(), values));
        Ok(TxRowId::Pending(temp_id))
    }

    pub(crate) fn add(
        &mut self,
        view: ReadView,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, StoreError> {
        let tx_id = self.add_cell_values(view, table, values)?;
        Ok(tx_id)
    }

    pub(crate) fn into_commit<'a, F>(self, table_meta_for: F) -> Result<DataCommit, StoreError>
    where
        F: Fn(TableOid) -> Option<&'a TableMeta>,
    {
        let TxnData {
            deps,
            author,
            pending,
            timestamp,
            message,
            ..
        } = self;

        info!(op_count = pending.len(), "commit txn");

        // TODO we could add an option to allow empty commit
        if pending.is_empty() {
            return Err(CommitApplyError::EmptyCommit.into());
        }

        Ok(Commit::from_commit_data(
            CommitData::new(deps, author, *timestamp.as_ref(), message, pending),
            table_meta_for,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        store::{Store, txn::ReadTxnable},
        test_utils::single_memoized_int_store,
    };
    use rstest::rstest;

    #[rstest]
    fn constructor_hash_collisions_preserve_distinct_rows(
        #[from(single_memoized_int_store)] store: Store,
    ) {
        let path = ir::Path::from("T");
        let read = store.begin_read();
        let oid = read.table_at(&path).unwrap().oid();
        let mut txn = TxnData::new(read.heads());
        let first = txn.add(read.read(), &path, vec![7i32]).unwrap();

        let second_values: TxTuple = vec![8i32].into();
        let mut hasher = DefaultHasher::new();
        second_values.as_slice().hash(&mut hasher);
        // Force the first row into the second row's bucket to simulate a collision.
        let rows = txn.pending_cons.get_mut(&oid).unwrap();
        let (_, ids) = rows.drain().next().unwrap();
        rows.insert(hasher.finish(), ids);

        let second = txn.add(read.read(), &path, second_values.clone()).unwrap();
        assert_ne!(first, second);
        assert_eq!(txn.add(read.read(), &path, second_values).unwrap(), second);
        assert_eq!(txn.pending.len(), 2);

        let ids = txn.pending_cons[&oid].values().next().unwrap();
        assert_eq!(ids, &[PendingRowId(0), PendingRowId(1)]);
        assert_eq!(
            txn.pending[0].column_values(),
            TxTuple::from(vec![7i32]).as_slice()
        );
        assert_eq!(
            txn.pending[1].column_values(),
            TxTuple::from(vec![8i32]).as_slice()
        );
    }
}
