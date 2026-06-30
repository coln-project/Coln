use crate::table::RowId;

mod hashdag;
pub(crate) mod rowing;

#[derive(Debug)]
pub(crate) enum ObservedOutcome {
    Inserted(RowId),
    KeptOld(RowId),
    Swap { old: RowId, new: RowId },
}
