// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir;

use crate::{
    table::{RowId, RowView},
    txn::RowHandle,
};

pub trait StoreRead {
    fn scan_table(&self, table: &ir::Path) -> Option<impl Iterator<Item = RowView> + '_>;

    fn row_by_handle(&self, table: &ir::Path, handle: &RowHandle) -> Option<RowView>;

    fn row_by_id(&self, table: &ir::Path, row_id: RowId) -> Option<RowView> {
        self.row_by_handle(table, &RowHandle::from_existing(row_id))
    }
}
