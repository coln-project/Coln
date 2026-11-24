// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Depending on the layer, a tuple (think a row) can contain different values
//! for scalars. For instance, coln-query does _not_ support a column which
//! contains a pair. The pair needs to be flattened into two columns first.
//! This module provides the base scalar value and the generic tuple representations
//! on which other modules can build.

use crate::{engine::query::QueryScalarType, ir};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{fmt, ops::Deref};
use subenum::subenum;

// Only indirectly exposed to the public through [`TxTuple`],
// [`PublicTuple`], [`StoreTuple`], [`QueryTuple`]. If we ever want to switch
// to a different layout (smallvec, for instance), these types benefit
// immediately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct Tuple<ScalarValue> {
    pub(crate) inner: Vec<ScalarValue>,
}

impl<V> Tuple<V> {
    pub fn iter(&self) -> std::slice::Iter<'_, V> {
        self.inner.iter()
    }
    pub fn arity(&self) -> usize {
        self.inner.len()
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

impl<V: fmt::Display> fmt::Display for Tuple<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        let mut iter = self.inner.iter();
        if let Some(first) = iter.next() {
            write!(f, "{first}")?;
            for scalar in iter {
                write!(f, ", {scalar}")?;
            }
        }
        f.write_str(")")
    }
}

/// Every supported row id must implement this to also specify a representation
/// of its type next the representation of its values and the demanded subtraits.
///
/// Note: For non row id types we use () as their type representation.
pub trait ScalarKind: fmt::Display {
    /// The _type_ of the scalar.
    type Ty: fmt::Display + Copy;

    /// Create the type which belongs to this value.
    fn ty(instance: &Self) -> Self::Ty;
}

// Serde is derived per enum rather than on the parent: subenum emits a child's
// own attributes before the parent's derives, so `serde(untagged)` on
// `PublicScalar` would otherwise precede the derive that introduces it.
#[subenum(
    NativeScalar(
        doc = "The scalars of every layer; [`QueryScalar`] and [`PublicScalar`] are subsets.",
        derive(Serialize, Deserialize)
    ),
    QueryScalar(
        doc = "The scalars coln-query computes with. Row ids are flattened into their parts.",
        derive(Serialize, Deserialize)
    ),
    PublicScalar(
        doc = "The scalars crossing the public (FFI) boundary: exactly the column types a Coln program can declare.\n\nThere are no 64-bit integers, so every value round-trips through a JS `number` without precision loss. That also makes the untagged encoding unambiguous: a row id is an object, an `I32` a number and a `String` a string. Adding another integer type would require deserializing against the column types instead.",
        derive(Serialize, Deserialize),
        serde(untagged)
    )
)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Type)]
pub enum NativeScalar<
    RowId,
    U64 = u64,
    U32 = u32,
    I64 = i64,
    I32 = i32,
    String = std::string::String,
> {
    #[subenum(PublicScalar)]
    RowId(RowId),
    #[subenum(QueryScalar)]
    U64(U64),
    #[subenum(QueryScalar)]
    U32(U32),
    #[subenum(QueryScalar)]
    I64(I64),
    #[subenum(QueryScalar, PublicScalar)]
    I32(I32),
    #[subenum(QueryScalar, PublicScalar)]
    String(String),
}

impl<R1> NativeScalar<R1> {
    pub fn map<R2, F: FnOnce(R1) -> R2>(self, f: F) -> NativeScalar<R2> {
        self.try_map(|r1| Some(f(r1)))
            .expect("closure is infallible")
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

impl<R1> PublicScalar<R1> {
    pub fn map<R2, F: FnOnce(R1) -> R2>(self, f: F) -> PublicScalar<R2> {
        self.try_map(|r1| Some(f(r1)))
            .expect("closure is infallible")
    }

    /// Maps the row id, returning `None` when `f` fails. Non-id scalars are
    /// always passed through.
    pub fn try_map<R2, F: FnOnce(R1) -> Option<R2>>(self, f: F) -> Option<PublicScalar<R2>> {
        // `NativeScalar::try_map` preserves the variant, so narrowing back
        // to a public scalar cannot fail.
        Some(NativeScalar::from(self).try_map(f)?.expect_public())
    }
}

impl<RowId: ScalarKind> PublicScalar<RowId> {
    pub fn ty(&self) -> NativeScalarType<RowId::Ty> {
        match self {
            Self::RowId(row_id) => NativeScalar::RowId(ScalarKind::ty(row_id)),
            Self::I32(_) => NativeScalar::I32(()),
            Self::String(_) => NativeScalar::String(()),
        }
    }
}

impl<RowId> NativeScalar<RowId> {
    /// Narrows a stored value to a [`PublicScalar`].
    ///
    /// # Panics
    ///
    /// On the query-only variants. Store columns are typed by
    /// [`ir::ColType`], which only has public scalar types, so values read
    /// from the store never panic here.
    pub fn expect_public(self) -> PublicScalar<RowId> {
        match self {
            Self::RowId(id) => PublicScalar::RowId(id),
            Self::I32(x) => PublicScalar::I32(x),
            Self::String(s) => PublicScalar::String(s),
            other @ (Self::U64(_) | Self::U32(_) | Self::I64(_)) => {
                panic!("{:?} is not a public scalar", other.map(|_| ()))
            }
        }
    }
}

impl<RowId> From<i32> for PublicScalar<RowId> {
    fn from(value: i32) -> Self {
        Self::I32(value)
    }
}

impl<RowId> From<std::string::String> for PublicScalar<RowId> {
    fn from(value: std::string::String) -> Self {
        Self::String(value)
    }
}

impl<RowId> From<&str> for PublicScalar<RowId> {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl<RowId: fmt::Display> fmt::Display for PublicScalar<RowId> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RowId(id) => id.fmt(f),
            Self::I32(x) => write!(f, "{x}"),
            Self::String(s) => write!(f, "{s:?}"),
        }
    }
}

