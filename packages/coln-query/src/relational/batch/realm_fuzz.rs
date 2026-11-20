// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Random commits on whole realms, batch against incremental.
//!
//! Each case is a seed. It grows a realm's base tables over several commits,
//! inserting rows and deleting rows of earlier commits, and after every
//! commit each derived view must hold the same rows on both backends (see
//! [`Twin`]). Deleting a row also deletes the rows referencing it, so every
//! reference points to a live row, as in a store that enforces its foreign
//! keys.
//!
//! Seeds grow the cases: the first ones keep the tables tiny, later ones
//! allow more rows per commit, up to a limit per realm. The first failing
//! seed is therefore a small case, and its message names the seed and the
//! commits that led to it.
//!
//! `COLN_FUZZ_CASES` sets the number of seeds per realm (default
//! [`DEFAULT_CASES`]), `COLN_FUZZ_SEED` runs a single seed. For a long run:
//! `COLN_FUZZ_CASES=2000 cargo test --release -p coln-query realm_fuzz`.

use anyhow::{Context, Result};
use coln_batch::rng::SplitMix64;

use super::realm_tests::Twin;
use crate::test_utils::flir::{
    BipartiteGraphOverRealm, BipartiteGraphRealm, Blue, BlueOver, BlueRed, BlueRedOver, Edge,
    Entity, EntityPath, Flir, GraphRealm, JsonFlir, Red, RedBlue, RedBlueOver, RedOver, RowId,
    TransitiveClosureRealm, TriangleRealm, Vertex,
};

/// Seeds per realm in a normal test run, enough to reach the largest cases
/// a few times while the debug build stays quick.
const DEFAULT_CASES: u64 = 30;

/// Commits per case.
const COMMITS: usize = 6;

/// The seeds to run: `COLN_FUZZ_SEED` alone, or `0..COLN_FUZZ_CASES`.
fn seeds() -> Vec<u64> {
    let var = |name: &str| std::env::var(name).ok().map(|value| value.parse::<u64>());
    if let Some(seed) = var("COLN_FUZZ_SEED") {
        return vec![seed.expect("COLN_FUZZ_SEED is a number")];
    }
    let cases = var("COLN_FUZZ_CASES")
        .transpose()
        .expect("COLN_FUZZ_CASES is a number")
        .unwrap_or(DEFAULT_CASES);
    (0..cases).collect()
}

/// The most rows a commit of case `seed` may insert per kind of row: tiny
/// at first, larger for later seeds, at most `limit`.
fn max_inserts(seed: u64, limit: u64) -> u64 {
    [1, 2, 3, 5, 8, 13][(seed / 5).min(5) as usize].min(limit)
}

/// A row id, comparable.
type Key = (u64, u64);

fn key(row_id: RowId) -> Key {
    (row_id.hash(), row_id.ctr())
}

/// A live row, the rows it references, and the commit it was inserted in.
struct LiveRow<E> {
    row: E,
    references: Vec<Key>,
    commit: usize,
}

/// The live rows of one base table.
struct Live<E> {
    rows: Vec<LiveRow<E>>,
}

