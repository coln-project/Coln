// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! What the batch backend expects from the store: a handle per table, and
//! from each handle sorted tables the join reads directly.
//!
//! @Vincent: this is the handle as the batch backend uses it. The traits
//! live on our side because coln-store depends on coln-query, so coln-query
//! cannot name your `TableHandle`. Your store and its handles would
//! implement them, for base tables and derived views alike. Leo's sketch of
//! `ColnQuery::adhoc_query` names the source `&dyn RelationSource`.
//!
//! Everything an implementation needs is re-exported in
//! `coln_query::api::batch`: these traits, [`SortedTable`] with [`ColId`],
//! [`Key`] and [`Dictionary`] for the encoding, `SourceId` for table names,
//! and `check_contract` to test the result. `tests/batch_contract.rs`
//! implements a store from outside the crate with just that.

pub use coln_batch::table::{ColId, SortedTable};
pub use coln_batch::types::{Dictionary, Key};

use crate::relational::expr::SourceId;

/// The store, as one batch query sees it.
pub trait RelationSource {
    /// The handle for `table`, or `None` if the store has no such table.
    fn table(&self, table: &SourceId) -> Option<Box<dyn TableHandle + '_>>;

    /// The dictionary every handle encodes strings with.
    ///
    /// @Vincent: one dictionary for all tables, so equal strings get equal
    /// keys and join across tables. It only grows: a code, once given,
    /// keeps its string. We copy it per query, add the plan's own strings,
    /// and decode the results with that copy.
    fn dictionary(&self) -> &Dictionary;
}

/// One table of the store.
pub trait TableHandle {
    /// The number of columns, in the query engine's view (see
    /// [`TableHandle::sorted`]). We check it against the plan before
    /// asking for any sorted table.
    fn arity(&self) -> usize;

    /// The table's rows sorted by `order`, as a [`SortedTable`] the join
    /// reads directly. `order` is a permutation of the table's columns.
    ///
    /// @Vincent, what we expect from the table:
    /// - Columns in the query engine's view, as the plan's schema for the
    ///   table has them: for a base table the row id comes first as two
    ///   columns (commit index, counter), and a row-id column takes two as
    ///   well. That is the layout your `From<PackedRowView> for TupleValue`
    ///   in coln-store's `pack/mod.rs` already produces.
    /// - Every cell is a [`Key`] as in `coln_batch::types`: a `Uint` as it
    ///   is, an `Iint` with its sign bit flipped, a `Bool` as 0 or 1, a
    ///   `Char` as its code point, a `String` as its code in
    ///   [`RelationSource::dictionary`]. `coln_batch::types::Value::to_key`
    ///   is this encoding.
    /// - Rows sorted lexicographically by `order`, each row once. Every
    ///   permutation must work: the join picks the order per query, and
    ///   one query can ask for the same table in two orders.
    /// - Nothing changes while the handle lives. Borrowing the store for
    ///   the handle's lifetime gives that for free.
    ///
    /// `coln_batch::table::check_contract` tests all of this except the
    /// encoding.
    fn sorted(&self, order: &[ColId]) -> Box<dyn SortedTable + '_>;
}
