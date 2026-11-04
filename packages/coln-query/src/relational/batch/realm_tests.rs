// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Whole realms from the compiler's golden tests, run on both backends.
//!
//! A realm goes the full way: its FLIR (`coln-flir-rs/tests/data`) through
//! the FLIR frontend into a plan, which the batch backend restricts to the
//! derived views ([`ViewProgram`]) and the incremental backend runs as a
//! whole. Both get the same base rows, and every view must hold the same
//! rows on both.

use std::collections::{BTreeSet, HashSet};

use anyhow::Result;
use coln_flir_rs::ir::EntityVariant;
use dbsp::typed_batch::IndexedZSetReader;

use super::views::ViewProgram;
use crate::api::transaction::StoreDelta;
use crate::api::query::FlirProgram;
use crate::pipeline::Pipeline;
use crate::relational::Runtime;
use crate::relational::expr::{SinkId, SourceId};
use crate::relational::relation::TupleValue;
use crate::test_utils::flir::{
    BipartiteGraphRealm, Blue, BlueRed, Edge, Flir, GraphRealm, JsonFlir, Red, RedBlue,
    TransitiveClosureRealm, TriangleRealm, Vertex,
};

/// Run the views of realm `R` over `rows` on the batch backend and compare
/// each with the incremental backend. Returns the number of rows per view.
fn views_match_incremental<R: JsonFlir>(rows: StoreDelta) -> Result<Vec<(String, usize)>> {
    let realm = R::load();
    let mut batch =
        Pipeline::batch().runtime(&mut ViewProgram::new(FlirProgram::from_flat_realm(&realm)?))?;
    let mut incremental =
        Pipeline::incremental().runtime(&mut FlirProgram::from_flat_realm(&realm)?)?;

    for table in rows.into_table_deltas() {
        let (entity, delta) = table.into_parts();
        let source = SourceId::from(entity.0);
        let delta: Vec<_> = delta
            .into_iter()
            .map(|zrow| zrow.map(TupleValue::from))
            .collect();
        assert!(batch.feed(&source, delta.clone())?, "batch knows {source}");
        assert!(
            incremental.feed(&source, delta)?,
            "incremental knows {source}"
        );
    }
    batch.commit()?;
    incremental.commit()?;

    // Batch offers exactly the views: no rule made it into its program.
    let views: HashSet<&SinkId> = batch.list_outputs().collect();
    let expected: HashSet<SinkId> = realm
        .tables
        .iter()
        .filter(|entry| matches!(entry.table.entity_variant, EntityVariant::View { .. }))
        .map(|entry| SinkId::from(entry.path.to_string()))
        .collect();
    assert_eq!(
        views,
        expected.iter().collect::<HashSet<_>>(),
        "the views of {}",
        R::NAME
    );

    let mut sizes = Vec::new();
    for view in views {
        // The batch backend reports a view as a set. The incremental one
        // counts how often a row of a recursive view is derived, e.g. twice
        // for a vertex on a cycle, which reaches itself directly and around
        // the cycle. So compare which rows each holds, not their counts.
        // One commit from empty, so the incremental delta is the full state.
        let on_incremental: BTreeSet<TupleValue> = incremental
            .output(view)?
            .debug_view(false)
            .to_zset()
            .iter()
            .map(|(row, (), weight)| {
                assert!(weight > 0, "view {view} holds {row:?} {weight} times");
                row
            })
            .collect();
        let on_batch = batch.output(view)?;
        let batch_rows: BTreeSet<TupleValue> = on_batch.rows().iter().cloned().collect();
        assert_eq!(
            batch_rows.len(),
            on_batch.len(),
            "view {view} repeats a row"
        );
        assert_eq!(batch_rows, on_incremental, "view {view} of {}", R::NAME);
        sizes.push((view.as_str().to_string(), on_batch.len()));
    }
    sizes.sort();
    Ok(sizes)
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
