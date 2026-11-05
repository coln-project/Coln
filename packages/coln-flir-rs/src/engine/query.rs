// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The query engine's [tuple](QueryTuple) and its supported
//! [scalar values](QueryScalarValue).

use crate::{
    engine::{
        packed::{PackedRowId, StoreScalarValue, StoreTuple},
        schema::{ColnSchema, DerivedView},
    },
    ir,
    public::PublicTuple,
    tuple::{QueryScalar, Tuple},
};
use std::iter;

/// The _value_ of a scalar in coln-query.
pub type QueryScalarValue = QueryScalar;

/// The _type_ representing the supported scalar values by coln-query.
pub type QueryScalarType = QueryScalar<(), (), (), (), ()>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTuple(Tuple<QueryScalarValue>);

impl From<StoreTuple> for QueryTuple {
    fn from(value: StoreTuple) -> Self {
        let inner = value
            .0
            .inner
            .into_iter()
            .flat_map(|scalar| match scalar {
                StoreScalarValue::RowId(row_id) => {
                    let hash = QueryScalarValue::U32(row_id.commit_idx);
                    let counter = QueryScalarValue::U32(row_id.counter);
                    iter::once(hash).chain(Some(counter))
                }
                v @ (StoreScalarValue::U64(_)
                | StoreScalarValue::U32(_)
                | StoreScalarValue::I64(_)
                | StoreScalarValue::I32(_)
                | StoreScalarValue::String(_)) => {
                    iter::once(QueryScalar::try_from(v).expect("non rowid scalar should match"))
                        .chain(None)
                }
            })
            .collect();
        Self(Tuple { inner })
    }
}

impl QueryTuple {
    pub fn inner(&self) -> &Tuple<QueryScalarValue> {
        &self.0
    }

    // @Leo we also need to convert a query tuple, i.e. a tuple in the derived view
    // into a store tuple to be put into a view table.
    // The allocate function is a bit of a leaky abstraction.... In short, we need to
    // allocate row ids for these derived tuples, but only the store knows how to do that
    // This probably needs schema as well.
    //
    // @Vincent: If we rebase on top of https://github.com/coln-project/Coln/tree/coln-query/logical-query-program-abstraction
    // there is not only a schema for base tables (with implicit row ids) but
    // also a schema for derived views (without implicit row ids). This can
    // become the `schema` param here guiding the conversion.
    //
    // @Leo: I'll use ir::Schema for now, once you have done the schema.rs integration, we can
    // switch over. Although I feel the plain schema representation is probably enough... We will see
    pub fn into_store(
        self,
        schema: &ir::Schema,
        mut id_alloc: impl FnMut() -> PackedRowId,
    ) -> StoreTuple {
        let mut scalars = self.into_iter();
        let mut values = Vec::with_capacity(schema.columns.len());

        for col in &schema.columns {
            let value = match col.col_type {
                crate::ir::ColType::RowId { .. } => {
                    // TODO: @Leo when you have u32 in query engine, update this as well
                    // instead of narrowing the U64 values returned by its adapter.
                    let commit_idx = match scalars.next() {
                        Some(QueryScalarValue::U32(value)) => value,
                        Some(QueryScalarValue::U64(value)) => {
                            u32::try_from(value).expect("row id commit index must fit in a U32")
                        }
                        _ => panic!("row id commit index must be a U32 or U64"),
                    };
                    let counter = match scalars.next() {
                        Some(QueryScalarValue::U32(value)) => value,
                        Some(QueryScalarValue::U64(value)) => {
                            u32::try_from(value).expect("row id counter must fit in a U32")
                        }
                        _ => panic!("row id counter must be a U32 or U64"),
                    };
                    StoreScalarValue::RowId(PackedRowId {
                        commit_idx,
                        counter,
                    })
                }
                crate::ir::ColType::BuiltinTy { .. } => scalars
                    .next()
                    .expect("query tuple is missing a column")
                    .into(),
            };
            values.push(value);
        }

        assert!(scalars.next().is_none(), "query tuple has extra columns");
        StoreTuple::from_id_values(id_alloc(), values)
    }

    pub fn into_public(self, _schema: &ColnSchema<DerivedView>) -> PublicTuple {
        // This for tuples bubbling upwards, that is, tuples which belong to
        // derived views. They do not contain a row id themselves but may
        // contain several foreign keys with row ids. This function needs to
        // deflatten row id hash ints (plus remap them to their hash) and row
        // id counter ints into pairs.
        // To do so, I think it needs the schema to know in which position
        // there are foreign keys. Note that I still need to adjust BaseTableSchema
        // to also accomodate DerivedViews which do not have a row id on their own
        // (which BaseTableSchema currently assumes).
        todo!("Call into coln-store's API")
    }
}

impl IntoIterator for QueryTuple {
    type Item = QueryScalarValue;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl FromIterator<QueryScalarValue> for QueryTuple {
    fn from_iter<T: IntoIterator<Item = QueryScalarValue>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}
