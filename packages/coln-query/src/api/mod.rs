// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module defines the public API of the query engine intended to be mainly
//! used by `coln-store`. See the other module level documentations.

use crate::{
    api::{
        deltas::{DerivedDataDelta, StoreDelta, TableDelta, ZRowIterExt},
        error::ColnQueryError,
        store::TxStore,
        transaction::{TxOutcome, UnsafeTxOutcome},
        violations::{ViolationsDelta, ViolationsSet},
    },
    error::{QueryEngineError, RuntimeError},
    pipeline::Pipeline,
    relational::{Runtime, expr::SourceId, incremental::DbspRuntime},
};
use coln_flir_rs::ir::{self, FlatRealm};
use query::FlirProgram;

pub mod deltas;
pub mod error;
pub mod query;
pub mod schema;
pub mod store;
pub mod transaction;
pub mod violations;

/// The main entrypoint for coln-store.
///
/// Keeps the program next to the runnable artifact built from it, because
/// [`take_code`](crate::program::QueryProgram::take_code) empties a program's
/// *statements* while leaving its per-rule metadata behind which is what says
/// whether a violated rule aborts a transaction or merely warns,
/// and what schema to rebuild a [`TableDelta`] by.
/// It doubles as the [`Catalog`](crate::relational::catalog::Catalog)
/// an ad-hoc query is compiled against.
#[derive(Debug)]
pub struct ColnQuery {
    flir_program: FlirProgram,
    /// The standing incremental computation over every rule declared at init.
    incremental_runtime: DbspRuntime,
    /// If a transaction is currently being applied, this is `Some(StoreDelta)`.
    ongoing_tx: Option<StoreDelta>,
}

