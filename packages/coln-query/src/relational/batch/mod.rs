// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A batch backend optimized for efficient evaluation of non-binary joins.

use super::{Backend, Runtime};
use crate::{
    api::deltas::ZRow,
    error::{BuildError, RuntimeError},
    host::resolver::ResolvedCode,
    relational::{
        catalog::SourceSchemas,
        expr::{SinkId, SourceId},
    },
    scalarial::{ColumnScalarEngine, column::VectorizedScalarEngine},
};
use std::num::NonZeroUsize;

/// The non-incremental backend: evaluates the plan eagerly over materialized
/// Z-sets. Bodies are the next slice of work.
pub struct BatchBackend<E: ColumnScalarEngine = VectorizedScalarEngine> {
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
        _plan: ResolvedCode,
        _sources: SourceSchemas,
    ) -> Result<BatchRuntime, Self::Error> {
        todo!("eager batch backend: build a RelExprVisitor over Z-sets")
    }
}

/// Accumulated source Z-sets + the compiled plan; `commit` recomputes the result.
pub struct BatchRuntime;

/// The full current state of a result relation. The natural output of a batch
/// backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot; // TBD.

impl Runtime for BatchRuntime {
    type Output = Snapshot;
    type Error = RuntimeError;

    fn feed(
        &mut self,
        _source: &SourceId,
        _rows: impl IntoIterator<Item = ZRow>,
    ) -> Result<bool, Self::Error> {
        todo!("eager batch feed: accumulate into source tables")
    }

    fn commit(&mut self) -> Result<(), Self::Error> {
        todo!("eager batch commit: recompute the plan from accumulated inputs")
    }

    fn output(&self, _out: &SinkId) -> Result<Snapshot, Self::Error> {
        todo!("eager batch output: read the materialized result")
    }

    fn list_outputs(&self) -> impl Iterator<Item = &'_ SinkId> {
        // Apparently, todo!() does not work with impl Trait syntax..
        std::iter::empty()
    }
}
