// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

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
    tuple::{NativeScalarType, PublicScalar, ScalarKind, Tuple},
};

/// The unique id that identifies each row in a table.
///
/// It is managed by the database and read-only for the user.
#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Serialize, Deserialize, Type,
)]
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
        Ok(*value1.min(value2))
    }
}

pub type PublicScalarValue = PublicScalar<PublicRowId>;

impl ScalarKind for PublicRowId {
    type Ty = PublicRowIdType;

    fn ty(_instance: &Self) -> Self::Ty {
        PublicRowIdType(())
    }
}

/// The _type_ representing all instances of a [`PublicRowId`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PublicRowIdType(());

impl fmt::Display for PublicRowIdType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicRowId")
    }
}

/// The _type_ representing the supported scalar values visible to the external
/// world.
pub type PublicScalarType = NativeScalarType<PublicRowIdType>;

/// A public facing value, consumed by the FFI/user, where the row_ids are resolved
/// to be (hash, counter)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PublicTuple(pub(crate) Tuple<PublicScalarValue>);

impl WithRowId for PublicTuple {
    type RowId = PublicRowId;
    type Scalar = PublicScalarValue;

    /// The row id a stored row leads with.
    fn row_id(&self) -> Self::RowId {
        match self.first() {
            Some(PublicScalarValue::RowId(id)) => *id,
            other => panic!("stored rows lead with their row id, got {other:?}"),
        }
    }

    fn column_values(&self) -> &[Self::Scalar] {
        &self[1..]
    }

    fn into_column_values(self) -> std::vec::IntoIter<Self::Scalar> {
        let mut values = self.into_iter();
        values.next();
        values
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
            .map(|scalar| scalar.map(&unpack).expect_public())
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
        fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::engine::tx::{PendingRowId, TxScalarValue};
    use crate::ffi::hash::ALL_ZERO_HASH;

    // The JS side relies on scalars being untagged: a row id is an object, an
    // int a number and a string a string.
    #[test]
    fn public_tuple_json_is_untagged() {
        let id = PublicRowId {
            commit: ALL_ZERO_HASH,
            counter: 3,
        };
        let tuple = PublicTuple::from(vec![
            PublicScalarValue::RowId(id),
            PublicScalarValue::I32(-7),
            PublicScalarValue::String("a".into()),
        ]);
        let expected = json!([{ "commit": "00".repeat(32), "counter": 3 }, -7, "a"]);
        assert_eq!(serde_json::to_value(&tuple).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<PublicTuple>(expected).unwrap(),
            tuple
        );
    }

    #[test]
    fn tx_tuple_json_is_untagged() {
        let id = PublicRowId {
            commit: ALL_ZERO_HASH,
            counter: 3,
        };
        let json = json!([
            { "type": "Pending", "value": 0 },
            { "type": "Existing", "value": { "commit": "00".repeat(32), "counter": 3 } },
            5,
            "x",
        ]);
        let tuple: TxTuple = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(
            tuple.as_slice(),
            [
                TxScalarValue::RowId(TxRowId::Pending(PendingRowId(0))),
                TxScalarValue::RowId(TxRowId::Existing(id)),
                TxScalarValue::I32(5),
                TxScalarValue::String("x".into()),
            ]
        );
        assert_eq!(serde_json::to_value(&tuple).unwrap(), json);
    }

    #[test]
    fn rejects_non_i32_numbers() {
        for bad in [json!([1.5]), json!([2147483648i64]), json!([true])] {
            assert!(serde_json::from_value::<PublicTuple>(bad).is_err());
        }
    }
}
