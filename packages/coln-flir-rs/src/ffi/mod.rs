//! Data structures expected by FFIs

use std::fmt;

use ena::unify::UnifyValue;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{ffi::hash::CommitHash, value::Value};

pub mod hash;
pub mod query;

/// The unique id that identifies each row in a table.
///
/// It is managed by the database and read-only for the user.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Serialize, Deserialize, Type)]
pub struct WireRowId {
    pub commit: CommitHash,
    pub counter: u32,
}

impl fmt::Display for WireRowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.commit.0[..6] {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ":{}", self.counter)
    }
}

// For id canonicalisation id coln-store, too lazy to use newtype in coln-store
impl UnifyValue for WireRowId {
    type Error = ena::unify::NoError;

    fn unify_values(value1: &Self, value2: &Self) -> Result<Self, Self::Error> {
        Ok((value1).min(value2).clone())
    }
}

pub type WireValue = Value<WireRowId>;

impl fmt::Display for WireValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireValue::Id(id) => write!(f, "#{id}"),
            WireValue::Int(value) => write!(f, "{value}"),
            WireValue::Str(value) => write!(f, "{value:?}"),
        }
    }
}

pub type WireTuple = Vec<WireValue>;

/// Public facing row value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireRowView {
    pub row_id: WireRowId,
    pub values: Vec<WireValue>,
}
