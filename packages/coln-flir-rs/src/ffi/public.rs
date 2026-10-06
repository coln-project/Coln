use std::fmt;
use std::ops::Deref;

use ena::unify::UnifyValue;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    engine::{
        packed::{PackedRowId, StoreTuple, WithRowId},
        tx::{TxRowId, TxTuple},
    },
    ffi::hash::CommitHash,
    tuple::{NativeScalar, Tuple},
};

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

impl WithRowId for PublicTuple {
    type RowId = PublicRowId;

    /// The row id a stored row leads with.
    fn row_id(&self) -> Self::RowId {
        match self.first() {
            Some(PublicScalarValue::RowId(id)) => id.clone(),
            other => panic!("stored rows lead with their row id, got {other:?}"),
        }
    }
}

impl PublicTuple {
    pub fn from_tx(tuple: TxTuple, promote: impl Fn(TxRowId) -> PublicRowId) -> Self {
        let inner = tuple
            .0
            .inner
            .into_iter()
            .map(|scalar| scalar.map(&promote))
            .collect();
        Self(Tuple { inner })
    }

    pub fn from_store(tuple: StoreTuple, unpack: impl Fn(PackedRowId) -> PublicRowId) -> Self {
        let inner = tuple
            .0
            .inner
            .into_iter()
            .map(|scalar| scalar.map(&unpack))
            .collect();
        Self(Tuple { inner })
    }
}

impl Deref for PublicTuple {
    type Target = [PublicScalarValue];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<PublicScalarValue>> for PublicTuple {
    fn from(values: Vec<PublicScalarValue>) -> Self {
        Self(values.into())
    }
}

impl FromIterator<PublicScalarValue> for PublicTuple {
    fn from_iter<T: IntoIterator<Item = PublicScalarValue>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for PublicTuple {
    type Item = PublicScalarValue;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl fmt::Display for PublicTuple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        for (i, scalar) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            match scalar {
                NativeScalar::RowId(id) => write!(f, "{id}")?,
                NativeScalar::U64(x) => write!(f, "{x}")?,
                NativeScalar::U32(x) => write!(f, "{x}")?,
                NativeScalar::I64(x) => write!(f, "{x}")?,
                NativeScalar::I32(x) => write!(f, "{x}")?,
                NativeScalar::String(s) => write!(f, "{s:?}")?,
            }
        }
        f.write_str(")")
    }
}
