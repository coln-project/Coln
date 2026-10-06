// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Z-set semantics of queries on random inputs of every shape: relations of
//! arity 0 to 3, the extreme values of every type, negative weights, rows
//! stored in several copies, heads that repeat a variable.
//!
//! Every executor is held against the definition in `common` and against
//! laws that need no definition: a query is linear in every relation it
//! reads once, scaling a relation scales the result by a power, and neither
//! the order of the atoms, the numbering of the variables nor the way rows
//! are stored matters. Each case seeds its own [`SplitMix64`], so a failing
//! case number reproduces in isolation.

mod common;

use std::collections::BTreeSet;
use std::ops::Range;

use coln_batch::fixpoint::Exec;
use coln_batch::query::{Atom, Catalog, Query, Term};
use coln_batch::relation::Relation;
use coln_batch::rng::SplitMix64;
use coln_batch::types::{Column, ScalarType, Schema, Weight};
use coln_batch::{binary_join, generic_join, reference};

use common::{EXECUTORS, Storage, atom, entries, stored};

const CASES: u64 = 1000;

/// Run `check` on every case in `cases` whose query has a variable, with
/// the case's generator for further choices; returns how many ran.
fn each_case(
    cases: Range<u64>,
    signed: bool,
    mut check: impl FnMut(u64, &mut SplitMix64, &Catalog, &[(String, Schema)], &Query),
) -> usize {
    let mut ran = 0;
    for case in cases {
        let mut rng = SplitMix64::new(case);
        let (cat, rels) = common::catalog(&mut rng, signed);
        if let Some(query) = common::query(&mut rng, &rels) {
            check(case, &mut rng, &cat, &rels, &query);
            ran += 1;
        }
    }
    ran
}

fn run(name: &str, exec: Exec, query: &Query, cat: &Catalog, case: u64) -> Relation {
    exec(query, cat).unwrap_or_else(|e| panic!("case {case}: {name} failed: {e}\n{query:#?}"))
}

fn replaced(cat: &Catalog, rel: Relation) -> Catalog {
    let mut cat = cat.clone();
    cat.insert(rel);
    cat
}

/// How much of the space of inputs a run of cases covered.
#[derive(Default)]
struct Coverage {
    ran: usize,
    non_empty: usize,
    negative: usize,
    nullary: usize,
    repeated_head: usize,
}

fn against_the_definition(cases: Range<u64>) -> Coverage {
    let mut seen = Coverage::default();
    seen.ran = each_case(cases, true, |case, _, cat, _, query| {
        let expected = common::query_definition(query, &common::stored_catalog(cat));
        for (name, exec) in EXECUTORS {
            let result = run(name, exec, query, cat, case);
            assert!(
                result.is_consolidated(),
                "case {case}: {name}: no normal form"
            );
            assert_eq!(
                common::zset_of(&result, cat.dictionary()),
                expected,
                "case {case}: {name} disagrees with the definition\n{query:#?}\n{cat:#?}"
            );
        }
        seen.non_empty += usize::from(!expected.is_empty());
        seen.negative += usize::from(expected.values().any(|&w| w < 0));
        seen.nullary += usize::from(query.atoms.iter().any(|a| a.terms.is_empty()));
        let head: BTreeSet<_> = query.head.iter().collect();
        seen.repeated_head += usize::from(head.len() < query.head.len());
    });
    seen
}

#[test]
fn random_queries_match_the_definition() {
    let seen = against_the_definition(0..CASES);
    // Guard against a generator that stopped producing what the suite is for.
    let cases = CASES as usize;
    assert!(seen.ran >= cases * 3 / 4, "{} ran", seen.ran);
    assert!(seen.non_empty >= cases / 5, "{} non-empty", seen.non_empty);
    assert!(seen.negative >= cases / 20, "{} negative", seen.negative);
    assert!(seen.nullary >= cases / 20, "{} nullary", seen.nullary);
    assert!(
        seen.repeated_head >= cases / 20,
        "{} repeats",
        seen.repeated_head
    );
}