impl<E: Entity> Live<E> {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }

    fn keys(&self) -> Vec<RowId> {
        self.rows.iter().map(|live| live.row.row_id()).collect()
    }

    fn insert(&mut self, row: E, references: &[RowId], commit: usize) {
        let references = references.iter().copied().map(key).collect();
        self.rows.push(LiveRow {
            row,
            references,
            commit,
        });
    }

    /// Delete a random row inserted before `commit`, if there is one. A
    /// store sends a row's insertion and deletion in different commits.
    fn delete_one<R>(
        &mut self,
        rng: &mut SplitMix64,
        driver: &mut Flir<R>,
        commit: usize,
    ) -> Option<Key>
    where
        R: JsonFlir,
        E: EntityPath<R>,
    {
        let older: Vec<usize> = (0..self.rows.len())
            .filter(|&at| self.rows[at].commit < commit)
            .collect();
        if older.is_empty() {
            return None;
        }
        let at = older[rng.below(older.len() as u64) as usize];
        let LiveRow { row, .. } = self.rows.swap_remove(at);
        driver.remove(&row);
        Some(key(row.row_id()))
    }

    /// Delete every row referencing the row `deleted`, and return their keys.
    fn delete_referencing<R>(&mut self, driver: &mut Flir<R>, deleted: Key) -> Vec<Key>
    where
        R: JsonFlir,
        E: EntityPath<R>,
    {
        let (gone, kept) = std::mem::take(&mut self.rows)
            .into_iter()
            .partition(|live| live.references.contains(&deleted));
        self.rows = kept;
        gone.into_iter()
            .map(|LiveRow { row, .. }| {
                driver.remove(&row);
                key(row.row_id())
            })
            .collect()
    }

    /// Each live row with the rows it references.
    fn with_references(&self) -> Vec<(RowId, Vec<RowId>)> {
        self.rows
            .iter()
            .map(|live| {
                let references = live
                    .references
                    .iter()
                    .map(|&(hash, ctr)| RowId::new(hash, ctr))
                    .collect();
                (live.row.row_id(), references)
            })
            .collect()
    }
}

/// Pick one of `rows`.
fn pick(rng: &mut SplitMix64, rows: &[RowId]) -> RowId {
    rows[rng.below(rows.len() as u64) as usize]
}

/// Run every seed through `case`, which drives one realm's commits, and fail
/// with the seed and the commits of the first failing case.
fn fuzz<R: JsonFlir>(case: impl Fn(u64, &mut Twin, &mut Vec<String>) -> Result<()>) {
    for seed in seeds() {
        let mut log = Vec::new();
        let outcome = Twin::new::<R>().and_then(|mut twin| case(seed, &mut twin, &mut log));
        if let Err(err) = outcome {
            panic!(
                "{} fails for seed {seed} (rerun with COLN_FUZZ_SEED={seed}): {err:#}\n\
                 commits:\n{}",
                R::NAME,
                log.join("\n")
            );
        }
    }
}

/// Random commits over the vertices and edges of a graph realm. Edges may
/// loop and repeat, and the few vertices of the early seeds close cycles.
fn graph_case<R>(seed: u64, twin: &mut Twin, log: &mut Vec<String>) -> Result<()>
where
    R: JsonFlir,
    Vertex: EntityPath<R>,
    Edge: EntityPath<R>,
{
    let mut rng = SplitMix64::new(seed);
    let mut driver = Flir::<R>::new();
    let mut vertices = Live::<Vertex>::new();
    let mut edges = Live::<Edge>::new();
    let most = max_inserts(seed, 13);

    for commit in 0..COMMITS {
        let new_vertices = rng.below(most + 1);
        for _ in 0..new_vertices {
            let vertex = driver.insert(Vertex::new);
            vertices.insert(vertex, &[], commit);
        }
        let targets = vertices.keys();
        let mut new_edges = 0;
        if !targets.is_empty() {
            new_edges = rng.below(2 * most + 1);
            for _ in 0..new_edges {
                let (from, to) = (pick(&mut rng, &targets), pick(&mut rng, &targets));
                let edge = driver.insert(|row_id| Edge::new(row_id, from, to));
                edges.insert(edge, &[from, to], commit);
            }
        }
        let mut deleted = 0;
        for _ in 0..rng.below(most / 2 + 1) {
            deleted += edges.delete_one(&mut rng, &mut driver, commit).is_some() as u64;
        }
        if rng.below(4) == 0
            && let Some(vertex) = vertices.delete_one(&mut rng, &mut driver, commit)
        {
            edges.delete_referencing(&mut driver, vertex);
            deleted += 1;
        }

        let sizes = twin
            .commit(driver.next_epoch())
            .with_context(|| format!("commit {commit}"))?;
        log.push(format!(
            "  {commit}: +{new_vertices} vertices, +{new_edges} edges, {deleted} deletions \
             (now {} vertices, {} edges) -> {sizes:?}",
            vertices.rows.len(),
            edges.rows.len()
        ));
    }
    Ok(())
}

