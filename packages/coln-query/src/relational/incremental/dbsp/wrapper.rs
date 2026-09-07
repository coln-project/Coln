// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::api::deltas::ZRow;
use crate::relational::incremental::schema::{SchemaTuple, StreamSchema, TupleKey};
use crate::relational::relation::{self, Relation, RelationData, RelationRef, TupleValue};
use cli_table::{Cell, Style, Table, format::Justify};
pub use dbsp::{
    DBSPHandle as DbspHandle, Error as DbspError, NestedCircuit, RootCircuit, Runtime, ZWeight,
};
use dbsp::{
    IndexedZSetHandle, IndexedZSetReader, OrdIndexedZSet, OutputHandle, Stream,
    typed_batch::SpineSnapshot, utils::Tup2,
};
#[allow(unused_imports, reason = "For testing purposes")]
pub use dbsp::{OrdZSet, indexed_zset, zset, zset_set};
use std::{
    any::Any,
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

/// A stream plus the schema its `(TupleKey, TupleValue)` pairs are laid out by:
/// the DBSP backend's concrete relation representation, and the single point
/// where the DBSP runtime plugs into the backend-neutral [`Relation`] envelope.
///
/// The schema rides *here*, next to the stream, rather than in [`Relation`]:
/// keying a relation is a DBSP requirement (`OrdIndexedZSet`), and the schema
/// changes as operators build the circuit, so each derived stream carries the
/// schema its own rows have. The pair is what every DBSP operator recovers via
/// [`as_dbsp`](AsDbspRelation::as_dbsp).
#[derive(Clone)]
pub struct DbspRelation {
    schema: StreamSchema,
    stream: StreamWrapper,
}

impl DbspRelation {
    pub fn new(schema: StreamSchema, stream: StreamWrapper) -> Self {
        Self { schema, stream }
    }
    pub fn schema(&self) -> &StreamSchema {
        &self.schema
    }
    pub fn stream(&self) -> &StreamWrapper {
        &self.stream
    }
}

impl RelationData for DbspRelation {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn RelationData> {
        Box::new(self.clone())
    }
}

impl Display for DbspRelation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.schema)
    }
}

impl Debug for DbspRelation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.schema)
    }
}

/// Recover the DBSP backend's own relation from the type-erased envelope the
/// host layer passes around. Every DBSP operator starts here.
pub trait AsDbspRelation {
    fn as_dbsp(&self) -> &DbspRelation;
}

impl AsDbspRelation for Relation {
    fn as_dbsp(&self) -> &DbspRelation {
        self.downcast_ref::<DbspRelation>()
    }
}

/// A fresh [`RelationRef`] over `stream` and the schema its rows have. The DBSP
/// backend's counterpart to [`relation::new_relation`], which takes the pair
/// pre-assembled.
pub fn new_relation(schema: StreamSchema, stream: StreamWrapper) -> RelationRef {
    relation::new_relation(DbspRelation::new(schema, stream))
}

#[derive(Default, Debug, Clone)]
pub struct DbspInputs {
    inputs: HashMap<String, DbspInput>,
}

