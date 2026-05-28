use std::fmt;
use std::io::{self};

use crate::commit::chunk::ChunkType;

// TODO PersistError needs a bit more variant to reduce the number of Other

#[derive(Debug)]
pub enum CodecError {
    HeaderError(String),
    IOError(io::Error),
    SchemaError(String),
    DataFormatError(String),
    DecodeError(hexane::PackError),
    ChunkMismatch { expected: ChunkType, got: ChunkType },
    Other(String),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::HeaderError(msg) => write!(f, "header error: {msg}"),
            CodecError::IOError(err) => write!(f, "io error: {err}"),
            CodecError::SchemaError(msg) => write!(f, "schema error: {msg}"),
            CodecError::DataFormatError(msg) => write!(f, "data format error: {msg}"),
            CodecError::DecodeError(err) => write!(f, "decode error: {err:?}"),
            CodecError::ChunkMismatch { expected, got } => write!(
                f,
                "chunk type mismatch, expecting: {expected:?}, got: {got:?}"
            ),
            CodecError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for CodecError {}

impl From<serde_json::Error> for CodecError {
    fn from(value: serde_json::Error) -> Self {
        Self::HeaderError(value.to_string())
    }
}

impl From<io::Error> for CodecError {
    fn from(value: io::Error) -> Self {
        Self::IOError(value)
    }
}

impl From<hexane::PackError> for CodecError {
    fn from(value: hexane::PackError) -> Self {
        Self::DecodeError(value)
    }
}
