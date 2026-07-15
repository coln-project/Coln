// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Ready-made example queries with matching data generators — one fixture
//! per query class the engine must serve. Used by tests and the demo.

use crate::generate;
use crate::query::{Atom, Catalog, Query, Term};

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
