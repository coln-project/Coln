// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This defines the [API for a transactional store](TxStore).

use super::{deltas::StoreDelta, transaction::ApplicationOutcome};
use std::error::Error;

/// A generic transactional engine/store.
pub trait TxStore {
    type ApplicationOk: Into<ApplicationOutcome>;
    type ApplicationError: Error + Clone;
    type RollbackError: Error + Clone;
    type CommitError: Error + Clone;

    /// Executes and applies the transaction given by the updates in `delta`.
    fn apply(&mut self, delta: StoreDelta) -> Result<Self::ApplicationOk, Self::ApplicationError>;
    /// Undoes the last transaction by rolling back every state change caused
    /// by that transaction. Should only fail in exceptional circumstances.
    fn rollback(&mut self) -> Result<(), Self::RollbackError>;
    /// Commits the last transaction. Possibly, a no-op or do some cleanup.
    /// Should only fail in exceptional circumstances.
    fn commit(&mut self) -> Result<(), Self::CommitError>;
}

// TODO: Implement for the engine!
