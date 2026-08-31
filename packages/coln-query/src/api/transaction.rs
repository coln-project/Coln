// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An interface for a [Transaction](Tx). A transaction can be in exactly one
//! state of [Prepare], [Pending], [Committed], [Aborted], [Rejected].

use crate::api::error::{ColnQueryError, UnsafeApplyError};

use super::{
    deltas::{DerivedDataDelta, StoreDelta, TableDelta},
    store::TxStore,
    violations::Violations,
};

/// We use the Typestate-Pattern for compile-time enforced transaction states
/// and their transitions to provide a hard-to-abuse API.
#[derive(Debug)]
pub struct Tx<State> {
    state: State,
}

/// This is the initial state of a Transaction and it is open to receive table
/// deltas.
#[derive(Debug)]
pub struct Prepare {
    delta: StoreDelta,
}

/// The transaction is ready to apply in theory, that is, all _mandatory_
/// constraints are met (although some _monitored_ constraints may be violated).
/// Yet, the transaction awaits either an approval or an end user abort. Without
/// an explicit approval, any state change caused by the transaction will be
/// undone.
#[derive(Debug)]
pub struct Pending<'a, Store: TxStore> {
    store: RollbackGuard<'a, Store>,
    derived_data_delta: DerivedDataDelta,
    soft_violations: Violations,
}

/// Rolls the store back when dropped, unless it has been
/// [`disarm`](Self::disarm)ed first. This is what makes the API foolproof: a
/// caller who neither commits nor aborts gets the conservative outcome.
///
/// The guard is a field of [`Pending`] rather than [`Pending`] itself carrying
/// the [`Drop`] impl, and that is the whole point of it existing. A type that
/// implements `Drop` cannot be destructured — Rust has to keep it whole to hand
/// it to `drop` — so `commit` and `abort` would have to move each field out from
/// under the destructor by hand, with a `ManuallyDrop` and one `ptr::read` per
/// field. Confining `Drop` to the one field that actually needs it leaves
/// `Pending` an ordinary struct that can be taken apart safely, and adding a
/// field to it costs nothing.
#[derive(Debug)]
struct RollbackGuard<'a, Store: TxStore> {
    store: &'a mut Store,
    armed: bool,
}

impl<'a, Store: TxStore> RollbackGuard<'a, Store> {
    fn armed(store: &'a mut Store) -> Self {
        Self { store, armed: true }
    }
    /// The store, with the guard left armed, so an operation that fails still
    /// rolls back on the way out.
    fn store(&mut self) -> &mut Store {
        self.store
    }
    /// Stop the implicit rollback: from here on dropping the guard does nothing.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl<Store: TxStore> Drop for RollbackGuard<'_, Store> {
    fn drop(&mut self) {
        if self.armed {
            // Happens in best-effort manner to avoid panicking in Drop impls.
            // Possibly log the event, though.
            let _ = self.store.rollback();
        }
    }
}

/// The transaction is finalized and applied to both the storage and query
/// engine. Any state caused by the transaction is already committed.
#[derive(Debug)]
pub struct Committed {
    derived_data_delta: DerivedDataDelta,
    soft_violations: Violations,
}

/// The transaction is committable in theory, that is, it does _not_ violate any
/// constraint but the end user decided to abort regardless. Any state caused by
/// the transaction is already rolled back.
#[derive(Debug)]
pub struct Aborted {}

/// The transaction _must be_ rejected because some _mandatory_ constraints are
/// violated. Any state caused by the transaction is already rolled back.
#[derive(Debug)]
pub struct Rejected {
    violations: Violations,
}

/// The outcomes that can happen if updates are applied to the store:
///
/// 1. [`Self::DerivedDataDelta`], if no constraints are violated.
/// 2. [`Self::HardViolations`], if mandatory constraints are violated.
/// 3. [`Self::SoftViolations`], if monitored constraints are violated.
///
/// We treat constraint violations as perfectly normal use and report them back
/// as part of the `Ok` case of a `Result` and reserve the `Err` case for hard
/// engine errors.
pub enum TxOutcome {
    /// All constraints are met and updates in derived data are communicated
    /// back.
    DerivedDataDelta(DerivedDataDelta),
    /// Mandatory constraints are violated.
    HardViolations(Violations),
    /// Monitored constraints are violated. Since they only issue a warning but
    /// are tolerated in general, we nevertheless apply the transaction, obtain
    /// the derived data delta, and report back about the violations.
    SoftViolations(DerivedDataDelta, Violations),
}

/// Unlike [`TxOutcome`] this omits the case of [`TxOutcome::HardViolations`]
/// because the transaction data is assumed to have been validated in some
/// previous applications.
pub enum UnsafeTxOutcome {
    /// All constraints are met and updates in derived data are communicated
    /// back.
    DerivedDataDelta(DerivedDataDelta),
    /// Monitored constraints are violated. Since they only issue a warning but
    /// are tolerated in general, we nevertheless apply the transaction, obtain
    /// the derived data delta, and report back about the violations.
    SoftViolations(DerivedDataDelta, Violations),
}

impl TryFrom<TxOutcome> for UnsafeTxOutcome {
    type Error = ColnQueryError;

