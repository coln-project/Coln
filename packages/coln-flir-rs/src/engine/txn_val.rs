//! For dealing with temporary ids in the middle of a transactions

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    PublicRowId,
    value::{NativeScalar, Tuple},
    hash::CommitHash,
};

/// A temporary row ID that is valid only within a transaction.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct PendingRowId(pub u32);

impl PendingRowId {
    pub fn resolve(self, commit: CommitHash) -> PublicRowId {
        PublicRowId {
            commit,
            counter: self.0,
        }
    }

    pub fn counter(self) -> u32 {
        self.0
    }
}

impl From<u32> for PendingRowId {
    fn from(value: u32) -> Self {
        PendingRowId(value)
    }
}

/// Its difference with a pure WireRowId is that it can be a pending id
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "value")]
pub enum TxRowId {
    Existing(PublicRowId),
    Pending(PendingRowId),
}

impl TxRowId {
    pub fn resolve(self, commit: CommitHash) -> PublicRowId {
        match self {
            TxRowId::Existing(row_id) => row_id,
            TxRowId::Pending(temp_id) => temp_id.resolve(commit),
        }
    }
}

impl From<PublicRowId> for TxRowId {
    fn from(value: PublicRowId) -> Self {
        Self::Existing(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxScalarValue(pub(crate) NativeScalar<TxRowId>);

impl From<TxRowId> for TxScalarValue {
    fn from(value: TxRowId) -> Self {
        TxScalarValue(NativeScalar::RowId(value))
    }
}

impl From<PublicRowId> for TxScalarValue {
    fn from(value: PublicRowId) -> Self {
        TxScalarValue(NativeScalar::RowId(TxRowId::from(value)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TxTuple(pub(crate) Tuple<TxScalarValue>);

impl FromIterator<TxScalarValue> for TxTuple {
    fn from_iter<T: IntoIterator<Item = TxScalarValue>>(iter: T) -> Self {
        let mut t = TxTuple(Tuple { inner: Vec::new() });
        for i in iter {
            t.0.inner.push(i)
        }
        t
    }
}

impl<T: Into<TxScalarValue>> From<Vec<T>> for TxTuple {
    fn from(value: Vec<T>) -> Self {
        value.into_iter().map(T::into).collect()
    }
}

impl IntoIterator for TxTuple {
    type Item = TxScalarValue;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a TxTuple {
    type Item = &'a TxScalarValue;
    type IntoIter = std::slice::Iter<'a, TxScalarValue>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.inner.iter()
    }
}

impl TxTuple {
    pub fn len(&self) -> usize {
        self.0.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_slice(&self) -> &[TxScalarValue] {
        &self.0.inner
    }

    pub fn empty() -> Self {
        TxTuple(Tuple { inner: vec![] })
    }
}