/// The same check on 50 000 further cases.
/// Run with: cargo test -p coln-batch --release -- --include-ignored
#[test]
#[ignore = "large; run explicitly (use --release)"]
fn random_queries_match_the_definition_at_scale() {
    let seen = against_the_definition(CASES..CASES + 50_000);
    assert!(seen.non_empty >= 10_000, "{} non-empty", seen.non_empty);
}

/// `rel`'s stored rows in two parts that add up to it: each row goes to one
/// part, or its weight is split across both.
fn split(rng: &mut SplitMix64, rel: &Relation) -> [Relation; 2] {
    let mut parts: [(Vec<usize>, Vec<Weight>); 2] = Default::default();
    for (i, &w) in rel.weights.iter().enumerate() {
        let weights = match rng.below(3) {
            0 => [Some(w), None],
            1 => [None, Some(w)],
            _ => {
                let k = common::pick(rng, &[-1, 1, 2]);
                [Some(w - k), Some(k)]
            }
        };
        for (part, weight) in parts.iter_mut().zip(weights) {
            if let Some(weight) = weight {
                part.0.push(i);
                part.1.push(weight);
            }
        }
    }
    parts.map(|(rows, weights)| {
        let cols = rel
            .cols
            .iter()
            .map(|col| rows.iter().map(|&i| col[i]).collect())
            .collect();
        Relation::with_weights(rel.name.clone(), rel.schema.clone(), cols, weights)
    })
}

#[test]
fn a_query_is_linear_in_every_relation_it_reads_once() {
    let mut checked = 0;
    each_case(0..CASES, true, |case, rng, cat, rels, query| {
        for (name, _) in rels {
            if query.atoms.iter().filter(|a| &a.relation == name).count() != 1 {
                continue;
            }
            let [with_a, with_b] = split(rng, cat.get(name).unwrap()).map(|p| replaced(cat, p));
            for (exec_name, exec) in EXECUTORS {
                let whole = run(exec_name, exec, query, cat, case);
                let parts = run(exec_name, exec, query, &with_a, case)
                    .plus(&run(exec_name, exec, query, &with_b, case));
                assert_eq!(
                    whole, parts,
                    "case {case}: {exec_name} not linear in {name}"
                );
            }
            checked += 1;
        }
    });
    assert!(checked >= CASES as usize / 2, "{checked} checked");
}

fn scaled(rel: &Relation, k: Weight) -> Relation {
    let mut rel = rel.clone();
    for w in &mut rel.weights {
        *w *= k;
    }
    rel
}

#[test]
fn scaling_a_relation_scales_the_result_by_a_power() {
    let mut squares = 0;
    let checked = each_case(0..CASES, true, |case, rng, cat, _, query| {
        let atom = &query.atoms[rng.below(query.atoms.len() as u64) as usize];
        let times = query
            .atoms
            .iter()
            .filter(|a| a.relation == atom.relation)
            .count();
        let k: Weight = common::pick(rng, &[-1, 2, 3, -2]);
        let with_scaled = replaced(cat, scaled(cat.get(&atom.relation).unwrap(), k));
        for (name, exec) in EXECUTORS {
            let expected = scaled(&run(name, exec, query, cat, case), k.pow(times as u32));
            let result = run(name, exec, query, &with_scaled, case);
            assert_eq!(
                result, expected,
                "case {case}: {name}, scaled by {k} read {times} times"
            );
        }
        squares += usize::from(times > 1);
    });
    assert!(checked >= CASES as usize * 3 / 4, "{checked} checked");
    assert!(
        squares >= CASES as usize / 10,
        "{squares} read twice or more"
    );
}

