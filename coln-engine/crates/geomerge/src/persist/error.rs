use std::fmt;
use std::io::{self};

#[derive(Debug)]
pub enum PersisError {
    HeaderError(String),
    IOError(io::Error),
    SchemaError(String),
    DataFormatError(String),
    DecodeError(hexane::PackError),
    Other(String),
}

impl fmt::Display for PersisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersisError::HeaderError(msg) => write!(f, "header error: {msg}"),
            PersisError::IOError(err) => write!(f, "io error: {err}"),
            PersisError::SchemaError(msg) => write!(f, "schema error: {msg}"),
            PersisError::DataFormatError(msg) => write!(f, "data format error: {msg}"),
            PersisError::DecodeError(err) => write!(f, "decode error: {err:?}"),
            PersisError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for PersisError {}

impl From<serde_json::Error> for PersisError {
    fn from(value: serde_json::Error) -> Self {
        Self::HeaderError(value.to_string())
    }
}

impl From<io::Error> for PersisError {
    fn from(value: io::Error) -> Self {
        Self::IOError(value)
    }
}

impl From<hexane::PackError> for PersisError {
    fn from(value: hexane::PackError) -> Self {
        Self::DecodeError(value)
    }
}
