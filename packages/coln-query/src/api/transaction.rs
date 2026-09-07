// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An interface for a [Transaction](Tx). A transaction can be in exactly one
//! state of [Prepare], [Pending], [Committed], [Aborted], [Rejected].

use super::{
    deltas::{DerivedDataDelta, StoreDelta, TableDelta},
    error::UnsafeApplyError,
    store::TxStore,
    violations::{ViolationsDelta, ViolationsSet},
};

/// The query engine's primary API is a transaction represented as a state
/// machine with the following five states:
///
/// 1. [`Prepare`]: Data can be fed into the base tables (EDPs) in
///    row-oriented format.
/// 2. [`Pending`]: The query processing finished and all hard (enforced)
///    constraints are met. The **caller still has to explicitly call
///    [`commit`](Tx<Pending>::commit) to avoid a rollback**, or can call
///    [`abort`](Tx<Pending>::abort) to rollback the transaction for some
///    reason (maybe an end-user requested an abort).
/// 3. [`Committed`]: The transaction cannot be rolled back anymore and has
///    been committed. Any query results (derived views, monitored constraint
///    violations) can now be obtained.
/// 4. [`Aborted`]: For some reason the caller decided to abort.
///    The query engine's state has been rolled back as if the transaction
///    never happened.
/// 5. [`Rejected`]: The transaction could not commit because of some violations
///    of hard (enforced) constraints. Any state caused by the transaction
///    is already rolled back within the query engine. Any violation can be
///    reported back.
///
/// ```text
/// +------------+
/// |   Prepare  |
/// +------+-----+
///        |
///        | try_commit()
///        | (runs query engine and checks hard constraints)
///        |
///        +----------------+
///        |                |
///        met              violated
///        |                |
///        v                v
/// +------------+    +------------+
/// |  Pending   |    |  Rejected  |
/// +------+-----+    +------------+
///        |
///        +----------------+
///        |                |
///        commit()        abort()
///        |                |
///        v                v
/// +------------+    +------------+
/// | Committed  |    |  Aborted   |
/// +------------+    +------------+
/// ```
#[derive(Debug)]
pub struct Tx<State> {
    state: State,
}

/// This is the initial state of a [transaction](Tx) and it is open to receive
/// table deltas.
#[derive(Debug)]
pub struct Prepare {
    delta: StoreDelta,
}

/// The transaction is ready to apply in theory, that is, all _enforced_
/// constraints are met (although some _monitored_ constraints may be violated).
/// Yet, the transaction awaits either an approval or an end user abort. Without
/// an explicit approval, any state change caused by the transaction will be
/// undone.
#[derive(Debug)]
pub struct Pending<'a, Store: TxStore> {
    store: RollbackGuard<'a, Store>,
    delta: DataDelta,
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

/// The transaction is done and fully applied to the query engine. No rollback
/// is possible anymore.
#[derive(Debug)]
pub struct Committed {
    delta: DataDelta,
}

/// The transaction is committable in theory, that is, it does _not_ violate any
/// constraint, _but the caller decided to abort regardless_.
/// Any state caused by the transaction is already rolled back.
#[derive(Debug)]
pub struct Aborted {}

/// The transaction _must be_ rejected because some hard (enforced) constraints
/// are violated. Any state caused by the transaction is already rolled back.
#[derive(Debug)]
pub struct Rejected {
    violations: ViolationsSet,
}

