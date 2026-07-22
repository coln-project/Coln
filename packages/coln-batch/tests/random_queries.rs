// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Randomized differential testing: hundreds of generated conjunctive
//! queries over generated catalogs, each checked three ways (binary join,
//! generic join, brute-force oracle) like `tests/differential.rs`, but
//! covering a far larger space of query shapes than hand-written
//! fixtures.
//!
//! The value domain is deliberately small: matching and non-matching
//! joins, empty relations, cross products and unsatisfied literals all
//! occur naturally.
//! Each case seeds its own [`SplitMix64`], so a failing case number
//! reproduces in isolation.

use coln_batch::query::{Atom, Catalog, Query, Term};
use coln_batch::relation::Relation;
use coln_batch::rng::SplitMix64;
use coln_batch::{binary_join, generic_join, reference};

const CASES: u64 = 250;
/// Values are drawn from `0..DOMAIN`, small so collisions are common.
const DOMAIN: u64 = 8;

/// Random catalog: 1..=4 relations, arity 1..=3, 0..=20 rows each
/// (zero-row relations included deliberately).
fn random_catalog(rng: &mut SplitMix64) -> (Catalog, Vec<(String, usize)>) {
    let mut cat = Catalog::new();
    let mut rels = Vec::new();
    for r in 0..1 + rng.below(4) {
        let arity = 1 + rng.below(3) as usize;
        let rows = rng.below(21);
        let mut cols: Vec<Vec<u64>> = vec![Vec::new(); arity];
        for _ in 0..rows {
            for col in cols.iter_mut() {
                col.push(rng.below(DOMAIN));
            }
        }
        let name = format!("R{r}");
        let col_names: Vec<String> = (0..arity).map(|c| format!("c{c}")).collect();
        cat.insert(Relation::new(name.clone(), col_names, cols));
        rels.push((name, arity));
    }
    (cat, rels)
}

/// Random query: 1..=4 atoms over the given relations, up to 4 variables,
/// 25% literal terms. Returns `None` for the rare all-literal draw, which
/// leaves no variable for the head.
fn random_query(rng: &mut SplitMix64, rels: &[(String, usize)]) -> Option<Query> {
    let var_pool = 1 + rng.below(4) as usize;
    let mut atoms = Vec::new();
    for _ in 0..1 + rng.below(4) {
        let (name, arity) = &rels[rng.below(rels.len() as u64) as usize];
        let terms = (0..*arity)
            .map(|_| {
                if rng.below(4) == 0 {
                    Term::Lit(rng.below(DOMAIN))
                } else {
                    Term::Var(rng.below(var_pool as u64) as usize)
                }
            })
            .collect();
        atoms.push(Atom {
            relation: name.clone(),
            terms,
        });
    }

    // Renumber the variables that actually occur to a dense 0..n range,
    // since `Query::validate` rejects declared-but-unused variables.
    let mut remap: Vec<Option<usize>> = vec![None; var_pool];
    let mut next = 0;
    for atom in &mut atoms {
        for term in &mut atom.terms {
            if let Term::Var(v) = term {
                let id = *remap[*v].get_or_insert_with(|| {
                    next += 1;
                    next - 1
                });
                *term = Term::Var(id);
            }
        }
    }
    if next == 0 {
        return None;
    }

    // Head: a random non-empty subset of the variables, in random order.
    let mut vars: Vec<usize> = (0..next).collect();
    for i in (1..vars.len()).rev() {
        vars.swap(i, rng.below(i as u64 + 1) as usize);
    }
    vars.truncate(1 + rng.below(next as u64) as usize);

    Some(Query {
        var_names: (0..next).map(|v| format!("v{v}")).collect(),
        atoms,
        head: vars,
    })
}

#[test]
fn random_queries_agree_with_oracle() {
    let mut ran = 0;
    let mut non_empty = 0;
    for case in 0..CASES {
        let mut rng = SplitMix64::new(case);
        let (cat, rels) = random_catalog(&mut rng);
        let Some(query) = random_query(&mut rng, &rels) else {
            continue;
        };
        let oracle = reference::execute(&query, &cat).unwrap();
        let binary = binary_join::execute(&query, &cat).unwrap();
        let generic = generic_join::execute(&query, &cat).unwrap();
        assert_eq!(
            oracle, binary,
            "case {case}: binary join disagrees with oracle\n{query:#?}\n{cat:#?}"
        );
        assert_eq!(
            oracle, generic,
            "case {case}: generic join disagrees with oracle\n{query:#?}\n{cat:#?}"
        );
        ran += 1;
        if !oracle.is_empty() {
            non_empty += 1;
        }
    }
    // Guard against a degenerate generator: it must neither skip most
    // cases nor produce only trivial ones.
    assert!(ran >= CASES * 8 / 10, "only {ran}/{CASES} cases ran");
    assert!(
        non_empty >= CASES / 10,
        "only {non_empty} non-empty results"
    );
}
