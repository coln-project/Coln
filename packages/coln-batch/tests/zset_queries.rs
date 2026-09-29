// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Z-set semantics of queries, on random inputs of every shape: relations
//! of arity 0 to 3, the extreme values of every type, negative weights,
//! rows stored in several copies, and heads that repeat a variable.
//!
//! Every executor is held against the definition in `common`, and against
//! laws that need no definition at all: a query is linear in every
//! relation it reads once, scaling a relation scales the result by a power
//! of the factor, and neither the order of the atoms, the numbering of the
//! variables nor the way rows are stored changes the result. Each case
//! seeds its own [`SplitMix64`], so a failing case number reproduces in
//! isolation.

mod common;

use coln_batch::fixpoint::Exec;
use coln_batch::query::{Atom, Catalog, Query, Term};
use coln_batch::relation::Relation;
use coln_batch::rng::SplitMix64;
use coln_batch::types::{Column, ScalarType, Schema, Weight};
use coln_batch::{binary_join, generic_join, reference};

use common::Storage;

const CASES: u64 = 1000;

const EXECUTORS: [(&str, Exec); 3] = [
    ("oracle", reference::execute as Exec),
    ("binary join", binary_join::execute as Exec),
    ("generic join", generic_join::execute as Exec),
];

/// A random catalog and a query over it.
struct Case {
    /// The generator, for further random choices.
    rng: SplitMix64,
    cat: Catalog,
    rels: Vec<(String, Schema)>,
    query: Query,
}

/// Case number `case`; `None` if its query came out without variables.
fn random_case(case: u64, signed: bool) -> Option<Case> {
    let mut rng = SplitMix64::new(case);
    let (cat, rels) = common::catalog(&mut rng, signed);
    let query = common::query(&mut rng, &rels)?;
    Some(Case {
        rng,
        cat,
        rels,
        query,
    })
}

