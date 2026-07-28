// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The scalar evaluation is on the hot path, hence, performance critical.
//!
//! Relational operators evaluate scalar fragments once per tuple (a selection
//! condition, a projection attribute, a join key). By the host/relational split
//! invariant, no [`RelExpr`](crate::relational::expr::RelExpr) node ever appears
//! inside such a fragment, so scalar evaluation is a closed unit with no
//! knowledge of relations, streams, or backends.
//!
//! Two independent axes govern how a scalar engine works. Keeping them apart
//! matters, because conflating them suggests upgrade paths that do not exist.
//!
//! 1. **Data granularity per dispatch** — one value at a time, or a batch of `N`.
//!    This axis is the **protocol**, expressed as a choice of trait.
//! 2. **Dispatch representation** — a recursive walk over the AST, a flattened
//!    bytecode program, or compiled closures / JIT-ed machine code. This axis is
//!    a per-engine implementation detail, expressed through the engine's
//!    [`Program`](RowScalarEngine::Program) associated type. The trait signatures
//!    accommodate every option, so no protocol prescribes one.
//!
//! The two axes are orthogonal: a vectorized tree-walker is a perfectly coherent
//! (and industry-standard) engine, as is a row-at-a-time bytecode VM.
//!
//! Each protocol is a concrete trait with fixed (non-generic) data types, so a
//! [`backend`](crate::relational::Backend) selects an engine with a plain bound
//! (`E: RowScalarEngine`):
//!
//! - [`RowScalarEngine`]: row-at-a-time, that is, one tuple context in,
//!   one `Value` out. The
//!   [incremental backend](crate::relational::incremental::DbspBackend)
//!   uses this approach. `TreeWalk` today; a bytecode VM is one option on the
//!   dispatch axis, attractive chiefly *because* evaluation here stays
//!   row-at-a-time — flattening the AST is how a row engine drives down its
//!   per-value interpretive overhead. Both impls are drop-in because they share
//!   this exact signature.
//! - [`ColumnScalarEngine`]: vectorized, that is, a column batch in,
//!   a column/mask out. Driven by a future vectorized backend. Its first
//!   implementor is expected to be a *vectorized tree-walker*, not a VM:
//!   batching already amortizes dispatch over `N` values, which is the very cost
//!   bytecode exists to reduce, so the case for a VM largely evaporates once
//!   evaluation goes columnar.
//!
//! Because the two protocols feed different data, the type system enforces
//! backend↔engine compatibility for free: a vectorized engine simply is not a
//! `RowScalarEngine`, so it cannot be handed to the DBSP backend.

pub mod column;
pub mod row;
pub mod scalar;

pub use column::ColumnScalarEngine;
pub use row::{RowScalarEngine, TreeWalk};
pub use scalar::{ScalarType, ScalarTypedValue};
