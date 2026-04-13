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
