// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::table::{TableOid, WireRowId, WireValue};

pub const OP_KIND_ADD: u32 = 0;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Op {
    Add {
        row_id: WireRowId,
        table: TableOid,
        values: Vec<WireValue>,
    },
    // Delete {
    //     row_id: RowId,
    //     table: TableOid,
    // }, // TODO Delete + Update
}

impl Op {
    pub fn id(&self) -> WireRowId {
        match self {
            Op::Add { row_id, .. } => *row_id,
        }
    }

    pub fn table(&self) -> TableOid {
        match self {
            Op::Add { table, .. } => *table,
        }
    }
}
