// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Whole realms from the compiler's golden tests, run on both backends.
//!
//! A realm goes the full way: its FLIR (`coln-flir-rs/tests/data`) through
//! the FLIR frontend into a plan, which the batch backend restricts to the
//! derived views ([`ViewProgram`]) and the incremental backend runs as a
//! whole. Both get the same base rows, and every view must hold the same
//! rows on both. Where Soufflé is installed, it runs the frontend's Datalog on
//! the same rows as a third opinion (`souffle.rs`). [`Twin`] does that commit
//! by commit; the fuzz tests (`realm_fuzz.rs`) drive it with random commits.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::{Context, Result, bail, ensure};
use coln_flir_rs::ir::EntityVariant;
use dbsp::typed_batch::IndexedZSetReader;

use super::BatchRuntime;
use super::souffle::Souffle;
use super::views::ViewProgram;
use crate::api::deltas::StoreDelta;
use crate::api::query::FlirProgram;
use crate::pipeline::Pipeline;
use crate::relational::Runtime;
use crate::relational::expr::{SinkId, SourceId};
use crate::relational::incremental::DbspRuntime;
use crate::relational::relation::TupleValue;
use crate::test_utils::flir::{
    BipartiteGraphRealm, Blue, BlueRed, Edge, Flir, GraphRealm, JsonFlir, Red, RedBlue,
    TransitiveClosureRealm, TriangleRealm, Vertex,
};

/// Both backends over one realm, fed the same commits: batch runs the realm's
/// derived views, incremental the whole realm. After every commit, each view
/// must hold the same rows on both, and on Soufflé if it is installed.
pub(super) struct Twin {
    name: &'static str,
    batch: BatchRuntime,
    incremental: DbspRuntime,
    views: Vec<SinkId>,
    /// Every view's state on the incremental backend, summed up from the
    /// changes it reports per commit.
    incremental_views: HashMap<SinkId, BTreeMap<TupleValue, i64>>,
    souffle: Option<Souffle>,
}

impl Twin {
    pub(super) fn new<R: JsonFlir>() -> Result<Self> {
        let realm = R::load();
        let batch = Pipeline::batch()
            .runtime(&mut ViewProgram::new(FlirProgram::from_flat_realm(&realm)?))?;
        let incremental =
            Pipeline::incremental().runtime(&mut FlirProgram::from_flat_realm(&realm)?)?;

        // Batch offers exactly the views: no rule made it into its program.
        let mut views: Vec<SinkId> = batch.list_outputs().cloned().collect();
        views.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let mut expected: Vec<SinkId> = realm
            .tables
            .iter()
            .filter(|entry| matches!(entry.table.entity_variant, EntityVariant::View { .. }))
            .map(|entry| SinkId::from(entry.path.to_string()))
            .collect();
        expected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        ensure!(views == expected, "the views of {}", R::NAME);
        let souffle = Souffle::new(&FlirProgram::from_flat_realm(&realm)?)?;

        Ok(Self {
            name: R::NAME,
            batch,
            incremental,
            incremental_views: views
                .iter()
                .map(|view| (view.clone(), BTreeMap::new()))
                .collect(),
            views,
            souffle,
        })
    }

    /// Commit `rows` on both backends and compare every view. Returns the
    /// number of rows per view.
    pub(super) fn commit(&mut self, rows: StoreDelta) -> Result<Vec<(String, usize)>> {
        for table in rows.into_table_deltas() {
            let source = SourceId::from(table.for_entity());
            let delta = table.into_delta();
            if let Some(souffle) = &mut self.souffle {
                souffle.feed(source.as_str(), &delta)?;
            }
            ensure!(
                self.batch.feed(&source, delta.clone())?,
                "batch knows {source}"
            );
            ensure!(
                self.incremental.feed(&source, delta)?,
                "incremental knows {source}"
            );
        }
        self.batch.commit()?;
        self.incremental.commit()?;

        let on_souffle = match &self.souffle {
            Some(souffle) => {
                let views: Vec<&str> = self.views.iter().map(SinkId::as_str).collect();
                Some(souffle.evaluate(&views)?)
            }
            None => None,
        };

        let mut sizes = Vec::new();
        for view in &self.views {
            // Reading an incremental output drains what changed since the
            // last read, so the sum over all commits is the view's state.
            let state = self.incremental_views.get_mut(view).expect("one per view");
            for (row, (), weight) in self
                .incremental
                .output(view)?
                .debug_view(false)
                .to_zset()
                .iter()
            {
                let entry = state.entry(row.clone()).or_insert(0);
                *entry += weight;
                if *entry == 0 {
                    state.remove(&row);
                }
            }
            // The batch backend reports a view as a set. The incremental one
            // counts how often a row of a recursive view is derived, e.g.
            // twice for a vertex on a cycle, which reaches itself directly and
            // around the cycle. So compare which rows each holds.
            if let Some((row, weight)) = state.iter().find(|(_, weight)| **weight < 0) {
                bail!(
                    "view {view} of {} holds {row:?} {weight} times on incremental",
                    self.name
                );
            }
            let on_incremental: BTreeSet<&TupleValue> = state.keys().collect();

            let on_batch = self.batch.output(view)?;
            let batch_rows: BTreeSet<&TupleValue> = on_batch.rows().iter().collect();
            ensure!(
                batch_rows.len() == on_batch.len(),
                "view {view} repeats a row on batch"
            );
            same(self.name, view, &batch_rows, "incremental", &on_incremental)?;
            if let Some(on_souffle) = &on_souffle {
                let rows = on_souffle[view.as_str()].iter().collect();
                same(self.name, view, &batch_rows, "Soufflé", &rows)?;
            }
            sizes.push((view.as_str().to_string(), on_batch.len()));
        }
        Ok(sizes)
    }
}

