// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::table::RowId;

pub(crate) mod rowing;

#[derive(Debug)]
pub(crate) enum ObservedOutcome {
    Inserted(RowId),
    KeptOld(RowId),
    Swap { old: RowId, new: RowId },
}
