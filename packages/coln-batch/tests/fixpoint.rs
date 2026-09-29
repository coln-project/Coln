// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Differential testing for recursive evaluation: semi-naive must agree
//! with the naive oracle, with either query executor underneath — and
//! with exact expected results where we know them, weights included.

use coln_batch::fixpoint::{self, Exec};
use coln_batch::query::{Atom, Catalog, Tables, Term};
use coln_batch::relation::Relation;
use coln_batch::rule::{Program, Rule};
use coln_batch::table::{ArrowSortedTable, ColId, SortedTable};
use coln_batch::types::{Dictionary, Schema, Value, Weight};
use coln_batch::{binary_join, fixtures, generate, generic_join, reference};

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
                base.get(idb).unwrap(),
                other.get(idb).unwrap(),
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
        via_reference.get("ancestor").unwrap(),
        via_generic.get("ancestor").unwrap()
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
    let program = Program::new(vec![coln_batch::rule::Rule {
        var_names: vec!["x".into(), "y".into()],
        head: Atom {
            relation: "flagged".into(),
            terms: vec![Term::Var(0), Term::lit(1u64)],
        },
        body: vec![Atom {
            relation: "parent".into(),
            terms: vec![Term::Var(0), Term::Var(1)],
        }],
        weight: 1,
    }]);
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
        weight: 1,
    };
    let program = Program::new(vec![step("odd", "even"), step("even", "odd")]);

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
    let program = Program::new(vec![
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
            weight: 1,
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
            weight: 1,
        },
    ]);
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
        weight: 1,
    });
    let edb = fixtures::ancestor_chain_catalog(5);
    let result = agree(&program, &edb, &["ancestor"]);
    assert_eq!(result.get("ancestor").unwrap().len(), 10);
}

#[test]
fn nonrecursive_program_runs_once() {
    // No derived relation reads itself, so its rule runs a single time.
    let program = Program::new(vec![Rule {
        var_names: vec!["x".into(), "y".into()],
        head: Atom {
            relation: "copy".into(),
            terms: vec![Term::Var(0), Term::Var(1)],
        },
        body: vec![Atom {
            relation: "parent".into(),
            terms: vec![Term::Var(0), Term::Var(1)],
        }],
        weight: 1,
    }]);
    let edb = fixtures::ancestor_chain_catalog(4);

    let result = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec).unwrap();
    assert_eq!(result.catalog.get("copy").unwrap().len(), 3);
    assert_eq!(result.stats.rounds, 1);
    assert_eq!(result.stats.new_facts_per_round, vec![3]);

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

#[test]
fn labeled_reachability_is_typed() {
    // Reachability along edges of one label; derived rows carry the
    // label as a string column.
    let mut edb = Catalog::new();
    edb.insert_rows(
        "edge",
        generate::labeled_edges_schema(),
        vec![
            vec![0u64.into(), 1u64.into(), "road".into(), 1i64.into()],
            vec![1u64.into(), 2u64.into(), "road".into(), (-2i64).into()],
            vec![2u64.into(), 3u64.into(), "rail".into(), 5i64.into()],
            vec![1u64.into(), 3u64.into(), "rail".into(), 0i64.into()],
        ],
    )
    .unwrap();
    let program = fixtures::labeled_reach_program();
    let result = agree(&program, &edb, &["reach"]);

    let mut got = result.rows_of("reach").unwrap();
    got.sort();
    let mut expected: Vec<Vec<Value>> = vec![
        vec![0u64.into(), 1u64.into(), "road".into()],
        vec![1u64.into(), 2u64.into(), "road".into()],
        vec![0u64.into(), 2u64.into(), "road".into()],
        vec![2u64.into(), 3u64.into(), "rail".into()],
        vec![1u64.into(), 3u64.into(), "rail".into()],
    ];
    expected.sort();
    assert_eq!(got, expected);
}

#[test]
fn head_literals_can_be_strings() {
    // tagged(x, "seen") ← parent(x, y): the head literal enters the
    // dictionary at compile time and decodes on the way out.
    let program = Program::new(vec![Rule {
        var_names: vec!["x".into(), "y".into()],
        head: Atom {
            relation: "tagged".into(),
            terms: vec![Term::Var(0), Term::lit("seen")],
        },
        body: vec![Atom {
            relation: "parent".into(),
            terms: vec![Term::Var(0), Term::Var(1)],
        }],
        weight: 1,
    }]);
    let edb = fixtures::ancestor_chain_catalog(3);
    let result = agree(&program, &edb, &["tagged"]);
    assert_eq!(
        result.rows_of("tagged").unwrap(),
        vec![
            vec![Value::Uint(0), "seen".into()],
            vec![Value::Uint(1), "seen".into()],
        ]
    );
}

