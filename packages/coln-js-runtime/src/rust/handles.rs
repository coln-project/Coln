use coln_store::{
    commit::hash::CommitHash as StoreCommitHash,
    ir,
    store::Store,
    table::RowId as StoreRowId,
    txn::{OwnedTransaction, ops::RowHandle as StoreRowHandle},
};
use js_sys::Reflect;

use crate::dto::{CommitChunk, CommitHash, RowId, RowRef, RowView, Value};
use crate::error::{js_error, set_panic_hook};

use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub struct StoreHandle {
    store: Option<Store>,
}

#[wasm_bindgen]
pub struct TransactionHandle {
    tx: Option<OwnedTransaction>,
    recovered_store: Option<Store>,

    pending_handles: Vec<(StoreRowHandle, JsValue)>,
}

fn resolve_value_id(js_value: &JsValue, row_id: RowId) -> Result<(), JsValue> {
    let row_id = Value::existing_id(row_id);
    let row_id_js = serde_wasm_bindgen::to_value(&row_id).map_err(js_error)?;

    let new_row_ref = Reflect::get(&row_id_js, &"value".into())?;
    Reflect::set(js_value, &"value".into(), &new_row_ref)?;

    Ok(())
}

#[wasm_bindgen]
impl TransactionHandle {
    pub fn add(&mut self, path: String, values: Vec<Value>) -> Result<JsValue, JsValue> {
        set_panic_hook();
        let path = ir::Path::from(path);
        let values = values.into_iter().map(|v| v.into()).collect::<Vec<_>>();
        let handle = self.tx()?.add(&path, values).map_err(js_error)?;

        let (tx_id, counter) = handle.pending_ids().map_err(js_error)?;
        let temp_id = Value::temp_id(tx_id, counter);
        let js_value = serde_wasm_bindgen::to_value(&temp_id)?;
        self.pending_handles.push((handle, js_value.clone()));

        Ok(js_value)
    }

    pub fn commit(&mut self) -> Result<CommitResult, JsValue> {
        set_panic_hook();
        let tx = self
            .tx
            .take()
            .ok_or_else(|| js_error("transaction has already been committed"))?;

        match tx.commit() {
            Ok((commit, store)) => {
                // at this point the rowhandles would have been resolved to rowids already
                for (handle, value) in &self.pending_handles {
                    let row_id = handle.row_id().map_err(js_error)?;
                    resolve_value_id(&value, row_id.into())?;
                }

                Ok(CommitResult {
                    commit: commit.to_string(),
                    store: Some(StoreHandle { store: Some(store) }),
                })
            }
            Err((err, store)) => {
                self.recovered_store = Some(store);
                Err(js_error(format!(
                    "{err}; recover the store with TransactionHandle.takeStore()"
                )))
            }
        }
    }

    // TODO adjust this API to not use take_store to recover but return the store
    // after committing
    #[wasm_bindgen(js_name = takeStore)]
    pub fn take_store(&mut self) -> Result<StoreHandle, JsValue> {
        set_panic_hook();
        let store = self
            .recovered_store
            .take()
            .ok_or_else(|| js_error("transaction does not have a recovered store"))?;

        Ok(StoreHandle { store: Some(store) })
    }
}

#[wasm_bindgen]
pub struct CommitResult {
    commit: String,
    store: Option<StoreHandle>,
}

#[wasm_bindgen]
impl StoreHandle {
    #[wasm_bindgen(js_name = fromTheory)]
    pub fn from_theory(flat_theory_json: String) -> Result<StoreHandle, JsValue> {
        set_panic_hook();
        let theory = serde_json::from_str::<ir::FlatTheory>(&flat_theory_json)
            .map_err(|err| js_error(format!("invalid flat theory JSON: {err}")))?;
        let store = Store::try_from_theory(theory).map_err(js_error)?;

        Ok(Self { store: Some(store) })
    }

    #[wasm_bindgen(js_name = scanTable)]
    pub fn scan_table(&self, path: String) -> Result<Vec<RowView>, JsValue> {
        set_panic_hook();
        let path = ir::Path::from(path);
        let rows = self
            .store()?
            .scan_table(&path)
            .map(|rows| rows.map(RowView::from).collect::<Vec<_>>())
            .unwrap_or_default();

        Ok(rows)
    }

    #[wasm_bindgen(js_name = rowById)]
    pub fn row_by_id(&self, path: String, row_id: RowRef) -> Result<Option<RowView>, JsValue> {
        set_panic_hook();
        let path = ir::Path::from(path);
        let row_id = StoreRowId::try_from(row_id).map_err(js_error)?;

        Ok(self.store()?.row_by_id(&path, row_id).map(RowView::from))
    }

    #[wasm_bindgen(js_name = beginTransaction)]
    pub fn begin_transaction(&mut self) -> Result<TransactionHandle, JsValue> {
        set_panic_hook();
        let store = self
            .store
            .take()
            .ok_or_else(|| js_error("store handle has already been moved into a transaction"))?;

        Ok(TransactionHandle {
            tx: Some(store.into_transaction()),
            recovered_store: None,

            pending_handles: Vec::new(),
        })
    }
}

#[wasm_bindgen]
impl StoreHandle {
    // For automerge-repo interfacing

    pub fn heads(&self) -> Result<Vec<CommitHash>, JsValue> {
        set_panic_hook();

        let heads = self
            .store()?
            .heads()
            .into_iter()
            .map(CommitHash::from)
            .collect::<Vec<_>>();

        Ok(heads)
    }

    #[wasm_bindgen(js_name = commitChunksAfter)]
    pub fn commit_chunks_after(
        &self,
        have_heads: Vec<CommitHash>,
    ) -> Result<Vec<CommitChunk>, JsValue> {
        set_panic_hook();

        let have_heads = have_heads
            .into_iter()
            .map(StoreCommitHash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(js_error)?;

        let chunks = self
            .store()?
            .commit_chunks_after(&have_heads)
            .into_iter()
            .map(CommitChunk::from)
            .collect::<Vec<_>>();

        Ok(chunks)
    }

    #[wasm_bindgen(js_name = applyChunkBytes)]
    pub fn apply_chunk_bytes(&mut self, chunk_bytes: JsValue) -> Result<(), JsValue> {
        set_panic_hook();

        let chunk_bytes =
            serde_wasm_bindgen::from_value::<Vec<Vec<u8>>>(chunk_bytes).map_err(js_error)?;

        self.store_mut()?
            .apply_chunk_bytes(chunk_bytes)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl CommitResult {
    #[wasm_bindgen(getter)]
    pub fn commit(&self) -> String {
        self.commit.clone()
    }

    #[wasm_bindgen(js_name = takeStore)]
    pub fn take_store(&mut self) -> Result<StoreHandle, JsValue> {
        set_panic_hook();
        self.store
            .take()
            .ok_or_else(|| js_error("commit result store has already been taken"))
    }
}

impl StoreHandle {
    fn store(&self) -> Result<&Store, JsValue> {
        self.store
            .as_ref()
            .ok_or_else(|| js_error("store handle has been moved into a transaction"))
    }

    fn store_mut(&mut self) -> Result<&mut Store, JsValue> {
        self.store
            .as_mut()
            .ok_or_else(|| js_error("store handle has been moved into a transaction"))
    }
}

impl TransactionHandle {
    fn tx(&mut self) -> Result<&mut OwnedTransaction, JsValue> {
        self.tx
            .as_mut()
            .ok_or_else(|| js_error("transaction has already been committed"))
    }
}
