//! Packed representation of ids
//! Internally used by storage and query engines

use crate::value::Value;

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

pub type PackedValue = Value<PackedRowId>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedTuple(Vec<PackedValue>);

impl std::ops::Deref for PackedTuple {
    type Target = [PackedValue];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<PackedValue>> for PackedTuple {
    fn from(values: Vec<PackedValue>) -> Self {
        Self(values)
    }
}

impl IntoIterator for PackedTuple {
    type Item = PackedValue;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a PackedTuple {
    type Item = &'a PackedValue;
    type IntoIter = std::slice::Iter<'a, PackedValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl FromIterator<PackedValue> for PackedTuple {
    fn from_iter<T: IntoIterator<Item = PackedValue>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

pub struct PackedRowView {
    pub row_id: PackedRowId,
    pub values: PackedTuple,
}

// TODO @Leo perhaps move your TupleValue definitions here as well
// Or maybe it's sufficient to define just one representation to save some computation
// impl From<PackedRowView> for coln_query::api::deltas::TupleValue {
//     fn from(packed_view: PackedRowView) -> Self {
//         let PackedRowView { row_id, values } = packed_view;
//         std::iter::once(PackedValue::Id(row_id))
//             .chain(values)
//             .flat_map(|values| match values {
//                 PackedValue::Id(prid) => vec![
//                     ScalarTypedValue::Uint(prid.commit_idx as u64),
//                     ScalarTypedValue::Uint(prid.counter as u64),
//                 ],
//                 PackedValue::Int(i) => vec![ScalarTypedValue::Iint(i as i64)],
//                 PackedValue::Str(s) => vec![ScalarTypedValue::String(s)],
//             })
//             .collect()
//     }
// }