impl DbspInputs {
    pub fn from_named_inputs<I: IntoIterator<Item = (String, DbspInput)>>(inputs: I) -> Self {
        Self {
            inputs: HashMap::from_iter(inputs),
        }
    }
    pub fn get<Q: AsRef<str>>(&self, name: Q) -> Option<&DbspInput> {
        self.inputs.get(name.as_ref())
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
    schema: StreamSchema,
    handle: OrdIndexedStreamInputHandle,
}

impl DbspInput {
    pub fn new(schema: StreamSchema, handle: OrdIndexedStreamInputHandle) -> Self {
        Self { schema, handle }
    }
    /// Feed a batch of value tuples (with z-weights) into this input. The tuple
    /// key is derived from the value by picking the schema's key fields, so
    /// callers only supply the value — matching the neutral `Runtime::feed`.
    pub fn feed(&self, rows: impl IntoIterator<Item = ZRow>) {
        let tuple_names: Vec<String> = self.schema.tuple.field_names(&None).collect();
        let key_indices: Vec<usize> = self
            .schema
            .key
            .field_names(&None)
            .map(|key_field| {
                tuple_names
                    .iter()
                    .position(|name| *name == key_field)
                    .expect("key field must appear in the tuple schema")
            })
            .collect();
        let mut batch = rows
            .into_iter()
            .map(|row_delta| {
                let zweight = row_delta.zweight();
                let row = row_delta.into_row();
                let key = TupleKey {
                    data: key_indices.iter().map(|&i| row.data[i].clone()).collect(),
                };
                Tup2(key, Tup2(row, zweight))
            })
            .collect();
        self.handle.append(&mut batch);
    }
    pub fn handle(&self) -> &OrdIndexedStreamInputHandle {
        &self.handle
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
    schema: StreamSchema,
}

impl DbspOutput {
    pub fn drain(&self) -> DbspOutputDelta {
        // This can already be iterated and saved into a collection, e.g., a Vector.
        // Yet, I believe this does not guarantee that each (TupleKey, TupleValue)
        // pair is unique but instead could appear multiple times with different
        // zweights which would need to be accumulated for each
        // (TupleKey, TupleValue) pair.
        let delta: SpineSnapshot<OrdIndexedZSet<TupleKey, TupleValue>> = self.handle.concat();
        // Therefore, we play it safe and consolidate here, which guarantees that
        // each (TupleKey, TupleValue) pair is unique with its accumulated zweight.
        // If at some point, the accumulation should happen through a custom data
        // structure, this step may be omitted for performance reasons.
        let delta: OrdIndexedZSet<TupleKey, TupleValue> = delta.consolidate();
        DbspOutputDelta {
            schema: self.schema.clone(),
            delta,
        }
    }
}

impl std::fmt::Debug for DbspOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbspOutput")
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

impl From<&Relation> for DbspOutput {
    fn from(relation: &Relation) -> Self {
        let relation = relation.as_dbsp();
        Self {
            schema: relation.schema().clone(),
            handle: relation.stream().output(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DbspOutputDelta {
    schema: StreamSchema,
    delta: OrdIndexedZSet<TupleKey, TupleValue>,
}

impl DbspOutputDelta {
    const JUSTIFICATION: Justify = Justify::Right;

    pub fn schema(&self) -> &StreamSchema {
        &self.schema
    }
    pub fn as_table(&self) -> impl Display {
        self.delta
            .iter()
            .map(|(key, tuple, weight)| {
                iter::once(weight.to_string().cell().justify(Self::JUSTIFICATION)).chain(
                    SchemaTuple::new(&self.schema.tuple, &tuple)
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
        self.delta
            .iter()
            .map(|(key, tuple, weight)| {
                // We ensure that the key and tuple data lengths match the
                // respective schema field lengths.
                debug_assert!(key.data.len() == self.schema.key.full_len());
                debug_assert!(tuple.data.len() == self.schema.tuple.full_len());
                iter::once(weight.to_string().cell().justify(Self::JUSTIFICATION))
                    .chain(
                        SchemaTuple::new(&self.schema.key, &key)
                            .all_fields()
                            .map(|attribute| {
                                attribute.to_string().cell().justify(Self::JUSTIFICATION)
                            })
                            .collect::<Vec<_>>(),
                    )
                    .chain(
                        SchemaTuple::new(&self.schema.tuple, &tuple)
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
    /// Outputs only the visible columns of the output.
    fn as_data(&self) -> impl Iterator<Item = (ZWeight, TupleValue)> {
        self.delta.iter().map(|(_key, tuple, zweight)| {
            let tuple: TupleValue = SchemaTuple::new(&self.schema.tuple, &tuple)
                .fields()
                .cloned()
                .collect();
            (zweight, tuple)
        })
    }
    /// Unlike [`as_data`](Self::as_data), this Includes hidden/inactive
    /// columns in its output.
    fn as_debug_data(&self) -> impl Iterator<Item = (ZWeight, TupleValue)> {
        self.delta
            .iter()
            .map(|(_key, tuple, zweight)| (zweight, tuple))
    }
    pub fn as_zrows(&self) -> impl Iterator<Item = ZRow> {
        self.as_data()
            .filter_map(|(zweight, tuple)| ZRow::new(zweight, tuple))
    }
    pub fn as_debug_zrows(&self) -> impl Iterator<Item = ZRow> {
        self.as_debug_data()
            .filter_map(|(zweight, tuple)| ZRow::new(zweight, tuple))
    }
    pub fn to_zset(&self) -> OrdZSet<TupleValue> {
        let keys = self
            .as_data()
            .map(|(zweight, tuple)| Tup2(tuple, zweight))
            .collect::<Vec<_>>();
        OrdZSet::from_keys((), keys)
    }
    pub fn to_debug_zset(&self) -> OrdZSet<TupleValue> {
        let keys = self
            .as_debug_data()
            .map(|(zweight, tuple)| Tup2(tuple, zweight))
            .collect::<Vec<_>>();
        OrdZSet::from_keys((), keys)
    }
}
