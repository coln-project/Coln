// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    expr::{Literal, LiteralExpr},
    relation::{Relation, RelationRef, RelationSchema, SchemaTuple, TupleKey, TupleValue},
};
use cli_table::{Cell, Style, Table, format::Justify};
pub use dbsp::{
    DBSPHandle as DbspHandle, Error as DbspError, NestedCircuit, RootCircuit, Runtime, ZWeight,
};
use dbsp::{
    IndexedZSetHandle, IndexedZSetReader, OrdIndexedZSet, OrdZSet, OutputHandle, Stream,
    typed_batch::SpineSnapshot, utils::Tup2,
};
#[allow(unused_imports, reason = "For testing purposes")]
pub use dbsp::{indexed_zset, zset, zset_set};
use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    iter,
};

type OrdStream = Stream<RootCircuit, OrdZSet<TupleValue>>;

pub fn new_ord_indexed_stream(
    circuit: &mut RootCircuit,
) -> (OrdIndexedRootStream, OrdIndexedStreamInputHandle) {
    circuit.add_input_indexed_zset::<TupleKey, TupleValue>()
}

pub type OrdIndexedStreamInputHandle = IndexedZSetHandle<TupleKey, TupleValue>;

pub type OrdIndexedStreamOutputHandle =
    OutputHandle<SpineSnapshot<OrdIndexedZSet<TupleKey, TupleValue>>>;

pub type OrdIndexedStream<Circuit> = Stream<Circuit, OrdIndexedZSet<TupleKey, TupleValue>>;

pub type OrdIndexedRootStream = OrdIndexedStream<RootCircuit>;
pub type OrdIndexedNestedStream = OrdIndexedStream<NestedCircuit>;

/// A wrapper of DBSP's streams carrying [`dbsp::OrdIndexedZSet`] but
/// generic-free over the circuit type. This limits the nesting level to one
/// level but this does not matter for practical applications.
#[derive(Clone)]
pub enum StreamWrapper {
    Root(OrdIndexedRootStream),
    Nested(OrdIndexedNestedStream),
}

impl StreamWrapper {
    pub fn distinct(&self) -> StreamWrapper {
        match self {
            Self::Root(stream) => Self::Root(stream.distinct()),
            Self::Nested(stream) => Self::Nested(stream.distinct()),
        }
    }

    pub fn sum<'a, I>(&'a self, streams: I) -> StreamWrapper
    where
        I: IntoIterator<Item = &'a Self>,
    {
        match self {
            Self::Root(stream) => {
                Self::Root(stream.sum(streams.into_iter().map(|s| s.expect_root())))
            }
            Self::Nested(stream) => {
                Self::Nested(stream.sum(streams.into_iter().map(|s| s.expect_nested())))
            }
        }
    }

    pub fn plus(&self, other: &Self) -> Self {
        match self {
            Self::Root(stream) => Self::Root(stream.plus(other.expect_root())),
            Self::Nested(stream) => Self::Nested(stream.plus(other.expect_nested())),
        }
    }

    pub fn minus(&self, other: &Self) -> Self {
        match self {
            Self::Root(stream) => Self::Root(stream.minus(other.expect_root())),
            Self::Nested(stream) => Self::Nested(stream.minus(other.expect_nested())),
        }
    }

    pub fn map_index<F>(&self, map_func: F) -> Self
    where
        F: Fn((&TupleKey, &TupleValue)) -> (TupleKey, TupleValue) + 'static,
    {
        match self {
            Self::Root(stream) => Self::Root(stream.map_index(map_func)),
            Self::Nested(stream) => Self::Nested(stream.map_index(map_func)),
        }
    }

    pub fn filter<F>(&self, filter_func: F) -> Self
    where
        F: Fn((&TupleKey, &TupleValue)) -> bool + 'static,
    {
        match self {
            Self::Root(stream) => Self::Root(stream.filter(filter_func)),
            Self::Nested(stream) => Self::Nested(stream.filter(filter_func)),
        }
    }

    pub fn join_index<F, It>(&self, other: &Self, join: F) -> Self
    where
        F: Fn(&TupleKey, &TupleValue, &TupleValue) -> It + Clone + 'static,
        It: IntoIterator<Item = (TupleKey, TupleValue)> + 'static,
    {
        match self {
            Self::Root(stream) => Self::Root(stream.join_index(other.expect_root(), join)),
            Self::Nested(stream) => Self::Nested(stream.join_index(other.expect_nested(), join)),
        }
    }

    pub fn anti_join_index(&self, other: &Self) -> Self {
        match self {
            Self::Root(stream) => Self::Root(stream.antijoin(other.expect_root())),
            Self::Nested(stream) => Self::Nested(stream.antijoin(other.expect_nested())),
        }
    }

    /// The delta0 operator imports a stream from the parent circuit into the
    /// child circuit.
    pub fn delta0(&self, child_circuit: &NestedCircuit) -> Self {
        match self {
            // Transitions from RootStream to NestedStream
            Self::Root(stream) => Self::Nested(stream.delta0(child_circuit)),
            Self::Nested(stream) => panic!("No further nesting for beyond NestedStreams"),
        }
    }

    pub fn output(&self) -> OrdIndexedStreamOutputHandle {
        match self {
            Self::Root(stream) => stream.accumulate_output(),
            Self::Nested(stream) => panic!("Nested streams do not support output()"),
        }
    }

    fn expect_root(&self) -> &OrdIndexedRootStream {
        if let Self::Root(stream) = self {
            stream
        } else {
            panic!("Expected RootStream")
        }
    }

    pub fn expect_nested(&self) -> &OrdIndexedNestedStream {
        if let Self::Nested(stream) = self {
            stream
        } else {
            panic!("Expected NestedStream")
        }
    }
}

