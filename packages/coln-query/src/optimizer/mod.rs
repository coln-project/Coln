// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{error::OptimizationError, stmt::Code};

/// An optimizer does a series of transformations on the relational algebra IR
/// which retain the semantics of the program but intend to improve performance.
/// For instance, it could do:
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
