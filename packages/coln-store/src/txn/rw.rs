// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::{self, Path};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    store::error::{QueryError, StoreError},
    table::{WireRowId, WireValue, cell::WireTuple, handle::WireRowView},
    txn::{TxnLiveRowId, TxnLiveValue},
};

#[derive(Debug, Type, Serialize, Deserialize)]
pub struct WhereClause {
    pub table_name: Path,
    pub row_id: Option<WireRowId>,
    pub values: Vec<WireValue>, // A prefix of column values
}

pub trait StoreRead {
    // Return a vec for external world
    // TODO we might want another version of the API which does vectorised processing model for query processing
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<WireRowView>>;

    fn row_by_liveid(&self, table: &ir::Path, live_id: &TxnLiveRowId) -> Option<WireRowView>;

    fn row_by_id(&self, table: &ir::Path, row_id: WireRowId) -> Option<WireRowView> {
        self.row_by_liveid(table, &TxnLiveRowId::from_existing(row_id))
    }

    fn all_proj(&self, query: &WhereClause, select: &[u32]) -> Result<Vec<WireTuple>, StoreError>;

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<WireRowId>, StoreError>;

    fn one_proj(&self, query: &WhereClause, select: &[u32]) -> Result<WireTuple, StoreError> {
        let mut all_tuples = self.all_proj(query, select)?;
        if all_tuples.len() == 1 {
            Ok(all_tuples.pop().unwrap())
        } else if all_tuples.is_empty() {
            Err(QueryError::ZeroMatchingTuple.into())
        } else {
            Err(QueryError::MultipleMatchingTuple.into())
        }
    }

    fn exists(&self, query: &WhereClause) -> Result<bool, StoreError> {
        self.all_row_id(query).map(|v| !v.is_empty())
    }
}

pub trait StoreWrite {
    fn add<V: Into<TxnLiveValue>>(
        &mut self,
        table: &ir::Path,
        values: Vec<V>,
    ) -> Result<TxnLiveRowId, StoreError>;
}