/// Random commits over the two colours of a bipartite realm and the edges
/// between them, in both directions.
fn bipartite_case(seed: u64, twin: &mut Twin, log: &mut Vec<String>) -> Result<()> {
    type R = BipartiteGraphRealm;
    let mut rng = SplitMix64::new(seed);
    let mut driver = Flir::<R>::new();
    let mut reds = Live::<Red>::new();
    let mut blues = Live::<Blue>::new();
    let mut red_blue = Live::<RedBlue>::new();
    let mut blue_red = Live::<BlueRed>::new();
    // The incremental backend slows down sharply on larger mutually
    // recursive cases, so these stay smaller.
    let most = max_inserts(seed, 3);

    for commit in 0..COMMITS {
        for _ in 0..rng.below(most + 1) {
            let red = driver.insert(Red::new);
            reds.insert(red, &[], commit);
        }
        for _ in 0..rng.below(most + 1) {
            let blue = driver.insert(Blue::new);
            blues.insert(blue, &[], commit);
        }
        let (red_keys, blue_keys) = (reds.keys(), blues.keys());
        if !red_keys.is_empty() && !blue_keys.is_empty() {
            for _ in 0..rng.below(2 * most + 1) {
                let (red, blue) = (pick(&mut rng, &red_keys), pick(&mut rng, &blue_keys));
                let edge = driver.insert(|row_id| RedBlue::new(row_id, red, blue));
                red_blue.insert(edge, &[red, blue], commit);
            }
            for _ in 0..rng.below(2 * most + 1) {
                let (blue, red) = (pick(&mut rng, &blue_keys), pick(&mut rng, &red_keys));
                let edge = driver.insert(|row_id| BlueRed::new(row_id, blue, red));
                blue_red.insert(edge, &[blue, red], commit);
            }
        }
        for _ in 0..rng.below(most / 2 + 1) {
            red_blue.delete_one(&mut rng, &mut driver, commit);
            blue_red.delete_one(&mut rng, &mut driver, commit);
        }
        if rng.below(4) == 0
            && let Some(red) = reds.delete_one(&mut rng, &mut driver, commit)
        {
            red_blue.delete_referencing(&mut driver, red);
            blue_red.delete_referencing(&mut driver, red);
        }
        if rng.below(4) == 0
            && let Some(blue) = blues.delete_one(&mut rng, &mut driver, commit)
        {
            red_blue.delete_referencing(&mut driver, blue);
            blue_red.delete_referencing(&mut driver, blue);
        }

        let sizes = twin
            .commit(driver.next_epoch())
            .with_context(|| format!("commit {commit}"))?;
        log.push(format!(
            "  {commit}: {} reds, {} blues, {} red-blue, {} blue-red -> {sizes:?}",
            reds.rows.len(),
            blues.rows.len(),
            red_blue.rows.len(),
            blue_red.rows.len()
        ));
    }
    Ok(())
}

