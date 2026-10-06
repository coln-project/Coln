use coln_flir_rs::{
    engine::tx::{TxRowId, TxTuple},
    ir,
    public::{PublicRowId, PublicTuple},
    query::WhereClause,
};
use coln_query::api::violations::{Violations, ViolationsDelta};

use crate::error::{BouncerError, UserError};

pub trait DbRead {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<PublicTuple>>;

    fn row_by_id(&self, table: &ir::Path, row_id: &PublicRowId) -> Option<PublicTuple>;

    // TODO @Leo & Jan, I am not sure about where the WhereClause should be. My feeling is that it should be
    // handled by the query engine (a filter operation), and the store will just do the basic table scan,
    // and the query engine can do the filtering, projection, etc
    fn all_proj(
        &self,
        query: &WhereClause,
        select: &[u32],
    ) -> Result<Vec<PublicTuple>, BouncerError>;

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<PublicRowId>, BouncerError>;

    fn one_proj(&self, query: &WhereClause, select: &[u32]) -> Result<PublicTuple, BouncerError> {
        let mut all_tuples = self.all_proj(query, select)?;
        if all_tuples.len() == 1 {
            Ok(all_tuples.pop().unwrap())
        } else if all_tuples.is_empty() {
            Err(UserError::ZeroMatchingTuple.into())
        } else {
            Err(UserError::MultipleMatchingTuple.into())
        }
    }

    fn exists(&self, query: &WhereClause) -> Result<bool, BouncerError> {
        self.all_row_id(query).map(|v| !v.is_empty())
    }
}

pub trait DbWrite {
    fn transaction(&mut self);

    fn commit(&mut self) -> Result<ViolationsDelta, BouncerError>;

    fn abort(&mut self);

    // TODO @Leo we need to define a common data structure here for representing
    // a tuple and a rowid as used in the txn, and then convert it into what is
    // expected by coln-store and coln-query
    fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxTuple>,
    ) -> Result<TxRowId, BouncerError>;
}