/// `head(vars) ← body(vars), …` over the variables `0..`, all uint.
fn rule(head: (&str, &[usize]), body: &[(&str, &[usize])]) -> Rule {
    let atom = |(relation, vars): (&str, &[usize])| Atom {
        relation: relation.into(),
        terms: vars.iter().map(|&v| Term::Var(v)).collect(),
    };
    let num_vars = body
        .iter()
        .flat_map(|(_, vars)| vars.iter())
        .max()
        .map_or(0, |&v| v + 1);
    Rule {
        var_names: (0..num_vars).map(|v| format!("v{v}")).collect(),
        head: atom(head),
        body: body.iter().copied().map(atom).collect(),
        weight: 1,
    }
}

fn weighted(name: &str, rows: &[(u64, u64, Weight)]) -> Relation {
    Relation::with_weights(
        name,
        Schema::uint(["x", "y"]),
        vec![
            rows.iter().map(|r| r.0).collect(),
            rows.iter().map(|r| r.1).collect(),
        ],
        rows.iter().map(|r| r.2).collect(),
    )
}

fn entries(rel: &Relation) -> Vec<(Vec<u64>, Weight)> {
    (0..rel.len())
        .map(|i| (rel.row(i), rel.weight(i)))
        .collect()
}

#[test]
fn nonrecursive_relations_keep_their_weights() {
    // Two ways from 0 to 3, over 1 and over 2, and a second rule that
    // derives (0, 3) once more.
    let mut edb = Catalog::new();
    edb.insert(parent(vec![vec![0, 0, 1, 2], vec![1, 2, 3, 3]]));
    edb.insert(Relation::new("hub", ["x", "y"], vec![vec![0], vec![3]]));
    let program = Program::new(vec![
        rule(
            ("two_hop", &[0, 2]),
            &[("parent", &[0, 1]), ("parent", &[1, 2])],
        ),
        rule(("two_hop", &[0, 1]), &[("hub", &[0, 1])]),
    ]);
    let result = agree(&program, &edb, &["two_hop"]);
    assert_eq!(
        entries(result.get("two_hop").unwrap()),
        vec![(vec![0, 3], 3)]
    );
}

#[test]
fn weights_count_what_recursion_derived() {
    // ancestor is a set. Projecting it onto the ancestor counts the
    // descendants: 0 -> 1 -> 2 -> 3.
    let edb = fixtures::ancestor_chain_catalog(4);
    let mut program = fixtures::ancestor_program();
    program
        .rules
        .push(rule(("descendants", &[0]), &[("ancestor", &[0, 1])]));
    let result = agree(&program, &edb, &["ancestor", "descendants"]);
    let ancestor = result.get("ancestor").unwrap();
    assert_eq!(ancestor.len(), 6);
    assert!(ancestor.weights.iter().all(|&w| w == 1));
    assert_eq!(
        entries(result.get("descendants").unwrap()),
        vec![(vec![0], 3), (vec![1], 2), (vec![2], 1)]
    );
}

#[test]
fn recursion_over_weighted_input_is_a_set() {
    // Every edge of the cycle is present more than once; the closure is
    // the same set as over single edges.
    let mut edb = Catalog::new();
    edb.insert(weighted("parent", &[(0, 1, 2), (1, 2, 2), (2, 0, 3)]));
    let program = fixtures::ancestor_program();
    let result = agree(&program, &edb, &["ancestor"]);
    let ancestor = result.get("ancestor").unwrap();
    assert_eq!(ancestor.len(), 9);
    assert!(ancestor.weights.iter().all(|&w| w == 1));
}

#[test]
fn recursion_rejects_rows_of_non_positive_weight() {
    let program = fixtures::ancestor_program();
    let mut edb = Catalog::new();
    edb.insert(weighted("parent", &[(0, 1, 1), (1, 2, -1)]));
    for result in [
        fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec),
        fixpoint::naive(&program, &edb, generic_join::execute as Exec),
    ] {
        let err = result
            .err()
            .expect("weight -1 must be rejected")
            .to_string();
        assert!(err.contains("parent holds a row of weight -1"), "{err}");
    }

    // Initial facts of a recursive relation are an input too.
    let mut edb = fixtures::ancestor_chain_catalog(3);
    edb.insert(weighted("ancestor", &[(7, 8, -2)]));
    let err = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec)
        .err()
        .expect("weight -2 must be rejected")
        .to_string();
    assert!(
        err.contains("initial facts of the recursive relation ancestor"),
        "{err}"
    );
}

