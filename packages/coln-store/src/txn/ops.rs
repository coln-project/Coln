use std::{cell::RefCell, rc::Rc};

use crate::{
    commit::hash::CommitHash,
    ir,
    store::error::StoreIntError,
    table::{CellValue, RowId, ValidationError},
};

pub const OP_KIND_ADD: u32 = 0;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct TxnId(u64);

impl TxnId {
    pub fn new(n: u64) -> Self {
        TxnId(n)
    }
}

#[derive(Clone, Debug)]
pub struct RowHandle {
    // ? Arc
    state: Rc<RefCell<RowHandleState>>,
}

impl RowHandle {
    pub fn row_id(&self) -> Result<RowId, Box<StoreIntError>> {
        match &*self.state.borrow() {
            RowHandleState::Existing(row_id) => Ok(*row_id),
            RowHandleState::Pending { .. } => Err(ValidationError::InvalidRowHandle {
                reason: "row handle is still pending".to_string(),
            }
            .into()),
            RowHandleState::Invalid(reason) => Err(ValidationError::InvalidRowHandle {
                reason: reason.clone(),
            }
            .into()),
        }
    }

    pub(crate) fn to_txn_cell_value(
        &self,
        current_tx: TxnId,
    ) -> Result<TxnCellValue, Box<StoreIntError>> {
        match &*self.state.borrow() {
            RowHandleState::Existing(row_id) => Ok(TxnCellValue::Id(RowRef::Existing(*row_id))),
            RowHandleState::Pending { tx_id, counter } if *tx_id == current_tx => {
                Ok(TxnCellValue::Id(RowRef::Pending(TempRowId::from(*counter))))
            }
            RowHandleState::Pending { tx_id, .. } => Err(ValidationError::TxnIdMismatch {
                current: current_tx,
                got: *tx_id,
            }
            .into()),
            RowHandleState::Invalid(reason) => Err(ValidationError::InvalidRowHandle {
                reason: reason.clone(),
            }
            .into()),
        }
    }

    pub(crate) fn finalize(&self, commit: CommitHash) {
        let mut state = self.state.borrow_mut();
        if let RowHandleState::Pending { counter, .. } = *state {
            *state = RowHandleState::Existing(RowId { commit, counter });
        }
    }
    pub(crate) fn invalidate(&self, reason: &str) {
        *self.state.borrow_mut() = RowHandleState::Invalid(reason.into());
    }
}

impl RowHandle {
    pub(crate) fn pending(tx_id: TxnId, value: TempRowId) -> Self {
        let state = Rc::new(RefCell::new(RowHandleState::Pending {
            tx_id,
            counter: value.0,
        }));
        RowHandle { state }
    }
}

#[derive(Clone, Debug)]
enum RowHandleState {
    Pending { tx_id: TxnId, counter: u32 },
    Existing(RowId),
    Invalid(String),
}

#[derive(Clone)]
pub enum TxnValue {
    Id(RowHandle),
    Int(i64),
    Str(String),
}

impl TxnValue {
    pub(crate) fn to_txn_cell_value(
        &self,
        current_tx: TxnId,
    ) -> Result<TxnCellValue, Box<StoreIntError>> {
        match self {
            TxnValue::Id(handle) => handle.to_txn_cell_value(current_tx),
            TxnValue::Int(value) => Ok(TxnCellValue::Int(*value)),
            TxnValue::Str(value) => Ok(TxnCellValue::Str(value.clone())),
        }
    }
}

impl From<RowHandle> for TxnValue {
    fn from(value: RowHandle) -> Self {
        TxnValue::Id(value)
    }
}

impl From<RowId> for TxnValue {
    fn from(value: RowId) -> Self {
        TxnValue::Id(RowHandle {
            state: Rc::new(RefCell::new(RowHandleState::Existing(value))),
        })
    }
}

impl From<i64> for TxnValue {
    fn from(value: i64) -> Self {
        TxnValue::Int(value)
    }
}

