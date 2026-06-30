// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::dbsp::DbspError;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
/// Public error type for any Incremental Datalog error.
pub enum IncLogError {
    #[error(transparent)]
    Syntax(#[from] SyntaxError),
    #[error(transparent)]
    Optimization(#[from] OptimizationError),
    #[error(transparent)]
    Engine(#[from] EngineError),
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
// TODO: Instead of being generic, we could introduce:
// - a type error
// - a reference error
pub struct EngineError {
    message: String,
}

impl EngineError {
    pub fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("{message}")]
/// An error that occurs during runtime of the underlying (incremental)
/// query execution engine (currently only DBSP).
pub struct RuntimeError {
    message: String,
}

impl From<DbspError> for IncLogError {
    fn from(value: DbspError) -> Self {
        IncLogError::Runtime(RuntimeError {
            message: value.to_string(),
        })
    }
}
