// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::commit::error::CodecError;
use crate::solver::compile::CompileError;
use crate::solver::validate::RuleViolation;
use crate::table::ValidationError;

/// Store integrity error
#[derive(Debug, thiserror::Error)]
pub enum StoreIntError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Rule(#[from] Box<RuleViolation>),
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Encode(#[from] CodecError),
    #[error(transparent)]
    Commit(#[from] CommitApplyError),
}

#[derive(Debug, thiserror::Error)]
pub enum CommitApplyError {
    #[error("missing commit dependency")]
    MissingDep,
    #[error("disconnected commit")]
    DisconnectedCommit,
    // A commit that should definitely exist but is missing
    #[error("missing commit")]
    MissingCommit,
    #[error("An existing commit has conflict payload")]
    ConflictPayload,
    #[error("Root commit cannot be applied")]
    RootCommit,
}