#[test]
fn strata_run_in_dependency_order() {
    // reach is recursive over parent. cycle_node reads reach once, without
    // recursion, and counts how many nodes share a cycle with each node.
    // on_cycle is recursive again, over cycle_node: a set, whatever the
    // weights it reads. Edges: 0 -> 1 -> 2 -> 1 and 3 -> 3.
    let mut edb = Catalog::new();
    edb.insert(parent(vec![vec![0, 1, 2, 3], vec![1, 2, 1, 3]]));
    let program = Program::new(vec![
        rule(("reach", &[0, 1]), &[("parent", &[0, 1])]),
        rule(
            ("reach", &[0, 2]),
            &[("reach", &[0, 1]), ("parent", &[1, 2])],
        ),
        rule(
            ("cycle_node", &[0]),
            &[("reach", &[0, 1]), ("reach", &[1, 0])],
        ),
        rule(
            ("on_cycle", &[0, 1]),
            &[("cycle_node", &[0]), ("parent", &[0, 1])],
        ),
        rule(
            ("on_cycle", &[0, 2]),
            &[("on_cycle", &[0, 1]), ("parent", &[1, 2])],
        ),
    ]);
    let result = agree(&program, &edb, &["reach", "cycle_node", "on_cycle"]);
    assert_eq!(
        rows(result.get("reach").unwrap()),
        vec![
            vec![0, 1],
            vec![0, 2],
            vec![1, 1],
            vec![1, 2],
            vec![2, 1],
            vec![2, 2],
            vec![3, 3]
        ]
    );
    assert_eq!(
        entries(result.get("cycle_node").unwrap()),
        vec![(vec![1], 2), (vec![2], 2), (vec![3], 1)]
    );
    let on_cycle = result.get("on_cycle").unwrap();
    assert_eq!(
        rows(on_cycle),
        vec![vec![1, 1], vec![1, 2], vec![2, 1], vec![2, 2], vec![3, 3]]
    );
    assert!(on_cycle.weights.iter().all(|&w| w == 1));
}

/// The same rule with another weight.
fn weighing(weight: Weight, rule: Rule) -> Rule {
    Rule { weight, ..rule }
}

#[test]
fn a_rule_of_weight_minus_one_subtracts() {
    // diff = a - b, as Z-sets: a row only in b comes out negative.
    let mut edb = Catalog::new();
    edb.insert(Relation::new("a", ["x", "y"], vec![vec![1, 2], vec![1, 2]]));
    edb.insert(Relation::new("b", ["x", "y"], vec![vec![2, 3], vec![2, 3]]));
    let program = Program::new(vec![
        rule(("diff", &[0, 1]), &[("a", &[0, 1])]),
        weighing(-1, rule(("diff", &[0, 1]), &[("b", &[0, 1])])),
    ]);
    let result = agree(&program, &edb, &["diff"]);
    assert_eq!(
        entries(result.get("diff").unwrap()),
        vec![(vec![1, 1], 1), (vec![3, 3], -1)]
    );
}

#[test]
fn anti_join_is_a_subtraction_of_a_distinct_semi_join() {
    // leaf(x) ← node(x), minus node(x), has_child(x): the nodes without a
    // child. 0 has two children, so has_child must be distinct, or leaf
    // would take 0 away twice.
    let mut edb = Catalog::new();
    edb.insert(parent(vec![vec![0, 0, 1, 2], vec![1, 2, 2, 3]]));
    edb.insert(Relation::new("node", ["x"], vec![vec![0, 1, 2, 3]]));
    let mut program = Program::new(vec![
        rule(("has_child", &[0]), &[("parent", &[0, 1])]),
        rule(("leaf", &[0]), &[("node", &[0])]),
        weighing(
            -1,
            rule(("leaf", &[0]), &[("node", &[0]), ("has_child", &[0])]),
        ),
    ]);

    let as_bag = agree(&program, &edb, &["leaf"]);
    assert_eq!(
        entries(as_bag.get("leaf").unwrap()),
        vec![(vec![0], -1), (vec![3], 1)]
    );

    program.distinct.insert("has_child".into());
    let result = agree(&program, &edb, &["has_child", "leaf"]);
    assert_eq!(
        entries(result.get("has_child").unwrap()),
        vec![(vec![0], 1), (vec![1], 1), (vec![2], 1)]
    );
    assert_eq!(entries(result.get("leaf").unwrap()), vec![(vec![3], 1)]);
}

#[test]
fn a_distinct_relation_keeps_each_row_once() {
    // Without the declaration (0, 3) weighs 3, see
    // `nonrecursive_relations_keep_their_weights`.
    let mut edb = Catalog::new();
    edb.insert(parent(vec![vec![0, 0, 1, 2], vec![1, 2, 3, 3]]));
    edb.insert(Relation::new("hub", ["x", "y"], vec![vec![0], vec![3]]));
    let mut program = Program::new(vec![
        rule(
            ("two_hop", &[0, 2]),
            &[("parent", &[0, 1]), ("parent", &[1, 2])],
        ),
        rule(("two_hop", &[0, 1]), &[("hub", &[0, 1])]),
    ]);
    program.distinct.insert("two_hop".into());
    let result = agree(&program, &edb, &["two_hop"]);
    assert_eq!(
        entries(result.get("two_hop").unwrap()),
        vec![(vec![0, 3], 1)]
    );
}