pub type NativeScalarType<RowIdType> = NativeScalar<RowIdType, (), (), (), (), ()>;

impl<RowId: ScalarKind> NativeScalar<RowId> {
    pub fn ty(&self) -> NativeScalarType<RowId::Ty> {
        use NativeScalar::*;
        match self {
            RowId(row_id) => RowId(ScalarKind::ty(row_id)),
            U64(_) => U64(()),
            U32(_) => U32(()),
            I64(_) => I64(()),
            I32(_) => I32(()),
            String(_) => String(()),
        }
    }
}

impl<RowIdType: fmt::Display> fmt::Display for NativeScalarType<RowIdType> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RowId(id) => write!(f, "{id}"),
            Self::U64(_) => write!(f, "u64"),
            Self::U32(_) => write!(f, "u32"),
            Self::I64(_) => write!(f, "i64"),
            Self::I32(_) => write!(f, "i32"),
            Self::String(_) => write!(f, "string"),
        }
    }
}

impl<T1> NativeScalarType<T1> {
    pub fn sloppy_eq<T2>(&self, other: &NativeScalarType<T2>) -> bool {
        use NativeScalar::*;
        matches!(
            (self, other),
            (RowId(_), RowId(_))
                | (U64(_), U64(_))
                | (U32(_), U32(_))
                | (I64(_), I64(_))
                | (I32(_), I32(_))
                | (String(_), String(_))
        )
    }
}

impl<RowId> From<ir::Lit> for NativeScalar<RowId> {
    fn from(value: ir::Lit) -> Self {
        match value {
            ir::Lit::Int { value } => NativeScalar::I32(value),
            ir::Lit::String { value } => NativeScalar::String(value),
        }
    }
}

/// Implements `From<&ir::BuiltinTy>` for both type representations, that is,
/// either [`NativeScalarType<RowId>`] or [`QueryScalarType`].
macro_rules! impl_from_builtinty {
    ($target:ty $(, $generic:ident)?) => {
        impl$(<$generic>)? From<&ir::BuiltinTy> for $target {
            fn from(value: &ir::BuiltinTy) -> Self {
                use ir::BuiltinTy::*;
                match value {
                    BuiltinInt => Self::I32(()),
                    BuiltinStr => Self::String(()),
                }
            }
        }
    };
}

impl_from_builtinty!(NativeScalarType<RowId>, RowId);
impl_from_builtinty!(QueryScalarType);

/// Implements [`fmt::Display`] for a scalar enum carrying the primitive
/// variants of [`NativeScalar`] and, if `$row_id` is given, its row id.
macro_rules! impl_display {
    ($target:ty $(, $row_id:ident)?) => {
        impl$(<$row_id: fmt::Display>)? fmt::Display for $target {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Self::RowId(id) => <$row_id as fmt::Display>::fmt(id, f),)?
                    Self::U64(x) => write!(f, "{x}"),
                    Self::U32(x) => write!(f, "{x}"),
                    Self::I64(x) => write!(f, "{x}"),
                    Self::I32(x) => write!(f, "{x}"),
                    Self::String(s) => write!(f, "{s:?}"),
                }
            }
        }
    };
}

impl_display!(NativeScalar<RowId>, RowId);
impl_display!(QueryScalar);

/// Implements `From<primitive>` for a scalar enum carrying the primitive
/// variants of [`NativeScalar`].
macro_rules! impl_from_primitives {
    ($target:ty $(, $generic:ident)?) => {
        impl$(<$generic>)? From<u64> for $target {
            fn from(value: u64) -> Self {
                Self::U64(value)
            }
        }
        impl$(<$generic>)? From<u32> for $target {
            fn from(value: u32) -> Self {
                Self::U32(value)
            }
        }
        impl$(<$generic>)? From<i64> for $target {
            fn from(value: i64) -> Self {
                Self::I64(value)
            }
        }
        impl$(<$generic>)? From<i32> for $target {
            fn from(value: i32) -> Self {
                Self::I32(value)
            }
        }
        impl$(<$generic>)? From<String> for $target {
            fn from(value: String) -> Self {
                Self::String(value)
            }
        }
        impl$(<$generic>)? From<&str> for $target {
            fn from(value: &str) -> Self {
                Self::String(value.to_owned())
            }
        }
    };
}

impl_from_primitives!(NativeScalar<RowId>, RowId);
impl_from_primitives!(QueryScalar);
