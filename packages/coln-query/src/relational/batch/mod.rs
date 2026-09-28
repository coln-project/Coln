// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A batch backend optimized for efficient evaluation of non-binary joins.
//!
//! The non-incremental half of the pipeline. [`Backend::build`] lowers the
//! resolved plan into one coln-batch Datalog program,
//! [`BatchRuntime::commit_from`] computes the whole result from the store's
//! current tables (a semi-naive fixpoint over worst-case-optimal joins),
//! and [`Runtime::output`] hands back the full current state of a sink as a
//! [`Snapshot`]. Where the incremental backend reports deltas, this backend
//! reports states.
//!
//! Values keep their plan types end to end: unsigned and signed integers,
//! booleans, characters and strings. The engine compares every cell as a
//! typed key (see `coln_batch::types`); a string's key is its code in the
//! store's dictionary. `Null` is the one plan type the backend refuses, at
//! build time.
//!
//! # Base tables come from the store
//!
//! The backend never copies a base table. At commit it asks the store for
//! a handle per base table the plan reads ([`RelationSource`]), and the
//! join asks each handle for the table sorted in the column order it needs
//! ([`TableHandle`]). The contract for those tables is in `source.rs`. The
//! plan's own constants are the one input that is not the store's: small
//! tables, encoded per commit against a copy of the store's dictionary and
//! read next to the store's tables.
//!
//! Deltas have no place here. [`Runtime::feed`] and the plain
//! [`Runtime::commit`] are the incremental backend's verbs; this backend
//! refuses both with an error that names [`BatchRuntime::commit_from`].

mod lowering;
mod source;
mod values;

#[cfg(test)]
pub(crate) use self::source::MemoryStore;
pub use self::source::{ColId, Dictionary, Key, RelationSource, SortedTable, TableHandle};

use std::collections::HashMap;
use std::num::NonZeroUsize;

use anyhow::{Context, bail};
use coln_batch::fixpoint::{self, Exec, FixpointResult};
use coln_batch::generic_join;
use coln_batch::query::{Catalog as BatchCatalog, Layered, Tables};
use coln_batch::relation::Relation;
use coln_batch::rule::Program;
use coln_batch::types::Schema;
use dbsp::{OrdZSet, utils::Tup2};

use self::lowering::{ConstantTable, LoweredPlan, lower};
use self::values::pipeline_value;
use super::{Backend, Runtime};
use crate::{
    api::deltas::ZRow,
    error::{BuildError, RuntimeError},
    host::resolver::ResolvedCode,
    relational::{
        catalog::SourceSchemas,
        expr::{SinkId, SourceId},
        relation::TupleValue,
    },
    scalarial::{ColumnScalarEngine, column::VectorizedScalarEngine},
};

/// The non-incremental backend: lowers the plan to a coln-batch Datalog
/// program at build time and computes it from the store's tables on every
/// [`BatchRuntime::commit_from`].
pub struct BatchBackend<E: ColumnScalarEngine = VectorizedScalarEngine> {
    // Reserved for the scalar slice: computed columns and general
    // conditions will run on this engine.
    #[allow(dead_code)]
    scalar_engine: E,
}

impl Default for BatchBackend<VectorizedScalarEngine> {
    fn default() -> Self {
        Self {
            scalar_engine: VectorizedScalarEngine::default(),
        }
    }
}

impl<E: ColumnScalarEngine> Backend for BatchBackend<E> {
    type Runtime = BatchRuntime;
    type Error = BuildError;

    fn build(
        self,
        _threads: NonZeroUsize,
        plan: ResolvedCode,
        sources: SourceSchemas,
    ) -> Result<BatchRuntime, Self::Error> {
        let LoweredPlan {
            program,
            sources: used_sources,
            constants,
            outputs,
            schemas,
        } = lower(plan.as_code(), &sources)
            .map_err(|error| BuildError::new(format!("{error:#}")))?;
        // Constants are encoded per commit, against the store's dictionary.
        // Encoding them once here surfaces a value the engine cannot store
        // at build time instead of at the first commit.
        encode_constants(&constants, Dictionary::new())
            .map_err(|error| BuildError::new(format!("{error:#}")))?;
        let mut sources: Vec<String> = used_sources.into_keys().collect();
        sources.sort();
        let sinks = outputs
            .keys()
            .map(|sink| SinkId::from(sink.as_str()))
            .collect();
        Ok(BatchRuntime {
            program,
            outputs,
            schemas,
            sources,
            constants,
            sinks,
            results: None,
        })
    }
}

