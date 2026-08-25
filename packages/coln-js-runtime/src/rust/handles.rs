// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir;
use coln_store::{
    commit::{chunk::Chunk, hash::CommitHash as StoreCommitHash, pst::decode_commit_chunks},
    store::{
        Store,
        error::{CommitApplyError, StoreIntError},
    },
    table::RowId as StoreRowId,
    txn::{OwnedTransaction, RowHandle as StoreRowHandle},
};
use js_sys::Reflect;

use crate::dto::{CommitChunk, CommitHash, RowId, RowRef, RowView, Value};
use crate::error::js_error;

use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub struct StoreHandle {
    state: StoreHandleState,
}

enum StoreHandleState {
    Uninitialized {
        chunks: Vec<Vec<u8>>,
        has_root: bool,
    },
    Ready {
        store: Box<Store>,
        pending_chunks: Vec<Vec<u8>>,
    },
    Moved,
}

#[wasm_bindgen]
pub struct TransactionHandle {
    tx: Option<OwnedTransaction>,
    recovered_store: Option<Store>,
    pending_chunks: Vec<Vec<u8>>,

    pending_handles: Vec<(StoreRowHandle, JsValue)>,
}

/*
This function turns something like
{
  tag: "row_id",
  value: {
    pending: {
      txId: 1,      // u64 from the transaction
      counter: 0    // u32, first `add` in that txn is typically 0
    }
  }
}
into something like
{
  tag: "row_id",
  value: {
    existing: {
      commit: 0xff,      // u64 from the transaction
      counter: 0    // u32, first `add` in that txn is typically 0
    }
  }
}
i.e. it transforms a value that is a `RowRef` from pending to existing
*/
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
        let tx = self
            .tx
            .take()
            .ok_or_else(|| js_error("transaction has already been committed"))?;

        match tx.commit() {
            Ok((commit, store)) => {
                // at this point the rowhandles would have been resolved to rowids already
                for (handle, value) in &self.pending_handles {
                    let row_id = handle.row_id().map_err(js_error)?;
                    resolve_value_id(value, row_id.into())?;
                }

                Ok(CommitResult {
                    commit: commit.to_string(),
                    store: Some(StoreHandle::ready_with_pending(
                        store,
                        std::mem::take(&mut self.pending_chunks),
                    )),
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
        if let Some(tx) = self.tx.take() {
            return Ok(StoreHandle::ready_with_pending(
                tx.abort(),
                std::mem::take(&mut self.pending_chunks),
            ));
        }
        let store = self
            .recovered_store
            .take()
            .ok_or_else(|| js_error("transaction does not have a recovered store"))?;

        Ok(StoreHandle::ready_with_pending(
            store,
            std::mem::take(&mut self.pending_chunks),
        ))
    }
}

#[wasm_bindgen]
pub struct CommitResult {
    commit: String,
    store: Option<StoreHandle>,
}

#[wasm_bindgen]
impl StoreHandle {
    pub fn empty() -> StoreHandle {
        Self {
            state: StoreHandleState::Uninitialized {
                chunks: Vec::new(),
                has_root: false,
            },
        }
    }

    #[wasm_bindgen(js_name = fromTheory)]
    pub fn from_theory(flat_theory_json: String) -> Result<StoreHandle, JsValue> {
        let theory = serde_json::from_str::<ir::FlatRealm>(&flat_theory_json)
            .map_err(|err| js_error(format!("invalid flat theory JSON: {err}")))?;
        let store = Store::try_from_ir(theory).map_err(js_error)?;

        Ok(Self::ready(store))
    }

    #[wasm_bindgen(js_name = jsonIR)]
    pub fn json_ir(&self) -> Result<String, JsValue> {
        self.store()?.json_ir().map_err(js_error)
    }

    #[wasm_bindgen(js_name = scanTable)]
    pub fn scan_table(&self, path: String) -> Result<Vec<RowView>, JsValue> {
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
        let path = ir::Path::from(path);
        let row_id = StoreRowId::try_from(row_id).map_err(js_error)?;

        Ok(self.store()?.row_by_id(&path, row_id).map(RowView::from))
    }

    #[wasm_bindgen(js_name = beginTransaction)]
    pub fn begin_transaction(&mut self) -> Result<TransactionHandle, JsValue> {
        let state = std::mem::replace(&mut self.state, StoreHandleState::Moved);
        let (store, pending_chunks) = match state {
            StoreHandleState::Ready {
                store,
                pending_chunks,
            } => (*store, pending_chunks),
            state @ StoreHandleState::Uninitialized { .. } => {
                self.state = state;
                return Err(js_error("store handle has not been initialized"));
            }
            StoreHandleState::Moved => {
                return Err(js_error(
                    "store handle has already been moved into a transaction",
                ));
            }
        };

        Ok(TransactionHandle {
            tx: Some(store.into_transaction()),
            recovered_store: None,
            pending_chunks,

            pending_handles: Vec::new(),
        })
    }
}

#[wasm_bindgen]
impl StoreHandle {
    // For automerge-repo interfacing

    pub fn heads(&self) -> Result<Vec<CommitHash>, JsValue> {
        let heads = match &self.state {
            StoreHandleState::Uninitialized { .. } => return Ok(Vec::new()),
            StoreHandleState::Ready { store, .. } => store,
            StoreHandleState::Moved => {
                return Err(js_error(
                    "store handle has already been moved into a transaction",
                ));
            }
        }
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
        if matches!(self.state, StoreHandleState::Uninitialized { .. }) {
            return Ok(Vec::new());
        }
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
        let chunk_bytes =
            serde_wasm_bindgen::from_value::<Vec<Vec<u8>>>(chunk_bytes).map_err(js_error)?;
        self.apply_chunks(chunk_bytes).map_err(js_error)
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
        self.store
            .take()
            .ok_or_else(|| js_error("commit result store has already been taken"))
    }
}

impl StoreHandle {
    fn ready(store: Store) -> Self {
        Self::ready_with_pending(store, Vec::new())
    }

    fn ready_with_pending(store: Store, pending_chunks: Vec<Vec<u8>>) -> Self {
        Self {
            state: StoreHandleState::Ready {
                store: Box::new(store),
                pending_chunks,
            },
        }
    }

    fn apply_chunks(&mut self, chunk_bytes: Vec<Vec<u8>>) -> Result<(), String> {
        match &mut self.state {
            StoreHandleState::Uninitialized { chunks, has_root } => {
                let decoded = chunk_bytes
                    .iter()
                    .map(|bytes| Chunk::decode(bytes))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?;
                let previous_len = chunks.len();
                let previously_had_root = *has_root;
                *has_root |= decoded.iter().any(Chunk::is_root);
                chunks.extend(chunk_bytes);
                if *has_root {
                    match decode_commit_chunks(chunks.iter()) {
                        Ok(store) => self.state = StoreHandle::ready(store).state,
                        Err(StoreIntError::Commit(CommitApplyError::MissingDep)) => return Ok(()),
                        Err(error) => {
                            chunks.truncate(previous_len);
                            *has_root = previously_had_root;
                            return Err(error.to_string());
                        }
                    }
                }
                Ok(())
            }
            StoreHandleState::Ready {
                store,
                pending_chunks,
            } => {
                let previous_len = pending_chunks.len();
                pending_chunks.extend(chunk_bytes);
                match store.apply_chunk_bytes(pending_chunks.iter().cloned()) {
                    Ok(()) => {
                        pending_chunks.clear();
                        Ok(())
                    }
                    Err(StoreIntError::Commit(CommitApplyError::MissingDep)) => Ok(()),
                    Err(error) => {
                        pending_chunks.truncate(previous_len);
                        Err(error.to_string())
                    }
                }
            }
            StoreHandleState::Moved => {
                Err("store handle has already been moved into a transaction".into())
            }
        }
    }

    fn store(&self) -> Result<&Store, JsValue> {
        match &self.state {
            StoreHandleState::Uninitialized { .. } => {
                Err(js_error("store handle has not been initialized"))
            }
            StoreHandleState::Ready { store, .. } => Ok(store),
            StoreHandleState::Moved => Err(js_error(
                "store handle has already been moved into a transaction",
            )),
        }
    }
}

impl TransactionHandle {
    fn tx(&mut self) -> Result<&mut OwnedTransaction, JsValue> {
        self.tx
            .as_mut()
            .ok_or_else(|| js_error("transaction has already been committed"))
    }
}

#[cfg(test)]
mod tests {
    use coln_flir_rs::ir::{
        BuiltinTy, ColType, ColumnEntry, EntityVariant, FlatRealm, Path, Schema, TableEntry,
    };

    use super::*;

    fn source_store() -> Store {
        let theory = FlatRealm {
            tables: vec![TableEntry {
                path: Path::from("T"),
                table: Schema {
                    entity_variant: EntityVariant::Table,
                    columns: vec![ColumnEntry {
                        path: Path::from("value"),
                        col_type: ColType::BuiltinTy {
                            builtin_ty: BuiltinTy::BuiltinInt,
                        },
                    }],
                    primary_key: None,
                },
            }],
            rules: vec![],
        };
        let mut store = Store::try_from_theory(theory).expect("store");
        let mut transaction = store.transaction();
        transaction
            .add(&Path::from("T"), vec![42_i64.into()])
            .expect("add row");
        transaction.commit().expect("commit");
        store
    }

    #[test]
    fn empty_handle_buffers_data_until_root_arrives() {
        let source = source_store();
        let (root, data): (Vec<_>, Vec<_>) = source
            .commit_chunks_after(&[])
            .into_iter()
            .map(|chunk| chunk.bytes)
            .partition(|bytes| Chunk::decode(bytes).expect("chunk").is_root());
        let mut handle = StoreHandle::empty();

        handle.apply_chunks(data).expect("buffer data");
        assert!(matches!(
            handle.state,
            StoreHandleState::Uninitialized { .. }
        ));
        assert!(handle.heads().expect("heads").is_empty());

        handle.apply_chunks(root).expect("apply root");
        let store = handle.store().expect("initialized store");
        let table = store.table_at(&Path::from("T")).expect("table");
        assert_eq!(table.row_count(), 1);
    }

    #[test]
    fn empty_handle_retries_bootstrap_when_missing_parent_arrives() {
        let mut source = source_store();
        let mut transaction = source.transaction();
        transaction
            .add(&Path::from("T"), vec![84_i64.into()])
            .expect("add second row");
        transaction.commit().expect("second commit");

        let mut root = None;
        let mut data = Vec::new();
        for chunk in source.commit_chunks_after(&[]) {
            if Chunk::decode(&chunk.bytes).expect("chunk").is_root() {
                root = Some(chunk.bytes);
            } else {
                data.push(chunk.bytes);
            }
        }
        assert_eq!(data.len(), 2);

        let mut handle = StoreHandle::empty();
        let child = data.pop().expect("child commit");
        handle
            .apply_chunks(vec![root.expect("root"), child])
            .expect("buffer child");

        handle.apply_chunks(data).expect("retry with parent");
        let table = handle
            .store()
            .expect("initialized store")
            .table_at(&Path::from("T"))
            .expect("table");
        assert_eq!(table.row_count(), 2);
    }

    #[test]
    fn ready_handle_retries_commit_when_missing_parent_arrives() {
        let mut source = source_store();
        let mut transaction = source.transaction();
        transaction
            .add(&Path::from("T"), vec![84_i64.into()])
            .expect("add second row");
        transaction.commit().expect("second commit");

        let mut root = None;
        let mut data = Vec::new();
        for chunk in source.commit_chunks_after(&[]) {
            if Chunk::decode(&chunk.bytes).expect("chunk").is_root() {
                root = Some(chunk.bytes);
            } else {
                data.push(chunk.bytes);
            }
        }
        assert_eq!(data.len(), 2);

        let mut handle = StoreHandle::empty();
        handle
            .apply_chunks(vec![root.expect("root")])
            .expect("apply root");
        assert!(matches!(handle.state, StoreHandleState::Ready { .. }));

        let child = data.pop().expect("child commit");
        handle.apply_chunks(vec![child]).expect("buffer child");
        let mut transaction = handle.begin_transaction().expect("begin transaction");
        handle = transaction.take_store().expect("abort transaction");
        handle.apply_chunks(data).expect("retry with parent");

        let table = handle
            .store()
            .expect("initialized store")
            .table_at(&Path::from("T"))
            .expect("table");
        assert_eq!(table.row_count(), 2);
    }

    #[test]
    fn active_transaction_can_return_its_store_without_committing() {
        let mut handle = StoreHandle::ready(source_store());
        let mut transaction = handle.begin_transaction().expect("transaction");
        transaction
            .tx()
            .expect("owned transaction")
            .add(&Path::from("T"), vec![84_i64.into()])
            .expect("stage row");

        let recovered = transaction.take_store().expect("recover store");
        let table = recovered
            .store()
            .expect("store")
            .table_at(&Path::from("T"))
            .expect("table");
        assert_eq!(table.row_count(), 1);
    }

    #[test]
    fn malformed_batch_does_not_poison_later_bootstrap() {
        let source = source_store();
        let chunks = source
            .commit_chunks_after(&[])
            .into_iter()
            .map(|chunk| chunk.bytes)
            .collect::<Vec<_>>();
        let root = chunks
            .iter()
            .find(|bytes| Chunk::decode(bytes).expect("chunk").is_root())
            .expect("root")
            .clone();
        let mut handle = StoreHandle::empty();

        assert!(handle.apply_chunks(vec![root, vec![0xff]]).is_err());
        handle.apply_chunks(chunks).expect("valid retry");

        assert_eq!(
            handle
                .store()
                .expect("initialized store")
                .table_at(&Path::from("T"))
                .expect("table")
                .row_count(),
            1
        );
    }
}