impl From<String> for TxnValue {
    fn from(value: String) -> Self {
        TxnValue::Str(value)
    }
}

impl From<&str> for TxnValue {
    fn from(value: &str) -> Self {
        TxnValue::Str(value.to_owned())
    }
}

impl From<CellValue> for TxnValue {
    fn from(value: CellValue) -> Self {
        match value {
            CellValue::Id(id) => TxnValue::Id(RowHandle {
                state: Rc::new(RefCell::new(RowHandleState::Existing(id))),
            }),
            CellValue::Int(value) => TxnValue::Int(value),
            CellValue::Str(value) => TxnValue::Str(value),
        }
    }
}

// This is a temporary rowid only valid during a transaction. Not persisted, no hash
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct TempRowId(pub(crate) u32);

impl TempRowId {
    pub(crate) fn resolve(self, commit: CommitHash) -> RowId {
        RowId {
            commit,
            counter: self.0,
        }
    }

    pub fn counter(self) -> u32 {
        self.0
    }
}

impl From<u32> for TempRowId {
    fn from(value: u32) -> Self {
        TempRowId(value)
    }
}

/// The temporary ops in flight for a transaction
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PendingOp {
    Add {
        row_id: TempRowId,
        table: ir::Path,
        values: Vec<TxnCellValue>,
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
                table: table.clone(),
                values: values.iter().map(|value| value.resolve(commit)).collect(),
            },
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum RowRef {
    Existing(RowId),
    Pending(TempRowId),
}

impl RowRef {
    fn resolve(&self, commit: CommitHash) -> RowId {
        match self {
            RowRef::Existing(row_id) => *row_id,
            RowRef::Pending(temp_id) => temp_id.resolve(commit),
        }
    }
}

impl From<RowId> for RowRef {
    fn from(value: RowId) -> Self {
        RowRef::Existing(value)
    }
}

impl From<TempRowId> for RowRef {
    fn from(value: TempRowId) -> Self {
        RowRef::Pending(value)
    }
}

/// This is the internal representation which is derived from `TxnValue`
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TxnCellValue {
    Id(RowRef),
    Int(i64),
    Str(String),
}

impl TxnCellValue {
    fn resolve(&self, commit: CommitHash) -> CellValue {
        match self {
            TxnCellValue::Id(row_ref) => CellValue::Id(row_ref.resolve(commit)),
            TxnCellValue::Int(value) => CellValue::Int(*value),
            TxnCellValue::Str(value) => CellValue::Str(value.clone()),
        }
    }
}

impl From<RowRef> for TxnCellValue {
    fn from(value: RowRef) -> Self {
        TxnCellValue::Id(value)
    }
}

impl From<RowId> for TxnCellValue {
    fn from(value: RowId) -> Self {
        TxnCellValue::Id(RowRef::Existing(value))
    }
}

impl From<TempRowId> for TxnCellValue {
    fn from(value: TempRowId) -> Self {
        TxnCellValue::Id(RowRef::Pending(value))
    }
}

impl From<i64> for TxnCellValue {
    fn from(value: i64) -> Self {
        TxnCellValue::Int(value)
    }
}

impl From<String> for TxnCellValue {
    fn from(value: String) -> Self {
        TxnCellValue::Str(value)
    }
}

impl From<&str> for TxnCellValue {
    fn from(value: &str) -> Self {
        TxnCellValue::Str(value.to_owned())
    }
}

impl From<CellValue> for TxnCellValue {
    fn from(value: CellValue) -> Self {
        match value {
            CellValue::Id(id) => TxnCellValue::Id(RowRef::Existing(id)),
            CellValue::Int(value) => TxnCellValue::Int(value),
            CellValue::Str(value) => TxnCellValue::Str(value),
        }
    }
}

pub enum Op {
    Add {
        row_id: RowId,
        table: ir::Path, // using path so it's stable across replicas
        values: Vec<CellValue>,
    },
}

impl Op {
    pub fn id(&self) -> RowId {
        match self {
            Op::Add { row_id, .. } => *row_id,
        }
    }
}
