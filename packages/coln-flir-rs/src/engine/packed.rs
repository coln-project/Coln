// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The storage engine's [tuple](StoreTuple) and its supported
//! [scalar values](StoreScalarValue). Also, how row ids are represented in
//! the storage engine: [PackedRowId].

use std::fmt;
use std::ops::Deref;

use crate::public::{PublicRowId, PublicTuple};
use crate::tuple::{NativeScalar, Tuple};

/// A compact row id representation that dictionary-encodes commit hashes.
///
/// This is only meaningful together with coln-store's id packer which produced
/// it, so it never crosses the store boundary. Packed ids order by
/// `(commit_idx, counter)`, which depends on dictionary insertion order.
/// Deterministic ordering across stores must compare unpacked row ids.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct PackedRowId {
    pub commit_idx: u32,
    pub counter: u32,
}

impl fmt::Display for PackedRowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Mirrors `PublicRowId`'s `<commit>:<counter>` shape, but the commit
        // part is a dictionary index rather than a hash prefix, so prefix it
        // with `#` to make clear this id is only meaningful store-locally.
        write!(f, "#{}:{}", self.commit_idx, self.counter)
    }
}

pub type StoreScalarValue = NativeScalar<PackedRowId>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreTuple(pub(crate) Tuple<StoreScalarValue>);

pub trait WithRowId {
    type RowId;
    fn row_id(&self) -> Self::RowId;
}

impl WithRowId for StoreTuple {
    type RowId = PackedRowId;

    /// The row id a stored row leads with.
    fn row_id(&self) -> Self::RowId {
        match self.first() {
            Some(StoreScalarValue::RowId(id)) => *id,
            other => panic!("stored rows lead with their row id, got {other:?}"),
        }
    }
}

impl StoreTuple {
    pub fn from_public(tuple: PublicTuple, pack: impl Fn(PublicRowId) -> PackedRowId) -> Self {
        let inner = tuple
            .0
            .inner
            .into_iter()
            .map(|scalar| scalar.map(&pack))
            .collect();
        Self(Tuple { inner })
    }

    /// A stored row with `row_id` in column 0 followed by `values`.
    pub fn from_id_values(
        row_id: PackedRowId,
        values: impl IntoIterator<Item = StoreScalarValue>,
    ) -> Self {
        std::iter::once(StoreScalarValue::RowId(row_id))
            .chain(values)
            .collect()
    }

    /// The schema columns of a stored row, without the leading row id.
    pub fn values(&self) -> &[StoreScalarValue] {
        &self[1..]
    }
}

impl Deref for StoreTuple {
    type Target = [StoreScalarValue];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<StoreScalarValue>> for StoreTuple {
    fn from(values: Vec<StoreScalarValue>) -> Self {
        Self(values.into())
    }
}

impl IntoIterator for StoreTuple {
    type Item = StoreScalarValue;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a StoreTuple {
    type Item = &'a StoreScalarValue;
    type IntoIter = std::slice::Iter<'a, StoreScalarValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl FromIterator<StoreScalarValue> for StoreTuple {
    fn from_iter<T: IntoIterator<Item = StoreScalarValue>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}
