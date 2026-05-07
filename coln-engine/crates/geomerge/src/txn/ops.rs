use crate::{
    commit::CommitHash,
    ir,
    table::{CellValue, RowId},
};

// This is a temporary rowid only valid during a transaction. Not persisted, no hash
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct TempRowId(pub(crate) u32);

impl TempRowId {
    pub(crate) fn resolve(self, commit: CommitHash) -> RowId {
        RowId {
            commit,
            counter: self.0,
        }
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
pub enum RowRef {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TxnCellValue {
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
