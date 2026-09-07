// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A batch backend optimized for efficient evaluation of non-binary joins.
//!
//! The non-incremental half of the pipeline. [`Backend::build`] lowers the
//! resolved plan into one coln-batch Datalog program, [`Runtime::feed`]
//! stages rows per source, [`Runtime::commit`] recomputes the whole result
//! eagerly (a semi-naive fixpoint over worst-case-optimal joins), and
//! [`Runtime::output`] hands back the full current state of a sink as a
//! [`Snapshot`]. Where the incremental backend reports deltas, this backend
//! reports states.
//!
//! Value scope of this slice: unsigned integers and booleans, mapped onto
//! the engine's u64 universe.
// TODO(Jan): the remaining scalar types (strings first, via dictionary
// encoding) land after the end-to-end slice is complete.
//!
//! # Interim: base tables arrive by push
//!
//! The pipeline has one input door, [`Runtime::feed`], and it speaks
//! deltas: rows with z-weights, pushed by the store transaction by
//! transaction. That is what the incremental backend needs. A batch
//! backend wants the opposite: pull the full snapshot of every base table
//! at query time. That pull API does not exist in the pipeline yet; the
//! store side of it does (`SortedTableSnapshot` in coln-store).
//!
//! Until it lands, this backend integrates the pushed deltas itself:
//! [`Runtime::feed`] keeps a net z-weight per row and
//! `materialize_sources` turns that into the base tables right before
//! every recomputation. This is correct as long as the runtime sees every
//! delta from the start, which holds for what a batch query is today: one
//! feed-commit-output cycle over a snapshot pushed through `feed`. It is a
//! stopgap, not the design. The store already holds these tables, copying
//! them through `feed` is wasted work, and a runtime created later cannot
//! catch up on deltas it never saw.
// TODO(Jan): replace the push-side integration with the pull API once the
// pipeline offers one. `materialize_sources` is the single seam to swap;
// lowering, fixpoint, and output stay as they are.

mod lowering;

use std::collections::HashMap;
use std::num::NonZeroUsize;

use coln_batch::fixpoint::{self, Exec};
use coln_batch::generic_join;
use coln_batch::query::Catalog as BatchCatalog;
use coln_batch::relation::Relation;
use coln_batch::rule::Program;
use dbsp::{OrdZSet, utils::Tup2};

use self::lowering::{LoweredPlan, lower};
use super::{Backend, Runtime};
use crate::{
    api::deltas::{ZRow, ZWeight},
    error::{BuildError, RuntimeError},
    host::resolver::ResolvedCode,
    relational::{
        catalog::SourceSchemas,
        expr::{SinkId, SourceId},
        relation::TupleValue,
    },
    scalarial::{ColumnScalarEngine, ScalarTypedValue, column::VectorizedScalarEngine},
};

/// The non-incremental backend: lowers the plan to a coln-batch Datalog
/// program at build time and recomputes it eagerly on every commit.
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
            outputs,
            schemas,
        } = lower(plan.as_code(), &sources)
            .map_err(|error| BuildError::new(format!("{error:#}")))?;
        let inputs = used_sources
            .into_keys()
            .map(|source| (source, HashMap::new()))
            .collect();
        let sinks = outputs
            .keys()
            .map(|sink| SinkId::from(sink.as_str()))
            .collect();
        Ok(BatchRuntime {
            program,
            outputs,
            schemas,
            inputs,
            sinks,
            results: None,
        })
    }
}

/// Staged source rows plus the compiled program; [`Runtime::commit`]
/// recomputes the full result from the accumulated inputs.
pub struct BatchRuntime {
    program: Program,
    /// Sink id to the derived relation `output` reads.
    outputs: HashMap<String, String>,
    /// Column names per relation, sources and derived alike.
    schemas: HashMap<String, Vec<String>>,
    /// Per used source: the net z-weight of every row fed so far. This is
    /// the interim snapshot store described in the module docs; the pull
    /// API replaces it.
    inputs: HashMap<String, HashMap<Vec<u64>, ZWeight>>,
    sinks: Vec<SinkId>,
    /// The relations of the last commit.
    results: Option<BatchCatalog>,
}

/// The full current state of a result relation, the natural output of a
/// batch backend (the incremental backend reports deltas instead).
///
/// Rows are sorted and deduplicated: results are sets. Values come back as
/// unsigned integers, matching the value slice the backend accepts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot {
    columns: Vec<String>,
    rows: Vec<TupleValue>,
}

