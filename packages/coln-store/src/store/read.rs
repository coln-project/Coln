// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir;

use crate::{
    table::{WireRowId, table_handle::WireRowView},
    txn::TxnLiveRowId,
};

pub trait StoreRead {
    fn scan_table(&self, table: &ir::Path) -> Option<impl Iterator<Item = WireRowView> + '_>;

    fn row_by_liveid(&self, table: &ir::Path, handle: &TxnLiveRowId) -> Option<WireRowView>;

    fn row_by_id(&self, table: &ir::Path, row_id: WireRowId) -> Option<WireRowView> {
        self.row_by_liveid(table, &TxnLiveRowId::from_existing(row_id))
    }
}