impl ColnQuery {
    pub fn init(realm: &FlatRealm) -> Result<Self, QueryEngineError> {
        let flir_program = FlirProgram::from_flat_realm(realm)?;
        Self::with_flir_program(flir_program)
    }
    fn with_flir_program(mut flir_program: FlirProgram) -> Result<Self, QueryEngineError> {
        let incremental_runtime = Pipeline::incremental().runtime(&mut flir_program)?;
        Ok(Self {
            flir_program,
            incremental_runtime,
            ongoing_tx: None,
        })
    }
    /// Compile one ad-hoc query and evaluate it on the batch backend.
    ///
    /// The query is a conjunctive query's worth of [`ir::Prop`]s rather than a
    /// whole realm, so it lowers through the same machinery a rule's body does.
    /// What comes back is a [`Snapshot`](crate::relational::batch::Snapshot)
    /// and not a [`Delta`](crate::relational::incremental::dbsp::DbspOutputDelta):
    /// there is no previous state of an ad-hoc query for a delta to be relative to.
    fn adhoc_query(&mut self, query: Vec<ir::Prop>) -> Result<(), QueryEngineError> {
        // ```ignore
        // pub fn adhoc_query(
        //     &mut self,
        //     query: Vec<ir::Prop>,
        //     source: &dyn RelationSource, <-- This is TBD.
        // ) -> Result<Snapshot, QueryEngineError>
        // ```
        //
        // Adhoc queries need to have a catalog available with both base tables
        // and derived views..
        //
        // It answers for **derived views as well as base tables**. A view's rows
        // already travel back to coln-store as a `DerivedDataDelta`, so coln-store
        // holds them and can serve them like any table — which is what lets an
        // ad-hoc query read one, and is why the trait is not named for tables.
        //
        // That has a consequence for the catalog, and it is the part to settle
        // first. An ad-hoc plan names a view with a `SourceExpr` leaf, because
        // nothing binds it to a host variable the way a rule's statement does — but
        // `impl Catalog for FlirProgram` answers for base tables *only*, deliberately
        // (see its docs). The fix is not to widen that impl, which would let a rule
        // body reference a view as a source and bypass the circuit, but to add a
        // second view of the same program for ad-hoc compilation:
        // `fn adhoc_catalog(&self) -> impl Catalog + '_`, answering from
        // `base_tables` and then from `derived_views`, whose `RuleMeta` already
        // carries the `output_schema` such a leaf needs.
        //
        // Which also means a rule name and a base table name must not collide.
        // `base_table` and `rule_declaration` each reject duplicates within their own
        // map, and nothing checks across the two — harmless while the two namespaces
        // are separate, ambiguous the moment one catalog answers from both.
        todo!("Run adhoc query on batch query engine");
    }
    /// This intended for use during restarts. We already know that the data
    /// we are feeding in fulfills all constraints, so the bookkeeping to
    /// potentially undo a transaction can be skipped.
    ///
    /// # Correctness
    ///
    /// **Never** use this for unchecked inputs, otherwise, constraints may be
    /// violated without notice. If this returns an `Err`, the engine is in
    /// an unrecoverable state and it indicates a genuine bug. [`ColnQuery`]
    /// has to be recreated to recover from this.
    pub fn unsafe_apply(&mut self, delta: StoreDelta) -> Result<UnsafeTxOutcome, ColnQueryError> {
        self.internal_apply(delta)?;
        UnsafeTxOutcome::try_from(self.interpret_outputs()?)
    }
    fn internal_apply(&mut self, delta: StoreDelta) -> Result<(), QueryEngineError> {
        for delta in delta.into_table_deltas() {
            let source_id = SourceId::from(delta.for_entity());
            let delta = delta.into_delta().into_iter();
            let effective = self.incremental_runtime.feed(&source_id, delta)?;
            if !effective {
                // TODO: Find a solution for logging.
                println!("Delta references unknown source '{}'", source_id);
            }
        }
        self.incremental_runtime.commit()?;
        Ok(())
    }
    /// Drain every readable output, discarding what they hold. For resetting the
    /// handles after a commit whose results must not be observed, e.g., a rollback.
    fn discard_outputs(&self) {
        for (_sink_id, _delta) in self.incremental_runtime.all_outputs() {}
    }
    fn interpret_outputs(&mut self) -> Result<TxOutcome, QueryEngineError> {
        let mut hard_violations = ViolationsSet::empty();
        let mut soft_violations = ViolationsDelta::empty();
        let mut derived_data_delta = DerivedDataDelta::empty();
        // We must drain all outputs first, so upon short-circuiting due to an
        // error while processing below, we have absorbed all effects of the
        // ongoing commit and they don't leak into the next commit.
        let drained: Vec<_> = self.incremental_runtime.all_outputs().collect();
        for (sink_id, delta) in drained {
            let sink_meta = self.flir_program.sink_meta(sink_id).ok_or_else(|| {
                RuntimeError::new(format!(
                    "Bug: FLIR program does not know output sink {}",
                    sink_id
                ))
            })?;
            let delta = TableDelta::new(sink_id, delta.as_zrows().collect());
            if delta.is_empty() {
                continue;
            }
            match sink_meta.kind() {
                // How to deal with the schema mismatch between coln-query,
                // coln-store, and coln-compiler? Reporting may require the
                // latter view, while coln-store may want to store it in its
                // view. Looks like we need transformations in all directions...
                ir::RuleVariant::Enforced => {
                    // An enforced rule's violation set is empty after every
                    // committed transaction, because any transaction that violates
                    // one is rolled back. So there is never a hard violation
                    // for a transaction to retract, and a negative zweight here
                    // means that an invariant broke, that is, some path fed the
                    // circuit without checking: `unsafe_apply` returning an
                    // `UnsafeApplyError` is the one that can.
                    debug_assert!(
                        delta.iter().retractions().next().is_none(),
                        "enforced rule {} retracts a violation it never reported",
                        delta.for_entity()
                    );
                    hard_violations.extend(Some(delta));
                }
                ir::RuleVariant::Monitored => {
                    soft_violations.extend(Some(delta));
                }
                ir::RuleVariant::Chased => {
                    derived_data_delta.extend(Some(delta));
                }
            }
        }
        // Both guards are the same emptiness check, but they answer different
        // questions, and [`TxOutcome`] documents why: for an enforced rule the
        // circuit reports against an empty set, so a non-empty delta *is* the
        // violation set; for a monitored one it reports against what previous
        // transactions left behind, so a non-empty delta means the set changed.
        // Which is why the retractions are passed on rather than filtered out.
        if !hard_violations.is_empty() {
            return Ok(TxOutcome::HardViolationsSet(hard_violations));
        }
        if !soft_violations.is_empty() {
            return Ok(TxOutcome::SoftViolationsDelta(
                derived_data_delta,
                soft_violations,
            ));
        }
        Ok(TxOutcome::DerivedDataDelta(derived_data_delta))
    }
}

impl TxStore for ColnQuery {
    type Error = ColnQueryError;

    fn apply(&mut self, delta: StoreDelta) -> Result<TxOutcome, Self::Error> {
        // Prepare to potentially undo the transaction.
        self.ongoing_tx = Some(delta.clone());
        self.internal_apply(delta)?;
        self.interpret_outputs().map_err(ColnQueryError::from)
    }

    fn rollback(&mut self) -> Result<(), Self::Error> {
        let retracted_delta = if let Some(tx) = self.ongoing_tx.take() {
            tx.retract()
        } else {
            return Ok(());
        };
        self.internal_apply(retracted_delta)?;
        self.discard_outputs();
        Ok(())
    }

