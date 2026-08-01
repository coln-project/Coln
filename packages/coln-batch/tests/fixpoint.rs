// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Differential testing for recursive evaluation: semi-naive must agree
//! with the naive oracle, with either query executor underneath — and
//! with exact expected results where we know them.

use coln_batch::fixpoint::{self, Exec};
use coln_batch::query::{Atom, Catalog, Term};
use coln_batch::relation::Relation;
use coln_batch::rule::Program;
use coln_batch::{binary_join, fixtures, generic_join, reference};

/// Run the program under every (strategy × executor) combination and
/// require identical IDB results; returns the semi-naive/generic one.
fn agree(program: &Program, edb: &Catalog, idb_names: &[&str]) -> Catalog {
    let runs: Vec<(&str, Catalog)> = vec![
        ("semi+generic", {
            fixpoint::semi_naive(program, edb, generic_join::execute as Exec)
                .unwrap()
                .catalog
        }),
        ("semi+binary", {
            fixpoint::semi_naive(program, edb, binary_join::execute as Exec)
                .unwrap()
                .catalog
        }),
        ("naive+generic", {
            fixpoint::naive(program, edb, generic_join::execute as Exec)
                .unwrap()
                .catalog
        }),
        ("naive+binary", {
            fixpoint::naive(program, edb, binary_join::execute as Exec)
                .unwrap()
                .catalog
        }),
    ];
    let (base_name, base) = &runs[0];
    for (name, other) in &runs[1..] {
        for idb in idb_names {
            assert_eq!(
                base.get(idb).unwrap().cols,
                other.get(idb).unwrap().cols,
                "{name} disagrees with {base_name} on {idb}"
            );
        }
    }
    runs.into_iter().next().unwrap().1
}

fn rows(rel: &Relation) -> Vec<Vec<u64>> {
    (0..rel.len()).map(|i| rel.row(i)).collect()
}

#[test]
fn chain_has_exact_closure() {
    let k = 6;
    let edb = fixtures::ancestor_chain_catalog(k);
    let program = fixtures::ancestor_program();

    let result = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec).unwrap();
    let ancestor = result.catalog.get("ancestor").unwrap();
    assert_eq!(ancestor.len() as u64, k * (k - 1) / 2);
    // Rounds: one per path length, plus the final empty round.
    assert_eq!(result.stats.rounds as u64, k);
    assert_eq!(result.stats.new_facts_per_round, vec![5, 4, 3, 2, 1, 0]);

    agree(&program, &edb, &["ancestor"]);
}

#[test]
fn dag_strategies_and_executors_agree() {
    let edb = fixtures::ancestor_dag_catalog(2_000, 4_000, 11);
    let program = fixtures::ancestor_program();
    let result = agree(&program, &edb, &["ancestor"]);
    let ancestor = result.get("ancestor").unwrap();
    assert!(ancestor.len() > 4_000, "closure should exceed the edge set");
}

#[test]
fn points_to_matches_souffle_reference() {
    let program = fixtures::points_to::program();
    let edb = fixtures::points_to::catalog();

    let result = agree(&program, &edb, &["VarPointsTo", "CallGraph"]);
    assert_eq!(
        rows(result.get("VarPointsTo").unwrap()),
        {
            let mut expected = fixtures::points_to::expected_var_points_to();
            expected.sort();
            expected
        },
        "VarPointsTo"
    );
    assert_eq!(
        rows(result.get("CallGraph").unwrap()),
        {
            let mut expected = fixtures::points_to::expected_call_graph();
            expected.sort();
            expected
        },
        "CallGraph"
    );
}

#[test]
fn initial_idb_facts_are_respected() {
    // parent: 0 -> 1 -> 2, plus a pre-seeded ancestor fact (7, 8).
    let mut edb = fixtures::ancestor_chain_catalog(3);
    edb.insert(Relation::new(
        "ancestor",
        ["x", "y"],
        vec![vec![7], vec![8]],
    ));
    let program = fixtures::ancestor_program();
    let result = agree(&program, &edb, &["ancestor"]);
    assert_eq!(
        rows(result.get("ancestor").unwrap()),
        vec![vec![0, 1], vec![0, 2], vec![1, 2], vec![7, 8],]
    );
}

#[test]
fn recursion_also_works_with_the_reference_executor() {
    // The brute-force query oracle can drive the fixpoint too (tiny data).
    let edb = fixtures::ancestor_chain_catalog(5);
    let program = fixtures::ancestor_program();
    let via_reference = fixpoint::semi_naive(&program, &edb, reference::execute as Exec)
        .unwrap()
        .catalog;
    let via_generic = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec)
        .unwrap()
        .catalog;
    assert_eq!(
        via_reference.get("ancestor").unwrap().cols,
        via_generic.get("ancestor").unwrap().cols
    );
}

#[test]
fn empty_edb_terminates_with_empty_idb() {
    // No parent facts at all: the closure is empty, and evaluation stops
    // after a single round instead of looping or crashing.
    let mut edb = Catalog::new();
    edb.insert(Relation::new(
        "parent",
        ["x", "y"],
        vec![Vec::new(), Vec::new()],
    ));
    let program = fixtures::ancestor_program();

    let result = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec).unwrap();
    assert_eq!(result.catalog.get("ancestor").unwrap().len(), 0);
    assert_eq!(result.stats.new_facts_per_round, vec![0]);

    agree(&program, &edb, &["ancestor"]);
}

#[test]
fn head_literals_work() {
    // flagged(x, 1) ← parent(x, y) — a head with a literal column.
    let program = Program {
        rules: vec![coln_batch::rule::Rule {
            var_names: vec!["x".into(), "y".into()],
            head: Atom {
                relation: "flagged".into(),
                terms: vec![Term::Var(0), Term::Lit(1)],
            },
            body: vec![Atom {
                relation: "parent".into(),
                terms: vec![Term::Var(0), Term::Var(1)],
            }],
        }],
    };
    let edb = fixtures::ancestor_chain_catalog(4);
    let result = agree(&program, &edb, &["flagged"]);
    assert_eq!(
        rows(result.get("flagged").unwrap()),
        vec![vec![0, 1], vec![1, 1], vec![2, 1]]
    );
}
