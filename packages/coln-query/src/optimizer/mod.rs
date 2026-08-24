// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{error::OptimizationError, host::Code};

pub mod rewrite;

/// An optimizer does a series of transformations on the relational algebra IR
/// which retain the semantics of the program but intend to improve performance.
/// For instance, it could do:
/// - Collapse a PROJECT(JOIN(R, S, ON, []), ATTRS) into a JOIN(R, S, ON, ATTRS).
/// - Some Projections can be turned into a simple schema operations, e.g.:
///     - column rename
///     - column omission/reordering (beware that operations which require schemas
///       to be equal may fail in a "dirty" state, e.g., set difference and union)
/// - Decide a binary join ordering of an NWayJoin
/// - predicate pushdown
/// - expression simplification
pub trait Optimizer: Clone {
    fn optimize(self, code: Code) -> Result<Code, OptimizationError> {
        // The default impl does nothing and simply returns the IR as is.
        Ok(code)
    }
}

/// A stupid stub implementation which does not optimize anything.
#[derive(Clone, Debug, Default)]
pub struct NoOptimizer {}

impl Optimizer for NoOptimizer {}