/// The compiled program and what [`Runtime::output`] needs to read its
/// results; [`BatchRuntime::commit_from`] computes them from the store.
pub struct BatchRuntime {
    program: Program,
    /// Sink id to the derived relation `output` reads.
    outputs: HashMap<String, String>,
    /// Schema (column names and types) per relation, sources and derived
    /// alike.
    schemas: HashMap<String, Schema>,
    /// The base tables the plan reads, every one of them from the store.
    sources: Vec<String>,
    /// The tables the plan states itself (see [`LoweredPlan::constants`]),
    /// as typed rows. Their strings need the store's dictionary, so they
    /// are encoded per commit.
    constants: Vec<ConstantTable>,
    sinks: Vec<SinkId>,
    /// The derived relations of the last commit, with the dictionary that
    /// decodes them.
    results: Option<BatchCatalog>,
}

/// The full current state of a result relation, the natural output of a
/// batch backend (the incremental backend reports deltas instead).
///
/// Rows are sorted and deduplicated: results are sets. Every cell carries
/// the type the plan gave its column.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    columns: Vec<String>,
    rows: Vec<TupleValue>,
}

impl Snapshot {
    fn from_relation(
        columns: Vec<String>,
        relation: &Relation,
        dictionary: &Dictionary,
    ) -> Result<Self, RuntimeError> {
        let mut rows = Vec::with_capacity(relation.len());
        for i in 0..relation.len() {
            let values = relation
                .row_values(i, dictionary)
                .map_err(|error| RuntimeError::new(format!("{error:#}")))?;
            rows.push(TupleValue {
                data: values.into_iter().map(pipeline_value).collect(),
            });
        }
        Ok(Self { columns, rows })
    }

    /// The result's column names, in tuple order.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// The rows, sorted and deduplicated.
    pub fn rows(&self) -> &[TupleValue] {
        &self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The snapshot as a z-set with every weight `+1`, for set-level
    /// comparison against the incremental backend's consolidated output.
    pub fn to_debug_zset(&self) -> OrdZSet<TupleValue> {
        let keys = self
            .rows
            .iter()
            .map(|row| Tup2(row.clone(), 1))
            .collect::<Vec<_>>();
        OrdZSet::from_keys((), keys)
    }
}

impl BatchRuntime {
    /// Compute every output from the store's current tables.
    ///
    /// @Vincent: this is the call the bouncer (or Leo's `adhoc_query`)
    /// makes, once per query, with the store behind `source`. `output`
    /// reads the results afterwards.
    pub fn commit_from(&mut self, source: &dyn RelationSource) -> Result<(), RuntimeError> {
        let result = self.evaluate(source)?;
        self.results = Some(result.catalog);
        Ok(())
    }

    fn evaluate(&self, source: &dyn RelationSource) -> Result<FixpointResult, RuntimeError> {
        let mut handles = HashMap::new();
        for name in &self.sources {
            // @Vincent: one handle per base table the query reads, asked
            // for once per query.
            let handle = source
                .table(&SourceId::from(name.as_str()))
                .ok_or_else(|| RuntimeError::new(format!("the store has no table '{name}'")))?;
            let expected = self.schemas.get(name).map_or(0, Schema::arity);
            if handle.arity() != expected {
                return Err(RuntimeError::new(format!(
                    "base table '{name}': the store's table has width {}, the plan reads width \
                     {expected}",
                    handle.arity()
                )));
            }
            handles.insert(name.as_str(), handle);
        }
        let store = StoreTables {
            handles,
            schemas: &self.schemas,
            dictionary: source.dictionary(),
        };
        let constants = encode_constants(&self.constants, source.dictionary().clone())
            .map_err(|error| RuntimeError::new(format!("{error:#}")))?;
        let input = Layered::new(&constants, &store);
        fixpoint::semi_naive_over(&self.program, &input, generic_join::execute as Exec)
            .map_err(|error| RuntimeError::new(format!("{error:#}")))
    }
}

/// The store's tables as the executors read them: a handle per base table
/// the plan reads, the schemas the plan gave them, and the store's
/// dictionary for their strings.
struct StoreTables<'s> {
    handles: HashMap<&'s str, Box<dyn TableHandle + 's>>,
    schemas: &'s HashMap<String, Schema>,
    dictionary: &'s Dictionary,
}

