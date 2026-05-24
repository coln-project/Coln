use std::error::Error;
use std::fmt;

use crate::commit::error::PersistError;
use crate::solver::compile::CompileError;
use crate::solver::validate::LawViolation;
use crate::table::ValidationError;

/// Store integrity error
#[derive(Debug)]
pub enum StoreIntError {
    Validation(ValidationError),
    Law(Box<LawViolation>),
    Compile(CompileError),
    Encode(PersistError),
}

impl fmt::Display for StoreIntError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreIntError::Validation(err) => write!(f, "{err}"),
            StoreIntError::Law(err) => write!(f, "{err}"),
            StoreIntError::Compile(err) => write!(f, "{err:?}"),
            StoreIntError::Encode(err) => write!(f, "{err:?}"),
        }
    }
}

impl Error for StoreIntError {}

impl From<ValidationError> for StoreIntError {
    fn from(value: ValidationError) -> Self {
        Self::Validation(value)
    }
}

impl From<LawViolation> for StoreIntError {
    fn from(value: LawViolation) -> Self {
        Self::Law(Box::new(value))
    }
}

impl From<Box<LawViolation>> for StoreIntError {
    fn from(value: Box<LawViolation>) -> Self {
        Self::Law(value)
    }
}

impl From<CompileError> for StoreIntError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

impl From<PersistError> for Box<StoreIntError> {
    fn from(value: PersistError) -> Self {
        Box::new(StoreIntError::Encode(value))
    }
}

impl From<ValidationError> for Box<StoreIntError> {
    fn from(value: ValidationError) -> Self {
        Box::new(StoreIntError::from(value))
    }
}

impl From<CompileError> for Box<StoreIntError> {
    fn from(value: CompileError) -> Self {
        Box::new(StoreIntError::from(value))
    }
}

impl From<LawViolation> for Box<StoreIntError> {
    fn from(value: LawViolation) -> Self {
        Box::new(StoreIntError::from(value))
    }
}

impl From<Box<LawViolation>> for Box<StoreIntError> {
    fn from(value: Box<LawViolation>) -> Self {
        Box::new(StoreIntError::from(value))
    }
}
