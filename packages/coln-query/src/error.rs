// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::relational::incremental::dbsp::DbspError;
use std::fmt;
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

#[derive(Debug, Clone, PartialEq, Eq)]
/// An error that occurs during parsing or static analysis at compile time.
///
/// Carries the [`Frame`]s it was raised within, so that the function raising
/// it only has to say what it knows itself: each caller that knows more adds
/// its frame [`within`](Self::within) on the way up.
///
/// `{}` renders the frames as a one-line prefix, outermost first. `{:#}` puts
/// the message first and each frame on a line of its own, in full.
pub struct SyntaxError {
    // TODO: source location
    message: String,
    /// Innermost first, as frames are added on the way up.
    context: Vec<Frame>,
}

impl SyntaxError {
    pub fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
            context: Vec::new(),
        }
    }

    /// The error as raised, without any of its frames.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The frames the error was raised within, outermost first.
    pub fn context(&self) -> impl Iterator<Item = &Frame> {
        self.context.iter().rev()
    }

    /// Adds `frame` around the frames the error already carries.
    pub fn within(mut self, frame: Frame) -> Self {
        self.context.push(frame);
        self
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match f.alternate() {
            true => {
                // Innermost first, like a backtrace reads.
                f.write_str(&self.message)?;
                self.context
                    .iter()
                    .try_for_each(|frame| write!(f, "\n  {frame:#}"))
            }
            false => {
                self.context()
                    .try_for_each(|frame| write!(f, "{frame}: "))?;
                f.write_str(&self.message)
            }
        }
    }
}

impl std::error::Error for SyntaxError {}

/// Something a [`SyntaxError`] was raised within.
///
/// Names are rendered to strings when the frame is made: the frontend's
/// identifiers are generic, and the error is not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Predicate {
        name: String,
    },
    Rule {
        name: String,
        /// The rule in Datalog notation, shown by `{:#}` only.
        text: String,
    },
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Frame::Predicate { name } => write!(f, "in predicate '{name}'"),
            Frame::Rule { name, text } => match f.alternate() {
                true => write!(f, "in rule '{name}': {text}"),
                false => write!(f, "in rule '{name}'"),
            },
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
        // Rendered, so that the frames survive the conversion.
        Self {
            message: value.to_string(),
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
        // Rendered, so that the frames survive the conversion.
        Self {
            message: value.to_string(),
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
