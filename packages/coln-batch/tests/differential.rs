// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Differential testing: the two real executors must agree with each other
//! on every query — and, on small inputs, with the brute-force oracle.

use coln_batch::query::{Atom, Catalog, Query, Term};
use coln_batch::relation::Relation;
use coln_batch::types::{Column, ScalarType, Schema, Value};
use coln_batch::{binary_join, fixtures, generic_join, reference};

/// Run all three executors and require identical results.
fn agree_with_oracle(query: &Query, catalog: &Catalog) -> Relation {
    let oracle = reference::execute(query, catalog).unwrap();
    let binary = binary_join::execute(query, catalog).unwrap();
    let generic = generic_join::execute(query, catalog).unwrap();
    assert_eq!(oracle, binary, "binary join disagrees with oracle");
    assert_eq!(oracle, generic, "generic join disagrees with oracle");
    oracle
}

/// Run the two real executors (data too big for the oracle).
fn agree(query: &Query, catalog: &Catalog) -> Relation {
    let binary = binary_join::execute(query, catalog).unwrap();
    let generic = generic_join::execute(query, catalog).unwrap();
    assert_eq!(binary, generic, "executors disagree");
    binary
}

fn atom(relation: &str, terms: Vec<Term>) -> Atom {
    Atom {
        relation: relation.into(),
        terms,
    }
}

#[test]
fn hand_cases() {
    let mut cat = Catalog::new();
    cat.insert(Relation::new(
        "R",
        ["a", "b"],
        vec![vec![1, 1, 2], vec![2, 4, 3]],
    ));
    cat.insert(Relation::new(
        "S",
        ["a", "b"],
        vec![vec![2, 4, 9], vec![5, 7, 9]],
    ));
    cat.insert(Relation::new("U", ["a"], vec![vec![1, 2]]));
    cat.insert(Relation::new("V", ["a"], vec![vec![7]]));
    cat.insert(Relation::new(
        "T",
        ["a", "b"],
        vec![vec![4, 5, 7], vec![4, 6, 7]],
    ));
    let (x, y, z) = (0, 1, 2);

    // Chain: Q(x,y,z) ← R(x,y), S(y,z)
    let chain = Query {
        var_names: vec!["x".into(), "y".into(), "z".into()],
        atoms: vec![
            atom("R", vec![Term::Var(x), Term::Var(y)]),
            atom("S", vec![Term::Var(y), Term::Var(z)]),
        ],
        head: vec![x, y, z],
    };
    let r = agree_with_oracle(&chain, &cat);
    assert_eq!(r.len(), 2);

    // Literal filter: Q(y) ← R(1, y)
    let lit = Query {
        var_names: vec!["y".into()],
        atoms: vec![atom("R", vec![Term::lit(1u64), Term::Var(0)])],
        head: vec![0],
    };
    let r = agree_with_oracle(&lit, &cat);
    assert_eq!((r.row(0), r.row(1)), (vec![2], vec![4]));

    // Literal miss: Q(y) ← R(9, y)
    let miss = Query {
        var_names: vec!["y".into()],
        atoms: vec![atom("R", vec![Term::lit(9u64), Term::Var(0)])],
        head: vec![0],
    };
    assert_eq!(agree_with_oracle(&miss, &cat).len(), 0);

    // Repeated variable inside one atom: Q(x) ← T(x, x)
    let rep = Query {
        var_names: vec!["x".into()],
        atoms: vec![atom("T", vec![Term::Var(0), Term::Var(0)])],
        head: vec![0],
    };
    let r = agree_with_oracle(&rep, &cat);
    assert_eq!((r.row(0), r.row(1)), (vec![4], vec![7]));

    // Cartesian product: Q(x,y) ← U(x), V(y)
    let cart = Query {
        var_names: vec!["x".into(), "y".into()],
        atoms: vec![atom("U", vec![Term::Var(0)]), atom("V", vec![Term::Var(1)])],
        head: vec![0, 1],
    };
    let r = agree_with_oracle(&cart, &cat);
    assert_eq!(r.len(), 2);

    // Existence filter (all-literal atom), present and absent:
    let exists = Query {
        var_names: vec!["x".into()],
        atoms: vec![
            atom("U", vec![Term::Var(0)]),
            atom("S", vec![Term::lit(9u64), Term::lit(9u64)]),
        ],
        head: vec![0],
    };
    assert_eq!(agree_with_oracle(&exists, &cat).len(), 2);
    let not_exists = Query {
        var_names: vec!["x".into()],
        atoms: vec![
            atom("U", vec![Term::Var(0)]),
            atom("S", vec![Term::lit(9u64), Term::lit(8u64)]),
        ],
        head: vec![0],
    };
    assert_eq!(agree_with_oracle(&not_exists, &cat).len(), 0);
}

