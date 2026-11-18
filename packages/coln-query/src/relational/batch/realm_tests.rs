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

use std::collections::HashSet;

use anyhow::Result;
use coln_flir_rs::ir::EntityVariant;

use super::views::ViewProgram;
use crate::api::deltas::StoreDelta;
use crate::api::query::FlirProgram;
use crate::pipeline::Pipeline;
use crate::relational::Runtime;
use crate::relational::expr::{SinkId, SourceId};
use crate::test_utils::flir::{Edge, Flir, GraphRealm, JsonFlir, TriangleRealm, Vertex};

/// Run the views of realm `R` over `rows` on the batch backend and compare
/// each with the incremental backend. Returns the number of rows per view.
fn views_match_incremental<R: JsonFlir>(rows: StoreDelta) -> Result<Vec<(String, usize)>> {
    let realm = R::load();
    let mut batch =
        Pipeline::batch().runtime(&mut ViewProgram::new(FlirProgram::from_flat_realm(&realm)?))?;
    let mut incremental =
        Pipeline::incremental().runtime(&mut FlirProgram::from_flat_realm(&realm)?)?;

    for table in rows.into_table_deltas() {
        let source = SourceId::from(table.for_entity());
        let delta = table.into_delta();
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
        // One commit from empty, so the incremental delta is the full state.
        let on_incremental = incremental.output(view)?.debug_view(false).to_zset();
        let on_batch = batch.output(view)?;
        assert_eq!(
            on_batch.to_debug_zset(),
            on_incremental,
            "view {view} of {}",
            R::NAME
        );
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
