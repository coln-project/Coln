use thiserror::Error;

use crate::{api::violations::ViolationsSet, error::QueryEngineError};

#[derive(Error, Debug, Clone, PartialEq, Eq)]
/// Public error type for the coln frontend.
pub enum ColnQueryError {
    /// A error in the underlying query engine(s).
    #[error(transparent)]
    Engine(#[from] QueryEngineError),
    #[error(transparent)]
    UnsafeApply(#[from] UnsafeApplyError),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("Violations occurred during unsafe application of facts: {violations}")]
/// A hard constraint has been violated but this should not have happened.
/// This usually indicates a bug.
pub struct UnsafeApplyError {
    pub violations: ViolationsSet,
}
