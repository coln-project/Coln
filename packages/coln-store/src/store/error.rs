use std::error::Error;
use std::fmt;

use crate::commit::error::CodecError;
use crate::solver::compile::CompileError;
use crate::solver::validate::RuleViolation;
use crate::table::ValidationError;

/// Store integrity error
#[derive(Debug)]
pub enum StoreIntError {
    Validation(ValidationError),
    Law(Box<RuleViolation>),
    Compile(CompileError),
    Encode(CodecError),
    Commit(CommitApplyError),
}

#[derive(Debug)]
pub enum CommitApplyError {
    MissingDep,
    DisconnectedCommit,
    // A commit that should definitely exist but is missing
    MissingCommit,
    ConflictPayload,
    RootCommit,
}

impl fmt::Display for CommitApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommitApplyError::MissingDep => write!(f, "missing commit dependency"),
            CommitApplyError::DisconnectedCommit => write!(f, "disconnected commit"),
            CommitApplyError::MissingCommit => write!(f, "missing commit"),
            CommitApplyError::ConflictPayload => {
                write!(f, "An existing commit has conflict payload")
            }
            CommitApplyError::RootCommit => write!(f, "Root commit cannot be applied"),
        }
    }
}

impl Error for CommitApplyError {}

impl fmt::Display for StoreIntError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreIntError::Validation(err) => write!(f, "{err}"),
            StoreIntError::Law(err) => write!(f, "{err}"),
            StoreIntError::Compile(err) => write!(f, "{err:?}"),
            StoreIntError::Encode(err) => write!(f, "{err:?}"),
            StoreIntError::Commit(err) => write!(f, "{err}"),
        }
    }
}

impl Error for StoreIntError {}

impl From<ValidationError> for StoreIntError {
    fn from(value: ValidationError) -> Self {
        Self::Validation(value)
    }
}

impl From<RuleViolation> for StoreIntError {
    fn from(value: RuleViolation) -> Self {
        Self::Law(Box::new(value))
    }
}

impl From<Box<RuleViolation>> for StoreIntError {
    fn from(value: Box<RuleViolation>) -> Self {
        Self::Law(value)
    }
}

impl From<CompileError> for StoreIntError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}

impl From<CommitApplyError> for StoreIntError {
    fn from(value: CommitApplyError) -> Self {
        Self::Commit(value)
    }
}

impl From<CodecError> for Box<StoreIntError> {
    fn from(value: CodecError) -> Self {
        Box::new(StoreIntError::Encode(value))
    }
}

impl From<CommitApplyError> for Box<StoreIntError> {
    fn from(value: CommitApplyError) -> Self {
        Box::new(StoreIntError::from(value))
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

impl From<RuleViolation> for Box<StoreIntError> {
    fn from(value: RuleViolation) -> Self {
        Box::new(StoreIntError::from(value))
    }
}

impl From<Box<RuleViolation>> for Box<StoreIntError> {
    fn from(value: Box<RuleViolation>) -> Self {
        Box::new(StoreIntError::from(value))
    }
}
