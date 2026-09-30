//! Packed representation of ids
//! Internally used by storage and query engines

use crate::{
    PublicRowId, PublicTuple,
    value::{NativeScalar, Tuple},
};

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
        self.0.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a StoreTuple {
    type Item = &'a StoreScalarValue;
    type IntoIter = std::slice::Iter<'a, StoreScalarValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.inner.iter()
    }
}

impl FromIterator<StoreScalarValue> for StoreTuple {
    fn from_iter<T: IntoIterator<Item = StoreScalarValue>>(iter: T) -> Self {
        Self(Tuple {
            inner: iter.into_iter().collect(),
        })
    }
}

pub struct PackedRowView {
    pub row_id: PackedRowId,
    pub values: StoreTuple,
}

// TODO @Leo perhaps move your TupleValue definitions here as well
// Or maybe it's sufficient to define just one representation to save some computation
// impl From<PackedRowView> for coln_query::api::deltas::TupleValue {
//     fn from(packed_view: PackedRowView) -> Self {
//         let PackedRowView { row_id, values } = packed_view;
//         std::iter::once(StoreScalarValue::Id(row_id))
//             .chain(values)
//             .flat_map(|values| match values {
//                 StoreScalarValue::Id(prid) => vec![
//                     ScalarTypedValue::Uint(prid.commit_idx as u64),
//                     ScalarTypedValue::Uint(prid.counter as u64),
//                 ],
//                 StoreScalarValue::Int(i) => vec![ScalarTypedValue::Iint(i as i64)],
//                 StoreScalarValue::Str(s) => vec![ScalarTypedValue::String(s)],
//             })
//             .collect()
//     }
// }
