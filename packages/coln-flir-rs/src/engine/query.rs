use std::iter;

use crate::{
    PublicTuple,
    engine::{
        packed::{PackedRowId, StoreScalarValue, StoreTuple},
        schema::BaseTableSchema,
    },
    value::{QueryScalarValue, Tuple},
};

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
                StoreScalarValue::U64(scalar) => {
                    iter::once(QueryScalarValue::U64(scalar)).chain(None)
                }
                StoreScalarValue::U32(scalar) => {
                    iter::once(QueryScalarValue::U32(scalar)).chain(None)
                }
                StoreScalarValue::I64(scalar) => {
                    iter::once(QueryScalarValue::I64(scalar)).chain(None)
                }
                StoreScalarValue::I32(scalar) => {
                    iter::once(QueryScalarValue::I32(scalar)).chain(None)
                }
                StoreScalarValue::String(scalar) => {
                    iter::once(QueryScalarValue::String(scalar)).chain(None)
                }
            })
            .collect();
        Self(Tuple { inner })
    }
}

impl QueryTuple {
    // @Leo we also need to convert a query tuple, i.e. a tuple in the derived view
    // into a store tuple to be put into a view table.
    // The allocate function is a bit of a leaky abstraction.... In short, we need to
    // allocate row ids for these derived tuples, but only the store knows how to do that
    // This probably needs schema as well.
    pub fn into_store(
        self,
        _schema: &BaseTableSchema,
        _alloc: impl FnMut() -> PackedRowId,
    ) -> StoreTuple {
        todo!()
    }

    pub fn into_public(self, _schema: &BaseTableSchema) -> PublicTuple {
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