/// The outcomes that can happen if updates are applied to the store:
///
/// 1. [`Self::DerivedDataDelta`], if no hard constraints are violated (but
///    possibly soft constraints).
/// 2. [`Self::HardViolationsSet`], if hard constraints are violated.
///
/// We treat hard constraint violations as perfectly normal use and report them
/// back as part of the `Ok` case of a `Result` and reserve the `Err` case
/// for hard engine errors.
///
/// # A set on one arm, a delta on the other
///
/// [`HardViolationsSet`](Self::HardViolationsSet) reports the violations that
/// *exist*. [`DataDelta`](Self::DerivedDataDelta) reports how the monitored
/// violations *changed*. The rows are the same shape either way, which
/// is why they are [two types](super::violations) rather than one.
/// Which of the two a rule's violations carry follows from what the engine
/// does when one occurs:
///
/// A transaction violating an enforced (hard) constraint is rolled back,
/// so the set of those violations is empty after every committed transaction.
/// Whatever the engine reports is therefore measured against nothing,
/// which makes it the whole set.
///
/// A monitored (soft) violation is tolerated and committed, so *that* set
/// accumulates across transactions, and the engine reports it relative to what
/// was already there: a positive [`ZWeight`](super::deltas::ZWeight) is a
/// violation that appeared, a negative one a violation this transaction resolved.
/// Which is the right shape, because the engine does not keep the set
/// (the caller does, and a caller can only maintain one if needed).
/// To know whether any monitored violation is currently outstanding,
/// apply these deltas to your own set and ask *it* by integrating over the
/// respective `zweight`s.
pub enum TxOutcome {
    /// All constraints are met and updates in derived data are communicated
    /// back next to updates in the soft violations produced by monitored rules.
    DerivedDataDelta(DataDelta),
    /// Enforced constraints are violated. The violations, in full, as a set.
    HardViolationsSet(ViolationsSet),
}

#[derive(Debug, Clone)]
pub struct DataDelta {
    derived: DerivedDataDelta,
    soft_violations: ViolationsDelta,
}

impl DataDelta {
    pub fn new(derived: DerivedDataDelta, soft_violations: ViolationsDelta) -> Self {
        Self {
            derived,
            soft_violations,
        }
    }
    pub fn take_derived_data_delta(&mut self) -> DerivedDataDelta {
        std::mem::take(&mut self.derived)
    }
    pub fn take_soft_violations(&mut self) -> ViolationsDelta {
        std::mem::take(&mut self.soft_violations)
    }
}

impl TryFrom<TxOutcome> for DataDelta {
    type Error = UnsafeApplyError;

    fn try_from(outcome: TxOutcome) -> Result<Self, Self::Error> {
        match outcome {
            TxOutcome::DerivedDataDelta(delta) => Ok(delta),
            TxOutcome::HardViolationsSet(violations) => Err(UnsafeApplyError { violations }),
        }
    }
}

pub enum TryCommitOk<'a, Store: TxStore> {
    Pending(Tx<Pending<'a, Store>>),
    Rejected(Tx<Rejected>),
}

#[cfg(test)]
impl<'a, Store: TxStore + std::fmt::Debug> TryCommitOk<'a, Store> {
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
                    delta,
                },
            })),
            TxOutcome::HardViolationsSet(violations) => {
                store.rollback().map_err(TryCommitErr::RollbackError)?;
                Ok(TryCommitOk::Rejected(Tx {
                    state: Rejected { violations },
                }))
            }
        }
    }
}

impl<Store: TxStore> Tx<Pending<'_, Store>> {
    pub fn commit(self) -> Result<Tx<Committed>, Store::Error> {
        // Plain destructuring: `Pending` has no `Drop` of its own, only its
        // guard field has.
        let Pending { mut store, delta } = self.state;
        // The guard stays armed across the commit, so a commit that fails is
        // undone when the guard drops on the way out — a half-committed
        // transaction is never left behind. Only a commit that succeeded has
        // nothing left to undo.
        store.store().commit()?;
        store.disarm();
        Ok(Tx {
            state: Committed { delta },
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
        self.state.delta.take_derived_data_delta()
    }
    pub fn take_soft_violations(&mut self) -> ViolationsDelta {
        self.state.delta.take_soft_violations()
    }
}

impl Tx<Rejected> {
    pub fn take_hard_violations(&mut self) -> ViolationsSet {
        std::mem::take(&mut self.state.violations)
    }
}
