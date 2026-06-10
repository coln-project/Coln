// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An interface for passing deltas of row-oriented data. There is
//! [RowDelta], [TableDelta], [StoreDelta], and [DerivedDataDelta].

use super::schema::TableRef;
use crate::scalar::ScalarTypedValue;

pub type ZWeight = i64;

/// An update of a row of some base table.
/// It either represents an insertion or a deletion of a row from a table,
/// see [`z_weight`](`Self::z_weight`) documentation.
pub struct RowDelta {
    /// A ZWeight value ...
    /// - `== 0` behaves as if there was no insertion happening at all.
    /// - `n if n > 0` represents an insertion. If `n > 1` it is a duplicated
    ///   insertion, that is, the row is inserted n-times.
    /// - `n if n < 0` represents a deletion. If `n < 1` we remove the row
    ///   n-times.
    z_weight: ZWeight,
    /// The row-oriented data.
    row: Vec<ScalarTypedValue>,
}

/// An update to a base table (part of the EDB).
pub struct TableDelta {
    /// A unique identifier to a table.
    table: TableRef,
    /// The row-oriented updates of the table.
    delta: Vec<RowDelta>,
}

/// An update of the EDB, that is, insertions or deletions of base facts.
pub struct StoreDelta {
    pub inner: Vec<TableDelta>,
}

/// An update of the IDB, that is, insertions or deletions of derived facts.
pub struct DerivedDataDelta {
    /// Contains the delta in the IDB after applying a delta in the EDB (the
    /// latter is a [`StoreDelta`]).
    inner: Vec<TableDelta>,
}