/// Random commits over the annotated bipartite realm: nodes of both colours,
/// annotations of them, and edges between annotated nodes. An edge mostly
/// names an annotation of its own node, the shape its rules join on, and
/// sometimes one of another node, which no rule matches.
fn bipartite_over_case(seed: u64, twin: &mut Twin, log: &mut Vec<String>) -> Result<()> {
    type R = BipartiteGraphOverRealm;
    let mut rng = SplitMix64::new(seed);
    let mut driver = Flir::<R>::new();
    let mut reds = Live::<Red>::new();
    let mut blues = Live::<Blue>::new();
    let mut red_overs = Live::<RedOver>::new();
    let mut blue_overs = Live::<BlueOver>::new();
    let mut red_blue = Live::<RedBlueOver>::new();
    let mut blue_red = Live::<BlueRedOver>::new();
    // Its rules join up to ten atoms, and the incremental backend slows down
    // sharply as these cases grow.
    let most = max_inserts(seed, 2);

    for commit in 0..COMMITS {
        for _ in 0..rng.below(most + 1) {
            let red = driver.insert(Red::new);
            reds.insert(red, &[], commit);
        }
        for _ in 0..rng.below(most + 1) {
            let blue = driver.insert(Blue::new);
            blues.insert(blue, &[], commit);
        }
        let (red_keys, blue_keys) = (reds.keys(), blues.keys());
        if !red_keys.is_empty() {
            for _ in 0..rng.below(most + 1) {
                let red = pick(&mut rng, &red_keys);
                let over = driver.insert(|row_id| RedOver::new(row_id, red));
                red_overs.insert(over, &[red], commit);
            }
        }
        if !blue_keys.is_empty() {
            for _ in 0..rng.below(most + 1) {
                let blue = pick(&mut rng, &blue_keys);
                let over = driver.insert(|row_id| BlueOver::new(row_id, blue));
                blue_overs.insert(over, &[blue], commit);
            }
        }
        let (reds_over, blues_over) = (red_overs.with_references(), blue_overs.with_references());
        if !reds_over.is_empty() && !blues_over.is_empty() {
            // An annotation and its node, or now and then another node.
            let annotated =
                |rng: &mut SplitMix64, overs: &[(RowId, Vec<RowId>)], nodes: &[RowId]| {
                    let (over, node) = &overs[rng.below(overs.len() as u64) as usize];
                    let node = match rng.below(5) {
                        0 => pick(rng, nodes),
                        _ => node[0],
                    };
                    (node, *over)
                };
            for _ in 0..rng.below(2 * most + 1) {
                let (v, a) = annotated(&mut rng, &reds_over, &red_keys);
                let (w, b) = annotated(&mut rng, &blues_over, &blue_keys);
                let edge = driver.insert(|row_id| RedBlueOver::new(row_id, v, a, w, b));
                red_blue.insert(edge, &[v, a, w, b], commit);
            }
            for _ in 0..rng.below(2 * most + 1) {
                let (w, a) = annotated(&mut rng, &blues_over, &blue_keys);
                let (v, b) = annotated(&mut rng, &reds_over, &red_keys);
                let edge = driver.insert(|row_id| BlueRedOver::new(row_id, w, a, v, b));
                blue_red.insert(edge, &[w, a, v, b], commit);
            }
        }
        for _ in 0..rng.below(most / 2 + 1) {
            red_blue.delete_one(&mut rng, &mut driver, commit);
            blue_red.delete_one(&mut rng, &mut driver, commit);
        }
        // A node takes its annotations along, and both take their edges.
        let mut gone = Vec::new();
        if rng.below(4) == 0
            && let Some(red) = reds.delete_one(&mut rng, &mut driver, commit)
        {
            gone.push(red);
            gone.extend(red_overs.delete_referencing(&mut driver, red));
        }
        if rng.below(4) == 0
            && let Some(blue) = blues.delete_one(&mut rng, &mut driver, commit)
        {
            gone.push(blue);
            gone.extend(blue_overs.delete_referencing(&mut driver, blue));
        }
        for row in gone {
            red_blue.delete_referencing(&mut driver, row);
            blue_red.delete_referencing(&mut driver, row);
        }

        let sizes = twin
            .commit(driver.next_epoch())
            .with_context(|| format!("commit {commit}"))?;
        log.push(format!(
            "  {commit}: {} reds, {} blues, {} red annotations, {} blue annotations, \
             {} red-blue, {} blue-red -> {sizes:?}",
            reds.rows.len(),
            blues.rows.len(),
            red_overs.rows.len(),
            blue_overs.rows.len(),
            red_blue.rows.len(),
            blue_red.rows.len()
        ));
    }
    Ok(())
}

#[test]
fn graph_realm_fuzz() {
    fuzz::<GraphRealm>(graph_case::<GraphRealm>);
}

#[test]
fn triangle_realm_fuzz() {
    fuzz::<TriangleRealm>(graph_case::<TriangleRealm>);
}

#[test]
fn transitive_closure_realm_fuzz() {
    fuzz::<TransitiveClosureRealm>(graph_case::<TransitiveClosureRealm>);
}

#[test]
fn bipartite_realm_fuzz() {
    fuzz::<BipartiteGraphRealm>(bipartite_case);
}

#[test]
fn bipartite_over_realm_fuzz() {
    fuzz::<BipartiteGraphOverRealm>(bipartite_over_case);
}
