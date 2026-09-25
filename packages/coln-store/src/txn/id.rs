// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    WireRowId,
    engine::txn_val::{TempRowId, TxnWireRowId, TxnWireTuple},
    hash::CommitHash,
};

use crate::{op::Op, table::TableOid};

pub fn empty_row() -> TxnWireTuple {
    TxnWireTuple(Vec::new())
}

pub trait Promote {
    /// Promoting pending ids to Wire ids, after successful transactions
    //  and also canonicalise them
    //  Will not check validity, callers is responsible for calling it with valid pending ids
    fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxnWireRowId>,
        hash: CommitHash,
    ) -> Vec<WireRowId>;

    fn promote_one(&self, pending_id: impl Into<TxnWireRowId>, hash: CommitHash) -> WireRowId {
        self.promote(std::iter::once(pending_id.into()), hash)
            .pop()
            .expect("ond id to promote")
    }
}

/// An operation staged within a transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PendingOp {
    Add {
        row_id: TempRowId,
        table: TableOid,
        values: TxnWireTuple,
    },
}

impl PendingOp {
    pub(crate) fn resolve(self, commit: CommitHash) -> Op {
        match self {
            PendingOp::Add {
                row_id,
                table,
                values,
            } => Op::Add {
                row_id: row_id.resolve(commit),
                table,
                values: values
                    .into_iter()
                    .map(|value| value.map_owned(|i| i.resolve(commit)))
                    .collect(),
            },
        }
    }
}
