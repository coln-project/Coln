//! Packed representation of ids
//! Internally used by storage and query engines

use crate::public::{PublicRowId, PublicTuple};
use crate::tuple::{NativeScalar, Tuple};

/// A compact [`RowId`] representation that dictionary-encodes commit hashes.
///
/// This is only meaningful together with the store-wide
/// [`IdPacker`](crate::id_packer::IdPacker) that produced it, so it never
/// crosses the store boundary. Packed ids order by `(commit_idx, counter)`,
/// which depends on dictionary insertion order. Deterministic ordering across
/// stores must compare unpacked [`RowId`]s.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct PackedRowId {
    pub commit_idx: u32,
    pub counter: u32,
}

pub type StoreScalarValue = NativeScalar<PackedRowId>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreTuple(pub(super) Tuple<StoreScalarValue>);

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
}

impl From<Vec<StoreScalarValue>> for StoreTuple {
    fn from(values: Vec<StoreScalarValue>) -> Self {
        Self(Tuple { inner: values })
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
