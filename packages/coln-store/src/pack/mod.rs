// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod id_packer;
use coln_flir_rs::engine::packed::{PackedRowId, PackedTuple};
pub(crate) use id_packer::{IdPacker, IdPackerSnapshot};

/// Packed representation of an operation staged for a table.
#[derive(Debug, Clone)]
pub(crate) enum PackedOp {
    Add {
        row_id: PackedRowId,
        values: PackedTuple,
    },
    Delete {
        row_id: PackedRowId,
    },
}
