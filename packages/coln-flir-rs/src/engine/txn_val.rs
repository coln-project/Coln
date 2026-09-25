//! For dealing with temporary ids in the middle of a transactions

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{WireRowId, hash::CommitHash, value::Value};

/// A temporary row ID that is valid only within a transaction.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct TempRowId(pub u32);

impl TempRowId {
    pub fn resolve(self, commit: CommitHash) -> WireRowId {
        WireRowId {
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

/// Its difference with a pure WireRowId is that it can be a pending id
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "value")]
pub enum TxnWireRowId {
    Existing(WireRowId),
    Pending(TempRowId),
}

impl TxnWireRowId {
    pub fn resolve(self, commit: CommitHash) -> WireRowId {
        match self {
            TxnWireRowId::Existing(row_id) => row_id,
            TxnWireRowId::Pending(temp_id) => temp_id.resolve(commit),
        }
    }
}

impl From<WireRowId> for TxnWireRowId {
    fn from(value: WireRowId) -> Self {
        Self::Existing(value)
    }
}

pub type TxnWireValue = Value<TxnWireRowId>;

impl From<TxnWireRowId> for TxnWireValue {
    fn from(value: TxnWireRowId) -> Self {
        TxnWireValue::Id(value)
    }
}

impl From<WireRowId> for TxnWireValue {
    fn from(value: WireRowId) -> Self {
        TxnWireValue::Id(TxnWireRowId::from(value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TxnWireTuple(pub Vec<TxnWireValue>);

impl FromIterator<TxnWireValue> for TxnWireTuple {
    fn from_iter<T: IntoIterator<Item = TxnWireValue>>(iter: T) -> Self {
        let mut t = TxnWireTuple(Vec::new());
        for i in iter {
            t.0.push(i)
        }
        t
    }
}

impl<T: Into<TxnWireValue>> From<Vec<T>> for TxnWireTuple {
    fn from(value: Vec<T>) -> Self {
        value.into_iter().map(T::into).collect()
    }
}

impl IntoIterator for TxnWireTuple {
    type Item = TxnWireValue;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a TxnWireTuple {
    type Item = &'a TxnWireValue;
    type IntoIter = std::slice::Iter<'a, TxnWireValue>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl TxnWireTuple {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_slice(&self) -> &[TxnWireValue] {
        &self.0
    }

    pub fn empty() -> Self {
        TxnWireTuple(vec![])
    }
}