#[test]
fn how_rows_are_stored_does_not_matter() {
    // The same rows in normal form, as split copies, and as copies of weight
    // 1 or -1.
    let mut checked = 0;
    for case in 0..CASES {
        let mut rng = SplitMix64::new(case);
        let (_, rels) = common::catalog(&mut rng, true);
        let contents: Vec<_> = rels
            .iter()
            .map(|(name, schema)| (name, schema, common::rows(&mut rng, schema, 8, true)))
            .collect();
        let Some(query) = common::query(&mut rng, &rels) else {
            continue;
        };
        let forms = [Storage::Consolidated, Storage::Copies, Storage::Units].map(|storage| {
            let mut cat = Catalog::new();
            for (name, schema, rows) in &contents {
                common::store(&mut rng, &mut cat, name, schema, rows.clone(), storage);
            }
            cat
        });
        for (name, exec) in EXECUTORS {
            let [a, b, c] = forms
                .each_ref()
                .map(|cat| common::zset_of(&run(name, exec, &query, cat, case), cat.dictionary()));
            assert!(
                a == b && a == c,
                "case {case}: {name} depends on the storage"
            );
        }
        checked += 1;
    }
    assert!(checked >= CASES as usize * 3 / 4, "{checked} checked");
}

#[test]
fn neither_atom_order_nor_variable_numbering_matters() {
    // The generic join eliminates variables in numbering order and sorts
    // every table to match, so a new numbering runs a different search.
    let checked = each_case(0..CASES, true, |case, rng, cat, _, query| {
        let mut renaming: Vec<usize> = (0..query.num_vars()).collect();
        let mut reordered = query.clone();
        common::shuffle(rng, &mut reordered.atoms);
        common::shuffle(rng, &mut renaming);
        for term in reordered.atoms.iter_mut().flat_map(|a| &mut a.terms) {
            if let Term::Var(v) = term {
                *v = renaming[*v];
            }
        }
        for (v, name) in query.var_names.iter().enumerate() {
            reordered.var_names[renaming[v]] = name.clone();
        }
        reordered.head = query.head.iter().map(|&v| renaming[v]).collect();
        for (name, exec) in EXECUTORS {
            let result = run(name, exec, query, cat, case);
            let other = run(name, exec, &reordered, cat, case);
            assert_eq!(
                result, other,
                "case {case}: {name}\n{query:#?}\n{reordered:#?}"
            );
        }
    });
    assert!(checked >= CASES as usize * 3 / 4, "{checked} checked");
}

#[test]
fn over_positive_weights_a_query_finds_the_rows_it_finds_over_sets() {
    let checked = each_case(0..CASES, false, |case, _, cat, rels, query| {
        let mut sets = cat.clone();
        for (name, _) in rels {
            sets.insert(cat.get(name).unwrap().clone().distinct());
        }
        for (name, exec) in EXECUTORS {
            let over_weights = run(name, exec, query, cat, case);
            let over_sets = run(name, exec, query, &sets, case);
            assert!(
                over_weights.weights.iter().all(|&w| w > 0),
                "case {case}: {name} derived a weight below 1 from positive input"
            );
            assert_eq!(
                over_weights.distinct(),
                over_sets.distinct(),
                "case {case}: {name}"
            );
        }
    });
    assert!(checked >= CASES as usize * 3 / 4, "{checked} checked");
}

// ---------------------------------------------------------------------------
// Hand-picked cases

fn query(num_vars: usize, atoms: Vec<Atom>, head: Vec<usize>) -> Query {
    Query {
        var_names: (0..num_vars).map(|v| format!("v{v}")).collect(),
        atoms,
        head,
    }
}

/// Run every executor; they must agree. Returns the result's entries.
fn agreed(query: &Query, cat: &Catalog) -> Vec<(Vec<u64>, Weight)> {
    let [oracle, rest @ ..] = EXECUTORS.map(|(name, exec)| run(name, exec, query, cat, 0));
    for (result, (name, _)) in rest.iter().zip(&EXECUTORS[1..]) {
        assert_eq!(&oracle, result, "{name} disagrees with the oracle");
    }
    entries(&oracle)
}

