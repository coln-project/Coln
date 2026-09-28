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

#[cfg(test)]
pub(crate) use self::memory::MemoryStore;

/// A store in memory, for tests, and the smallest implementation of the
/// contract above. It keeps each table as encoded rows with their summed
/// weights, the way a store integrates transactions, and sorts a copy per
/// request.
#[cfg(test)]
mod memory {
    use std::collections::HashMap;

    use coln_batch::relation::Relation;
    use coln_batch::table::ArrowSortedTable;
    use coln_batch::types::Schema;

    use super::{ColId, Dictionary, Key, RelationSource, SortedTable, TableHandle};
    use crate::{
        api::deltas::{ZRow, ZWeight},
        error::RuntimeError,
        relational::{
            batch::values::{batch_schema, engine_value},
            expr::SourceId,
            relation::TupleValue,
            schema::TableSchema,
        },
    };

    pub(crate) struct MemoryStore {
        dictionary: Dictionary,
        tables: HashMap<SourceId, Table>,
    }

    struct Table {
        schema: Schema,
        rows: HashMap<Vec<Key>, ZWeight>,
    }

    impl MemoryStore {
        /// A store with the given tables, all empty.
        pub(crate) fn new(schemas: impl IntoIterator<Item = TableSchema>) -> Self {
            let tables = schemas
                .into_iter()
                .map(|schema| {
                    let table = Table {
                        schema: batch_schema(&schema).expect("a type the engine stores"),
                        rows: HashMap::new(),
                    };
                    (SourceId::from(schema.name()), table)
                })
                .collect();
            Self {
                dictionary: Dictionary::new(),
                tables,
            }
        }

        /// Apply one transaction's rows to `table`: every row's weight adds
        /// to what the table holds, and a row is present while its sum is
        /// positive. Cells are checked against the schema and encoded on
        /// the way in.
        pub(crate) fn apply(
            &mut self,
            table: &SourceId,
            rows: impl IntoIterator<Item = ZRow>,
        ) -> Result<(), RuntimeError> {
            let entry = self
                .tables
                .get_mut(table)
                .ok_or_else(|| RuntimeError::new(format!("no table '{table}'")))?;
            for zrow in rows {
                let weight = zrow.zweight();
                let keys = encode(table, &entry.schema, &zrow.into_row(), &mut self.dictionary)?;
                *entry.rows.entry(keys).or_insert(0) += weight;
            }
            Ok(())
        }
    }

    /// Check one row against its table's schema and encode it.
    fn encode(
        table: &SourceId,
        schema: &Schema,
        row: &TupleValue,
        dictionary: &mut Dictionary,
    ) -> Result<Vec<Key>, RuntimeError> {
        if row.data.len() != schema.arity() {
            return Err(RuntimeError::new(format!(
                "row for table '{table}' has {} values, its schema has {} columns",
                row.data.len(),
                schema.arity()
            )));
        }
        let mut keys = Vec::with_capacity(row.data.len());
        for (col, cell) in row.data.iter().enumerate() {
            let value = engine_value(cell)
                .map_err(|error| RuntimeError::new(format!("table '{table}': {error:#}")))?;
            let expected = schema.column_type(col);
            if value.scalar_type() != expected {
                return Err(RuntimeError::new(format!(
                    "table '{table}', column {}: expected {expected}, got {value}",
                    schema.name(col)
                )));
            }
            keys.push(value.to_key(dictionary));
        }
        Ok(keys)
    }

    impl RelationSource for MemoryStore {
        fn table(&self, table: &SourceId) -> Option<Box<dyn TableHandle + '_>> {
            let (name, found) = self.tables.get_key_value(table)?;
            Some(Box::new(MemoryTable {
                name: name.as_str(),
                table: found,
            }))
        }

        fn dictionary(&self) -> &Dictionary {
            &self.dictionary
        }
    }

    struct MemoryTable<'a> {
        name: &'a str,
        table: &'a Table,
    }

    impl TableHandle for MemoryTable<'_> {
        fn arity(&self) -> usize {
            self.table.schema.arity()
        }

        fn sorted(&self, order: &[ColId]) -> Box<dyn SortedTable + '_> {
            let mut cols: Vec<Vec<Key>> = vec![Vec::new(); self.table.schema.arity()];
            for (row, weight) in &self.table.rows {
                if *weight > 0 {
                    for (column, key) in cols.iter_mut().zip(row) {
                        column.push(*key);
                    }
                }
            }
            let rel = Relation::with_schema(self.name, self.table.schema.clone(), cols);
            let table = ArrowSortedTable::from_relation(&rel, order.to_vec())
                .expect("the join asks for a permutation of the columns");
            Box::new(table)
        }
    }
}
