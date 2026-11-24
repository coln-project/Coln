// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Whole realms from the compiler's golden tests (`coln-flir-rs/tests/data`)
//! with hand-made rows, on batch, incremental and Soufflé ([`Twin`]). The
//! fuzz tests (`realm_fuzz.rs`) drive the same with random commits.

use anyhow::{Context, Result};

use super::twin::Twin;
use crate::api::deltas::StoreDelta;
use crate::test_utils::flir::{
    BipartiteGraphRealm, Blue, BlueRed, Edge, Flir, GraphRealm, JsonFlir, Red, RedBlue,
    TransitiveClosureRealm, TriangleRealm, Vertex,
};

/// Batch, incremental and Soufflé over realm `R`, see [`Twin`].
pub(super) fn twin<R: JsonFlir>() -> Result<Twin> {
    Twin::new(&R::load(), R::NAME, true)
}

/// Run the views of realm `R` over `rows`, one commit from empty, on both
/// backends. Returns the number of rows per view.
fn views_match_incremental<R: JsonFlir>(rows: StoreDelta) -> Result<Vec<(String, usize)>> {
    twin::<R>()?
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