impl Snapshot {
    fn from_relation(columns: Vec<String>, relation: &Relation) -> Self {
        let rows = (0..relation.len())
            .map(|row| TupleValue {
                data: relation
                    .row(row)
                    .into_iter()
                    .map(ScalarTypedValue::Uint)
                    .collect(),
            })
            .collect();
        Self { columns, rows }
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
    /// Build the base tables for one recomputation from the deltas fed so
    /// far: a row is present when its net z-weight is positive.
    ///
    /// Interim, see the module docs. This is the one seam where the pull
    /// API hooks in later: read the store's sorted snapshots instead of
    /// integrating pushed deltas. Nothing downstream needs to change.
    // TODO(Jan): swap for the pull API once the pipeline offers one.
    fn materialize_sources(&self) -> Result<BatchCatalog, RuntimeError> {
        let mut edb = BatchCatalog::new();
        for (source, staged) in &self.inputs {
            let columns = self.schemas.get(source).cloned().unwrap_or_default();
            let mut data: Vec<Vec<u64>> = vec![Vec::new(); columns.len()];
            for (row, weight) in staged {
                match weight {
                    weight if *weight < 0 => {
                        return Err(RuntimeError::new(format!(
                            "source '{source}': a row was deleted more often than inserted \
                             (net weight {weight})"
                        )));
                    }
                    0 => {}
                    // Sets: duplicated insertions collapse into one row.
                    _ => {
                        for (column, value) in data.iter_mut().zip(row) {
                            column.push(*value);
                        }
                    }
                }
            }
            edb.insert(Relation::new(source.clone(), columns, data));
        }
        Ok(edb)
    }
}

impl Runtime for BatchRuntime {
    type Output = Snapshot;
    type Error = RuntimeError;

    fn feed(
        &mut self,
        source: &SourceId,
        rows: impl IntoIterator<Item = ZRow>,
    ) -> Result<bool, Self::Error> {
        // Mirrors the incremental backend: a source the plan does not use
        // is `Ok(false)`, not an error. The caller decides what that means.
        let arity = self.schemas.get(source.as_str()).map_or(0, Vec::len);
        let Some(staged) = self.inputs.get_mut(source.as_str()) else {
            return Ok(false);
        };
        for zrow in rows {
            let weight = zrow.zweight();
            let row = convert_row(source, &zrow.into_row(), arity)?;
            *staged.entry(row).or_insert(0) += weight;
        }
        Ok(true)
    }

    fn commit(&mut self) -> Result<(), Self::Error> {
        let edb = self.materialize_sources()?;
        let result = fixpoint::semi_naive(&self.program, &edb, generic_join::execute as Exec)
            .map_err(|error| RuntimeError::new(format!("{error:#}")))?;
        self.results = Some(result.catalog);
        Ok(())
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
        let columns = self.schemas.get(relation).cloned().unwrap_or_default();
        let relation = results
            .get(relation)
            .map_err(|error| RuntimeError::new(format!("{error:#}")))?;
        Ok(Snapshot::from_relation(columns, relation))
    }

    fn list_outputs(&self) -> impl Iterator<Item = &'_ SinkId> {
        self.sinks.iter()
    }
}

/// Convert one staged row into the engine's u64 universe.
///
/// This slice accepts unsigned integers and booleans; everything else
/// fails loudly instead of computing something wrong.
// TODO(Jan): remaining scalar types after the end-to-end slice.
fn convert_row(
    source: &SourceId,
    row: &TupleValue,
    arity: usize,
) -> Result<Vec<u64>, RuntimeError> {
    if row.data.len() != arity {
        return Err(RuntimeError::new(format!(
            "row for source '{}' has {} values, its schema has {arity} columns",
            source.as_str(),
            row.data.len()
        )));
    }
    row.data
        .iter()
        .map(|value| match value {
            ScalarTypedValue::Uint(value) => Ok(*value),
            ScalarTypedValue::Bool(value) => Ok(u64::from(*value)),
            other => Err(RuntimeError::new(format!(
                "source '{}': the batch backend supports unsigned integer and boolean \
                 values for now, got {other:?}",
                source.as_str()
            ))),
        })
        .collect()
}
