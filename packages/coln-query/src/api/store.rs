// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! How coln-store tells us about new base facts: the [API for a transactional
//! store](TxStore), and coln-query's side of it.
//!
//! This is the *push* direction, and the only one the incremental backend needs.
//! A DBSP circuit holds the base data it was fed, so evaluating a standing query
//! never asks anybody for anything. coln-store hands over a [`StoreDelta`], we
//! route it into the circuit's inputs, step, and report what changed downstream.
//!
//! Note which way the trait points. [`TxStore`] is implemented by _this_ crate
//! and called by coln-store, because applying a delta is the engine's operation
//! to define. That makes the whole push path one-directional: nothing in here
//! ever calls back into coln-store.
//!
//! # Why this module is private
//!
//! [`Tx`](crate::api::transaction::Tx) is the only thing that may drive a
//! [`TxStore`]: calling [`apply`](TxStore::apply) on an implementor (such as
//! [`ColnQuery`](crate::api::ColnQuery)) directly would step the circuit
//! behind the typestate machine's back, leaving a transaction half-applied
//! with no rollback guard to undo it.
//!
//! `pub(crate)` is not an option, because the trait appears in the bounds of
//! public items and must stay reachable to the type system. But *reachable* and
//! *nameable* are two different things: a `pub` trait in a private module still
//! discharges those bounds, while a downstream crate has no path to `use` it —
//! and without the trait in scope, no call to its methods resolves. Sealing it
//! the usual way, with a private supertrait, would not do this; that stops
//! foreign `impl`s but not foreign calls.

use super::{deltas::StoreDelta, transaction::TxOutcome};

/// A generic transactional engine/store. Implemented by
/// [`ColnQuery`](super::ColnQuery), callable only from within this crate.
pub trait TxStore {
    type Error: std::error::Error + Clone;

    /// Executes and applies the transaction given by the updates in `delta`.
    fn apply(&mut self, delta: StoreDelta) -> Result<TxOutcome, Self::Error>;
    /// Undoes the last transaction by rolling back every state change caused
    /// by that transaction. Should only fail in exceptional circumstances.
    fn rollback(&mut self) -> Result<(), Self::Error>;
    /// Commits the last transaction. Possibly, a no-op or do some cleanup.
    /// Should only fail in exceptional circumstances.
    fn commit(&mut self) -> Result<(), Self::Error>;
}