/// Corner cases the random generator is not guaranteed to produce: empty
/// inputs, duplicate input rows, and projections that collapse rows.
#[test]
fn edge_cases() {
    let mut cat = Catalog::new();
    // (2,3) appears twice: relations are sets, so duplicate input rows
    // must not reach the output.
    cat.insert(Relation::new(
        "R",
        ["a", "b"],
        vec![vec![1, 1, 2, 2], vec![2, 4, 3, 3]],
    ));
    cat.insert(Relation::new("E", ["a", "b"], vec![Vec::new(), Vec::new()]));
    let (x, y, z) = (0, 1, 2);

    // Scanning an empty relation yields an empty result.
    let empty_scan = Query {
        var_names: vec!["x".into(), "y".into()],
        atoms: vec![atom("E", vec![Term::Var(x), Term::Var(y)])],
        head: vec![x, y],
    };
    assert_eq!(agree_with_oracle(&empty_scan, &cat).len(), 0);

    // An empty relation anywhere in the body empties the whole result.
    let empty_join = Query {
        var_names: vec!["x".into(), "y".into(), "z".into()],
        atoms: vec![
            atom("R", vec![Term::Var(x), Term::Var(y)]),
            atom("E", vec![Term::Var(y), Term::Var(z)]),
        ],
        head: vec![x, y, z],
    };
    assert_eq!(agree_with_oracle(&empty_join, &cat).len(), 0);

    // Projection collapses rows (set semantics): x=1 and x=2 each stem
    // from two body rows but appear once.
    let proj = Query {
        var_names: vec!["x".into(), "y".into()],
        atoms: vec![atom("R", vec![Term::Var(x), Term::Var(y)])],
        head: vec![x],
    };
    let r = agree_with_oracle(&proj, &cat);
    assert_eq!((r.len(), r.row(0), r.row(1)), (2, vec![1], vec![2]));

    // Duplicate input rows do not survive into the output.
    let scan = Query {
        var_names: vec!["x".into(), "y".into()],
        atoms: vec![atom("R", vec![Term::Var(x), Term::Var(y)])],
        head: vec![x, y],
    };
    assert_eq!(agree_with_oracle(&scan, &cat).len(), 3);
}

#[test]
fn fixtures_small_vs_oracle() {
    let cat = fixtures::fg_catalog(40, 120, 10, 6);
    let r = agree_with_oracle(&fixtures::fg_query(), &cat);
    assert!(
        r.len() >= 8,
        "expected most planted matches, got {}",
        r.len()
    );

    let cat = fixtures::triangle_catalog(40, 120, 10, 5);
    let r = agree_with_oracle(&fixtures::triangle_query(), &cat);
    assert!(
        r.len() >= 8,
        "expected most planted triangles, got {}",
        r.len()
    );
}

#[test]
fn fixtures_medium_executors_agree() {
    let cat = fixtures::fg_catalog(50_000, 200_000, 5_000, 43);
    let r = agree(&fixtures::fg_query(), &cat);
    assert!(r.len() >= 4_500, "got {}", r.len());

    let cat = fixtures::triangle_catalog(10_000, 40_000, 1_000, 42);
    let r = agree(&fixtures::triangle_query(), &cat);
    assert!(r.len() >= 900, "got {}", r.len());
}

/// Executor agreement at the 10M-row scale. Prints per-executor timings,
/// visible with `--nocapture`.
/// Run with: cargo test -p coln-batch --release -- --include-ignored
#[test]
#[ignore = "large; run explicitly (use --release)"]
fn fixtures_large_executors_agree() {
    let cases = [
        (
            "f(a, g(a))",
            fixtures::fg_catalog(5_000_000, 10_000_000, 200_000, 8),
            fixtures::fg_query(),
            180_000,
        ),
        (
            "triangle",
            fixtures::triangle_catalog(10_000_000, 10_000_000, 100_000, 7),
            fixtures::triangle_query(),
            90_000,
        ),
    ];
    for (name, cat, query, floor) in cases {
        let started = std::time::Instant::now();
        let binary = binary_join::execute(&query, &cat).unwrap();
        let t_binary = started.elapsed();
        let started = std::time::Instant::now();
        let generic = generic_join::execute(&query, &cat).unwrap();
        let t_generic = started.elapsed();
        assert_eq!(binary, generic, "{name}: executors disagree");
        assert!(binary.len() >= floor, "{name}: got {}", binary.len());
        eprintln!(
            "{name}: {} rows, binary join {t_binary:.2?}, generic join {t_generic:.2?}",
            binary.len()
        );
    }
}

/// Decoded rows of a result, sorted by value for order-independent
/// comparison.
fn decoded(result: &Relation, catalog: &Catalog) -> Vec<Vec<Value>> {
    let mut rows: Vec<Vec<Value>> = (0..result.len())
        .map(|i| result.row_values(i, catalog.dictionary()).unwrap())
        .collect();
    rows.sort();
    rows
}

