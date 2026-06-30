// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module is concerned with violations and how to report them. Many things
//! are still TBD.

use super::deltas::TableDelta;

/// For each query which is checking a constraint, this reports back identified
/// counterexamples.
pub struct Violations {
    /// Contains the counter examples for each unmet constraint. Note that
    /// [`TableRef`] refers to a derived view (defined through a query) rather
    /// than a physical base table here.
    inner: Vec<TableDelta>,
}

impl Violations {
    /// Report no violations.
    pub fn none() -> Self {
        Self {
            inner: Vec::new(), // Does not allocate.
        }
    }
}