impl From<OrdIndexedRootStream> for StreamWrapper {
    fn from(stream: OrdIndexedRootStream) -> Self {
        Self::Root(stream)
    }
}

impl From<OrdIndexedNestedStream> for StreamWrapper {
    fn from(stream: OrdIndexedNestedStream) -> Self {
        Self::Nested(stream)
    }
}

impl IntoIterator for &'_ StreamWrapper {
    type Item = Self;
    type IntoIter = std::iter::Once<Self>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self)
    }
}

#[derive(Default, Debug, Clone)]
pub struct DbspInputs {
    inputs: HashMap<String, DbspInput>,
}

impl DbspInputs {
    fn insert(&mut self, name: String, input: DbspInput) {
        self.inputs.insert(name, input);
    }
    pub fn get(&self, name: &str) -> Option<&DbspInput> {
        self.inputs.get(name)
    }
    pub fn take(&mut self, name: &str) -> Option<DbspInput> {
        self.inputs.remove(name)
    }
    pub fn iter(&self) -> impl Iterator<Item = &DbspInput> {
        self.inputs.values()
    }
}

#[derive(Clone)]
pub struct DbspInput {
    schema: RelationSchema,
    handle: OrdIndexedStreamInputHandle,
}

impl DbspInput {
    pub fn add(
        schema: RelationSchema,
        circuit: &mut RootCircuit,
        inputs: &mut DbspInputs,
    ) -> LiteralExpr {
        let (stream, handle) = new_ord_indexed_stream(circuit);
        let input = Self {
            schema: schema.clone(),
            handle,
        };
        inputs.insert(schema.name.clone(), input);
        LiteralExpr {
            value: Literal::Relation(Relation::new(schema, stream)),
        }
    }
    pub fn handle(&self) -> &OrdIndexedStreamInputHandle {
        &self.handle
    }
    pub fn insert_consume<T: Into<TupleKey> + Into<TupleValue> + Clone>(
        &self,
        tuples: impl IntoIterator<Item = (T, ZWeight)>,
    ) {
        let mut delta_batch = tuples
            .into_iter()
            .map(|(data, zweight)| {
                Tup2(
                    Into::<TupleKey>::into(data.clone()),
                    Tup2(Into::<TupleValue>::into(data), zweight),
                )
            })
            .collect();
        self.handle.append(&mut delta_batch);
    }
    pub fn insert<'a, T: Into<TupleKey> + Into<TupleValue> + Clone + 'a>(
        &self,
        tuples: impl IntoIterator<Item = (&'a T, ZWeight)>,
    ) {
        tuples.into_iter().for_each(|(tuple, z_weight)| {
            self.handle
                .push(tuple.clone().into(), (tuple.clone().into(), z_weight))
        })
    }
    pub fn insert_with_same_weight<'a, T: Into<TupleKey> + Into<TupleValue> + Clone + 'a>(
        &self,
        tuples: impl IntoIterator<Item = &'a T>,
        z_weight: ZWeight,
    ) {
        self.insert(tuples.into_iter().map(|tuple| (tuple, z_weight)));
    }
}

impl Debug for DbspInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbspInput")
            .field("schema", &self.schema)
            .finish()
    }
}

