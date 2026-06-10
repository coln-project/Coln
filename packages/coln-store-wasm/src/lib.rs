use std::array::TryFromSliceError;
use std::fmt;

use coln_store::{
    commit::hash::CommitHash,
    ir,
    store::Store,
    table::{CellValue as StoreCellValue, RowId as StoreRowId, RowView as StoreRowView},
    txn::ops::{
        RowRef as StoreRowRef, TempRowId as StoreTempRowId, TxnCellValue as StoreTxnCellValue,
    },
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct StoreHandle {
    store: Store,
}

#[wasm_bindgen]
impl StoreHandle {
    #[wasm_bindgen(js_name = fromTheory)]
    pub fn from_theory(flat_theory_json: String) -> Result<StoreHandle, JsValue> {
        let theory = serde_json::from_str::<ir::FlatTheory>(&flat_theory_json)
            .map_err(|err| js_error(format!("invalid flat theory JSON: {err}")))?;
        let store = Store::try_from_theory(theory).map_err(js_error)?;

        Ok(Self { store })
    }

    #[wasm_bindgen(js_name = scanTable)]
    pub fn scan_table(&self, path: String) -> Result<JsValue, JsValue> {
        let path = ir::Path::from(path);
        let rows = self
            .store
            .scan_table(&path)
            .map(|rows| rows.map(RowView::from).collect::<Vec<_>>())
            .unwrap_or_default();

        serde_wasm_bindgen::to_value(&rows).map_err(js_error)
    }

    #[wasm_bindgen(js_name = rowById)]
    pub fn row_by_id(&self, path: String, row_id: JsValue) -> Result<JsValue, JsValue> {
        let path = ir::Path::from(path);
        let row_id: RowId = serde_wasm_bindgen::from_value(row_id).map_err(js_error)?;
        let row_id = StoreRowId::try_from(row_id).map_err(js_error)?;

        match self.store.row_by_id(&path, row_id).map(RowView::from) {
            Some(row) => serde_wasm_bindgen::to_value(&row).map_err(js_error),
            None => Ok(JsValue::NULL),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowId {
    pub commit: String,
    pub counter: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tag", rename_all = "lowercase")]
pub enum RowRef {
    Existing { row_id: RowId },
    Pending { counter: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tag", content = "value", rename_all = "lowercase")]
pub enum CellValue {
    Id(RowRef),
    Int(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowView {
    pub row_id: RowId,
    pub values: Vec<CellValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryError {
    message: String,
}

impl BoundaryError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BoundaryError {}

fn js_error(error: impl fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

impl From<StoreRowId> for RowId {
    fn from(value: StoreRowId) -> Self {
        Self {
            commit: value.commit.to_string(),
            counter: value.counter,
        }
    }
}

impl TryFrom<RowId> for StoreRowId {
    type Error = BoundaryError;

    fn try_from(value: RowId) -> Result<Self, Self::Error> {
        let commit_bytes = decode_commit_hash(&value.commit)?;
        Ok(Self {
            commit: CommitHash(commit_bytes),
            counter: value.counter,
        })
    }
}

impl From<StoreRowRef> for RowRef {
    fn from(value: StoreRowRef) -> Self {
        match value {
            StoreRowRef::Existing(row_id) => Self::Existing {
                row_id: row_id.into(),
            },
            StoreRowRef::Pending(temp_id) => Self::Pending {
                counter: temp_id_counter(temp_id),
            },
        }
    }
}

impl TryFrom<RowRef> for StoreRowRef {
    type Error = BoundaryError;

    fn try_from(value: RowRef) -> Result<Self, Self::Error> {
        match value {
            RowRef::Existing { row_id } => Ok(Self::Existing(row_id.try_into()?)),
            RowRef::Pending { counter } => Ok(Self::Pending(StoreTempRowId::from(counter))),
        }
    }
}

impl From<StoreCellValue> for CellValue {
    fn from(value: StoreCellValue) -> Self {
        match value {
            StoreCellValue::Id(row_id) => Self::Id(StoreRowRef::Existing(row_id).into()),
            StoreCellValue::Int(value) => Self::Int(value),
            StoreCellValue::Str(value) => Self::String(value),
        }
    }
}

impl TryFrom<CellValue> for StoreTxnCellValue {
    type Error = BoundaryError;

    fn try_from(value: CellValue) -> Result<Self, Self::Error> {
        match value {
            CellValue::Id(row_ref) => Ok(Self::Id(row_ref.try_into()?)),
            CellValue::Int(value) => Ok(Self::Int(value)),
            CellValue::String(value) => Ok(Self::Str(value)),
        }
    }
}

impl From<StoreRowView> for RowView {
    fn from(value: StoreRowView) -> Self {
        Self {
            row_id: value.row_id.into(),
            values: value.values.into_iter().map(CellValue::from).collect(),
        }
    }
}

fn decode_commit_hash(value: &str) -> Result<[u8; 32], BoundaryError> {
    let bytes = hex::decode(value).map_err(|err| {
        BoundaryError::new(format!("invalid row id commit hash {value:?}: {err}"))
    })?;

    bytes
        .as_slice()
        .try_into()
        .map_err(|err: TryFromSliceError| {
            BoundaryError::new(format!(
                "invalid row id commit hash length: expected 32 bytes, got {}; {err}",
                bytes.len()
            ))
        })
}

fn temp_id_counter(value: StoreTempRowId) -> u32 {
    value.counter()
}
