// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::api::violations::ViolationsSet;
pub use crate::error::QueryEngineError;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
/// Public error type for the coln frontend.
pub enum ColnQueryError {
    /// A error in the underlying query engine(s).
    #[error(transparent)]
    Engine(#[from] QueryEngineError),
    #[error(transparent)]
    UnsafeApply(#[from] UnsafeApplyError),
    /// Another transaction holds the engine, i.e., a
    /// [`Pending`](super::transaction::Pending) transaction has neither been
    /// committed nor aborted yet. Not a bug of the engine, but of the caller.
    #[error("a transaction is already open on this query engine")]
    TxAlreadyOpen,
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("Violations occurred during unsafe application of facts: {violations}")]
/// A hard constraint has been violated but this should not have happened.
/// This usually indicates a bug.
pub struct UnsafeApplyError {
    pub violations: ViolationsSet,
}