#[test]
fn joins_multiply_weights_and_projections_add_them_up() {
    let v = Term::Var;
    let mut cat = Catalog::new();
    // Edge (1, 2) is present twice, edge (2, 3) three times.
    cat.insert(stored("E", 2, &[(&[1, 2], 2), (&[2, 3], 3)]));
    // Three ways from 1 to 3, one of them taken away.
    cat.insert(stored("P", 2, &[(&[1, 5], 1), (&[1, 6], 1), (&[1, 7], -1)]));
    cat.insert(stored("Q", 2, &[(&[5, 3], 1), (&[6, 3], 1), (&[7, 3], 1)]));
    // Two rows for x = 1 that cancel out, one for x = 2.
    cat.insert(stored("C", 2, &[(&[1, 6], 1), (&[1, 7], -1), (&[2, 8], 1)]));
    cat.insert(stored("F", 1, &[(&[9], 4)]));

    // Two hops: 2 · 3 = 6 ways from 1 to 3.
    let two_hop = query(
        3,
        vec![atom("E", vec![v(0), v(1)]), atom("E", vec![v(1), v(2)])],
        vec![0, 2],
    );
    assert_eq!(agreed(&two_hop, &cat), vec![(vec![1, 3], 6)]);
    // Every path keeps its own weight, the negative one included; projected
    // onto its ends, they add up to 1 + 1 - 1.
    let paths = query(
        3,
        vec![atom("P", vec![v(0), v(1)]), atom("Q", vec![v(1), v(2)])],
        vec![0, 1, 2],
    );
    let path_weights: Vec<Weight> = agreed(&paths, &cat).iter().map(|(_, w)| *w).collect();
    assert_eq!(path_weights, vec![1, 1, -1]);
    let ends = Query {
        head: vec![0, 2],
        ..paths
    };
    assert_eq!(agreed(&ends, &cat), vec![(vec![1, 3], 1)]);
    // Weights that cancel out remove the row.
    let cancel = query(2, vec![atom("C", vec![v(0), v(1)])], vec![0]);
    assert_eq!(agreed(&cancel, &cat), vec![(vec![2], 1)]);
    // An atom of literals multiplies by the weight of its row.
    let scaled = query(
        2,
        vec![
            atom("E", vec![v(0), v(1)]),
            atom("F", vec![Term::lit(9u64)]),
        ],
        vec![0, 1],
    );
    assert_eq!(
        agreed(&scaled, &cat),
        vec![(vec![1, 2], 8), (vec![2, 3], 12)]
    );
}

#[test]
fn a_nullary_relation_is_a_scalar() {
    // Q(x) ← R(x), Z(): the one row of Z multiplies every row of R by its
    // weight, whatever copies it is stored in.
    let q = query(
        1,
        vec![atom("R", vec![Term::Var(0)]), atom("Z", vec![])],
        vec![0],
    );
    let mut cat = Catalog::new();
    cat.insert(stored("R", 1, &[(&[1], 1), (&[2], 2)]));
    for (z, expected) in [
        (vec![3], vec![(vec![1], 3), (vec![2], 6)]),
        (vec![-1], vec![(vec![1], -1), (vec![2], -2)]),
        (vec![2, 1], vec![(vec![1], 3), (vec![2], 6)]),
        (vec![2, -2], vec![]),
        (vec![], vec![]),
    ] {
        let rows: Vec<(&[u64], Weight)> = z.iter().map(|&w| (&[][..], w)).collect();
        cat.insert(stored("Z", 0, &rows));
        assert_eq!(agreed(&q, &cat), expected, "Z stored as {z:?}");
    }
}

#[test]
fn a_head_may_repeat_a_variable() {
    // Q(x, x, y) ← R(x, y)
    let mut cat = Catalog::new();
    cat.insert(stored("R", 2, &[(&[1, 5], 2), (&[3, 4], -1)]));
    let q = query(
        2,
        vec![atom("R", vec![Term::Var(0), Term::Var(1)])],
        vec![0, 0, 1],
    );
    assert_eq!(
        agreed(&q, &cat),
        vec![(vec![1, 1, 5], 2), (vec![3, 3, 4], -1)]
    );
}