#[test]
fn recursion_cannot_take_rows_away() {
    // A subtracting rule inside the recursion is rejected up front.
    let mut program = fixtures::ancestor_program();
    program.rules[1].weight = -1;
    let edb = fixtures::ancestor_chain_catalog(3);
    let err = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec)
        .err()
        .expect("a subtracting recursive rule must be rejected")
        .to_string();
    assert!(err.contains("ancestor is recursive"), "{err}");

    // So is a negative row that a subtraction feeds into a recursion.
    let mut edb = Catalog::new();
    edb.insert(parent(vec![vec![0], vec![1]]));
    edb.insert(Relation::new("banned", ["x", "y"], vec![vec![5], vec![6]]));
    let program = Program::new(vec![
        rule(("allowed", &[0, 1]), &[("parent", &[0, 1])]),
        weighing(-1, rule(("allowed", &[0, 1]), &[("banned", &[0, 1])])),
        rule(("reach", &[0, 1]), &[("allowed", &[0, 1])]),
        rule(
            ("reach", &[0, 2]),
            &[("reach", &[0, 1]), ("allowed", &[1, 2])],
        ),
    ]);
    let err = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec)
        .err()
        .expect("a negative input to the recursion must be rejected")
        .to_string();
    assert!(err.contains("allowed holds a row of weight -1"), "{err}");
}

#[test]
fn recursion_reads_the_net_weight_of_stored_copies() {
    // parent stores (0, 1) twice, with weights 2 and -1: net 1, an edge.
    // (1, 2) is stored with weights 1 and -1: net 0, no edge at all.
    // Neither is an error.
    let mut edb = Catalog::new();
    edb.insert(weighted(
        "parent",
        &[(0, 1, 2), (1, 2, 1), (0, 1, -1), (1, 2, -1), (2, 3, 1)],
    ));
    let program = fixtures::ancestor_program();
    let result = agree(&program, &edb, &["ancestor"]);
    assert_eq!(
        rows(result.get("ancestor").unwrap()),
        vec![vec![0, 1], vec![2, 3]]
    );

    // Copies that add up to a negative weight are refused.
    let mut edb = Catalog::new();
    edb.insert(weighted("parent", &[(0, 1, 1), (0, 1, -2)]));
    let err = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec)
        .err()
        .expect("net weight -1 must be refused")
        .to_string();
    assert!(err.contains("parent holds a row of weight -1"), "{err}");
}

/// A source that breaks its promise to keep a table as it is while a query
/// reads it: after `honest` requests it serves every row of `catalog` with
/// its weight negated, as if the rows had been taken away meanwhile.
struct Changing {
    catalog: Catalog,
    honest: usize,
    served: std::cell::Cell<usize>,
}

impl Tables for Changing {
    fn schema(&self, relation: &str) -> anyhow::Result<&Schema> {
        Tables::schema(&self.catalog, relation)
    }

    fn sorted(&self, relation: &str, order: &[ColId]) -> anyhow::Result<Box<dyn SortedTable + '_>> {
        let served = self.served.get();
        self.served.set(served + 1);
        let mut rel = self.catalog.get(relation)?.clone();
        if served >= self.honest {
            for w in &mut rel.weights {
                *w = -*w;
            }
        }
        Ok(Box::new(ArrowSortedTable::from_relation(
            &rel,
            order.to_vec(),
        )?))
    }

    fn dictionary(&self) -> &Dictionary {
        self.catalog.dictionary()
    }
}

#[test]
fn a_recursion_whose_input_changes_stops_with_an_error() {
    // parent: 0 -> 1 -> 2 and 0 -> 2. The first three requests (the input
    // check and round one) see the edges; round two reads them taken away
    // and would silently lose the fact (0, 2) that round one derived.
    let mut catalog = Catalog::new();
    catalog.insert(parent(vec![vec![0, 1, 0], vec![1, 2, 2]]));
    let source = Changing {
        catalog,
        honest: 3,
        served: std::cell::Cell::new(0),
    };
    let err = fixpoint::semi_naive_over(
        &fixtures::ancestor_program(),
        &source,
        generic_join::execute as Exec,
    )
    .err()
    .expect("a lost fact must stop the recursion")
    .to_string();
    assert!(err.contains("ancestor lost a fact in round 2"), "{err}");
}
