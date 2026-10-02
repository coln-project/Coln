// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::public::{PublicRowId, PublicScalarValue};

use crate::table::TableOid;

pub const OP_KIND_ADD: u32 = 0;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Op {
    Add {
        row_id: PublicRowId,
        table: TableOid,
        values: Vec<PublicScalarValue>,
    },
    // Delete {
    //     row_id: RowId,
    //     table: TableOid,
    // }, // TODO Delete + Update
}

impl Op {
    pub fn id(&self) -> PublicRowId {
        match self {
            Op::Add { row_id, .. } => row_id.clone(),
        }
    }

    pub fn table(&self) -> TableOid {
        match self {
            Op::Add { table, .. } => *table,
        }
    }
}
