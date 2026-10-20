// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::store::ColnDef;
use crate::store::auto::AutoStore;
use crate::txn::id::TxnWireTuple;
use crate::txn::rw::StoreRead;
use crate::txn::rw::StoreWrite;
use coln_flir_rs::ir;

pub struct WireStore {
    store: AutoStore,
}

impl WireStore {
    pub fn from_ir(
        ir_json: String,
        coln_source: String,
        realm_name: String,
    ) -> Result<WireStore, String> {
        let ir = serde_json::from_str(&ir_json).expect("parse flir");
        let coln_def = ColnDef::new(coln_source, realm_name);
        Ok(WireStore {
            store: AutoStore::try_from_ir(ir, coln_def).expect("create store successful"),
        })
    }

    pub fn start_transaction(&mut self) {
        self.store.transaction();
    }

    pub fn end_transaction(&mut self) -> String {
        serde_json::to_string(&self.store.commit().unwrap()).unwrap()
    }

    pub fn abort_transaction(&mut self) {
        self.store.abort();
    }

    pub fn all_proj(&self, query: String, select: String) -> Result<String, String> {
        let res = self
            .store
            .all_proj(
                &(serde_json::from_str(&query).unwrap()),
                &(serde_json::from_str::<Vec<_>>(&select).unwrap()),
            )
            .unwrap();
        Ok(serde_json::to_string(&res).unwrap())
    }

    pub fn all_row_id(&self, query: String) -> Result<String, String> {
        let res = self
            .store
            .all_row_id(&(serde_json::from_str(&query).unwrap()))
            .unwrap();
        Ok(serde_json::to_string(&res).unwrap())
    }

    pub fn one_proj(&self, query: String, select: String) -> Result<String, String> {
        let res = self
            .store
            .one_proj(
                &(serde_json::from_str(&query).unwrap()),
                &(serde_json::from_str::<Vec<_>>(&select).unwrap()),
            )
            .unwrap();
        Ok(serde_json::to_string(&res).unwrap())
    }

    pub fn exists(&self, query: String) -> Result<bool, String> {
        Ok(self
            .store
            .exists(&(serde_json::from_str(&query).unwrap()))
            .unwrap())
    }

    pub fn add(&mut self, table_name: String, values: String) -> Result<String, String> {
        let res = self
            .store
            .add(
                &ir::Path(table_name),
                serde_json::from_str::<TxnWireTuple>(&values).unwrap(),
            )
            .unwrap();
        Ok(serde_json::to_string(&res).unwrap())
    }
}
