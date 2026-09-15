// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

#[cfg(not(target_arch = "wasm32"))]
use coln_query::api::error::ColnQueryError;
#[cfg(not(target_arch = "wasm32"))]
use coln_query::api::violations::ViolationsSet;

use crate::commit::error::CodecError;
use crate::commit::graph::CommitGraphError;
use crate::commit::hash::CommitHash;
use crate::table::ValidationError;

/// Store integrity error
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[cfg(not(target_arch = "wasm32"))]
    #[error(transparent)]
    Rule(#[from] RuleViolation),
    #[error(transparent)]
    Encode(#[from] CodecError),
    #[error(transparent)]
    Commit(#[from] CommitApplyError),
    #[error(transparent)]
    CommitGraph(#[from] CommitGraphError),
    #[cfg(not(target_arch = "wasm32"))]
    #[error(transparent)]
    CQError(#[from] ColnQueryError),
    #[error(transparent)]
    QError(#[from] QueryError),
}

// TODO remove this to query engine
#[derive(Debug, thiserror::Error)]
pub enum QueryError {
    #[error("results have multiple matching tuple")]
    MultipleMatchingTuple,
    #[error("results have zero matching tuple")]
    ZeroMatchingTuple,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, thiserror::Error)]
pub enum RuleViolation {
    #[error("A hardviolation {0}")]
    HardViolation(ViolationsSet),
}

#[derive(Debug, thiserror::Error)]
pub enum CommitApplyError {
    #[error("A commit {0} with no dependency")]
    DanglingCommit(CommitHash),
    #[error("An existing commit has conflict payload")]
    ConflictPayload(CommitHash),
    #[error("Root commit {0} cannot be applied")]
    RootCommit(CommitHash),
}
