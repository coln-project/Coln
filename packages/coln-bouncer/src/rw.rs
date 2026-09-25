use coln_flir_rs::{
    WireRowId, WireRowView, WireTuple,
    engine::txn_val::{TxnWireRowId, TxnWireTuple},
    ir,
    query::WhereClause,
};

use crate::error::{BouncerError, UserError};

pub trait DbRead {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<WireRowView>>;

    fn row_by_id(&self, table: &ir::Path, row_id: &WireRowId) -> Option<WireRowView>;

    // TODO @Leo & Jan, I am not sure about where the WhereClause should be. My feeling is that it should be
    // handled by the query engine (a filter operation), and the store will just do the basic table scan,
    // and the query engine can do the filtering, projection, etc
    fn all_proj(&self, query: &WhereClause, select: &[u32])
    -> Result<Vec<WireTuple>, BouncerError>;

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<WireRowId>, BouncerError>;

    fn one_proj(&self, query: &WhereClause, select: &[u32]) -> Result<WireTuple, BouncerError> {
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

    fn commit(&mut self);

    fn abort(&mut self);

    // TODO @Leo we need to define a common data structure here for representing
    // a tuple and a rowid as used in the txn, and then convert it into what is
    // expected by coln-store and coln-query
    fn add(
        &mut self,
        table: &ir::Path,
        values: impl Into<TxnWireTuple>,
    ) -> Result<TxnWireRowId, BouncerError>;
}
