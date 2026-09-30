//! Depending on the layer, a tuple (think a row) can contain different values
//! for scalars. For instance, coln-query does _not_ support a column which
//! contains a pair. The pair needs to be flattened into two columns first.
//! This module provides representations for tuples in each layer and offers
//! conversion methods between them, similar to what [mod@super::tuple] does
//! but to schemas.

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

// TODO @Leo, I removed the ValueMap thing because this type needs to derive specta::Type
// which does not support generics. I did not find a good way around either (◞‸◟；)
#[subenum(QueryScalar)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
pub enum NativeScalar<RowId> {
    RowId(RowId),
    #[subenum(QueryScalar)]
    U64(u64),
    #[subenum(QueryScalar)]
    U32(u32),
    #[subenum(QueryScalar)]
    I64(i64),
    #[subenum(QueryScalar)]
    I32(i32),
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

// #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// pub struct ValueMap {}

// impl NativeScalarMap for ValueMap {
//     type U64 = u64;
//     type U32 = u32;
//     type I64 = i64;
//     type I32 = i32;
//     type String = String;
// }

// pub struct TypeMap {}
// impl NativeScalarMap for TypeMap {
//     type U64 = ();
//     type U32 = ();
//     type I64 = ();
//     type I32 = ();
//     type String = ();
// }

pub type QueryScalarValue = QueryScalar;

// TODO move them to schema.rs?
// The type representations are useful in schema.rs.

pub type TxScalarType = NativeScalar<()>;
pub type PublicScalarType = NativeScalar<()>;
pub type StoreScalarType = NativeScalar<()>;
pub type QueryScalarType = QueryScalar;
