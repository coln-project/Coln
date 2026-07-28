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
