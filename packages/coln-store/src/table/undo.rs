use crate::table::{PackedCell, PackedRowId};

// Undo operations for each variant in the `Op` enum.
#[derive(Debug)]
pub(super) enum UndoOp {
    UndoAdd {
        row_id: PackedRowId,
    }, // undo add means delete the row_id
    UndoDelete {
        row_id: PackedRowId,
        values: Vec<PackedCell>,
    }, // undo delete means add back the row
}
