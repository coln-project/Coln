//! Depending on the layer, a tuple (think a row) can contain different values
//! for scalars. For instance, coln-query does _not_ support a column which
//! contains a pair. The pair needs to be flattened into two columns first.
//! This module provides the base scalar value and the generic tuple representations
//! on which other modules can build.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::{fmt, ops::Deref};
use subenum::subenum;

// Only indirectly exposed to the public through [`TxTuple`],
// [`PublicTuple`], [`StoreTuple`], [`QueryTuple`]. If we ever want to switch
// to a different layout (smallvec, for instance), these types benefit
// immediately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Tuple<ScalarValue> {
    pub(crate) inner: Vec<ScalarValue>,
}

impl<V> Tuple<V> {
    pub fn iter(&self) -> std::slice::Iter<'_, V> {
        self.inner.iter()
    }
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

impl<V> From<Vec<V>> for Tuple<V> {
    fn from(value: Vec<V>) -> Self {
        Self { inner: value }
    }
}

#[subenum(QueryScalar)]
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize, Type)]
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

impl<RowId: fmt::Display> fmt::Display for NativeScalar<RowId> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RowId(id) => write!(f, "{id}"),
            Self::U64(x) => write!(f, "{x}"),
            Self::U32(x) => write!(f, "{x}"),
            Self::I64(x) => write!(f, "{x}"),
            Self::I32(x) => write!(f, "{x}"),
            Self::String(s) => write!(f, "{s:?}"),
        }
    }
}

impl<R> NativeScalar<R> {
    pub fn typ(&self) -> NativeScalar<(), (), (), (), (), ()> {
        match self {
            NativeScalar::RowId(_) => NativeScalar::RowId(()),
            NativeScalar::U64(_) => NativeScalar::U64(()),
            NativeScalar::U32(_) => NativeScalar::U32(()),
            NativeScalar::I64(_) => NativeScalar::I64(()),
            NativeScalar::I32(_) => NativeScalar::I32(()),
            NativeScalar::String(_) => NativeScalar::String(()),
        }
    }
}

impl<R1> NativeScalar<R1> {
    pub fn map<R2, F: FnOnce(R1) -> R2>(self, f: F) -> NativeScalar<R2> {
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

    /// Maps the row id, returning `None` when `f` fails. Non-id scalars are
    /// always passed through.
    pub fn try_map<R2, F: FnOnce(R1) -> Option<R2>>(self, f: F) -> Option<NativeScalar<R2>> {
        use NativeScalar::*;
        Some(match self {
            RowId(r1) => RowId(f(r1)?),
            U64(x) => U64(x),
            U32(x) => U32(x),
            I64(x) => I64(x),
            I32(x) => I32(x),
            String(s) => String(s),
        })
    }
}

impl<RowId> From<u64> for NativeScalar<RowId> {
    fn from(value: u64) -> Self {
        NativeScalar::U64(value)
    }
}
impl<RowId> From<u32> for NativeScalar<RowId> {
    fn from(value: u32) -> Self {
        NativeScalar::U32(value)
    }
}
impl<RowId> From<i64> for NativeScalar<RowId> {
    fn from(value: i64) -> Self {
        NativeScalar::I64(value)
    }
}
impl<RowId> From<i32> for NativeScalar<RowId> {
    fn from(value: i32) -> Self {
        NativeScalar::I32(value)
    }
}
impl<RowId> From<String> for NativeScalar<RowId> {
    fn from(value: String) -> Self {
        NativeScalar::String(value)
    }
}
impl<RowId> From<&str> for NativeScalar<RowId> {
    fn from(value: &str) -> Self {
        NativeScalar::String(value.to_owned())
    }
}

// TODO move them to schema.rs?
// The type representations are useful in schema.rs.