impl Tables for StoreTables<'_> {
    fn schema(&self, relation: &str) -> anyhow::Result<&Schema> {
        if !self.handles.contains_key(relation) {
            bail!("the plan reads no base table '{relation}'");
        }
        self.schemas
            .get(relation)
            .with_context(|| format!("base table '{relation}' has no schema"))
    }

    fn sorted(&self, relation: &str, order: &[ColId]) -> anyhow::Result<Box<dyn SortedTable + '_>> {
        let arity = self.schema(relation)?.arity();
        let table = self.handles[relation].sorted(order);
        // The cheap part of the contract, checked on every table: the width
        // the plan expects and the order the join asked for.
        if table.arity() != arity || table.sort_order() != order {
            bail!(
                "base table '{relation}': the store served width {} sorted by {:?}, the join \
                 needs width {arity} sorted by {order:?}",
                table.arity(),
                table.sort_order()
            );
        }
        Ok(table)
    }

    fn dictionary(&self) -> &Dictionary {
        self.dictionary
    }
}

/// The plan's constant tables, encoded against `dictionary`. Per commit
/// that is a copy of the store's, so a constant's strings meet the store's
/// under the same keys.
fn encode_constants(
    constants: &[ConstantTable],
    dictionary: Dictionary,
) -> anyhow::Result<BatchCatalog> {
    let mut catalog = BatchCatalog::new();
    *catalog.dictionary_mut() = dictionary;
    for constant in constants {
        catalog.insert_rows(
            constant.name.clone(),
            constant.schema.clone(),
            constant.rows.clone(),
        )?;
    }
    Ok(catalog)
}

impl Runtime for BatchRuntime {
    type Output = Snapshot;
    type Error = RuntimeError;

    /// Refused. This backend reads finished tables from the store and has
    /// no use for deltas; see [`BatchRuntime::commit_from`].
    fn feed(
        &mut self,
        source: &SourceId,
        _rows: impl IntoIterator<Item = ZRow>,
    ) -> Result<bool, Self::Error> {
        Err(RuntimeError::new(format!(
            "the batch backend takes no deltas (source '{source}'): \
             it reads the store's tables in `commit_from`"
        )))
    }

    /// Refused. Without the store there is nothing to compute from; see
    /// [`BatchRuntime::commit_from`].
    fn commit(&mut self) -> Result<(), Self::Error> {
        Err(RuntimeError::new(
            "the batch backend computes from the store's tables: call `commit_from`",
        ))
    }

    fn output(&self, out: &SinkId) -> Result<Snapshot, Self::Error> {
        let Some(results) = &self.results else {
            return Err(RuntimeError::new("commit before reading an output"));
        };
        let Some(relation) = self.outputs.get(out.as_str()) else {
            return Err(RuntimeError::new(format!(
                "unknown output '{}' (print-only CLI taps are not readable)",
                out.as_str()
            )));
        };
        // The engine names rule variables generically (v0, v1, …); the
        // plan's speaking column names live in the schema table.
        let columns = self
            .schemas
            .get(relation)
            .map(Schema::names)
            .unwrap_or_default();
        let relation = results
            .get(relation)
            .map_err(|error| RuntimeError::new(format!("{error:#}")))?;
        Snapshot::from_relation(columns, relation, results.dictionary())
    }

    fn list_outputs(&self) -> impl Iterator<Item = &'_ SinkId> {
        self.sinks.iter()
    }
}
