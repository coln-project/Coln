// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Differential testing for recursive evaluation: semi-naive must agree
//! with the naive oracle, with either query executor underneath — and
//! with exact expected results where we know them.

use coln_batch::fixpoint::{self, Exec};
use coln_batch::query::{Atom, Catalog, Term};
use coln_batch::relation::Relation;
use coln_batch::rule::{Program, Rule};
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

fn parent(rows_: Vec<Vec<u64>>) -> Relation {
    Relation::new("parent", ["x", "y"], rows_)
}

#[test]
fn cycle_terminates_with_full_closure() {
    // 0 -> 1 -> 2 -> 0: every node reaches every node, including itself.
    // The interesting part is termination despite the cycle.
    let mut edb = Catalog::new();
    edb.insert(parent(vec![vec![0, 1, 2], vec![1, 2, 0]]));
    let program = fixtures::ancestor_program();

    let result = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec).unwrap();
    assert_eq!(result.catalog.get("ancestor").unwrap().len(), 9);
    // Path lengths 1, 2, 3; length 4 rediscovers length 1 and adds nothing.
    assert_eq!(result.stats.new_facts_per_round, vec![3, 3, 3, 0]);

    agree(&program, &edb, &["ancestor"]);
}

#[test]
fn self_loop_is_a_single_fact() {
    let mut edb = Catalog::new();
    edb.insert(parent(vec![vec![5], vec![5]]));
    let program = fixtures::ancestor_program();

    let result = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec).unwrap();
    assert_eq!(
        rows(result.catalog.get("ancestor").unwrap()),
        vec![vec![5, 5]]
    );
    assert_eq!(result.stats.new_facts_per_round, vec![1, 0]);

    agree(&program, &edb, &["ancestor"]);
}

#[test]
fn mutual_recursion_over_two_relations() {
    // even(0) is given; succ steps alternate the two derived relations:
    // odd(y) <- succ(x,y), even(x)   and   even(y) <- succ(x,y), odd(x).
    let mut edb = Catalog::new();
    edb.insert(Relation::new(
        "succ",
        ["x", "y"],
        vec![vec![0, 1, 2, 3], vec![1, 2, 3, 4]],
    ));
    edb.insert(Relation::new("even", ["x"], vec![vec![0]]));

    let step = |head: &str, from: &str| Rule {
        var_names: vec!["x".into(), "y".into()],
        head: Atom {
            relation: head.into(),
            terms: vec![Term::Var(1)],
        },
        body: vec![
            Atom {
                relation: "succ".into(),
                terms: vec![Term::Var(0), Term::Var(1)],
            },
            Atom {
                relation: from.into(),
                terms: vec![Term::Var(0)],
            },
        ],
    };
    let program = Program {
        rules: vec![step("odd", "even"), step("even", "odd")],
    };

    let result = agree(&program, &edb, &["even", "odd"]);
    assert_eq!(
        rows(result.get("even").unwrap()),
        vec![vec![0], vec![2], vec![4]]
    );
    assert_eq!(rows(result.get("odd").unwrap()), vec![vec![1], vec![3]]);
}

#[test]
fn idb_only_body_closes_in_fewer_rounds() {
    // Transitive closure by doubling: reach joins reach with itself, so
    // path lengths double per round and the chain closes in fewer rounds
    // than its length.
    let k = 8;
    let program = Program {
        rules: vec![
            Rule {
                var_names: vec!["x".into(), "y".into()],
                head: Atom {
                    relation: "reach".into(),
                    terms: vec![Term::Var(0), Term::Var(1)],
                },
                body: vec![Atom {
                    relation: "parent".into(),
                    terms: vec![Term::Var(0), Term::Var(1)],
                }],
            },
            Rule {
                var_names: vec!["x".into(), "y".into(), "z".into()],
                head: Atom {
                    relation: "reach".into(),
                    terms: vec![Term::Var(0), Term::Var(2)],
                },
                body: vec![
                    Atom {
                        relation: "reach".into(),
                        terms: vec![Term::Var(0), Term::Var(1)],
                    },
                    Atom {
                        relation: "reach".into(),
                        terms: vec![Term::Var(1), Term::Var(2)],
                    },
                ],
            },
        ],
    };
    let edb = fixtures::ancestor_chain_catalog(k);

    let result = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec).unwrap();
    assert_eq!(
        result.catalog.get("reach").unwrap().len() as u64,
        k * (k - 1) / 2
    );
    assert!(
        (result.stats.rounds as u64) < k,
        "doubling should close faster than one round per edge, got {} rounds",
        result.stats.rounds
    );

    agree(&program, &edb, &["reach"]);
}

