use std::fmt;
use std::io::{self};

use crate::commit::chunk::ChunkType;

// TODO PersistError needs a bit more variant to reduce the number of Other

#[derive(Debug)]
pub enum PersistError {
    HeaderError(String),
    IOError(io::Error),
    SchemaError(String),
    DataFormatError(String),
    DecodeError(hexane::PackError),
    ChunkMismatch { expected: ChunkType, got: ChunkType },
    Other(String),
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistError::HeaderError(msg) => write!(f, "header error: {msg}"),
            PersistError::IOError(err) => write!(f, "io error: {err}"),
            PersistError::SchemaError(msg) => write!(f, "schema error: {msg}"),
            PersistError::DataFormatError(msg) => write!(f, "data format error: {msg}"),
            PersistError::DecodeError(err) => write!(f, "decode error: {err:?}"),
            PersistError::ChunkMismatch { expected, got } => write!(
                f,
                "chunk type mismatch, expecting: {expected:?}, got: {got:?}"
            ),
            PersistError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for PersistError {}

impl From<serde_json::Error> for PersistError {
    fn from(value: serde_json::Error) -> Self {
        Self::HeaderError(value.to_string())
    }
}

impl From<io::Error> for PersistError {
    fn from(value: io::Error) -> Self {
        Self::IOError(value)
    }
}

impl From<hexane::PackError> for PersistError {
    fn from(value: hexane::PackError) -> Self {
        Self::DecodeError(value)
    }
}
