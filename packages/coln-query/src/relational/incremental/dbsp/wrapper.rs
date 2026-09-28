// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::api::deltas::ZRow;
use crate::relational::incremental::schema::{ColumnSelector, SchemaTuple, StreamSchema, TupleKey};
use crate::relational::relation::{self, Relation, RelationData, RelationRef, TupleValue};
use crate::scalarial::ScalarTypedValue;
use crate::utils::cli_table::{
    ChainedHeader, CliReport, CliTableHeader, TableDisplay, ToCliReport, ToCliTableIterExt,
    ZWeightedHeader,
};
use dbsp::{
    Circuit, IndexedZSetHandle, IndexedZSetReader, OrdIndexedZSet, OutputHandle, Stream,
    operator::ConstantGenerator, typed_batch::SpineSnapshot, utils::Tup2,
};
pub use dbsp::{
    DBSPHandle as DbspHandle, Error as DbspError, NestedCircuit, RootCircuit, Runtime, ZWeight,
};
#[allow(unused_imports, reason = "For testing purposes")]
pub use dbsp::{OrdZSet, indexed_zset, zset, zset_set};
use std::{
    any::Any,
    collections::HashMap,
    fmt::{Debug, Display},
};

type OrdStream = Stream<RootCircuit, OrdZSet<TupleValue>>;

pub fn new_ord_indexed_stream(
    circuit: &mut RootCircuit,
) -> (OrdIndexedRootStream, OrdIndexedStreamInputHandle) {
    circuit.add_input_indexed_zset::<TupleKey, TupleValue>()
}

/// Wires a relation the plan itself carries, a
/// [`ConstantExpr`](crate::relational::expr::ConstantExpr), into a root stream.
/// There is no handle because there is nobody to feed it.
///
/// `rows` are the relation's *contents*, but a root stream carries *changes*.
/// The constant is therefore emitted as one delta in the circuit's first step
/// and as nothing afterwards, which is what `differentiate` over a stream that
/// is constantly `rows` computes (`rows - z⁻¹(rows)`). Its integrated value is
/// `rows` at every step, which is exactly the claim the plan node makes.
///
/// [`ConstantGenerator`] is also what keeps the rows from being multiplied by
/// the worker count, as it emits them on worker 0 only, and the operators that
/// need the data partitioned shard it themselves. A hand-rolled
/// [`Generator`](::dbsp::operator::Generator) would emit a full copy per worker.
pub fn new_constant_stream(
    circuit: &RootCircuit,
    schema: &StreamSchema,
    rows: impl IntoIterator<Item = (TupleValue, ZWeight)>,
) -> OrdIndexedRootStream {
    let key_indices = key_indices(schema);
    let tuples = rows
        .into_iter()
        .map(|(row, weight)| Tup2(Tup2(key_of(&key_indices, &row), row), weight))
        .collect();
    let batch = OrdIndexedZSet::<TupleKey, TupleValue>::from_tuples((), tuples);
    circuit
        .add_source(ConstantGenerator::new(batch))
        .differentiate()
}

/// Where the key columns of `schema` sit in its rows.
///
/// A relation's key is not data of its own: it is a projection of the row (see
/// [`StreamSchema`]), which is what makes "the key determines the row" hold by
/// construction rather than by anyone's discipline.
fn key_indices(schema: &StreamSchema) -> Vec<usize> {
    let tuple_names: Vec<String> = schema
        .tuple
        .field_names(ColumnSelector::Visible, &None)
        .collect();
    schema
        .key
        .field_names(ColumnSelector::Visible, &None)
        .map(|key_field| {
            tuple_names
                .iter()
                .position(|name| *name == key_field)
                .expect("key field must appear in the tuple schema")
        })
        .collect()
}

