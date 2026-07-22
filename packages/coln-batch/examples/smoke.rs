// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Smoke test: a three-edge graph, one join, both executors.
//!
//! Run with: `cargo run -p coln-batch --example smoke`. The dataset is
//! small enough to check the output by hand.

use coln_batch::query::{Atom, Catalog, Query, Term};
use coln_batch::relation::Relation;
use coln_batch::{binary_join, generic_join};

fn main() -> anyhow::Result<()> {
    // A tiny graph: edges (1 -> 2), (2 -> 3), (3 -> 4).
    let mut catalog = Catalog::new();
    catalog.insert(Relation::new(
        "edge",
        ["src", "dst"],
        vec![vec![1, 2, 3], vec![2, 3, 4]],
    ));

    // Q(x, z) <- edge(x, y), edge(y, z): all paths of length two.
    let query = Query {
        var_names: vec!["x".into(), "y".into(), "z".into()],
        atoms: vec![
            Atom {
                relation: "edge".into(),
                terms: vec![Term::Var(0), Term::Var(1)],
            },
            Atom {
                relation: "edge".into(),
                terms: vec![Term::Var(1), Term::Var(2)],
            },
        ],
        head: vec![0, 2],
    };

    let generic = generic_join::execute(&query, &catalog)?;
    let binary = binary_join::execute(&query, &catalog)?;
    assert_eq!(generic, binary, "executors must agree");

    for i in 0..generic.len() {
        println!("path {:?}", generic.row(i));
    }
    Ok(())
}