fn typed_catalog() -> Catalog {
    let mut cat = Catalog::new();
    cat.insert_rows(
        "person",
        Schema::new([
            Column::new("id", ScalarType::Uint),
            Column::new("name", ScalarType::String),
            Column::new("age", ScalarType::Iint),
            Column::new("active", ScalarType::Bool),
            Column::new("initial", ScalarType::Char),
        ]),
        vec![
            vec![
                1u64.into(),
                "ann".into(),
                (-5i64).into(),
                true.into(),
                'a'.into(),
            ],
            vec![
                2u64.into(),
                "bob".into(),
                30i64.into(),
                false.into(),
                'b'.into(),
            ],
            vec![
                3u64.into(),
                "ann".into(),
                7i64.into(),
                true.into(),
                'a'.into(),
            ],
        ],
    )
    .unwrap();
    cat.insert_rows(
        "likes",
        Schema::new([
            Column::new("who", ScalarType::String),
            Column::new("what", ScalarType::String),
        ]),
        vec![
            vec!["ann".into(), "tea".into()],
            vec!["bob".into(), "tea".into()],
            vec!["ann".into(), "jazz".into()],
        ],
    )
    .unwrap();
    cat
}

#[test]
fn typed_hand_cases() {
    let cat = typed_catalog();
    let (id, name, age, active, initial, what) = (0, 1, 2, 3, 4, 5);
    let person = |terms: Vec<Term>| atom("person", terms);
    let all_vars = || {
        vec![
            Term::Var(id),
            Term::Var(name),
            Term::Var(age),
            Term::Var(active),
            Term::Var(initial),
        ]
    };

    // Join on a string column: Q(id, what) ← person(id, name, …), likes(name, what)
    let join = Query {
        var_names: ["id", "name", "age", "active", "initial", "what"]
            .into_iter()
            .map(String::from)
            .collect(),
        atoms: vec![
            person(all_vars()),
            atom("likes", vec![Term::Var(name), Term::Var(what)]),
        ],
        head: vec![id, what],
    };
    let r = agree_with_oracle(&join, &cat);
    assert_eq!(r.schema.types(), vec![ScalarType::Uint, ScalarType::String]);
    let mut expected: Vec<Vec<Value>> = vec![
        vec![1u64.into(), "tea".into()],
        vec![1u64.into(), "jazz".into()],
        vec![2u64.into(), "tea".into()],
        vec![3u64.into(), "tea".into()],
        vec![3u64.into(), "jazz".into()],
    ];
    expected.sort();
    assert_eq!(decoded(&r, &cat), expected);

    // Literals of every type, one per column of person; all other
    // columns are distinct variables, the head is the id.
    let with_literal = |col: usize, lit: Term| -> Query {
        let mut next = 0;
        let terms: Vec<Term> = (0..5)
            .map(|c| {
                if c == col {
                    lit.clone()
                } else {
                    next += 1;
                    Term::Var(next - 1)
                }
            })
            .collect();
        Query {
            var_names: (0..next).map(|v| format!("v{v}")).collect(),
            atoms: vec![person(terms)],
            head: vec![0],
        }
    };
    let ids = |q: &Query| decoded(&agree_with_oracle(q, &cat), &cat);
    assert_eq!(
        ids(&with_literal(2, Term::lit(-5i64))),
        vec![vec![1u64.into()]]
    );
    assert_eq!(
        ids(&with_literal(3, Term::lit(true))),
        vec![vec![1u64.into()], vec![3u64.into()]]
    );
    assert_eq!(
        ids(&with_literal(4, Term::lit('b'))),
        vec![vec![2u64.into()]]
    );
    assert_eq!(
        ids(&with_literal(1, Term::lit("ann"))),
        vec![vec![1u64.into()], vec![3u64.into()]]
    );

    // A string the dictionary has never seen matches nothing.
    let r = agree_with_oracle(&with_literal(1, Term::lit("zed")), &cat);
    assert!(r.is_empty());
    assert_eq!(r.schema.types(), vec![ScalarType::Uint]);
}

#[test]
fn typed_errors_are_reported_by_every_executor() {
    let cat = typed_catalog();
    // x stands in a uint column and a string column.
    let mixed = Query {
        var_names: vec!["x".into(), "a".into(), "b".into(), "c".into()],
        atoms: vec![atom(
            "person",
            vec![
                Term::Var(0),
                Term::Var(0),
                Term::Var(1),
                Term::Var(2),
                Term::Var(3),
            ],
        )],
        head: vec![0],
    };
    // A literal of the wrong type.
    let bad_lit = Query {
        var_names: vec!["who".into()],
        atoms: vec![atom("likes", vec![Term::Var(0), Term::lit(3u64)])],
        head: vec![0],
    };
    for query in [&mixed, &bad_lit] {
        assert!(reference::execute(query, &cat).is_err());
        assert!(binary_join::execute(query, &cat).is_err());
        assert!(generic_join::execute(query, &cat).is_err());
    }
}

#[test]
fn labeled_fixture_vs_oracle() {
    let cat = fixtures::labeled_catalog(12, 80, &["road", "rail", "sea"], 3);
    let r = agree_with_oracle(&fixtures::labeled_two_hop_query(), &cat);
    assert!(!r.is_empty(), "two hops with equal labels should exist");
    assert_eq!(
        r.schema.types(),
        vec![ScalarType::Uint, ScalarType::Uint, ScalarType::String]
    );
    // Every result row decodes, and its label is one of the generator's.
    for row in decoded(&r, &cat) {
        assert!(
            matches!(&row[2], Value::String(s) if ["road", "rail", "sea"].contains(&s.as_str()))
        );
    }
}
