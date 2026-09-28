//! Depending on the layer, a tuple (think a row) can contain different values
//! for scalars. For instance, coln-query does _not_ support a column which
//! contains a pair. The pair needs to be flattened into two columns first.
//! This module provides representations for tuples in each layer and offers
//! conversion methods between them, similar to what [mod@super::tuple] does
//! but to schemas.

use crate::{engine::schema::BaseTableSchema, hash::CommitHash};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::{fmt, iter};
use subenum::subenum;

// Only indirectly exposed to the public through [`WireTxTuple`],
// [`PublicTuple`], [`StoreTuple`], [`QueryTuple`]. If we ever want to switch
// to a different layout (smallvec, for instance), these types benefit
// immediately.
pub struct Tuple<ScalarValue> {
    inner: Vec<ScalarValue>,
}

// Placeholder but imagine this to be the main entrypoint to coln-store.
type ColnStore = ();

pub struct WireTxTuple(Tuple<WireTxScalarValue>);

pub struct PublicTuple(Tuple<PublicScalarValue>);

impl PublicTuple {
    pub fn from_wire(tuple: WireTxTuple, coln_store: &mut ColnStore) -> Self {
        let inner = tuple
            .0
            .inner
            .into_iter()
            .map(|scalar| match scalar {
                WireTxScalarValue::U64(scalar) => PublicScalarValue::U64(scalar),
                WireTxScalarValue::U32(scalar) => PublicScalarValue::U32(scalar),
                WireTxScalarValue::I64(scalar) => PublicScalarValue::I64(scalar),
                WireTxScalarValue::I32(scalar) => PublicScalarValue::I32(scalar),
                WireTxScalarValue::String(scalar) => PublicScalarValue::String(scalar),
                WireTxScalarValue::RowId(row_id) => PublicScalarValue::RowId(todo!(
                    "Call into coln-store's API to resolve the row id to a public one"
                )),
            })
            .collect();
        Self(Tuple { inner })
    }
}

pub struct StoreTuple(Tuple<StoreScalarValue>);

impl StoreTuple {
    pub fn from_public(tuple: PublicTuple, coln_store: &mut ColnStore) -> Self {
        let inner = tuple
            .0
            .inner
            .into_iter()
            .map(|scalar| match scalar {
                PublicScalarValue::U64(scalar) => StoreScalarValue::U64(scalar),
                PublicScalarValue::U32(scalar) => StoreScalarValue::U32(scalar),
                PublicScalarValue::I64(scalar) => StoreScalarValue::I64(scalar),
                PublicScalarValue::I32(scalar) => StoreScalarValue::I32(scalar),
                PublicScalarValue::String(scalar) => StoreScalarValue::String(scalar),
                PublicScalarValue::RowId(row_id) => StoreScalarValue::RowId(todo!(
                    "Call into coln-store's API to resolve the row id to a packed one"
                )),
            })
            .collect();
        Self(Tuple { inner })
    }
}

pub struct QueryTuple(Tuple<QueryScalarValue>);

impl QueryTuple {
    pub fn from_store(tuple: StoreTuple) -> QueryTuple {
        let inner = tuple
            .0
            .inner
            .into_iter()
            .flat_map(|scalar| match scalar {
                StoreScalarValue::RowId(row_id) => {
                    let hash = QueryScalarValue::U32(row_id.commit_idx);
                    let counter = QueryScalarValue::U32(row_id.counter);
                    iter::once(hash).chain(Some(counter))
                }
                StoreScalarValue::U64(scalar) => {
                    iter::once(QueryScalarValue::U64(scalar)).chain(None)
                }
                StoreScalarValue::U32(scalar) => {
                    iter::once(QueryScalarValue::U32(scalar)).chain(None)
                }
                StoreScalarValue::I64(scalar) => {
                    iter::once(QueryScalarValue::I64(scalar)).chain(None)
                }
                StoreScalarValue::I32(scalar) => {
                    iter::once(QueryScalarValue::I32(scalar)).chain(None)
                }
                StoreScalarValue::String(scalar) => {
                    iter::once(QueryScalarValue::String(scalar)).chain(None)
                }
            })
            .collect();
        Self(Tuple { inner })
    }
    pub fn into_public(self, coln_store: &mut ColnStore, schema: &BaseTableSchema) -> PublicTuple {
        // This for tuples bubbling upwards, that is, tuples which belong to
        // derived views. They do not contain a row id themselves but may
        // contain several foreign keys with row ids. This function needs to
        // deflatten row id hash ints (plus remap them to their hash) and row
        // id counter ints into pairs.
        // To do so, I think it needs the schema to know in which position
        // there are foreign keys. Note that I still need to adjust BaseTableSchema
        // to also accomodate DerivedViews which do not have a row id on their own
        // (which BaseTableSchema currently assumes).
        todo!("Call into coln-store's API")
    }
}

