// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    engine::{packed::StoreTuple, schema::ColnDef, tx::TxTuple},
    ir,
    public::PublicTuple,
};
use coln_store::IdLookup;
use coln_store::store::auto::AutoStore;
use coln_store::txn::rw::StoreRead;
use coln_store::txn::rw::StoreWrite;
use napi::bindgen_prelude::*;
use napi_derive::napi;

#[napi]
pub struct AutoStoreWrapper {
    store: AutoStore,
}

impl AutoStoreWrapper {
    fn unpack_tuple(&self, tuple: StoreTuple) -> PublicTuple {
        tuple
            .into_iter()
            .map(|value| {
                value.map(|id| {
                    self.store
                        .id_lookup()
                        .unpacked(&id)
                        .expect("row ids from store can be unpacked")
                })
            })
            .collect()
    }
}

#[napi]
pub fn store_from_ir(
    ir_json: String,
    coln_source: String,
    realm_name: String,
) -> Result<AutoStoreWrapper> {
    let ir = serde_json::from_str(&ir_json).expect("parse flir");
    let coln_def = ColnDef::new(coln_source, realm_name);
    Ok(AutoStoreWrapper {
        store: AutoStore::try_from_ir(&ir, coln_def).expect("create store successful"),
    })
}

#[napi]
impl AutoStoreWrapper {
    #[napi]
    pub fn start_transaction(&mut self) {
        self.store.transaction();
    }

    #[napi]
    pub fn end_transaction(&mut self) -> String {
        self.store.try_commit().unwrap();
        serde_json::to_string(&self.store.commit().unwrap()).unwrap()
    }

    #[napi]
    pub fn abort_transaction(&mut self) {
        self.store.abort();
    }

    #[napi]
    pub fn all_proj(&self, query: String, select: String) -> Result<String> {
        let res = self
            .store
            .all_proj(
                &(serde_json::from_str(&query).unwrap()),
                &(serde_json::from_str::<Vec<_>>(&select).unwrap()),
            )
            .unwrap();
        let res: Vec<_> = res
            .into_iter()
            .map(|tuple| self.unpack_tuple(tuple))
            .collect();
        Ok(serde_json::to_string(&res).unwrap())
    }

    #[napi]
    pub fn all_row_id(&self, query: String) -> Result<String> {
        let res = self
            .store
            .all_row_id(&(serde_json::from_str(&query).unwrap()))
            .unwrap();
        let res: Vec<_> = res
            .into_iter()
            .map(|id| {
                self.store
                    .id_lookup()
                    .unpacked(&id)
                    .expect("row ids from store can be unpacked")
            })
            .collect();
        Ok(serde_json::to_string(&res).unwrap())
    }

    #[napi]
    pub fn one_proj(&self, query: String, select: String) -> Result<String> {
        let res = self
            .store
            .one_proj(
                &(serde_json::from_str(&query).unwrap()),
                &(serde_json::from_str::<Vec<_>>(&select).unwrap()),
            )
            .unwrap();
        Ok(serde_json::to_string(&self.unpack_tuple(res)).unwrap())
    }

    #[napi]
    pub fn exists(&self, query: String) -> Result<bool> {
        Ok(self
            .store
            .exists(&(serde_json::from_str(&query).unwrap()))
            .unwrap())
    }

    #[napi]
    pub fn add(&mut self, table_name: String, values: String) -> Result<String> {
        let res = self
            .store
            .add(
                &ir::Path(table_name),
                serde_json::from_str::<TxTuple>(&values).unwrap(),
            )
            .unwrap();
        Ok(serde_json::to_string(&res).unwrap())
    }
}