    fn try_from(value: TxOutcome) -> Result<Self, Self::Error> {
        match value {
            TxOutcome::HardViolations(violations) => {
                Err(ColnQueryError::UnsafeApply(UnsafeApplyError { violations }))
            }
            TxOutcome::SoftViolations(derived_data_delta, soft_violations) => Ok(
                UnsafeTxOutcome::SoftViolations(derived_data_delta, soft_violations),
            ),
            TxOutcome::DerivedDataDelta(derived_data_delta) => {
                Ok(UnsafeTxOutcome::DerivedDataDelta(derived_data_delta))
            }
        }
    }
}

pub enum TryCommitOk<'a, Store: TxStore> {
    Pending(Tx<Pending<'a, Store>>),
    Rejected(Tx<Rejected>),
}

impl<'a, Store: TxStore + std::fmt::Debug> TryCommitOk<'a, Store> {
    #[cfg(test)]
    pub fn expect_pending_and_commit(self) -> Tx<Committed> {
        match self {
            TryCommitOk::Pending(pending) => pending.commit().expect("valid tx"),
            TryCommitOk::Rejected(mut rejected) => {
                panic!(
                    "Expected valid tx but got hard constraint {}",
                    rejected.take_hard_violations()
                )
            }
        }
    }
    #[cfg(test)]
    pub fn expect_rejected(self) -> Tx<Rejected> {
        match self {
            TryCommitOk::Rejected(rejected) => rejected,
            TryCommitOk::Pending(pending) => {
                panic!("Expected invalid tx but got pending {:?}", pending)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum TryCommitErr<Error> {
    TxApplyError(Error),
    RollbackError(Error),
}

impl<Error: std::error::Error + Send + Sync + 'static> From<TryCommitErr<Error>> for anyhow::Error {
    fn from(value: TryCommitErr<Error>) -> Self {
        match value {
            TryCommitErr::TxApplyError(err) | TryCommitErr::RollbackError(err) => {
                anyhow::Error::new(err)
            }
        }
    }
}

impl Tx<Prepare> {
    pub fn empty() -> Self {
        Tx {
            state: Prepare {
                delta: StoreDelta::empty(),
            },
        }
    }
    pub fn new(store_delta: StoreDelta) -> Self {
        Tx {
            state: Prepare { delta: store_delta },
        }
    }
    /// Convenience method to add data beyond initialization.
    pub fn insert<I: IntoIterator<Item = TableDelta>>(&mut self, deltas: I) {
        self.state.delta.extend(deltas);
    }
    pub fn try_commit<'a, Store: TxStore>(
        self,
        store: &'a mut Store,
    ) -> Result<TryCommitOk<'a, Store>, TryCommitErr<Store::Error>> {
        match store
            .apply(self.state.delta)
            .map_err(TryCommitErr::TxApplyError)?
        {
            TxOutcome::DerivedDataDelta(delta) => Ok(TryCommitOk::Pending(Tx {
                state: Pending {
                    store: RollbackGuard::armed(store),
                    derived_data_delta: delta,
                    soft_violations: Violations::empty(),
                },
            })),
            TxOutcome::HardViolations(violations) => {
                store.rollback().map_err(TryCommitErr::RollbackError)?;
                Ok(TryCommitOk::Rejected(Tx {
                    state: Rejected { violations },
                }))
            }
            TxOutcome::SoftViolations(delta, violations) => Ok(TryCommitOk::Pending(Tx {
                state: Pending {
                    store: RollbackGuard::armed(store),
                    derived_data_delta: delta,
                    soft_violations: violations,
                },
            })),
        }
    }
}

impl<Store: TxStore> Tx<Pending<'_, Store>> {
    pub fn commit(self) -> Result<Tx<Committed>, Store::Error> {
        // Plain destructuring: `Pending` has no `Drop` of its own, only its
        // guard field has.
        let Pending {
            mut store,
            derived_data_delta,
            soft_violations,
        } = self.state;
        // The guard stays armed across the commit, so a commit that fails is
        // undone when the guard drops on the way out — a half-committed
        // transaction is never left behind. Only a commit that succeeded has
        // nothing left to undo.
        store.store().commit()?;
        store.disarm();
        Ok(Tx {
            state: Committed {
                derived_data_delta,
                soft_violations,
            },
        })
    }
    pub fn abort(self) -> Result<Tx<Aborted>, Store::Error> {
        // The two deltas are simply dropped here, as an aborted transaction has
        // no results to report.
        let Pending { mut store, .. } = self.state;
        // Disarmed *before* the rollback, unlike [`commit`](Self::commit) above:
        // the guard's own job is to roll back, so leaving it armed here would
        // only retry — inside a destructor, and with the error already reported —
        // an operation that just failed.
        store.disarm();
        store.store().rollback()?;
        Ok(Tx { state: Aborted {} })
    }
}

impl Tx<Committed> {
    pub fn take_derived_data_delta(&mut self) -> DerivedDataDelta {
        std::mem::take(&mut self.state.derived_data_delta)
    }
    pub fn take_soft_violations(&mut self) -> Violations {
        std::mem::take(&mut self.state.soft_violations)
    }
}

impl Tx<Rejected> {
    pub fn take_hard_violations(&mut self) -> Violations {
        std::mem::take(&mut self.state.violations)
    }
}
