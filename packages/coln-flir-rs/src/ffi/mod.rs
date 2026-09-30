//! Data structures expected by FFIs

use std::fmt;

use ena::unify::UnifyValue;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    engine::{
        txn_val::{TxRowId, TxTuple},
    },
        value::{NativeScalar, Tuple},
    ffi::hash::CommitHash,
};

pub mod hash;
pub mod query;

/// The unique id that identifies each row in a table.
///
/// It is managed by the database and read-only for the user.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Serialize, Deserialize, Type)]
pub struct PublicRowId {
    pub commit: CommitHash,
    pub counter: u32,
}

impl fmt::Display for PublicRowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.commit.0[..6] {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ":{}", self.counter)
    }
}

// For id canonicalisation id coln-store, too lazy to use newtype in coln-store
impl UnifyValue for PublicRowId {
    type Error = ena::unify::NoError;

    fn unify_values(value1: &Self, value2: &Self) -> Result<Self, Self::Error> {
        Ok((value1).min(value2).clone())
    }
}

pub type PublicScalarValue = NativeScalar<PublicRowId>;

/// A public facing value, consumed by the FFI/user, where the row_ids are resolved
/// to be (hash, counter)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PublicTuple(pub(crate) Tuple<PublicScalarValue>);

impl PublicTuple {
    pub fn from_tx(tuple: TxTuple, promote: impl Fn(TxRowId) -> PublicRowId) -> Self {
        let inner = tuple
            .0
            .inner
            .into_iter()
            .map(|scalar| scalar.0.map(&promote))
            .collect();
        Self(Tuple { inner })
    }
}

impl fmt::Display for PublicTuple {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

/// Public facing row value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireRowView {
    pub row_id: PublicRowId,
    pub values: PublicTuple,
}
