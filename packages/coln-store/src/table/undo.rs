use crate::table::PackedRowId;

// Undo operations for each variant in the `Op` enum.
#[derive(Debug)]
pub(super) enum UndoOp {
    UndoAdd { row_id: PackedRowId },
}
