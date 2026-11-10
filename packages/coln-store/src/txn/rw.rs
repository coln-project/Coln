// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::engine::packed::{PackedRowId, StoreTuple};
use coln_flir_rs::engine::tx::TxTuple;
use coln_flir_rs::{ir, query::WhereClause};

use crate::{store::error::StoreError, txn::TxRowId};

pub trait StoreRead {
    // Return a vec for external world
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<StoreTuple>>;

    fn row_by_id(&self, table: &ir::Path, row_id: &PackedRowId) -> Option<StoreTuple>;

    fn all_proj(&self, query: &WhereClause, select: &[u32]) -> Result<Vec<StoreTuple>, StoreError>;

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<PackedRowId>, StoreError>;

    fn one_proj(&self, query: &WhereClause, select: &[u32]) -> Result<StoreTuple, StoreError> {
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
    fn add(&mut self, table: &ir::Path, values: impl Into<TxTuple>) -> Result<TxRowId, StoreError>;
}
