// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{cell::RefCell, rc::Rc};

use crate::{
    commit::hash::CommitHash,
    op::Op,
    store::error::StoreError,
    table::{TableOid, ValidationError, WireRowId, WireValue},
    value::Value,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct TxnId(u64);

impl TxnId {
    pub fn new(n: u64) -> Self {
        TxnId(n)
    }

    pub(crate) fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for TxnId {
    fn from(value: u64) -> Self {
        TxnId::new(value)
    }
}

#[derive(Clone, Debug)]
enum TxnLiveRowIdState {
    Pending { tx_id: TxnId, counter: u32 },
    Existing(WireRowId),
    Invalid(String),
}

/// A TxnLiveRowId abstracts away the difference between a pending id and an existing
/// rowid. It is a reference counted handle that can be shared and will be automatically
/// converted from a temp rowid to a rowid on successful commit.
#[derive(Clone, Debug)]
pub struct TxnLiveRowId {
    // ? Arc
    state: Rc<RefCell<TxnLiveRowIdState>>,
}

impl Into<TxnLiveValue> for TxnLiveRowId {
    fn into(self) -> TxnLiveValue {
        TxnLiveValue::Id(self)
    }
}

impl TxnLiveRowId {
    pub fn row_id(&self) -> Result<WireRowId, StoreError> {
        match &*self.state.borrow() {
            TxnLiveRowIdState::Existing(row_id) => Ok(*row_id),
            TxnLiveRowIdState::Pending { .. } => Err(ValidationError::InvalidTxnLiveRowId {
                reason: "row handle is still pending".to_string(),
            }
            .into()),
            TxnLiveRowIdState::Invalid(reason) => Err(ValidationError::InvalidTxnLiveRowId {
                reason: reason.clone(),
            }
            .into()),
        }
    }

    /// For FFI authors only
    #[doc(hidden)]
    pub fn pending_ids(&self) -> Result<(u64, u32), StoreError> {
        match *self.state.borrow() {
            TxnLiveRowIdState::Pending { tx_id, counter } => Ok((tx_id.as_u64(), counter)),
            _ => Err(ValidationError::InvalidTxnLiveRowId {
                reason: "not txn id on existing ids or invalid handles".to_string(),
            }
            .into()),
        }
    }

    pub(crate) fn canonicalise(&self, new_row_id: WireRowId) -> Result<(), StoreError> {
        let mut state = self.state.borrow_mut();
        match &*state {
            TxnLiveRowIdState::Existing(..) => {
                *state = TxnLiveRowIdState::Existing(new_row_id);
                Ok(())
            }
            _ => Err(ValidationError::InvalidTxnLiveRowId {
                reason: "cannot replace row id on a non finalised rowhandle".to_string(),
            }
            .into()),
        }
    }

    pub(crate) fn to_txn_cell_value(&self, current_tx: TxnId) -> Result<TxnWireValue, StoreError> {
        match &*self.state.borrow() {
            TxnLiveRowIdState::Existing(row_id) => {
                Ok(TxnWireValue::Id(TxnWireRowId::Existing(*row_id)))
            }
            TxnLiveRowIdState::Pending { tx_id, counter } if *tx_id == current_tx => Ok(
                TxnWireValue::Id(TxnWireRowId::Pending(TempRowId::from(*counter))),
            ),
            TxnLiveRowIdState::Pending { tx_id, .. } => Err(ValidationError::TxnIdMismatch {
                current: current_tx,
                got: *tx_id,
            }
            .into()),
            TxnLiveRowIdState::Invalid(reason) => Err(ValidationError::InvalidTxnLiveRowId {
                reason: reason.clone(),
            }
            .into()),
        }
    }

    pub(crate) fn finalize(&self, commit: CommitHash, resolve: impl Fn(WireRowId) -> WireRowId) {
        let mut state = self.state.borrow_mut();
        if let TxnLiveRowIdState::Pending { counter, .. } = *state {
            *state = TxnLiveRowIdState::Existing(resolve(WireRowId { commit, counter }));
        }
    }

    pub(crate) fn invalidate(&self, reason: &str) {
        *self.state.borrow_mut() = TxnLiveRowIdState::Invalid(reason.into());
    }

    #[doc(hidden)]
    pub fn from_pending(tx_id: TxnId, counter: u32) -> Self {
        let state = Rc::new(RefCell::new(TxnLiveRowIdState::Pending { tx_id, counter }));
        TxnLiveRowId { state }
    }

    #[doc(hidden)]
    pub fn from_existing(row_id: WireRowId) -> Self {
        let state = Rc::new(RefCell::new(TxnLiveRowIdState::Existing(row_id)));
        TxnLiveRowId { state }
    }
}

pub type TxnLiveValue = Value<TxnLiveRowId>;

impl TxnLiveValue {
    pub(crate) fn to_txn_cell_value(&self, current_tx: TxnId) -> Result<TxnWireValue, StoreError> {
        match self {
            TxnLiveValue::Id(handle) => handle.to_txn_cell_value(current_tx),
            TxnLiveValue::Int(value) => Ok(TxnWireValue::Int(*value)),
            TxnLiveValue::Str(value) => Ok(TxnWireValue::Str(value.clone())),
        }
    }
}

pub fn liven(v: WireValue) -> TxnLiveValue {
    v.map_owned(TxnLiveRowId::from_existing)
}

pub fn liven_all(vs: Vec<WireValue>) -> Vec<TxnLiveValue> {
    vs.into_iter().map(liven).collect()
}

/// A temporary row ID that is valid only within a transaction.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct TempRowId(pub(crate) u32);

impl TempRowId {
    pub(crate) fn resolve(self, commit: CommitHash) -> WireRowId {
        WireRowId {
            commit,
            counter: self.0,
        }
    }

    pub(crate) fn counter(self) -> u32 {
        self.0
    }
}

impl From<u32> for TempRowId {
    fn from(value: u32) -> Self {
        TempRowId(value)
    }
}

/// A reference to an existing row or a pending row in the current transaction.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TxnWireRowId {
    Existing(WireRowId),
    Pending(TempRowId),
}

impl TxnWireRowId {
    fn resolve(&self, commit: CommitHash) -> WireRowId {
        match self {
            TxnWireRowId::Existing(row_id) => *row_id,
            TxnWireRowId::Pending(temp_id) => temp_id.resolve(commit),
        }
    }
}

impl From<WireRowId> for TxnWireRowId {
    fn from(value: WireRowId) -> Self {
        TxnWireRowId::Existing(value)
    }
}

impl From<TempRowId> for TxnWireRowId {
    fn from(value: TempRowId) -> Self {
        TxnWireRowId::Pending(value)
    }
}

pub type TxnWireValue = Value<TxnWireRowId>;

/// An operation staged within a transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PendingOp {
    Add {
        row_id: TempRowId,
        table: TableOid,
        values: Vec<TxnWireValue>,
    },
}

impl PendingOp {
    pub(crate) fn resolve(&self, commit: CommitHash) -> Op {
        match self {
            PendingOp::Add {
                row_id,
                table,
                values,
            } => Op::Add {
                row_id: row_id.resolve(commit),
                table: *table,
                values: values
                    .iter()
                    .map(|value| value.map(|i| i.resolve(commit)))
                    .collect(),
            },
        }
    }
}
