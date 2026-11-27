// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::public::PublicViolations;

#[derive(Debug, thiserror::Error)]
pub enum BouncerError {
    #[error(transparent)]
    StoreError(#[from] coln_store::store::error::StoreError),
    #[error(transparent)]
    QueryError(#[from] coln_query::api::error::ColnQueryError),
    #[error(transparent)]
    UserError(#[from] UserError),
    #[error("Violation of enforced laws:\n{0}")]
    Law(PublicViolations),
}

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("results have multiple matching tuple")]
    MultipleMatchingTuple,
    #[error("results have zero matching tuple")]
    ZeroMatchingTuple,
}