    fn commit(&mut self) -> Result<(), Self::Error> {
        self.ongoing_tx = None;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        api::{
            deltas::{TableDelta, ZRow},
            transaction::{TryCommitErr, TryCommitOk, Tx},
        },
        test_helper::{
            self,
            graph_flir::{self, Entity, JsonFlir},
        },
    };
    use anyhow::{Error, Result};

    /// @Vincent, does this API work for you?
    fn main_usage() -> Result<(), Error> {
        let flat_realm = FlatRealm {
            tables: vec![],
            rules: vec![],
        };
        let mut coln_query = ColnQuery::init(&flat_realm)?;

        let mut tx = Tx::new(StoreDelta::empty());
        // You can add deltas for tables until you call try_commit() on the tx.
        tx.insert(std::iter::once(TableDelta::new("SomeTable", vec![])));

        // If you are ready, try committing the transaction and pass in the handle:
        match tx.try_commit(&mut coln_query) {
            Ok(TryCommitOk::Pending(pending)) => {
                // Call either commit or abort on `pending`. If commit() isn't
                // called the tx is rolled back upon dropping `pending`!
                let mut committed = pending.commit()?;
                // This is how you can access the results:
                let derived_data_delta = committed.take_derived_data_delta();
                let monitored_constraints = committed.take_soft_violations();
                // Commit the tx in coln-store, too, and call it a day :)
                //
                // If for some reason you don't want to commit the transaction
                // you should better explicitly call abort() (and deal with
                // the potential error) instead of relying on the fallback
                // mechanism on dropping `pending`.
                // This is how you can call abort:
                // let aborted = pending.abort()?;
            }
            Ok(TryCommitOk::Rejected(mut rejected)) => {
                // A hard constraint has been violated.
                // At this point in time coln-query has already rolled back
                // its internal state.
                // In this arm, coln-store must roll back to keep the two in
                // sync and report back the violations:
                let violations = rejected.take_hard_violations();
            }
            // The next two can also be combined, as they share the same error
            // type and should be considered a bug, I guess.
            // Err(TryCommitErr::TxApplyError(err)) | Err(TryCommitErr::RollbackError(err)) => {}
            Err(TryCommitErr::TxApplyError(err)) => {
                // An error while trying to apply the tx.
            }
            Err(TryCommitErr::RollbackError(err)) => {
                // An error during the rollback of the pending tx.
            }
        };

        Ok(())
    }

    /// @Vincent, does this API work for you?
    fn restart_usage() -> Result<(), Error> {
        let flat_realm = FlatRealm {
            tables: vec![],
            rules: vec![],
        };
        let mut coln_query = ColnQuery::init(&flat_realm)?;

        // In a previous session all data we feed in here must have gone through
        // the transaction dance shown above. Make sure to feed in the whole DB
        // upon restarting.
        let trusted_history = StoreDelta::empty();

        match coln_query.unsafe_apply(trusted_history) {
            Ok(UnsafeTxOutcome::DerivedDataDelta(derived_data_delta)) => {
                // Update coln-store with the derived delta.
            }
            Ok(UnsafeTxOutcome::SoftViolationsDelta(derived_data_delta, soft_violations)) => {
                // Update coln-store with the derived delta and report back the
                // soft violations of monitored rules.
            }
            Err(err) => {
                // This is bad: If it is an UnsafeApplyError, a hard constraint
                // has been violated but the data should have been checked during
                // a previous run. A query engine error indicates a bug, too.
            }
        };

        Ok(())
    }

