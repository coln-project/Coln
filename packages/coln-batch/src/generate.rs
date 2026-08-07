// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Deterministic generators for e-matching-style join workloads.
//!
//! Both scenarios mirror the workloads from the "Relational E-matching"
//! paper (Zhang et al., POPL 2022) and the accompanying benchmark
//! notebooks: purely random data almost never produces matches, so we
//! *plant* a controlled number of matches and surround them with random
//! noise rows. Generated relations are deduplicated (relations are sets),
//! so `planted`/`noise` are approximate upper bounds, not exact row
//! counts. Tests that need exact ground truth should compute it with a
//! brute-force reference join at small scale.

use crate::relation::Relation;
use crate::rng::SplitMix64;

/// Triangle workload — the canonical *cyclic* join:
///
/// ```text
/// Q(x, y, z) <- R_f(x, y), R_g(y, z), R_h(z, x)
/// ```
///
/// Plants `planted` triangles over `nodes` vertices and adds `noise_edges`
/// random edges to each of the three relations.
pub fn triangle(nodes: u64, noise_edges: usize, planted: usize, seed: u64) -> [Relation; 3] {
    let mut rng = SplitMix64::new(seed);
    let mut f = [Vec::new(), Vec::new()];
    let mut g = [Vec::new(), Vec::new()];
    let mut h = [Vec::new(), Vec::new()];
    let push = |rel: &mut [Vec<u64>; 2], a: u64, b: u64| {
        rel[0].push(a);
        rel[1].push(b);
    };
    for _ in 0..planted {
        let (x, y, z) = (rng.below(nodes), rng.below(nodes), rng.below(nodes));
        push(&mut f, x, y);
        push(&mut g, y, z);
        push(&mut h, z, x);
    }
    for _ in 0..noise_edges {
        push(&mut f, rng.below(nodes), rng.below(nodes));
        push(&mut g, rng.below(nodes), rng.below(nodes));
        push(&mut h, rng.below(nodes), rng.below(nodes));
    }
    let rel = |name: &str, [a, b]: [Vec<u64>; 2]| {
        Relation::new(name, ["src", "dst"], vec![a, b]).sorted_dedup()
    };
    [rel("R_f", f), rel("R_g", g), rel("R_h", h)]
}

/// Acyclic e-matching workload for the pattern `f(α, g(α))`:
///
/// ```text
/// R_f(f_id, c1, c2)   an f-node in e-class f_id with children (c1, c2)
/// R_g(g_id, c1)       a g-node in e-class g_id with child c1
///
/// Q(f_id, α, g_id) <- R_f(f_id, α, g_id), R_g(g_id, α)
/// ```
///
/// Plants `planted` matches over `eclasses` e-class ids and adds `noise`
/// random nodes to each relation.
pub fn f_g_pattern(eclasses: u64, noise: usize, planted: usize, seed: u64) -> [Relation; 2] {
    let mut rng = SplitMix64::new(seed);
    let mut f = [Vec::new(), Vec::new(), Vec::new()];
    let mut g = [Vec::new(), Vec::new()];
    for _ in 0..planted {
        let alpha = rng.below(eclasses);
        let g_id = rng.below(eclasses);
        let f_id = rng.below(eclasses);
        g[0].push(g_id);
        g[1].push(alpha);
        f[0].push(f_id);
        f[1].push(alpha);
        f[2].push(g_id);
    }
    for _ in 0..noise {
        f[0].push(rng.below(eclasses));
        f[1].push(rng.below(eclasses));
        f[2].push(rng.below(eclasses));
        g[0].push(rng.below(eclasses));
        g[1].push(rng.below(eclasses));
    }
    let [f0, f1, f2] = f;
    let [g0, g1] = g;
    [
        Relation::new("R_f", ["id", "c1", "c2"], vec![f0, f1, f2]).sorted_dedup(),
        Relation::new("R_g", ["id", "c1"], vec![g0, g1]).sorted_dedup(),
    ]
}

/// Parent edges forming the chain 0 → 1 → … → k-1 (that is, k-1 rows).
/// Its transitive closure has exactly k·(k-1)/2 pairs, which makes the
/// chain an exact acceptance test for recursive evaluation.
pub fn chain(k: u64) -> Relation {
    assert!(k >= 2, "chain needs at least two nodes");
    Relation::new(
        "parent",
        ["parent", "child"],
        vec![(0..k - 1).collect(), (1..k).collect()],
    )
}

/// Random DAG: `edges` random edges a → b with a < b over `nodes`
/// vertices, deduplicated. Acyclic by construction; with sparse edge
/// counts the longest path (= number of fixpoint rounds) stays small.
pub fn dag(nodes: u64, edges: usize, seed: u64) -> Relation {
    assert!(nodes >= 2, "dag needs at least two nodes");
    let mut rng = SplitMix64::new(seed);
    let mut src = Vec::with_capacity(edges);
    let mut dst = Vec::with_capacity(edges);
    for _ in 0..edges {
        let a = rng.below(nodes - 1); // a in 0..=nodes-2
        let b = a + 1 + rng.below(nodes - 1 - a); // b in a+1..=nodes-1
        src.push(a);
        dst.push(b);
    }
    Relation::new("parent", ["parent", "child"], vec![src, dst]).sorted_dedup()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_and_dag_are_well_formed() {
        let c = chain(5);
        assert_eq!(c.len(), 4);
        assert_eq!(c.row(0), vec![0, 1]);
        assert_eq!(c.row(3), vec![3, 4]);

        let d = dag(100, 300, 9);
        assert!(!d.is_empty() && d.len() <= 300);
        for i in 0..d.len() {
            let row = d.row(i);
            assert!(row[0] < row[1], "edge must go forward: {row:?}");
            assert!(row[1] < 100);
        }
        assert_eq!(d, dag(100, 300, 9), "deterministic");
    }

    #[test]
    fn triangle_is_deterministic_and_dedduped() {
        let [f1, ..] = triangle(100, 500, 20, 1);
        let [f2, ..] = triangle(100, 500, 20, 1);
        assert_eq!(f1, f2);
        // sorted + deduped
        for i in 1..f1.len() {
            assert!(f1.row(i - 1) < f1.row(i));
        }
    }

    #[test]
    fn planted_triangles_exist() {
        // With zero noise every planted triangle must be present.
        let [f, g, h] = triangle(50, 0, 10, 3);
        let mut found = 0;
        for i in 0..f.len() {
            let (x, y) = (f.cols[0][i], f.cols[1][i]);
            for j in 0..g.len() {
                if g.cols[0][j] != y {
                    continue;
                }
                let z = g.cols[1][j];
                for k in 0..h.len() {
                    if h.cols[0][k] == z && h.cols[1][k] == x {
                        found += 1;
                    }
                }
            }
        }
        assert!(found >= 10 - 2, "collisions may merge a few, got {found}");
    }

    #[test]
    fn f_g_pattern_has_planted_matches() {
        let [f, g] = f_g_pattern(100, 0, 5, 9);
        let mut found = 0;
        for i in 0..f.len() {
            for j in 0..g.len() {
                if f.cols[2][i] == g.cols[0][j] && f.cols[1][i] == g.cols[1][j] {
                    found += 1;
                }
            }
        }
        assert!(found >= 3, "got {found}");
    }
}
