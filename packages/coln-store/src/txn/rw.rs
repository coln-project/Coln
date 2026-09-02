// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir;

use crate::{
    store::error::StoreError,
    table::{WireRowId, table_handle::WireRowView},
    txn::{TxnLiveRowId, TxnLiveValue},
};

pub trait StoreRead {
    // Return a vec for external world
    // TODO we might want another version of the API which does vectorised processing model for query processing
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<WireRowView>>;

    fn row_by_liveid(&self, table: &ir::Path, live_id: &TxnLiveRowId) -> Option<WireRowView>;

    fn row_by_id(&self, table: &ir::Path, row_id: WireRowId) -> Option<WireRowView> {
        self.row_by_liveid(table, &TxnLiveRowId::from_existing(row_id))
    }
}

pub trait StoreWrite {
    fn add<V: Into<TxnLiveValue>>(
        &mut self,
        table: &ir::Path,
        values: Vec<V>,
    ) -> Result<TxnLiveRowId, StoreError>;
}
