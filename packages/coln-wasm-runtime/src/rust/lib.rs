// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT
use wasm_bindgen::prelude::*;
use coln_store::store::wire::WireStore;

#[wasm_bindgen]
pub struct StoreWrapper {
    store: WireStore,
}

#[wasm_bindgen]
pub fn store_from_ir(ir_json: String, coln_source: String, realm_name: String) -> Result<StoreWrapper, String> {
    let store = WireStore::from_ir(ir_json, coln_source, realm_name)?;
    Ok(StoreWrapper { store })
}

#[wasm_bindgen]
impl StoreWrapper {
    #[wasm_bindgen]
    pub fn start_transaction(&mut self) {
        self.store.start_transaction();
    }

    #[wasm_bindgen]
    pub fn end_transaction(&mut self) -> String {
        self.store.end_transaction()
    }

    #[wasm_bindgen]
    pub fn abort_transaction(&mut self) {
        self.store.abort_transaction();
    }

    #[wasm_bindgen]
    pub fn all_proj(&self, query: String, select: String) -> Result<String, String> {
        self.store.all_proj(query, select)
    }

    #[wasm_bindgen]
    pub fn all_row_id(&self, query: String) -> Result<String, String> {
        self.store.all_row_id(query)
    }

    #[wasm_bindgen]
    pub fn one_proj(&self, query: String, select: String) -> Result<String, String> {
        self.store.one_proj(query, select)
    }

    #[wasm_bindgen]
    pub fn exists(&self, query: String) -> Result<bool, String> {
        self.store.exists(query)
    }

    #[wasm_bindgen]
    pub fn add(&mut self, table_name: String, values: String) -> Result<String, String> {
        self.store.add(table_name, values)
    }
}
