// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::relational::incremental::dbsp::DbspError;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
/// Public error type for any Incremental Datalog error.
pub enum QueryEngineError {
    /// An error that occurs during parsing or static analysis at compile time.
    #[error(transparent)]
    Syntax(#[from] SyntaxError),
    /// An error that occurs during an optimization pass prior to runtime.
    #[error(transparent)]
    Optimization(#[from] OptimizationError),
    /// An error that occurs while lowering the plan into the operator
    /// vocabulary of the chosen backend.
    #[error(transparent)]
    Lowering(#[from] LoweringError),
    /// An error which occurs during runtime of the circuit constructing,
    /// tree-walk interpreter.
    #[error(transparent)]
    Build(#[from] BuildError),
    /// An error that occurs during runtime of the underlying (incremental)
    /// query execution engine (currently only DBSP).
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("{message}")]
/// An error that occurs during parsing or static analysis at compile time.
pub struct SyntaxError {
    // TODO: source location
    pub message: String,
}

impl SyntaxError {
    pub fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("{message}")]
/// An error that occurs during an optimization pass prior to runtime.
pub struct OptimizationError {
    pub message: String,
}

impl OptimizationError {
    pub fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("{message}")]
/// An error that occurs while lowering the plan into the operator vocabulary of
/// the chosen backend, see [`Backend::lower`](crate::relational::Backend::lower).
///
/// Distinct from an [`OptimizationError`] because the two stages fail for
/// different reasons: an optimization is free to decline (and the pipeline is
/// just as correct without it), whereas a lowering that cannot proceed leaves
/// behind a plan the backend has no way to execute.
pub struct LoweringError {
    pub message: String,
}

impl LoweringError {
    pub fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// A lowering re-checks the invariants of the nodes it rewrites, because a plan
/// may have been assembled or rewritten by hand. Such a violation is a
/// [`SyntaxError`] by nature, but it surfaces here.
impl From<SyntaxError> for LoweringError {
    fn from(value: SyntaxError) -> Self {
        Self {
            message: value.message,
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("{message}")]
/// What a [transformation rule](crate::optimizer::rewrite::TransformationRule),
/// or the driver running one, failed with.
///
/// Deliberately *not* tied to a pipeline stage: the same rule machinery serves
/// the optimizer and the backend lowerings, so a rule reports in this shared
/// currency and each stage converts it into the error its own contract is
/// phrased in.
pub struct RewriteError {
    pub message: String,
}

impl RewriteError {
    pub fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// A rule re-checks the invariants of the nodes it rewrites, for the same
/// reason a lowering does.
impl From<SyntaxError> for RewriteError {
    fn from(value: SyntaxError) -> Self {
        Self {
            message: value.message,
        }
    }
}

impl From<RewriteError> for LoweringError {
    fn from(value: RewriteError) -> Self {
        Self {
            message: value.message,
        }
    }
}

impl From<RewriteError> for OptimizationError {
    fn from(value: RewriteError) -> Self {
        Self {
            message: value.message,
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("{message}")]
/// An error which occurs during runtime of the circuit constructing,
/// tree-walk interpreter.
// TODO: Instead of being general, we could introduce:
// - a type error
// - a reference error
// - ... ?
pub struct BuildError {
    pub message: String,
}

impl BuildError {
    pub fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<DbspError> for BuildError {
    fn from(value: DbspError) -> Self {
        Self {
            message: value.to_string(),
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("{message}")]
/// An error that occurs during runtime of the underlying (incremental)
/// query execution engine (currently only DBSP).
pub struct RuntimeError {
    pub message: String,
}

impl RuntimeError {
    pub fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<DbspError> for RuntimeError {
    fn from(value: DbspError) -> Self {
        Self {
            message: value.to_string(),
        }
    }
}
