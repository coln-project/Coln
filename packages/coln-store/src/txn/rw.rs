// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    WireRowId, WireRowView, WireTuple, engine::txn_val::TxnWireTuple, ir, query::WhereClause,
};

use crate::{store::error::StoreError, txn::TxnWireRowId};

pub trait StoreRead {
    // Return a vec for external world
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<WireRowView>>;

    fn row_by_id(&self, table: &ir::Path, row_id: &WireRowId) -> Option<WireRowView>;

    fn all_proj(&self, query: &WhereClause, select: &[u32]) -> Result<Vec<WireTuple>, StoreError>;

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<WireRowId>, StoreError>;

    fn one_proj(&self, query: &WhereClause, select: &[u32]) -> Result<WireTuple, StoreError> {
        let mut all_tuples = self.all_proj(query, select)?;
        if all_tuples.len() == 1 {
            Ok(all_tuples.pop().unwrap())
        } else if all_tuples.is_empty() {
            todo!("implement this in coln bouncer")
        } else {
            todo!("implement this in coln bouncer")
        }
    }

    fn exists(&self, query: &WhereClause) -> Result<bool, StoreError> {
        self.all_row_id(query).map(|v| !v.is_empty())
    }
}

pub trait StoreWrite {
    fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxnWireTuple>,
    ) -> Result<TxnWireRowId, StoreError>;
}