/// Fail unless `view` holds the same rows on batch and on `other`.
fn same(
    realm: &str,
    view: &SinkId,
    on_batch: &BTreeSet<&TupleValue>,
    other: &str,
    on_other: &BTreeSet<&TupleValue>,
) -> Result<()> {
    if on_batch == on_other {
        return Ok(());
    }
    let only = |a: &BTreeSet<&TupleValue>, b: &BTreeSet<&TupleValue>| {
        a.difference(b)
            .take(5)
            .map(|row| format!("{row:?}"))
            .collect::<Vec<_>>()
    };
    bail!(
        "view {view} of {realm} differs, only on batch: {:?}, only on {other}: {:?}",
        only(on_batch, on_other),
        only(on_other, on_batch),
    )
}

/// Run the views of realm `R` over `rows`, one commit from empty, on both
/// backends. Returns the number of rows per view.
fn views_match_incremental<R: JsonFlir>(rows: StoreDelta) -> Result<Vec<(String, usize)>> {
    Twin::new::<R>()?
        .commit(rows)
        .with_context(|| format!("realm {}", R::NAME))
}

#[test]
fn graph_views_match_incremental() -> Result<()> {
    let mut driver = Flir::<GraphRealm>::new();
    let [v0, v1, v2, _lonely] = [(); 4].map(|()| driver.insert(Vertex::new));
    driver.insert(|row_id| Edge::new(row_id, &v0, &v1));
    driver.insert(|row_id| Edge::new(row_id, &v1, &v2));
    driver.insert(|row_id| Edge::new(row_id, &v0, &v2));
    driver.insert(|row_id| Edge::new(row_id, &v2, &v2));

    // Every edge is one outgoing edge of its tail and one incoming edge of
    // its head; the lonely vertex has neither.
    assert_eq!(
        views_match_incremental::<GraphRealm>(driver.next_epoch())?,
        [
            ("view.incoming-edges".to_string(), 4),
            ("view.outgoing-edges".to_string(), 4)
        ]
    );
    Ok(())
}

#[test]
fn triangle_views_match_incremental() -> Result<()> {
    let mut driver = Flir::<TriangleRealm>::new();
    let [a, b, c, d] = [(); 4].map(|()| driver.insert(Vertex::new));
    // Two triangles, a b c and b c d, sharing the edge b → c.
    for (from, to) in [(&a, &b), (&b, &c), (&a, &c), (&c, &d), (&b, &d)] {
        driver.insert(|row_id| Edge::new(row_id, from, to));
    }
    // A cycle is not a triangle here: it needs a → c, not c → a.
    driver.insert(|row_id| Edge::new(row_id, &d, &a));

    assert_eq!(
        views_match_incremental::<TriangleRealm>(driver.next_epoch())?,
        [("view.triangle".to_string(), 2)]
    );
    Ok(())
}

#[test]
fn transitive_closure_views_match_incremental() -> Result<()> {
    let mut driver = Flir::<TransitiveClosureRealm>::new();
    let [a, b, c, _lonely] = [(); 4].map(|()| driver.insert(Vertex::new));
    // A path a → b → c, with a cycle between b and c.
    for (from, to) in [(&a, &b), (&b, &c), (&c, &b)] {
        driver.insert(|row_id| Edge::new(row_id, from, to));
    }

    // Every vertex reaches itself (4), and a reaches b and c, b reaches c,
    // c reaches b (4 more). The cycle adds nothing new after one round.
    assert_eq!(
        views_match_incremental::<TransitiveClosureRealm>(driver.next_epoch())?,
        [("init.trans-closure.connected".to_string(), 8)]
    );
    Ok(())
}

#[test]
fn bipartite_views_match_incremental() -> Result<()> {
    let mut driver = Flir::<BipartiteGraphRealm>::new();
    let [r0, r1, r2] = [(); 3].map(|()| driver.insert(Red::new));
    let [b0, b1] = [(); 2].map(|()| driver.insert(Blue::new));
    // Red to blue and back: r0 → b0 → r1 → b1 → r2.
    driver.insert(|row_id| RedBlue::new(row_id, &r0, &b0));
    driver.insert(|row_id| RedBlue::new(row_id, &r1, &b1));
    driver.insert(|row_id| BlueRed::new(row_id, &b0, &r1));
    driver.insert(|row_id| BlueRed::new(row_id, &b1, &r2));

    // Each colour connects to itself, and through the other colour along the
    // path: r0 to r1 and r2, r1 to r2, and b0 to b1, which needs r1 to reach
    // itself first. So both views recurse through each other.
    assert_eq!(
        views_match_incremental::<BipartiteGraphRealm>(driver.next_epoch())?,
        [
            ("init.connection-info.blue-blue".to_string(), 3),
            ("init.connection-info.red-red".to_string(), 6)
        ]
    );
    Ok(())
}