/// The key of one row, as [`key_indices`] located it.
fn key_of(key_indices: &[usize], row: &TupleValue) -> TupleKey {
    TupleKey {
        data: key_indices
            .iter()
            .map(|index| row.data[*index].clone())
            .collect(),
    }
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
    /// callers only supply the value, matching the neutral `Runtime::feed`.
    pub fn feed(&self, rows: impl IntoIterator<Item = ZRow>) {
        let key_indices = key_indices(&self.schema);
        let mut batch = rows
            .into_iter()
            .map(|row_delta| {
                let zweight = row_delta.zweight();
                let row = row_delta.into_row();
                Tup2(key_of(&key_indices, &row), Tup2(row, zweight))
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
    pub fn schema(&self) -> &StreamSchema {
        &self.schema
    }
    /// The delta as a query sees it: the columns the schema still exposes.
    pub fn view(&self) -> OutputView<'_> {
        OutputView {
            delta: self,
            columns: ColumnSelector::Visible,
            with_key: false,
        }
    }
    /// The delta as it is physically stored: every column, including the ones
    /// eliminated by, e.g., a projection, and the key columns alongside the
    /// values.
    pub fn debug_view(&self, with_key: bool) -> OutputView<'_> {
        OutputView {
            delta: self,
            columns: ColumnSelector::All,
            with_key,
        }
    }
}

/// One of the two ways of looking at a [`DbspOutputDelta`], see
/// [`view`](DbspOutputDelta::view) and
/// [`debug_view`](DbspOutputDelta::debug_view). Each accessor reports the
/// columns its view selects, so both views share one implementation of each.
#[derive(Clone, Copy)]
pub struct OutputView<'a> {
    delta: &'a DbspOutputDelta,
    columns: ColumnSelector,
    with_key: bool,
}

impl<'a> OutputView<'a> {
    fn rows(self) -> impl Iterator<Item = (ZWeight, Vec<ScalarTypedValue>)> + 'a {
        let columns = self.columns;
        let key_schema = &self.delta.schema.key;
        let tuple_schema = &self.delta.schema.tuple;
        self.delta.delta.iter().map(move |(key, tuple, zweight)| {
            // If the key is requested, too, we include _all_ key columns.
            let mut row = if self.with_key { key.data } else { vec![] };
            if columns == ColumnSelector::All || !tuple_schema.is_coalesced() {
                // If there is no need to filter columns, we pass them through.
                row.extend(tuple.data);
            } else {
                // Otherwise, we filter.
                row.extend(
                    SchemaTuple::new(tuple_schema, &tuple)
                        .fields(columns)
                        .cloned(),
                )
            };
            (zweight, row)
        })
    }
    pub fn to_zrows(self) -> impl Iterator<Item = ZRow> + 'a {
        self.rows()
            .filter_map(|(zweight, row)| ZRow::new(zweight, TupleValue::new(row)))
    }
    fn to_zrows_unchecked(self) -> impl Iterator<Item = ZRow> + 'a {
        self.rows()
            .map(|(zweight, row)| ZRow::new_unchecked(zweight, TupleValue::new(row)))
    }
    pub fn to_zset(self) -> OrdZSet<TupleValue> {
        let keys = self
            .rows()
            .map(|(zweight, row)| Tup2(TupleValue::new(row), zweight))
            .collect::<Vec<_>>();
        OrdZSet::from_keys((), keys)
    }
    pub fn to_cli_table(self) -> std::io::Result<TableDisplay> {
        self.to_zrows_unchecked().to_cli_table_with(self.header())
    }
    fn header(self) -> impl CliTableHeader + 'a {
        let key = self.with_key.then(|| {
            self.delta
                .schema
                .key
                // We include _all_ key columns.
                .header(ColumnSelector::All)
                .tagged("key")
        });
        let tuple = self.delta.schema.tuple.header(self.columns);
        let tuple = match key {
            Some(_) => tuple.tagged("value"),
            None => tuple,
        };
        ZWeightedHeader(ChainedHeader(key, tuple))
    }
}

impl ToCliReport for OutputView<'_> {
    fn to_cli_report(&self) -> std::io::Result<CliReport> {
        let mut report = CliReport::untitled();
        report.section(
            self.delta.schema.name.clone(),
            self.header(),
            self.to_zrows_unchecked(),
        )?;
        Ok(report)
    }
}