#[subenum(WireTxScalar, PublicScalar, StoreScalar, QueryScalar)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NativeScalar<RowId, T: NativeScalarMap> {
    #[subenum(WireTxScalar, PublicScalar, StoreScalar)]
    RowId(RowId),
    /// Unsigned 64-bit integer.
    #[subenum(WireTxScalar, PublicScalar, StoreScalar, QueryScalar)]
    U64(T::U64),
    /// Unsigned 32-bit integer.
    #[subenum(WireTxScalar, PublicScalar, StoreScalar, QueryScalar)]
    U32(T::U32),
    /// Signed 64-bit integer.
    #[subenum(WireTxScalar, PublicScalar, StoreScalar, QueryScalar)]
    I64(T::I64),
    /// Signed 32-bit integer.
    #[subenum(WireTxScalar, PublicScalar, StoreScalar, QueryScalar)]
    I32(T::I32),
    /// String.
    #[subenum(WireTxScalar, PublicScalar, StoreScalar, QueryScalar)]
    String(T::String),
    // Add more :)
}

pub trait NativeScalarMap {
    type U64: Eq + PartialEq;
    type U32: Eq + PartialEq;
    type I64: Eq + PartialEq;
    type I32: Eq + PartialEq;
    type String: Eq + PartialEq;
}

pub struct ValueMap {}
impl NativeScalarMap for ValueMap {
    type U64 = u64;
    type U32 = u32;
    type I64 = i64;
    type I32 = i32;
    type String = String;
}

pub struct TypeMap {}
impl NativeScalarMap for TypeMap {
    type U64 = ();
    type U32 = ();
    type I64 = ();
    type I32 = ();
    type String = ();
}

pub type WireTxScalarValue = WireTxScalar<WireRowId, ValueMap>;
pub type PublicScalarValue = PublicScalar<PublicRowId, ValueMap>;
pub type StoreScalarValue = StoreScalar<PackedRowId, ValueMap>;
pub type QueryScalarValue = QueryScalar<ValueMap>;

// The type representations are useful in schema.rs.

pub type WireTxScalarType = WireTxScalar<(), TypeMap>;
pub type PublicScalarType = PublicScalar<(), TypeMap>;
pub type StoreScalarType = StoreScalar<(), TypeMap>;
pub type QueryScalarType = QueryScalar<TypeMap>;

// Different row id representations.

/// The unique id which belongs to exactly one row in the entire database.
///
/// It is managed by the database and read-only for the user.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Serialize, Deserialize, Type)]
pub struct PublicRowId {
    commit: CommitHash,
    counter: u32,
}

impl fmt::Display for PublicRowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.commit.0[..6] {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ":{}", self.counter)
    }
}

/// A compact [`RowId`] representation that dictionary-encodes commit hashes.
/// Purely for internal consumption within coln-store and coln-query.
///
/// This is only meaningful together with the store-wide
/// [`IdPacker`](crate::id_packer::IdPacker) that produced it, so it never
/// crosses the store boundary. Packed ids order by `(commit_idx, counter)`,
/// which depends on dictionary insertion order. Deterministic ordering across
/// stores must compare unpacked [`RowId`]s.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackedRowId {
    commit_idx: u32,
    counter: u32,
}

/// A temporary row id which is valid only within a single transaction and
/// will be resolved into a [`PublicRowId`] at commit time.
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

/// Unlike [`PublicRowId`] this row id can additionally be a [`PendingRowId`]
/// to be resolved at a later point in time.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", content = "value")]
pub enum WireRowId {
    Existing(PublicRowId),
    Pending(PendingRowId),
}

impl WireRowId {
    pub fn resolve(self, commit: CommitHash) -> PublicRowId {
        match self {
            WireRowId::Existing(existing_id) => existing_id,
            WireRowId::Pending(pending_id) => pending_id.resolve(commit),
        }
    }
}

impl From<PublicRowId> for WireRowId {
    fn from(value: PublicRowId) -> Self {
        Self::Existing(value)
    }
}
