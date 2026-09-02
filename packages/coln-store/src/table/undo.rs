// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::table::{PackedValue, PackedRowId};

// Undo operations for each variant in the `Op` enum.
#[derive(Debug)]
pub(super) enum UndoOp {
    UndoAdd {
        row_id: PackedRowId,
    }, // undo add means delete the row_id
    UndoDelete {
        row_id: PackedRowId,
        values: Vec<PackedValue>,
    }, // undo delete means add back the row
}
