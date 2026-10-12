// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! For dealing with temporary ids in the middle of a transaction.

use std::{fmt, ops::Deref};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    engine::packed::WithRowId,
    hash::CommitHash,
    public::PublicRowId,
    tuple::{NativeScalar, NativeScalarType, ScalarKind, Tuple},
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

impl fmt::Display for TxRowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxRowId::Existing(public) => write!(f, "existing {public}"),
            TxRowId::Pending(pending) => write!(f, "pending {}", pending.0),
        }
    }
}

pub type TxScalarValue = NativeScalar<TxRowId>;

impl From<TxRowId> for TxScalarValue {
    fn from(value: TxRowId) -> Self {
        NativeScalar::RowId(value)
    }
}

impl From<PublicRowId> for TxScalarValue {
    fn from(value: PublicRowId) -> Self {
        NativeScalar::RowId(TxRowId::from(value))
    }
}

impl ScalarKind for TxRowId {
    type Ty = TxRowIdType;

    fn ty(_instance: &Self) -> Self::Ty {
        TxRowIdType(())
    }
}

/// The _type_ representing all instances of a [`TxRowId`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TxRowIdType(());

impl fmt::Display for TxRowIdType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TxRowId")
    }
}

/// The _type_ representing the supported scalar values visible to the external
/// world mid of transaction.
pub type TxScalarType = NativeScalarType<TxRowIdType>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TxTuple(pub(crate) Tuple<TxScalarValue>);

impl Deref for TxTuple {
    type Target = [TxScalarValue];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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

impl WithRowId for TxTuple {
    type RowId = PendingRowId;

    fn row_id(&self) -> Self::RowId {
        match self.first() {
            Some(TxScalarValue::RowId(id)) => {
                let TxRowId::Pending(id) = id else {
                    panic!("tx scalar value must start with a pending id")
                };
                *id
            }
            other => panic!("TxTuple lead with their row id, got {other:?}"),
        }
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
