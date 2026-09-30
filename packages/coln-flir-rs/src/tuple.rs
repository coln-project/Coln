//! Depending on the layer, a tuple (think a row) can contain different values
//! for scalars. For instance, coln-query does _not_ support a column which
//! contains a pair. The pair needs to be flattened into two columns first.
//! This module provides the base scalar value and the generic tuple representations
//! on which other modules can build.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::ops::Deref;
use subenum::subenum;

// Only indirectly exposed to the public through [`TxTuple`],
// [`PublicTuple`], [`StoreTuple`], [`QueryTuple`]. If we ever want to switch
// to a different layout (smallvec, for instance), these types benefit
// immediately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Tuple<ScalarValue> {
    pub(crate) inner: Vec<ScalarValue>,
}

impl<T> Deref for Tuple<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<V> IntoIterator for Tuple<V> {
    type Item = V;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, V> IntoIterator for &'a Tuple<V> {
    type Item = &'a V;
    type IntoIter = std::slice::Iter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<V> FromIterator<V> for Tuple<V> {
    fn from_iter<T: IntoIterator<Item = V>>(iter: T) -> Self {
        Self {
            inner: iter.into_iter().collect(),
        }
    }
}

#[subenum(QueryScalar)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
pub enum NativeScalar<
    RowId,
    U64 = u64,
    U32 = u32,
    I64 = i64,
    I32 = i32,
    String = std::string::String,
> {
    RowId(RowId),
    #[subenum(QueryScalar)]
    U64(U64),
    #[subenum(QueryScalar)]
    U32(U32),
    #[subenum(QueryScalar)]
    I64(I64),
    #[subenum(QueryScalar)]
    I32(I32),
    #[subenum(QueryScalar)]
    String(String),
}

impl<R1> NativeScalar<R1> {
    pub fn map<R2, F: Fn(R1) -> R2>(self, f: F) -> NativeScalar<R2> {
        use NativeScalar::*;
        match self {
            RowId(r1) => RowId(f(r1)),
            U64(x) => U64(x),
            U32(x) => U32(x),
            I64(x) => I64(x),
            I32(x) => I32(x),
            String(s) => String(s),
        }
    }
}

// TODO move them to schema.rs?
// The type representations are useful in schema.rs.

pub type TxScalarType = NativeScalar<(), (), (), (), (), ()>;
pub type PublicScalarType = NativeScalar<(), (), (), (), (), ()>;
pub type StoreScalarType = NativeScalar<(), (), (), (), (), ()>;
pub type QueryScalarType = QueryScalar<(), (), (), (), ()>;
