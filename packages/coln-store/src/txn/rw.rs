// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir;

use crate::{
    store::error::StoreError,
    table::{RowId, RowView},
    txn::{RowHandle, TxnValue},
};

pub trait StoreRead {
    // Return a vec for external world
    // TODO we might want another version of the API which does vectorised processing model for query processing
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<RowView>>;

    fn row_by_handle(&self, table: &ir::Path, handle: &RowHandle) -> Option<RowView>;

    fn row_by_id(&self, table: &ir::Path, row_id: RowId) -> Option<RowView> {
        self.row_by_handle(table, &RowHandle::from_existing(row_id))
    }
}

pub trait StoreWrite {
    // TODO this API is a bit awkward to use, clients have to call .into() all
    // the time on their values
    fn add(&mut self, table: &ir::Path, values: Vec<TxnValue>) -> Result<RowHandle, StoreError>;
}
