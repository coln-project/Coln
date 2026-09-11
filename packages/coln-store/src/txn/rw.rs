// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::{self, Path};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    store::error::StoreError,
    table::{WireRowId, WireValue, cell::WireTuple, table_handle::WireRowView},
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

    fn all(&self, query: &WhereClause, select: &[u32]) -> Option<Vec<WireTuple>>;

    fn one(&self, query: &WhereClause, select: &[u32]) -> Option<WireTuple> {
        self.all(query, select)?.pop()
    }

    fn exists(&self, query: &WhereClause) -> bool {
        self.all(query, &[0]).is_some_and(|v| !v.is_empty())
    }
}

pub trait StoreWrite {
    fn add<V: Into<TxnLiveValue>>(
        &mut self,
        table: &ir::Path,
        values: Vec<V>,
    ) -> Result<TxnLiveRowId, StoreError>;
}
