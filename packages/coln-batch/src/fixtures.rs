// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Ready-made example queries with matching data generators — one fixture
//! per query class the engine must serve. Used by tests and the demo.

use crate::generate;
use crate::query::{Atom, Catalog, Query, Term};
use crate::relation::Relation;
use crate::rule::{Program, Rule};

/// The acyclic e-matching pattern `f(α, g(α))`:
///
/// ```text
/// Q(f, α, g) ← R_f(f, α, g), R_g(g, α)
/// ```
///
/// Variable order (= elimination order for the generic join): α, g, f.
pub fn fg_query() -> Query {
    let (alpha, g, f) = (0, 1, 2);
    Query {
        var_names: vec!["alpha".into(), "g".into(), "f".into()],
        atoms: vec![
            Atom {
                relation: "R_f".into(),
                terms: vec![Term::Var(f), Term::Var(alpha), Term::Var(g)],
            },
            Atom {
                relation: "R_g".into(),
                terms: vec![Term::Var(g), Term::Var(alpha)],
            },
        ],
        head: vec![f, alpha, g],
    }
}

/// Catalog with generated data for [`fg_query`].
pub fn fg_catalog(eclasses: u64, noise: usize, planted: usize, seed: u64) -> Catalog {
    let mut cat = Catalog::new();
    for rel in generate::f_g_pattern(eclasses, noise, planted, seed) {
        cat.insert(rel);
    }
    cat
}

/// The cyclic triangle query:
///
/// ```text
/// Q(x, y, z) ← R_f(x, y), R_g(y, z), R_h(z, x)
/// ```
pub fn triangle_query() -> Query {
    let (x, y, z) = (0, 1, 2);
    Query {
        var_names: vec!["x".into(), "y".into(), "z".into()],
        atoms: vec![
            Atom {
                relation: "R_f".into(),
                terms: vec![Term::Var(x), Term::Var(y)],
            },
            Atom {
                relation: "R_g".into(),
                terms: vec![Term::Var(y), Term::Var(z)],
            },
            Atom {
                relation: "R_h".into(),
                terms: vec![Term::Var(z), Term::Var(x)],
            },
        ],
        head: vec![x, y, z],
    }
}

/// Catalog with generated data for [`triangle_query`].
pub fn triangle_catalog(nodes: u64, noise_edges: usize, planted: usize, seed: u64) -> Catalog {
    let mut cat = Catalog::new();
    for rel in generate::triangle(nodes, noise_edges, planted, seed) {
        cat.insert(rel);
    }
    cat
}

/// The classic recursive program:
///
/// ```text
/// ancestor(x, y) ← parent(x, y)
/// ancestor(x, z) ← parent(x, y), ancestor(y, z)
/// ```
pub fn ancestor_program() -> Program {
    let (x, y, z) = (0, 1, 2);
    Program {
        rules: vec![
            Rule {
                var_names: vec!["x".into(), "y".into()],
                head: Atom {
                    relation: "ancestor".into(),
                    terms: vec![Term::Var(x), Term::Var(y)],
                },
                body: vec![Atom {
                    relation: "parent".into(),
                    terms: vec![Term::Var(x), Term::Var(y)],
                }],
            },
            Rule {
                var_names: vec!["x".into(), "y".into(), "z".into()],
                head: Atom {
                    relation: "ancestor".into(),
                    terms: vec![Term::Var(x), Term::Var(z)],
                },
                body: vec![
                    Atom {
                        relation: "parent".into(),
                        terms: vec![Term::Var(x), Term::Var(y)],
                    },
                    Atom {
                        relation: "ancestor".into(),
                        terms: vec![Term::Var(y), Term::Var(z)],
                    },
                ],
            },
        ],
    }
}

/// Catalog with a chain 0 → 1 → … → k-1 as the `parent` relation. The
/// ancestor fixpoint has exactly k·(k-1)/2 facts.
pub fn ancestor_chain_catalog(k: u64) -> Catalog {
    let mut cat = Catalog::new();
    cat.insert(generate::chain(k));
    cat
}

/// Catalog with a random DAG as the `parent` relation.
pub fn ancestor_dag_catalog(nodes: u64, edges: usize, seed: u64) -> Catalog {
    let mut cat = Catalog::new();
    cat.insert(generate::dag(nodes, edges, seed));
    cat
}

/// A u64 encoding of the miniature points-to + call-graph analysis from
/// `coln-query/src/dbsp/points_to_step_3.dl` (mutually recursive:
/// VarPointsTo needs CallGraph, CallGraph needs VarPointsTo):
///
/// ```text
/// VarPointsTo(v, o)  ← Alloc(v, o)
/// VarPointsTo(d, o)  ← Assign(d, s), VarPointsTo(s, o)
/// CallGraph(s, m)    ← VirtualCall(s, r, sig), VarPointsTo(r, o),
///                      HeapType(o, t), Dispatch(t, sig, m)
/// VarPointsTo(p, o)  ← CallGraph(s, m), ActualArg(s, a),
///                      FormalParam(m, p), VarPointsTo(a, o)
/// ```
pub mod points_to {
    use super::*;

