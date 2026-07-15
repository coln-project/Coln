// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Differential testing: the two real executors must agree with each other
//! on every query — and, on small inputs, with the brute-force oracle.

use coln_batch::query::{Atom, Catalog, Query, Term};
use coln_batch::relation::Relation;
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
        atoms: vec![atom("R", vec![Term::Lit(1), Term::Var(0)])],
        head: vec![0],
    };
    let r = agree_with_oracle(&lit, &cat);
    assert_eq!((r.row(0), r.row(1)), (vec![2], vec![4]));

    // Literal miss: Q(y) ← R(9, y)
    let miss = Query {
        var_names: vec!["y".into()],
        atoms: vec![atom("R", vec![Term::Lit(9), Term::Var(0)])],
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
            atom("S", vec![Term::Lit(9), Term::Lit(9)]),
        ],
        head: vec![0],
    };
    assert_eq!(agree_with_oracle(&exists, &cat).len(), 2);
    let not_exists = Query {
        var_names: vec!["x".into()],
        atoms: vec![
            atom("U", vec![Term::Var(0)]),
            atom("S", vec![Term::Lit(9), Term::Lit(8)]),
        ],
        head: vec![0],
    };
    assert_eq!(agree_with_oracle(&not_exists, &cat).len(), 0);
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

/// AP5 acceptance test at the 1M-row scale.
/// Run with: cargo test -p coln-batch --release -- --include-ignored
#[test]
#[ignore = "large; run explicitly (use --release)"]
fn fixtures_large_executors_agree() {
    let cat = fixtures::fg_catalog(500_000, 1_000_000, 20_000, 8);
    let r = agree(&fixtures::fg_query(), &cat);
    assert!(r.len() >= 18_000, "got {}", r.len());

    let cat = fixtures::triangle_catalog(1_000_000, 1_000_000, 10_000, 7);
    let r = agree(&fixtures::triangle_query(), &cat);
    assert!(r.len() >= 9_000, "got {}", r.len());
}
