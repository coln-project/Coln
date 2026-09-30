use std::iter;

use crate::{
    engine::{
        packed::{PackedRowId, StoreScalarValue, StoreTuple},
        schema::BaseTableSchema,
    },
    public::PublicTuple,
    value::{QueryScalar, Tuple},
};

pub type QueryScalarValue = QueryScalar;

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

impl IntoIterator for QueryTuple {
    type Item = QueryScalarValue;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
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
        _id_alloc: impl FnMut() -> PackedRowId,
    ) -> StoreTuple {
        todo!()
        // self.into_iter()
        //     .map(|zrow| {
        //         let row_id = id_alloc();
        //         if zrow.zweight() > 0 {
        //             let mut val_iter = zrow.into_row().data.into_iter();
        //             let mut packed_val = Vec::new();

        //             for col in &self.schema().columns {
        //                 match col.col_type {
        //                     ir::ColType::RowId { .. } => {
        //                         let ScalarTypedValue::Uint(commit_idx) =
        //                             val_iter.next().expect("coln-query returns valid data")
        //                         else {
        //                             panic!("invalid data from coln-query");
        //                         };
        //                         let ScalarTypedValue::Uint(counter) =
        //                             val_iter.next().expect("coln-query returns valid data")
        //                         else {
        //                             panic!("invalid data from coln-query");
        //                         };
        //                         packed_val.push(PackedValue::Id(PackedRowId {
        //                             commit_idx: commit_idx as u32,
        //                             counter: counter as u32,
        //                         }));
        //                     }
        //                     ir::ColType::BuiltinTy {
        //                         builtin_ty: ir::BuiltinTy::BuiltinInt,
        //                     } => {
        //                         let ScalarTypedValue::String(s) = val_iter.next().unwrap() else {
        //                             panic!("invalid data from coln-query");
        //                         };
        //                         packed_val.push(PackedValue::Str(s));
        //                     }
        //                     ir::ColType::BuiltinTy {
        //                         builtin_ty: ir::BuiltinTy::BuiltinStr,
        //                     } => {
        //                         let ScalarTypedValue::Iint(i) = val_iter.next().unwrap() else {
        //                             panic!("invalid data from coln-query");
        //                         };
        //                         packed_val.push(PackedValue::Int(i as i32));
        //                     }
        //                 }
        //             }

        //             PackedOp::Add {
        //                 row_id,
        //                 values: packed_val.into(),
        //             }
        //         } else if zrow.zweight() < 0 {
        //             // TODO don't know how to remove yet
        //             todo!()
        //         } else {
        //             unreachable!("zero zweight impossible")
        //         }
        //     })
        //     .collect()
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
