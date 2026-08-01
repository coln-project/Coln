// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    ir,
    table::{CellValue, RowId},
};

pub const OP_KIND_ADD: u32 = 0;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Op {
    Add {
        row_id: RowId,
        table: ir::Path, // using path so it's stable across replicas
        values: Vec<CellValue>,
    },
    // TODO Delete + Update
}

impl Op {
    pub fn id(&self) -> RowId {
        match self {
            Op::Add { row_id, .. } => *row_id,
        }
    }

    pub fn table(&self) -> &ir::Path {
        match self {
            Op::Add { table, .. } => table,
        }
    }
}
