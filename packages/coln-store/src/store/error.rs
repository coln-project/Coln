// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::hash::CommitHash;

use crate::commit::error::CodecError;
use crate::commit::graph::CommitGraphError;
use crate::table::ValidationError;

/// Store integrity error
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Encode(#[from] CodecError),
    #[error(transparent)]
    Commit(#[from] CommitApplyError),
    #[error(transparent)]
    CommitGraph(#[from] CommitGraphError),
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