    #[test]
    fn graph_flir() -> Result<()> {
        let mut graph_flir = test_helper::graph_flir::GraphFlir::init();
        let flat_realm = graph_flir.load();
        let flir_program = FlirProgram::from_flat_realm(&flat_realm)?;
        let mut coln_query = ColnQuery::with_flir_program(flir_program)?;

        let mut tx0 = Tx::empty();
        let v0 = graph_flir.insert_vertex();
        let v1 = graph_flir.insert_vertex();
        let v2 = graph_flir.insert_vertex();
        tx0.insert(graph_flir.next_epoch().into_table_deltas());
        let mut tx0 = tx0.try_commit(&mut coln_query)?.expect_pending_and_commit();
        assert!(tx0.take_derived_data_delta().is_empty());
        assert!(tx0.take_soft_violations().is_empty());

        let mut tx1 = Tx::empty();
        let e0 = graph_flir.insert_edge(&v0, &v1);
        let e1 = graph_flir.insert_edge(&v1, &v2);
        tx1.insert(graph_flir.next_epoch().into_table_deltas());
        let mut tx1 = tx1.try_commit(&mut coln_query)?.expect_pending_and_commit();
        assert!(tx1.take_derived_data_delta().is_empty());
        assert!(tx1.take_soft_violations().is_empty());

        let mut tx2 = Tx::empty();
        // Just some ints which haven't been used yet for sure.
        let dangling_hash = 999;
        let dangling_ctr = 999;
        // Although the vertex does not violate a contraint, this vertex must be
        // rolled back because tx3 is invalid due to the other inserts.
        let v_rollback = graph_flir.insert_vertex();
        let invalid_edge_to = graph_flir::Edge::new(
            graph_flir.epoch(),
            graph_flir.next_ctr(),
            v0.row_id().hash(),
            v0.row_id().ctr(),
            dangling_hash,
            dangling_ctr,
        );
        let invalid_edge_from = graph_flir::Edge::new(
            graph_flir.epoch(),
            graph_flir.next_ctr(),
            dangling_hash,
            dangling_ctr,
            v1.row_id().hash(),
            v1.row_id().ctr(),
        );
        let invalid_edge = graph_flir::Edge::new(
            graph_flir.epoch(),
            graph_flir.next_ctr(),
            dangling_hash,
            dangling_ctr,
            dangling_hash + 1,
            dangling_ctr + 1,
        );
        graph_flir.insert_raw_edge(invalid_edge_to);
        graph_flir.insert_raw_edge(invalid_edge_from);
        graph_flir.insert_raw_edge(invalid_edge);
        tx2.insert(graph_flir.next_epoch().into_table_deltas());
        // println!("{tx2:#?}");
        let mut tx2 = tx2.try_commit(&mut coln_query)?.expect_rejected();
        let violations = tx2.take_hard_violations();
        println!("{}", violations);
        let violations = violations.into_inner();
        assert_eq!(violations.len(), 1);
        let violation = &violations[0];
        assert_eq!(violation.for_entity().id(), "Graph.E.foreignKey");
        assert_eq!(violation.delta().len(), 3);

        let mut tx3 = Tx::empty();
        let e0 = graph_flir.insert_edge(&v0, &v_rollback);
        tx3.insert(graph_flir.next_epoch().into_table_deltas());
        // println!("{tx3:#?}");
        let mut tx3 = tx3.try_commit(&mut coln_query)?.expect_rejected();
        let violations = tx3.take_hard_violations();
        println!("{}", violations);
        let violations = violations.into_inner();
        assert_eq!(violations.len(), 1);
        let violation = &violations[0];
        assert_eq!(violation.for_entity().id(), "Graph.E.foreignKey");
        assert_eq!(violation.delta().len(), 1);

        Ok(())
    }

    /// One transaction to break a monitored rule, one to repair it. The point of
    /// the pair is the *second* one: it changes the monitored violations without
    /// introducing any, which is the only case where the delta a monitored sink
    /// reports and the set of violations that exist come apart.
    #[test]
    fn a_transaction_repairing_a_monitored_violation_reports_it_as_resolved() -> Result<()> {
        use test_helper::monitored_flir::{self as monitored, PERMITTED, RULE, TABLE};

        let mut coln_query = ColnQuery::init(&monitored::realm())?;
        // `PERMITTED` is the only value of `a` the rule tolerates, so this row
        // violates it and no other.
        let offending = monitored::row(0, 0, PERMITTED + 1);
        let tx_with = |zweight| {
            let mut tx = Tx::empty();
            tx.insert(Some(TableDelta::new(
                TABLE,
                vec![ZRow::new(zweight, offending.clone()).expect("non-zero zweight")],
            )));
            tx
        };

        // Inserting it makes the violation appear. Monitored, so the transaction
        // still commits.
        let mut committed = tx_with(1)
            .try_commit(&mut coln_query)?
            .expect_pending_and_commit();
        let appeared = committed.take_soft_violations();
        assert_eq!(
            appeared
                .iter()
                .map(|delta| delta.for_entity().id())
                .collect::<Vec<_>>(),
            vec![RULE],
            "the monitored rule must report on its own sink"
        );
        assert!(
            appeared
                .iter()
                .all(|table| table.iter().all(ZRow::is_assertion)),
            "a violation that appeared is asserted, not retracted: {appeared}"
        );

        // Retracting the same row repairs it.
        let mut committed = tx_with(-1)
            .try_commit(&mut coln_query)?
            .expect_pending_and_commit();
        let resolved = committed.take_soft_violations();
        assert_eq!(
            appeared
                .iter()
                .map(|delta| delta.for_entity().id())
                .collect::<Vec<_>>(),
            vec![RULE],
            "the monitored rule must report on its own sink and a retraction of \
            a monitored violation must not be swallowed"
        );
        assert!(
            resolved
                .iter()
                .all(|table| table.iter().all(ZRow::is_retraction)),
            "repairing a violation must not read as introducing one: {resolved}"
        );

        Ok(())
    }
}
