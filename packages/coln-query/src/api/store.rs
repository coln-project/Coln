// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! How coln-store tells us about new base facts: the [API for a transactional
//! store](TxStore), and the engine's side of it.
//!
//! This is the *push* direction, and the only one the incremental backend needs.
//! A DBSP circuit holds the base data it was fed, so evaluating a standing query
//! never asks anybody for anything — coln-store hands over a [`StoreDelta`], we
//! route it into the circuit's inputs, step, and report what changed downstream.
//!
//! Note which way the trait points. [`TxStore`] is implemented by *this* crate
//! and called by coln-store, because applying a delta is the engine's operation
//! to define. That makes the whole push path one-directional: nothing in here
//! ever calls back into coln-store.

use super::{deltas::StoreDelta, transaction::TxOutcome};

/// A generic transactional engine/store.
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
