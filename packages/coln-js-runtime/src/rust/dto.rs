use serde::{Deserialize, Serialize};
use std::array::TryFromSliceError;
use tsify::Tsify;
use wasm_bindgen::prelude::wasm_bindgen;

use coln_store::{
    commit::hash::CommitHash as StoreCommitHash,
    store::CommitChunk as StoreCommitChunk,
    table::{CellValue as StoreCellValue, RowId as StoreRowId, RowView as StoreRowView},
    txn::ops::{RowHandle, TxnValue as StoreTxnValue},
};

use crate::error::BoundaryError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct CommitChunk {
    pub hash: CommitHash,
    pub parents: Vec<CommitHash>,
    pub bytes: Vec<u8>,
}

impl From<StoreCommitChunk> for CommitChunk {
    fn from(value: StoreCommitChunk) -> Self {
        Self {
            hash: value.hash.into(),
            parents: value.parents.into_iter().map(CommitHash::from).collect(),
            bytes: value.bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(transparent)]
pub struct CommitHash {
    value: String,
}

impl From<StoreCommitHash> for CommitHash {
    fn from(value: StoreCommitHash) -> Self {
        Self {
            value: value.to_string(),
        }
    }
}

impl TryFrom<CommitHash> for StoreCommitHash {
    type Error = BoundaryError;

    fn try_from(value: CommitHash) -> Result<Self, Self::Error> {
        Ok(StoreCommitHash(decode_commit_hash(&value.value)?))
    }
}

/// TempRowId for JS runtime, different from TempRowId in coln-store
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct TempRowId {
    pub tx_id: u64,
    pub counter: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct RowId {
    pub commit: CommitHash,
    pub counter: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub enum RowRef {
    Pending(TempRowId),
    Existing(RowId),
}

impl From<TempRowId> for RowRef {
    fn from(value: TempRowId) -> Self {
        RowRef::Pending(value)
    }
}

impl From<RowId> for RowRef {
    fn from(value: RowId) -> Self {
        RowRef::Existing(value)
    }
}

impl TryFrom<RowRef> for StoreRowId {
    type Error = BoundaryError;
    fn try_from(value: RowRef) -> Result<Self, Self::Error> {
        match value {
            RowRef::Pending(_) => Err(BoundaryError::new(
                "pending row ids cannot be turned into real row ids",
            )),
            RowRef::Existing(row_id) => row_id.try_into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(from_wasm_abi, into_wasm_abi)]
#[serde(tag = "tag", content = "value", rename_all = "lowercase")]
pub enum Value {
    #[serde(rename = "row_id")]
    Id(RowRef),
    Int(i64),
    String(String),
}

impl Value {
    pub(crate) fn temp_id(tx_id: u64, counter: u32) -> Self {
        let temp_id = TempRowId { tx_id, counter };
        Value::Id(RowRef::Pending(temp_id))
    }

    pub(crate) fn existing_id(row_id: RowId) -> Self {
        Value::Id(RowRef::Existing(row_id))
    }

    fn row_ref(&self) -> Option<RowRef> {
        match self {
            Value::Id(row_ref) => Some(row_ref.clone()),
            Value::Int(_) => None,
            Value::String(_) => None,
        }
    }
}

#[wasm_bindgen(js_name = valueEqual)]
pub fn value_equal(v0: Value, v1: Value) -> bool {
    v0 == v1
}

#[wasm_bindgen(js_name = getRowRef)]
pub fn value_row_ref(v: Value) -> Option<RowRef> {
    v.row_ref()
}

// For reading
impl From<StoreCellValue> for Value {
    fn from(value: StoreCellValue) -> Self {
        match value {
            StoreCellValue::Id(id) => Value::Id(RowId::from(id).into()),
            StoreCellValue::Int(i) => Value::Int(i),
            StoreCellValue::Str(s) => Value::String(s),
        }
    }
}

impl From<Value> for StoreTxnValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Id(row_ref) => {
                let handle = match row_ref {
                    RowRef::Pending(temp_row_id) => {
                        RowHandle::from_pending(temp_row_id.tx_id.into(), temp_row_id.counter)
                    }
                    RowRef::Existing(row_id) => RowHandle::from_existing(
                        row_id.try_into().expect("commit hash not messed up"),
                    ),
                };
                handle.into()
            }
            Value::Int(i) => StoreTxnValue::Int(i),
            Value::String(s) => StoreTxnValue::Str(s),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct RowView {
    pub row_id: RowRef,
    pub values: Vec<Value>,
}

impl From<StoreRowId> for RowId {
    fn from(value: StoreRowId) -> Self {
        Self {
            commit: value.commit.into(),
            counter: value.counter,
        }
    }
}

impl TryFrom<RowId> for StoreRowId {
    type Error = BoundaryError;

    fn try_from(value: RowId) -> Result<Self, Self::Error> {
        Ok(Self {
            commit: StoreCommitHash::try_from(value.commit)?,
            counter: value.counter,
        })
    }
}

impl From<StoreRowView> for RowView {
    fn from(value: StoreRowView) -> Self {
        let row_id: RowId = value.row_id.into();
        Self {
            row_id: row_id.into(),
            values: value.values.into_iter().map(Value::from).collect(),
        }
    }
}

fn decode_commit_hash(value: &str) -> Result<[u8; 32], BoundaryError> {
    let bytes = hex::decode(value)
        .map_err(|err| BoundaryError::new(format!("invalid commit hash {value:?}: {err}")))?;

    bytes
        .as_slice()
        .try_into()
        .map_err(|err: TryFromSliceError| {
            BoundaryError::new(format!(
                "invalid commit hash length: expected 32 bytes, got {}; {err}",
                bytes.len()
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hash(byte: u8) -> StoreCommitHash {
        StoreCommitHash([byte; 32])
    }

    #[test]
    fn commit_hash_serializes_as_hex_string() {
        let dto = CommitHash::from(hash(0xab));
        let value = serde_json::to_value(&dto).expect("serialize commit hash");

        assert_eq!(
            value,
            json!("abababababababababababababababababababababababababababababababab")
        );
    }

    #[test]
    fn commit_hash_round_trips_to_store_hash() {
        let store_hash = hash(0x42);
        let dto = CommitHash::from(store_hash);
        let decoded = StoreCommitHash::try_from(dto).expect("decode commit hash");

        assert_eq!(decoded, store_hash);
    }

    #[test]
    fn commit_hash_rejects_invalid_hex_length() {
        let dto = CommitHash {
            value: "abcd".to_string(),
        };
        let err = StoreCommitHash::try_from(dto).expect_err("reject short commit hash");

        assert!(err.to_string().contains("invalid commit hash length"));
    }

    #[test]
    fn row_id_serializes_with_camel_case_fields() {
        let row_id = RowId {
            commit: CommitHash::from(hash(0x01)),
            counter: 7,
        };
        let value = serde_json::to_value(&row_id).expect("serialize row id");

        assert_eq!(
            value,
            json!({
                "commit": "0101010101010101010101010101010101010101010101010101010101010101",
                "counter": 7
            })
        );
    }

    #[test]
    fn row_id_round_trips_to_store_row_id() {
        let store_row_id = StoreRowId {
            commit: hash(0x11),
            counter: 3,
        };
        let dto = RowId::from(store_row_id);
        let decoded = StoreRowId::try_from(dto).expect("decode row id");

        assert_eq!(decoded, store_row_id);
    }
}
