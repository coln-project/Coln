// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The execution stage: compile a resolved and optimized plan into a runnable
//! computation, and drive it.
//!
//! Two traits split the concern:
//!
//! - [`Backend`] — the last *compile* step: a [`ResolvedCode`] plan → a runnable
//!   artifact, plus the [`lower`](Backend::lower) pass that first restricts the
//!   plan to the operators this backend can execute. One impl per execution
//!   strategy ([`DbspBackend`](incremental::DbspBackend) incremental,
//!   [`BatchBackend`](batch::BatchBackend) eager).
//! - [`Runtime`] — the runnable artifact: feed input changes, advance, read
//!   results. This is where incremental vs batch actually differ — DBSP's
//!   `commit` runs one incremental transaction and yields per-step
//!   [`DbspOutputDelta`](incremental::dbsp::DbspOutputDelta)s; the batch engine
//!   recomputes from the accumulated inputs and yields
//!   [`Snapshot`](batch::Snapshot)s.
//!
//! Which extensional inputs to wire comes from the plan itself, since every one
//! of them is a [`SourceExpr`](crate::relational::expr::SourceExpr) leaf. What
//! those leaves *are* arrives alongside as [`SourceSchemas`] — a leaf names its
//! relation without describing it, so a relation the plan references `N` times
//! is described once. The pipeline resolves that against the program's
//! [`Catalog`](catalog::Catalog) before calling [`Backend::build`].

pub mod batch;
pub mod catalog;
pub mod expr;
pub mod incremental;
pub mod relation;
pub mod schema;

use crate::{
    api::deltas::ZRow,
    error::{BuildError, LoweringError, RuntimeError},
    host::{QueryIr, resolver::ResolvedCode},
    relational::{
        catalog::SourceSchemas,
        expr::{SinkId, SourceId},
    },
};
pub use relation::{RelationRef, TupleValue};
use std::num::NonZeroUsize;

/// The last compile step: a resolved plan → a runnable computation. One impl per
/// execution strategy; the plan and (row) scalar evaluation are shared.
pub trait Backend {
    type Runtime: Runtime;
    type Error: Into<BuildError>;

    /// Rewrite the plan into the operator vocabulary this backend can execute.
    /// The default keeps it as it is, for a backend that supports the full
    /// [`RelExpr`](expr::RelExpr) vocabulary natively.
    ///
    /// Runs after logical optimization and, crucially, *before* resolution, so
    /// a pass may freely mint nodes: the [`Resolver`](crate::host::resolver)
    /// has not assigned variable slots yet, and [`ResolvedCode`] exists to
    /// promise [`build`](Self::build) that they have been.
    ///
    /// This is **not** an [`Optimizer`](crate::optimizer::Optimizer), even
    /// though both are semantics-preserving rewrites of the plan. An optimizer
    /// is chosen independently of the backend and may always decline to do
    /// anything; a lowering is mandatory, and skipping it hands the backend a
    /// node it cannot compile. Correctness must not depend on which optimizer
    /// the pipeline was configured with.
    ///
    /// Takes `&self` rather than `self` so it can run before
    /// [`build`](Self::build) consumes the backend.
    fn lower(&self, plan: QueryIr) -> Result<QueryIr, LoweringError> {
        Ok(plan)
    }

    /// `sources` describes the plan's [`SourceExpr`](expr::SourceExpr) leaves,
    /// which name their relations without describing them.
    ///
    /// A backend receives the *resolved* schemas rather than the
    /// [`Catalog`](catalog::Catalog) they came from: the pipeline consults the
    /// catalog once ([`resolve_sources`](catalog::resolve_sources)) and hands
    /// the result on, so a backend never has to reach back into a frontend's
    /// data structure — and, for an incremental one, could not, since the owned
    /// schemas have to cross into a `Send + 'static` circuit constructor.
    fn build(
        self,
        threads: NonZeroUsize,
        plan: ResolvedCode,
        sources: SourceSchemas,
    ) -> Result<Self::Runtime, Self::Error>;
}

/// A runnable computation. `feed` stages input changes, `commit` advances the
/// computation, `output` reads a result. Incremental and batch backends differ
/// only in [`Runtime::Output`] and in how `commit` honors it.
pub trait Runtime {
    /// The natural result form:
    /// [`DbspOutputDelta`](incremental::dbsp::DbspOutputDelta) (incremental) or
    /// [`Snapshot`](batch::Snapshot) (batch).
    type Output;
    /// A runtime error.
    type Error: Into<RuntimeError> + std::fmt::Debug;

    /// Stage input rows (with z-weights) for a named source. The tuple
    /// key is derived from the source schema, so only the row is supplied.
    /// Returns `Ok(true)` if the input source is known and data has been fed.
    /// Otherwise, it returns `Ok(false)`.
    #[must_use = "Do not miss a missed update"]
    fn feed(
        &mut self,
        source: &SourceId,
        rows: impl IntoIterator<Item = ZRow>,
    ) -> Result<bool, Self::Error>;

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
    /// inquiring all outputs (through calling [`output`](Self::output) for
    /// all valid [`SinkId`]s) for new results after a call to
    /// [`commit`](Self::commit).
    fn all_outputs(&self) -> impl Iterator<Item = (&'_ SinkId, Self::Output)> {
        self.list_outputs().map(|sink_id| {
            (
                sink_id,
                self.output(sink_id)
                    .expect("list_outputs() impl must only return valid sink ids"),
            )
        })
    }
}
