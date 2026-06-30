// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An interface for a [Transaction](Tx). A transaction can be in exactly one
//! state of [Prepare], [Pending], [Committed], [Aborted], [Rejected].

use super::{
    deltas::{DerivedDataDelta, StoreDelta, TableDelta},
    store::TxStore,
    violations::Violations,
};

/// We use the Typestate-Pattern for compile-time enforced transaction states
/// and their transitions to provide a hard-to-abuse API.
pub struct Tx<State> {
    state: State,
}

/// This is the initial state of a Transaction and it is open to receive table
/// deltas.
pub struct Prepare {
    delta: StoreDelta,
}

/// The transaction is ready to apply in theory, that is, all _mandatory_
/// constraints are met (although some _monitored_ constraints may be violated).
/// Yet, the transaction awaits either an approval or an end user abort. Without
/// an explicit approval, any state change caused by the transaction will be
/// undone.
pub struct Pending<'a, Store: TxStore> {
    store: &'a mut Store,
    derived_data_delta: DerivedDataDelta,
    soft_violations: Violations,
}

/// The transaction is finalized and applied to both the storage and query
/// engine. Any state caused by the transaction is already committed.
pub struct Committed {
    derived_data_delta: DerivedDataDelta,
}

/// The transaction is committable in theory, that is, it does _not_ violate any
/// constraint but the end user decided to abort regardless. Any state caused by
/// the transaction is already rolled back.
pub struct Aborted {}

/// The transaction _must be_ rejected because some _mandatory_ constraints are
/// violated. Any state caused by the transaction is already rolled back.
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
pub enum ApplicationOutcome {
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

pub enum TryCommitOk<'a, Store: TxStore> {
    Pending(Tx<Pending<'a, Store>>),
    Rejected(Tx<Rejected>),
}

pub enum TryCommitErr<ApplicationError, RollbackError> {
    ApplicationError(ApplicationError),
    RollbackError(RollbackError),
}

impl Tx<Prepare> {
    pub fn new(store_delta: StoreDelta) -> Self {
        Tx {
            state: Prepare { delta: store_delta },
        }
    }
    /// Convenience method to add data beyond initialization.
    pub fn insert<I: IntoIterator<Item = TableDelta>>(&mut self, deltas: I) {
        self.state.delta.inner.extend(deltas);
    }
    pub fn try_commit<'a, Store: TxStore>(
        self,
        store: &'a mut Store,
    ) -> Result<TryCommitOk<'a, Store>, TryCommitErr<Store::ApplicationError, Store::RollbackError>>
    {
        match store
            .apply(self.state.delta)
            .map_err(TryCommitErr::ApplicationError)
            .map(Into::<ApplicationOutcome>::into)?
        {
            ApplicationOutcome::DerivedDataDelta(delta) => Ok(TryCommitOk::Pending(Tx {
                state: Pending {
                    store,
                    derived_data_delta: delta,
                    soft_violations: Violations::none(),
                },
            })),
            ApplicationOutcome::HardViolations(violations) => {
                store.rollback().map_err(TryCommitErr::RollbackError)?;
                Ok(TryCommitOk::Rejected(Tx {
                    state: Rejected { violations },
                }))
            }
            ApplicationOutcome::SoftViolations(delta, violations) => Ok(TryCommitOk::Pending(Tx {
                state: Pending {
                    store,
                    derived_data_delta: delta,
                    soft_violations: violations,
                },
            })),
        }
    }
}

impl<'a, Store: TxStore> Tx<Pending<'a, Store>> {
    pub fn commit(self) -> Result<Tx<Committed>, Store::CommitError> {
        // Prevent the custom Drop implementation from running at this point.
        let md = std::mem::ManuallyDrop::new(self.state);
        // Move the store ref. This is safe because `md` will never be dropped,
        // so we avoid a double-free, *and* because the returned `Committed`
        // state inherits the same lifetime as `self`.
        let store = unsafe { std::ptr::read(&md.store) };
        // Move the vector. This is safe because `md` will never be dropped, so
        // we avoid a double-free.
        let derived_data_delta = unsafe { std::ptr::read(&md.derived_data_delta) };
        store.commit()?;
        Ok(Tx {
            state: Committed { derived_data_delta },
        })
    }
    pub fn abort(self) -> Result<Tx<Aborted>, Store::RollbackError> {
        // Prevent the custom Drop implementation from running at this point.
        let md = std::mem::ManuallyDrop::new(self.state);
        // Move the store ref. This is safe because `md` will never be dropped,
        // so we avoid a double-free, *and* because the returned `Aborted`
        // state inherits the same lifetime as `self`.
        let store = unsafe { std::ptr::read(&md.store) };
        // Move the vector. This is safe because `md` will never be dropped, so
        // we avoid a double-free, but also required to free the heap allocation
        // behind the vector.
        let derived_data_delta = unsafe { std::ptr::read(&md.derived_data_delta) };
        store.rollback()?;
        Ok(Tx { state: Aborted {} })
    }
}

// This is to make the API foolproof: If the caller does neither commit nor
// abort the transaction, we take the conservative approach and rollback any
// state change caused by it.
impl<'a, Store: TxStore> Drop for Pending<'a, Store> {
    fn drop(&mut self) {
        // Happens in best-effort manner to avoid panicking in Drop impls.
        // Possibly log the event, though.
        let _ = self.store.rollback();
    }
}
