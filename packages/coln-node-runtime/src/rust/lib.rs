// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_store::store::wire::WireStore;
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi]
pub struct StoreWrapper {
    store: WireStore,
}

fn as_napi_error(s: String) -> Error<Status> {
    Error::new(Status::GenericFailure, s)
}

#[napi]
pub fn store_from_ir(ir_json: String, coln_source: String, realm_name: String) -> Result<StoreWrapper> {
    let store = WireStore::from_ir(ir_json, coln_source, realm_name).map_err(as_napi_error)?;
    Ok(StoreWrapper { store })
}


#[napi]
impl StoreWrapper {
    #[napi]
    pub fn start_transaction(&mut self) {
        self.store.start_transaction();
    }

    #[napi]
    pub fn end_transaction(&mut self) -> String {
        self.store.end_transaction()
    }

    #[napi]
    pub fn abort_transaction(&mut self) {
        self.store.abort_transaction();
    }

    #[napi]
    pub fn all_proj(&self, query: String, select: String) -> Result<String> {
        self.store.all_proj(query, select).map_err(as_napi_error)
    }

    #[napi]
    pub fn all_row_id(&self, query: String) -> Result<String> {
        self.store.all_row_id(query).map_err(as_napi_error)
    }

    #[napi]
    pub fn one_proj(&self, query: String, select: String) -> Result<String> {
        self.store.one_proj(query, select).map_err(as_napi_error)
    }

    #[napi]
    pub fn exists(&self, query: String) -> Result<bool> {
        self.store.exists(query).map_err(as_napi_error)
    }

    #[napi]
    pub fn add(&mut self, table_name: String, values: String) -> Result<String> {
        self.store.add(table_name, values).map_err(as_napi_error)
    }
}
