// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::fmt;

use crate::commit::error::CodecError;
use crate::store::error::StoreIntError;
use crate::table::ValidationError;

#[derive(Debug)]
pub enum ReplError {
    NoSchemaLoaded,
    UnknownTable(String),
    UnknownBinding(String),
    DuplicateBinding(String),
    ColumnCountMismatch { expected: usize, got: usize },
    BadValue { column: usize, message: String },
    Io(std::io::Error),
    Json(serde_json::Error),
    Store(StoreIntError),
    Persist(CodecError),
}

/// Parse failure for a single cell inside a `begin batch` block (before column index is known).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchCellParseError {
    UnknownBinding(String),
    InvalidValue(String),
}

impl fmt::Display for ReplError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplError::NoSchemaLoaded => write!(f, "no schema loaded"),
            ReplError::UnknownTable(table) => write!(f, "unknown table: {table}"),
            ReplError::UnknownBinding(name) => write!(f, "unknown binding: {name}"),
            ReplError::DuplicateBinding(name) => write!(f, "duplicate binding: {name}"),
            ReplError::ColumnCountMismatch { expected, got } => {
                write!(f, "column count mismatch: expected {expected}, got {got}")
            }
            ReplError::BadValue { column, message } => write!(f, "column {column}: {message}"),
            ReplError::Io(err) => write!(f, "{err}"),
            ReplError::Json(err) => write!(f, "{err}"),
            ReplError::Store(err) => write!(f, "{err}"),
            ReplError::Persist(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ReplError {}

impl From<std::io::Error> for ReplError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for ReplError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<StoreIntError> for ReplError {
    fn from(value: StoreIntError) -> Self {
        Self::Store(value)
    }
}

impl From<CodecError> for ReplError {
    fn from(value: CodecError) -> Self {
        Self::Persist(value)
    }
}

impl From<ValidationError> for ReplError {
    fn from(value: ValidationError) -> Self {
        Self::Store(value.into())
    }
}

impl From<BatchCellParseError> for ReplError {
    fn from(value: BatchCellParseError) -> Self {
        match value {
            BatchCellParseError::UnknownBinding(name) => Self::UnknownBinding(name),
            BatchCellParseError::InvalidValue(message) => Self::BadValue { column: 0, message },
        }
    }
}