    // Symbol table (the Souffle program uses strings; we use u64 ids).
    // Variables:
    pub const G: u64 = 1; // g
    pub const D: u64 = 2; // d
    pub const C: u64 = 3; // c
    pub const M: u64 = 4; // m
    pub const X: u64 = 5; // x (Greeter.greet's parameter)
    // Allocation sites / objects:
    pub const O_G: u64 = 10;
    pub const O_DOG: u64 = 11;
    pub const O_CAT: u64 = 12;
    pub const O_MOUSE: u64 = 13;
    // Call sites:
    pub const S1: u64 = 20;
    pub const S3: u64 = 21;
    pub const S4: u64 = 22;
    // Signatures:
    pub const GREET: u64 = 30;
    pub const SPEAK: u64 = 31;
    // Types:
    pub const T_GREETER: u64 = 40;
    pub const T_DOG: u64 = 41;
    pub const T_CAT: u64 = 42;
    pub const T_MOUSE: u64 = 43;
    // Methods:
    pub const GREETER_GREET: u64 = 50;
    pub const DOG_SPEAK: u64 = 51;
    pub const CAT_SPEAK: u64 = 52;
    pub const MOUSE_SPEAK: u64 = 53;

    pub fn program() -> Program {
        let atom = |relation: &str, terms: Vec<Term>| Atom {
            relation: relation.into(),
            terms,
        };
        let v = Term::Var;
        Program {
            rules: vec![
                // VarPointsTo(v, o) ← Alloc(v, o)
                Rule {
                    var_names: vec!["v".into(), "o".into()],
                    head: atom("VarPointsTo", vec![v(0), v(1)]),
                    body: vec![atom("Alloc", vec![v(0), v(1)])],
                },
                // VarPointsTo(d, o) ← Assign(d, s), VarPointsTo(s, o)
                Rule {
                    var_names: vec!["d".into(), "s".into(), "o".into()],
                    head: atom("VarPointsTo", vec![v(0), v(2)]),
                    body: vec![
                        atom("Assign", vec![v(0), v(1)]),
                        atom("VarPointsTo", vec![v(1), v(2)]),
                    ],
                },
                // CallGraph(s, m) ← VirtualCall(s, r, sig), VarPointsTo(r, o),
                //                   HeapType(o, t), Dispatch(t, sig, m)
                Rule {
                    var_names: vec![
                        "site".into(),
                        "recv".into(),
                        "sig".into(),
                        "obj".into(),
                        "ty".into(),
                        "meth".into(),
                    ],
                    head: atom("CallGraph", vec![v(0), v(5)]),
                    body: vec![
                        atom("VirtualCall", vec![v(0), v(1), v(2)]),
                        atom("VarPointsTo", vec![v(1), v(3)]),
                        atom("HeapType", vec![v(3), v(4)]),
                        atom("Dispatch", vec![v(4), v(2), v(5)]),
                    ],
                },
                // VarPointsTo(p, o) ← CallGraph(s, m), ActualArg(s, a),
                //                     FormalParam(m, p), VarPointsTo(a, o)
                Rule {
                    var_names: vec![
                        "site".into(),
                        "meth".into(),
                        "arg".into(),
                        "param".into(),
                        "obj".into(),
                    ],
                    head: atom("VarPointsTo", vec![v(3), v(4)]),
                    body: vec![
                        atom("CallGraph", vec![v(0), v(1)]),
                        atom("ActualArg", vec![v(0), v(2)]),
                        atom("FormalParam", vec![v(1), v(3)]),
                        atom("VarPointsTo", vec![v(2), v(4)]),
                    ],
                },
            ],
        }
    }

    pub fn catalog() -> Catalog {
        let mut cat = Catalog::new();
        let rel = |name: &str, names: [&str; 2], rows: &[(u64, u64)]| {
            Relation::new(
                name,
                names,
                vec![
                    rows.iter().map(|r| r.0).collect(),
                    rows.iter().map(|r| r.1).collect(),
                ],
            )
        };
        cat.insert(rel(
            "Alloc",
            ["var", "obj"],
            &[(G, O_G), (D, O_DOG), (C, O_CAT), (M, O_MOUSE)],
        ));
        cat.insert(rel("Assign", ["dst", "src"], &[]));
        cat.insert(Relation::new(
            "VirtualCall",
            ["site", "recv", "sig"],
            vec![vec![S1, S3, S4], vec![G, X, G], vec![GREET, SPEAK, GREET]],
        ));
        cat.insert(rel(
            "HeapType",
            ["obj", "ty"],
            &[
                (O_G, T_GREETER),
                (O_DOG, T_DOG),
                (O_CAT, T_CAT),
                (O_MOUSE, T_MOUSE),
            ],
        ));
        cat.insert(Relation::new(
            "Dispatch",
            ["ty", "sig", "meth"],
            vec![
                vec![T_GREETER, T_DOG, T_CAT, T_MOUSE],
                vec![GREET, SPEAK, SPEAK, SPEAK],
                vec![GREETER_GREET, DOG_SPEAK, CAT_SPEAK, MOUSE_SPEAK],
            ],
        ));
        cat.insert(rel("ActualArg", ["site", "arg"], &[(S1, D), (S4, M)]));
        cat.insert(rel("FormalParam", ["meth", "param"], &[(GREETER_GREET, X)]));
        cat
    }

    /// Expected fixpoint, straight from the Souffle reference output.
    pub fn expected_var_points_to() -> Vec<Vec<u64>> {
        vec![
            vec![G, O_G],
            vec![D, O_DOG],
            vec![C, O_CAT],
            vec![M, O_MOUSE],
            vec![X, O_DOG],
            vec![X, O_MOUSE],
        ]
    }

    pub fn expected_call_graph() -> Vec<Vec<u64>> {
        vec![
            vec![S1, GREETER_GREET],
            vec![S3, DOG_SPEAK],
            vec![S3, MOUSE_SPEAK],
            vec![S4, GREETER_GREET],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_are_well_formed() {
        let cat = fg_catalog(50, 100, 5, 1);
        cat.check(&fg_query()).unwrap();

        let cat = triangle_catalog(50, 100, 5, 1);
        cat.check(&triangle_query()).unwrap();
    }
}
