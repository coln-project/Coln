use std::fmt;

use crate::store::StoreIntError;

#[derive(Debug)]
pub enum ReplError {
    NoSchemaLoaded,
    UnknownTable(String),
    ColumnCountMismatch { expected: usize, got: usize },
    BadValue { column: usize, message: String },
    Io(std::io::Error),
    Json(serde_json::Error),
    Store(StoreIntError),
}

impl fmt::Display for ReplError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplError::NoSchemaLoaded => write!(f, "no schema loaded"),
            ReplError::UnknownTable(table) => write!(f, "unknown table: {table}"),
            ReplError::ColumnCountMismatch { expected, got } => {
                write!(f, "column count mismatch: expected {expected}, got {got}")
            }
            ReplError::BadValue { column, message } => write!(f, "column {column}: {message}"),
            ReplError::Io(err) => write!(f, "{err}"),
            ReplError::Json(err) => write!(f, "{err}"),
            ReplError::Store(err) => write!(f, "{err}"),
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
