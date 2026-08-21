// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The execution stage: compile a resolved and optimized plan into a runnable
//! computation, and drive it.
//!
//! Two traits split the concern:
//!
//! - [`Backend`] — the last *compile* step: a [`ResolvedCode`] plan → a runnable
//!   artifact. One impl per execution strategy
//!   ([`DbspBackend`](incremental::DbspBackend) incremental,
//!   [`BatchBackend`](batch::BatchBackend) eager).
//! - [`Runtime`] — the runnable artifact: feed input changes, advance, read
//!   results. This is where incremental vs batch actually differ — DBSP's
//!   `commit` runs one incremental transaction and yields per-step [`Delta`]s;
//!   the batch engine recomputes from the accumulated inputs and yields
//!   [`Snapshot`]s.
//!
//! Sources are **not** passed in: the plan is self-describing. Every extensional
//! input is a [`SourceExpr`](crate::relational::expr::SourceExpr) leaf,
//! so [`Backend::build`] discovers and wires them from the plan itself.

pub mod batch;
pub mod expr;
pub mod incremental;
pub mod relation;

use std::num::NonZeroUsize;

use crate::{
    error::{BuildError, RuntimeError},
    host::resolver::ResolvedCode,
    relational::expr::{SinkId, SourceId},
};
use incremental::dbsp::{OrdZSet, ZWeight};
pub use relation::{RelationRef, RelationSchema, RelationType, TupleKey, TupleValue};

/// A change to a result relation since the last [`Runtime::commit`] — a Z-set of
/// ±weighted rows. The natural output of an incremental backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Delta(pub OrdZSet<TupleValue>);

/// The full current state of a result relation. The natural output of a batch
/// backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Snapshot(pub OrdZSet<TupleValue>);

/// The last compile step: a resolved plan → a runnable computation. One impl per
/// execution strategy; the plan and (row) scalar evaluation are shared.
pub trait Backend {
    type Runtime: Runtime;
    type Error: Into<BuildError>;

    fn build(self, threads: NonZeroUsize, plan: ResolvedCode)
    -> Result<Self::Runtime, Self::Error>;
}

/// A runnable computation. `feed` stages input changes, `commit` advances the
/// computation, `output` reads a result. Incremental and batch backends differ
/// only in [`Runtime::Output`] and in how `commit` honors it.
pub trait Runtime {
    /// The natural result form: [`Delta`] (incremental) or [`Snapshot`] (batch).
    type Output;
    /// A runtime error.
    type Error: Into<RuntimeError> + std::fmt::Debug;

    /// Stage input value tuples (with z-weights) for a named source. The tuple
    /// key is derived from the source schema, so only values are supplied.
    fn feed(
        &mut self,
        source: &SourceId,
        rows: impl IntoIterator<Item = (TupleValue, ZWeight)>,
    ) -> Result<(), Self::Error>;
    /// Advance the computation over everything fed since the last commit.
    fn commit(&mut self) -> Result<(), Self::Error>;
    /// Read a result relation by the name of the [`OutputExpr`](expr::OutputExpr)
    /// tap that produced it. Errors if no *readable* output carries that name,
    /// e.g. the name is unknown, or it belongs to a print-only
    /// [`OutputKind::Cli`](expr::OutputKind::Cli) tap. Contains all changes
    /// since the last call to [`commit`](Self::commit).
    fn output(&self, out: &SinkId) -> Result<Self::Output, Self::Error>;
    /// List all [`OutputExpr`](expr::OutputExpr) by their name (a [`SinkId`]).
    fn list_outputs(&self) -> impl Iterator<Item = &'_ SinkId>;
    /// Get an iterator over all known [`outputs`](Self::output). A shortcut for
    /// inquiring all outputs (by calling [`output`](Self::output) for all valid
    /// [`SinkId`]s) for new results after a call to [`commit`](Self::commit).
    fn all_outputs(&self) -> impl Iterator<Item = (&'_ SinkId, Self::Output)> {
        self.list_outputs().map(|sink_id| {
            (
                sink_id,
                self.output(sink_id)
                    .expect("list_outputs impl must only return valid sink ids"),
            )
        })
    }
}
