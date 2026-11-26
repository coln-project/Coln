// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_query::api::violations::ViolationsSet;

#[derive(Debug, thiserror::Error)]
pub enum BouncerError {
    #[error(transparent)]
    StoreError(#[from] coln_store::store::error::StoreError),
    #[error(transparent)]
    QueryError(#[from] coln_query::api::error::ColnQueryError),
    #[error(transparent)]
    UserError(#[from] UserError),
    #[error(transparent)]
    Law(#[from] LawViolation),
}

#[derive(Debug, thiserror::Error)]
pub enum LawViolation {
    #[error("Violation of enforced law {0}")]
    Hard(ViolationsSet),
    // @Vincent why an enum? Soft violations (violations of monitored laws)
    // won't be communicated via errors, right? We could inline the
    // `ViolationsSet` into the `BouncerError::Law` variant.
}

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("results have multiple matching tuple")]
    MultipleMatchingTuple,
    #[error("results have zero matching tuple")]
    ZeroMatchingTuple,
}