#[test]
fn rows_of_weight_zero_change_nothing() {
    // Q(x, y) ← R(x), S(x, y)
    let q = query(
        2,
        vec![
            atom("R", vec![Term::Var(0)]),
            atom("S", vec![Term::Var(0), Term::Var(1)]),
        ],
        vec![0, 1],
    );
    let mut cat = Catalog::new();
    cat.insert(stored("R", 1, &[(&[1], 2)]));
    cat.insert(stored("S", 2, &[(&[1, 7], 3)]));
    assert_eq!(agreed(&q, &cat), vec![(vec![1, 7], 6)]);
    cat.insert(stored(
        "R",
        1,
        &[(&[0], 0), (&[1], 2), (&[1], 0), (&[9], 0)],
    ));
    cat.insert(stored("S", 2, &[(&[1, 8], 0), (&[1, 7], 3), (&[9, 9], 0)]));
    assert_eq!(agreed(&q, &cat), vec![(vec![1, 7], 6)]);
}

#[test]
fn a_relation_read_three_times_cubes_its_weights() {
    // Q(x) ← R(x), R(x), R(x) over R(1) of weight -2: (-2)³ = -8.
    let mut cat = Catalog::new();
    cat.insert(stored("R", 1, &[(&[1], -2)]));
    let q = query(1, vec![atom("R", vec![Term::Var(0)]); 3], vec![0]);
    assert_eq!(agreed(&q, &cat), vec![(vec![1], -8)]);
}

#[test]
fn a_literal_nothing_holds_gives_the_empty_z_set() {
    let mut cat = Catalog::new();
    let schema = Schema::new([
        Column::new("s", ScalarType::String),
        Column::new("n", ScalarType::Uint),
    ]);
    cat.insert_rows("L", schema, vec![vec!["ant".into(), 1u64.into()]])
        .unwrap();
    let q = query(
        1,
        vec![atom("L", vec![Term::lit("nowhere"), Term::Var(0)])],
        vec![0],
    );
    assert_eq!(agreed(&q, &cat), vec![]);
}

/// Q(x) ← R(x), S(x) over rows of weight `r` and `s`.
fn product(r: Weight, s: Weight) -> (Query, Catalog) {
    let mut cat = Catalog::new();
    cat.insert(stored("R", 1, &[(&[1], r)]));
    cat.insert(stored("S", 1, &[(&[1], s)]));
    let q = query(
        1,
        vec![atom("R", vec![Term::Var(0)]), atom("S", vec![Term::Var(0)])],
        vec![0],
    );
    (q, cat)
}

#[test]
#[should_panic(expected = "weight overflow")]
fn an_overflowing_product_panics_in_the_generic_join() {
    let (q, cat) = product(1 << 32, 1 << 32);
    let _ = generic_join::execute(&q, &cat);
}

#[test]
#[should_panic(expected = "weight overflow")]
fn an_overflowing_product_panics_in_the_binary_join() {
    let (q, cat) = product(1 << 32, 1 << 32);
    let _ = binary_join::execute(&q, &cat);
}

#[test]
#[should_panic(expected = "weight overflow")]
fn an_overflowing_product_panics_in_the_oracle() {
    let (q, cat) = product(1 << 32, 1 << 32);
    let _ = reference::execute(&q, &cat);
}

#[test]
fn products_and_sums_up_to_the_largest_weight_are_exact() {
    let (q, cat) = product(1 << 31, (1 << 32) - 1);
    assert_eq!(
        agreed(&q, &cat),
        vec![(vec![1], (1 << 31) * ((1 << 32) - 1))]
    );

    // Two bindings adding up to i64::MAX.
    let mut cat = Catalog::new();
    cat.insert(stored("R", 2, &[(&[1, 1], Weight::MAX - 1), (&[1, 2], 1)]));
    let project = query(
        2,
        vec![atom("R", vec![Term::Var(0), Term::Var(1)])],
        vec![0],
    );
    assert_eq!(agreed(&project, &cat), vec![(vec![1], Weight::MAX)]);
}
