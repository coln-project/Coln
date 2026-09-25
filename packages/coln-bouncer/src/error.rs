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
    Rule(#[from] RuleViolation),
}

#[derive(Debug, thiserror::Error)]
pub enum RuleViolation {
    #[error("A hardviolation {0}")]
    HardViolation(ViolationsSet),
}

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("results have multiple matching tuple")]
    MultipleMatchingTuple,
    #[error("results have zero matching tuple")]
    ZeroMatchingTuple,
}