#[test]
fn overlapping_rules_do_not_duplicate() {
    // A redundant grandparent shortcut derives many facts twice; the
    // result must be the plain closure anyway.
    let mut program = fixtures::ancestor_program();
    program.rules.push(Rule {
        var_names: vec!["x".into(), "y".into(), "z".into()],
        head: Atom {
            relation: "ancestor".into(),
            terms: vec![Term::Var(0), Term::Var(2)],
        },
        body: vec![
            Atom {
                relation: "parent".into(),
                terms: vec![Term::Var(0), Term::Var(1)],
            },
            Atom {
                relation: "parent".into(),
                terms: vec![Term::Var(1), Term::Var(2)],
            },
        ],
    });
    let edb = fixtures::ancestor_chain_catalog(5);
    let result = agree(&program, &edb, &["ancestor"]);
    assert_eq!(result.get("ancestor").unwrap().len(), 10);
}

#[test]
fn nonrecursive_program_stops_after_two_rounds() {
    // No derived relation in any body: round one fires everything, round
    // two derives nothing and stops.
    let program = Program {
        rules: vec![Rule {
            var_names: vec!["x".into(), "y".into()],
            head: Atom {
                relation: "copy".into(),
                terms: vec![Term::Var(0), Term::Var(1)],
            },
            body: vec![Atom {
                relation: "parent".into(),
                terms: vec![Term::Var(0), Term::Var(1)],
            }],
        }],
    };
    let edb = fixtures::ancestor_chain_catalog(4);

    let result = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec).unwrap();
    assert_eq!(result.catalog.get("copy").unwrap().len(), 3);
    assert_eq!(result.stats.new_facts_per_round, vec![3, 0]);

    agree(&program, &edb, &["copy"]);
}

#[test]
fn duplicate_initial_facts_are_deduped() {
    // The pre-seeded fact (0,1) is also derivable from the base rule.
    let mut edb = fixtures::ancestor_chain_catalog(3);
    edb.insert(Relation::new(
        "ancestor",
        ["x", "y"],
        vec![vec![0], vec![1]],
    ));
    let program = fixtures::ancestor_program();
    let result = agree(&program, &edb, &["ancestor"]);
    assert_eq!(
        rows(result.get("ancestor").unwrap()),
        vec![vec![0, 1], vec![0, 2], vec![1, 2]]
    );
}

#[test]
fn duplicate_edges_are_deduped() {
    let mut edb = Catalog::new();
    edb.insert(parent(vec![vec![0, 0, 1], vec![1, 1, 2]]));
    let program = fixtures::ancestor_program();
    let result = agree(&program, &edb, &["ancestor"]);
    assert_eq!(
        rows(result.get("ancestor").unwrap()),
        vec![vec![0, 1], vec![0, 2], vec![1, 2]]
    );
}

#[test]
fn disconnected_components_stay_separate() {
    let mut edb = Catalog::new();
    edb.insert(parent(vec![vec![0, 10], vec![1, 11]]));
    let program = fixtures::ancestor_program();
    let result = agree(&program, &edb, &["ancestor"]);
    assert_eq!(
        rows(result.get("ancestor").unwrap()),
        vec![vec![0, 1], vec![10, 11]]
    );
}

#[test]
fn long_chain_exact_closure() {
    let k = 40;
    let edb = fixtures::ancestor_chain_catalog(k);
    let program = fixtures::ancestor_program();

    let result = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec).unwrap();
    assert_eq!(
        result.catalog.get("ancestor").unwrap().len() as u64,
        k * (k - 1) / 2
    );
    assert_eq!(result.stats.rounds as u64, k);

    agree(&program, &edb, &["ancestor"]);
}
