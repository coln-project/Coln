// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Ready-made example queries with matching data generators — one fixture
//! per query class the engine must serve. Used by tests and the demo.

use crate::generate;
use crate::query::{Atom, Catalog, Query, Term};
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

/// Two hops along edges with the same label, a typed join on a string
/// column:
///
/// ```text
/// Q(x, z, l) ← edge(x, y, l, w1), edge(y, z, l, w2)
/// ```
///
/// Variable order: l, y, x, z, w1, w2.
pub fn labeled_two_hop_query() -> Query {
    let (l, y, x, z, w1, w2) = (0, 1, 2, 3, 4, 5);
    Query {
        var_names: ["l", "y", "x", "z", "w1", "w2"]
            .into_iter()
            .map(String::from)
            .collect(),
        atoms: vec![
            Atom {
                relation: "edge".into(),
                terms: vec![Term::Var(x), Term::Var(y), Term::Var(l), Term::Var(w1)],
            },
            Atom {
                relation: "edge".into(),
                terms: vec![Term::Var(y), Term::Var(z), Term::Var(l), Term::Var(w2)],
            },
        ],
        head: vec![x, z, l],
    }
}

/// Catalog with generated data for [`labeled_two_hop_query`].
pub fn labeled_catalog(nodes: u64, edges: usize, labels: &[&str], seed: u64) -> Catalog {
    let mut cat = Catalog::new();
    let rel = generate::labeled_edges(nodes, edges, labels, seed, cat.dictionary_mut());
    cat.insert(rel);
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

/// Reachability along edges of one label, a typed recursive program:
///
/// ```text
/// reach(x, y, l) ← edge(x, y, l, w)
/// reach(x, z, l) ← reach(x, y, l), edge(y, z, l, w)
/// ```
pub fn labeled_reach_program() -> Program {
    let (x, y, z, l, w) = (0, 1, 2, 3, 4);
    Program {
        rules: vec![
            Rule {
                var_names: vec!["x".into(), "y".into(), "l".into(), "w".into()],
                head: Atom {
                    relation: "reach".into(),
                    terms: vec![Term::Var(0), Term::Var(1), Term::Var(2)],
                },
                body: vec![Atom {
                    relation: "edge".into(),
                    terms: vec![Term::Var(0), Term::Var(1), Term::Var(2), Term::Var(3)],
                }],
            },
            Rule {
                var_names: ["x", "y", "z", "l", "w"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                head: Atom {
                    relation: "reach".into(),
                    terms: vec![Term::Var(x), Term::Var(z), Term::Var(l)],
                },
                body: vec![
                    Atom {
                        relation: "reach".into(),
                        terms: vec![Term::Var(x), Term::Var(y), Term::Var(l)],
                    },
                    Atom {
                        relation: "edge".into(),
                        terms: vec![Term::Var(y), Term::Var(z), Term::Var(l), Term::Var(w)],
                    },
                ],
            },
        ],
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

        let cat = labeled_catalog(20, 60, &["road", "rail"], 1);
        let typing = cat.check(&labeled_two_hop_query()).unwrap();
        assert_eq!(
            typing.head_schema(&labeled_two_hop_query()).types(),
            vec![
                crate::types::ScalarType::Uint,
                crate::types::ScalarType::Uint,
                crate::types::ScalarType::String
            ]
        );
    }
}