fn run(name: &str, exec: Exec, query: &Query, cat: &Catalog, case: u64) -> Relation {
    exec(query, cat).unwrap_or_else(|e| panic!("case {case}: {name} failed: {e}\n{query:#?}"))
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

/// Hold every executor against the definition on the given cases.
fn against_the_definition(cases: std::ops::Range<u64>) -> Coverage {
    let mut seen = Coverage::default();
    for case in cases {
        let Some(Case { cat, query, .. }) = random_case(case, true) else {
            continue;
        };
        let expected = common::query_definition(&query, &common::stored_catalog(&cat));
        for (name, exec) in EXECUTORS {
            let result = run(name, exec, &query, &cat, case);
            assert!(
                result.is_consolidated(),
                "case {case}: {name} is not in normal form"
            );
            assert_eq!(
                common::zset_of(&result, cat.dictionary()),
                expected,
                "case {case}: {name} disagrees with the definition\n{query:#?}\n{cat:#?}"
            );
        }
        seen.ran += 1;
        seen.non_empty += usize::from(!expected.is_empty());
        seen.negative += usize::from(expected.values().any(|&w| w < 0));
        seen.nullary += usize::from(query.atoms.iter().any(|a| a.terms.is_empty()));
        let mut head = query.head.clone();
        head.sort_unstable();
        head.dedup();
        seen.repeated_head += usize::from(head.len() < query.head.len());
    }
    seen
}

#[test]
fn random_queries_match_the_definition() {
    let seen = against_the_definition(0..CASES);
    // Guard against a generator that stopped producing what the suite is for.
    let cases = CASES as usize;
    assert!(seen.ran >= cases * 3 / 4, "only {} cases ran", seen.ran);
    assert!(
        seen.non_empty >= cases / 5,
        "only {} non-empty results",
        seen.non_empty
    );
    assert!(
        seen.negative >= cases / 20,
        "only {} negative results",
        seen.negative
    );
    assert!(
        seen.nullary >= cases / 20,
        "only {} nullary atoms",
        seen.nullary
    );
    assert!(
        seen.repeated_head >= cases / 20,
        "only {} repeats",
        seen.repeated_head
    );
}

/// The same check on 50 000 further cases.
/// Run with: cargo test -p coln-batch --release -- --include-ignored
#[test]
#[ignore = "large; run explicitly (use --release)"]
fn random_queries_match_the_definition_at_scale() {
    let seen = against_the_definition(CASES..CASES + 50_000);
    assert!(
        seen.non_empty >= 10_000,
        "only {} non-empty results",
        seen.non_empty
    );
}

/// The stored rows of `rel` in two parts that add up to it: each row goes
/// to one part or has its weight split across both.
fn split(rng: &mut SplitMix64, rel: &Relation) -> (Relation, Relation) {
    let mut parts: [(Vec<usize>, Vec<Weight>); 2] = Default::default();
    for i in 0..rel.len() {
        let w = rel.weight(i);
        match rng.below(3) {
            0 => {
                parts[0].0.push(i);
                parts[0].1.push(w);
            }
            1 => {
                parts[1].0.push(i);
                parts[1].1.push(w);
            }
            _ => {
                let k = common::pick(rng, &[-1, 1, 2]);
                parts[0].0.push(i);
                parts[0].1.push(w - k);
                parts[1].0.push(i);
                parts[1].1.push(k);
            }
        }
    }
    let [a, b] = parts.map(|(rows, weights)| {
        let cols = rel
            .cols
            .iter()
            .map(|col| rows.iter().map(|&i| col[i]).collect())
            .collect();
        Relation::with_weights(rel.name.clone(), rel.schema.clone(), cols, weights)
    });
    (a, b)
}

fn replaced(cat: &Catalog, rel: Relation) -> Catalog {
    let mut cat = cat.clone();
    cat.insert(rel);
    cat
}

#[test]
fn a_query_is_linear_in_every_relation_it_reads_once() {
    let mut checked = 0;
    for case in 0..CASES {
        let Some(Case {
            mut rng,
            cat,
            rels,
            query,
        }) = random_case(case, true)
        else {
            continue;
        };
        for (name, _) in &rels {
            if query.atoms.iter().filter(|a| &a.relation == name).count() != 1 {
                continue;
            }
            let (a, b) = split(&mut rng, cat.get(name).unwrap());
            let (with_a, with_b) = (replaced(&cat, a), replaced(&cat, b));
            for (exec_name, exec) in EXECUTORS {
                let whole = run(exec_name, exec, &query, &cat, case);
                let parts = run(exec_name, exec, &query, &with_a, case)
                    .plus(&run(exec_name, exec, &query, &with_b, case));
                assert_eq!(
                    whole, parts,
                    "case {case}: {exec_name} is not linear in {name}"
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked >= CASES as usize / 2,
        "only {checked} relations checked"
    );
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
    let mut checked = 0;
    let mut squares = 0;
    for case in 0..CASES {
        let Some(Case {
            mut rng,
            cat,
            query,
            ..
        }) = random_case(case, true)
        else {
            continue;
        };
        let atom = &query.atoms[rng.below(query.atoms.len() as u64) as usize];
        let times = query
            .atoms
            .iter()
            .filter(|a| a.relation == atom.relation)
            .count() as u32;
        let k: Weight = common::pick(&mut rng, &[-1, 2, 3, -2]);
        let with_scaled = replaced(&cat, scaled(cat.get(&atom.relation).unwrap(), k));
        for (name, exec) in EXECUTORS {
            let expected = scaled(&run(name, exec, &query, &cat, case), k.pow(times));
            let result = run(name, exec, &query, &with_scaled, case);
            assert_eq!(
                result, expected,
                "case {case}: {name}, {} scaled by {k}, read {times} times",
                atom.relation
            );
        }
        checked += 1;
        squares += usize::from(times > 1);
    }
    assert!(
        checked >= CASES as usize * 3 / 4,
        "only {checked} cases checked"
    );
    assert!(
        squares >= CASES as usize / 10,
        "only {squares} relations read twice or more"
    );
}

#[test]
fn how_rows_are_stored_does_not_matter() {
    // The same rows, stored in normal form, as split copies, and as copies
    // of weight 1 or -1, must give the same result.
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
            let results: Vec<_> = forms
                .iter()
                .map(|cat| common::zset_of(&run(name, exec, &query, cat, case), cat.dictionary()))
                .collect();
            assert_eq!(results[0], results[1], "case {case}: {name}, copies differ");
            assert_eq!(
                results[0], results[2],
                "case {case}: {name}, unit copies differ"
            );
        }
        checked += 1;
    }
    assert!(
        checked >= CASES as usize * 3 / 4,
        "only {checked} cases checked"
    );
}

#[test]
fn neither_atom_order_nor_variable_numbering_matters() {
    // The generic join eliminates variables in numbering order and sorts
    // every table to match, so a new numbering exercises a different search.
    let mut checked = 0;
    for case in 0..CASES {
        let Some(Case {
            mut rng,
            cat,
            query,
            ..
        }) = random_case(case, true)
        else {
            continue;
        };
        let mut atoms = query.atoms.clone();
        common::shuffle(&mut rng, &mut atoms);
        let mut renaming: Vec<usize> = (0..query.num_vars()).collect();
        common::shuffle(&mut rng, &mut renaming);
        for atom in &mut atoms {
            for term in &mut atom.terms {
                if let Term::Var(v) = term {
                    *v = renaming[*v];
                }
            }
        }
        let mut var_names = query.var_names.clone();
        for (v, name) in query.var_names.iter().enumerate() {
            var_names[renaming[v]] = name.clone();
        }
        let reordered = Query {
            var_names,
            atoms,
            head: query.head.iter().map(|&v| renaming[v]).collect(),
        };
        for (name, exec) in EXECUTORS {
            let result = run(name, exec, &query, &cat, case);
            let other = run(name, exec, &reordered, &cat, case);
            assert_eq!(
                result, other,
                "case {case}: {name}\n{query:#?}\n{reordered:#?}"
            );
        }
        checked += 1;
    }
    assert!(
        checked >= CASES as usize * 3 / 4,
        "only {checked} cases checked"
    );
}

#[test]
fn over_positive_weights_a_query_finds_the_rows_it_finds_over_sets() {
    let mut checked = 0;
    for case in 0..CASES {
        let Some(Case {
            cat, rels, query, ..
        }) = random_case(case, false)
        else {
            continue;
        };
        let mut sets = cat.clone();
        for (name, _) in &rels {
            sets.insert(cat.get(name).unwrap().clone().distinct());
        }
        for (name, exec) in EXECUTORS {
            let over_weights = run(name, exec, &query, &cat, case);
            let over_sets = run(name, exec, &query, &sets, case);
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
        checked += 1;
    }
    assert!(
        checked >= CASES as usize * 3 / 4,
        "only {checked} cases checked"
    );
}

fn atom(relation: &str, terms: Vec<Term>) -> Atom {
    Atom {
        relation: relation.into(),
        terms,
    }
}

/// A relation of uint columns with given rows and weights, stored as is.
fn stored(name: &str, arity: usize, rows: &[(&[u64], Weight)]) -> Relation {
    let cols = (0..arity)
        .map(|c| rows.iter().map(|(row, _)| row[c]).collect())
        .collect();
    let schema = Schema::uint((0..arity).map(|c| format!("c{c}")));
    Relation::with_weights(name, schema, cols, rows.iter().map(|&(_, w)| w).collect())
}

/// Run every executor; they must agree, and the result is returned as
/// (row, weight) pairs.
fn agreed(query: &Query, cat: &Catalog) -> Vec<(Vec<u64>, Weight)> {
    let results: Vec<Relation> = EXECUTORS
        .iter()
        .map(|&(name, exec)| run(name, exec, query, cat, 0))
        .collect();
    for (result, (name, _)) in results.iter().zip(EXECUTORS).skip(1) {
        assert_eq!(&results[0], result, "{name} disagrees with the oracle");
    }
    let r = &results[0];
    (0..r.len()).map(|i| (r.row(i), r.weight(i))).collect()
}

#[test]
fn a_nullary_relation_is_a_scalar() {
    // Q(x) ← R(x), Z(): the one row of Z multiplies every row of R by its
    // weight, whatever copies it is stored in.
    let query = Query {
        var_names: vec!["x".into()],
        atoms: vec![atom("R", vec![Term::Var(0)]), atom("Z", vec![])],
        head: vec![0],
    };
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
        assert_eq!(agreed(&query, &cat), expected, "Z stored as {z:?}");
    }
}

#[test]
fn a_head_may_repeat_a_variable() {
    // Q(x, x, y) ← R(x, y): the head holds x twice.
    let mut cat = Catalog::new();
    cat.insert(stored("R", 2, &[(&[1, 5], 2), (&[3, 4], -1)]));
    let query = Query {
        var_names: vec!["x".into(), "y".into()],
        atoms: vec![atom("R", vec![Term::Var(0), Term::Var(1)])],
        head: vec![0, 0, 1],
    };
    assert_eq!(
        agreed(&query, &cat),
        vec![(vec![1, 1, 5], 2), (vec![3, 3, 4], -1)]
    );
}

#[test]
fn rows_of_weight_zero_change_nothing() {
    let query = Query {
        var_names: vec!["x".into(), "y".into()],
        atoms: vec![
            atom("R", vec![Term::Var(0)]),
            atom("S", vec![Term::Var(0), Term::Var(1)]),
        ],
        head: vec![0, 1],
    };
    let mut plain = Catalog::new();
    plain.insert(stored("R", 1, &[(&[1], 2)]));
    plain.insert(stored("S", 2, &[(&[1, 7], 3)]));
    let mut padded = Catalog::new();
    padded.insert(stored(
        "R",
        1,
        &[(&[0], 0), (&[1], 2), (&[1], 0), (&[9], 0)],
    ));
    padded.insert(stored("S", 2, &[(&[1, 8], 0), (&[1, 7], 3), (&[9, 9], 0)]));
    assert_eq!(agreed(&query, &plain), vec![(vec![1, 7], 6)]);
    assert_eq!(agreed(&query, &padded), agreed(&query, &plain));
}

#[test]
fn a_relation_read_three_times_cubes_its_weights() {
    // Q(x) ← R(x), R(x), R(x) over R(1) of weight -2: (-2)³ = -8.
    let mut cat = Catalog::new();
    cat.insert(stored("R", 1, &[(&[1], -2)]));
    let query = Query {
        var_names: vec!["x".into()],
        atoms: vec![atom("R", vec![Term::Var(0)]); 3],
        head: vec![0],
    };
    assert_eq!(agreed(&query, &cat), vec![(vec![1], -8)]);
}

#[test]
fn a_literal_nothing_holds_gives_the_empty_z_set() {
    let mut cat = Catalog::new();
    cat.insert_rows(
        "L",
        Schema::new([
            Column::new("s", ScalarType::String),
            Column::new("n", ScalarType::Uint),
        ]),
        vec![vec!["ant".into(), 1u64.into()]],
    )
    .unwrap();
    let query = Query {
        var_names: vec!["n".into()],
        atoms: vec![atom("L", vec![Term::lit("nowhere"), Term::Var(0)])],
        head: vec![0],
    };
    assert_eq!(agreed(&query, &cat), vec![]);
}

/// Q(x) ← R(x), S(x) over weights 2³² and 2³², whose product leaves `i64`.
fn overflowing_product() -> (Query, Catalog) {
    let mut cat = Catalog::new();
    cat.insert(stored("R", 1, &[(&[1], 1 << 32)]));
    cat.insert(stored("S", 1, &[(&[1], 1 << 32)]));
    let query = Query {
        var_names: vec!["x".into()],
        atoms: vec![atom("R", vec![Term::Var(0)]), atom("S", vec![Term::Var(0)])],
        head: vec![0],
    };
    (query, cat)
}

#[test]
#[should_panic(expected = "weight overflow")]
fn an_overflowing_product_panics_in_the_generic_join() {
    let (query, cat) = overflowing_product();
    let _ = generic_join::execute(&query, &cat);
}

#[test]
#[should_panic(expected = "weight overflow")]
fn an_overflowing_product_panics_in_the_binary_join() {
    let (query, cat) = overflowing_product();
    let _ = binary_join::execute(&query, &cat);
}

#[test]
#[should_panic(expected = "weight overflow")]
fn an_overflowing_product_panics_in_the_oracle() {
    let (query, cat) = overflowing_product();
    let _ = reference::execute(&query, &cat);
}

#[test]
fn products_up_to_the_largest_weight_are_exact() {
    // 2³¹ · (2³² - 1) stays in range, and two bindings adding up to
    // i64::MAX do too.
    let mut cat = Catalog::new();
    cat.insert(stored("R", 1, &[(&[1], 1 << 31)]));
    cat.insert(stored("S", 1, &[(&[1], (1 << 32) - 1)]));
    let (query, _) = overflowing_product();
    assert_eq!(
        agreed(&query, &cat),
        vec![(vec![1], (1 << 31) * ((1 << 32) - 1))]
    );

    cat.insert(stored("R", 2, &[(&[1, 1], Weight::MAX - 1), (&[1, 2], 1)]));
    let project = Query {
        var_names: vec!["x".into(), "y".into()],
        atoms: vec![atom("R", vec![Term::Var(0), Term::Var(1)])],
        head: vec![0],
    };
    assert_eq!(agreed(&project, &cat), vec![(vec![1], Weight::MAX)]);
}
