use crate::table::RowId;

mod hashdag;
mod rowing;

#[derive(Debug)]
pub(crate) enum ObservedOutcome {
    Inserted(RowId),
    KeptOld(RowId),
    Swap { old: RowId, new: RowId },
}