pub struct DbspOutput {
    handle: OrdIndexedStreamOutputHandle,
    schema: RelationSchema,
}

impl DbspOutput {
    pub fn new(schema: RelationSchema, handle: OrdIndexedStreamOutputHandle) -> Self {
        Self { schema, handle }
    }
    pub fn to_batch(&self) -> DbspOutputBatch<'_> {
        let inner = self.handle.concat().iter().collect::<Vec<_>>();
        DbspOutputBatch {
            schema: &self.schema,
            inner,
        }
    }
}

impl From<RelationRef> for DbspOutput {
    fn from(relation: RelationRef) -> Self {
        let relation = relation.borrow();
        let schema = relation.schema.clone();
        let handle = relation.inner.output();
        Self { schema, handle }
    }
}

pub struct DbspOutputBatch<'a> {
    schema: &'a RelationSchema,
    inner: Vec<(TupleKey, TupleValue, ZWeight)>,
}

impl DbspOutputBatch<'_> {
    const JUSTIFICATION: Justify = Justify::Right;

    pub fn as_table(&self) -> impl Display {
        self.inner
            .iter()
            .map(|(key, tuple, weight)| {
                iter::once(weight.to_string().cell().justify(Self::JUSTIFICATION)).chain(
                    SchemaTuple::new(&self.schema.tuple, tuple)
                        .fields()
                        .map(|attribute| attribute.to_string().cell().justify(Self::JUSTIFICATION))
                        .collect::<Vec<_>>(),
                )
            })
            .table()
            .title(
                iter::once("z-weight".cell())
                    .chain(self.schema.tuple.field_names(&None).map(|name| name.cell())),
            )
            .bold(true)
            .display()
            .expect("Table error")
    }
    pub fn as_debug_table(&self) -> impl Display {
        self.inner
            .iter()
            .map(|(key, tuple, weight)| {
                // We ensure that the key and tuple data lengths match the
                // respective schema field lengths.
                debug_assert!(key.data.len() == self.schema.key.full_len());
                debug_assert!(tuple.data.len() == self.schema.tuple.full_len());
                iter::once(weight.to_string().cell().justify(Self::JUSTIFICATION))
                    .chain(
                        SchemaTuple::new(&self.schema.key, key)
                            .all_fields()
                            .map(|attribute| {
                                attribute.to_string().cell().justify(Self::JUSTIFICATION)
                            })
                            .collect::<Vec<_>>(),
                    )
                    .chain(
                        SchemaTuple::new(&self.schema.tuple, tuple)
                            .all_fields()
                            .map(|attribute| {
                                attribute.to_string().cell().justify(Self::JUSTIFICATION)
                            })
                            .collect::<Vec<_>>(),
                    )
            })
            .table()
            .title(
                iter::once("z-weight".cell())
                    .chain(
                        self.schema
                            .key
                            .all_field_names(&None)
                            .map(|name| format!("[key] {name}").cell()),
                    )
                    .chain(
                        self.schema
                            .tuple
                            .all_field_names(&None)
                            .map(|name| format!("[value] {name}").cell()),
                    ),
            )
            .bold(true)
            .display()
            .expect("Table error")
    }
    pub fn as_data(&self) -> impl Iterator<Item = (ZWeight, &TupleValue)> {
        self.inner
            .iter()
            .map(|(_key, tuple, weight)| (*weight, tuple))
    }
    pub fn as_zset(&self) -> OrdZSet<TupleValue> {
        let keys = self
            .inner
            .iter()
            .map(|(_key, tuple, weight)| {
                let tuple: TupleValue = SchemaTuple::new(&self.schema.tuple, tuple)
                    .fields()
                    .cloned()
                    .collect();
                Tup2(tuple, *weight)
            })
            .collect::<Vec<_>>();
        OrdZSet::from_keys((), keys)
    }
    pub fn as_debug_zset(&self) -> OrdZSet<TupleValue> {
        let keys = self
            .inner
            .iter()
            .map(|(_key, tuple, weight)| Tup2(tuple.clone(), *weight))
            .collect::<Vec<_>>();
        OrdZSet::from_keys((), keys)
    }
}
