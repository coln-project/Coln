// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Randomized differential testing: hundreds of generated conjunctive
//! queries over generated catalogs, each checked three ways (binary join,
//! generic join, brute-force oracle) like `tests/differential.rs`, but
//! covering a far larger space of query shapes than hand-written
//! fixtures.
//!
//! Columns draw their types from every scalar type the engine knows, and
//! queries are generated well-typed: a variable only stands in columns
//! of its type, a literal matches its column. The value domains are
//! deliberately small: matching and non-matching joins, empty relations,
//! cross products and unsatisfied literals all occur naturally.
//! Each case seeds its own [`SplitMix64`], so a failing case number
//! reproduces in isolation.

use coln_batch::query::{Atom, Catalog, Query, Term};
use coln_batch::rng::SplitMix64;
use coln_batch::types::{Column, ScalarType, Schema, Value};
use coln_batch::{binary_join, generic_join, reference};

const CASES: u64 = 250;
/// Unsigned values are drawn from `0..DOMAIN`, small so collisions are
/// common; the other types have domains of similar size.
const DOMAIN: u64 = 8;
const WORDS: [&str; 6] = ["ant", "bee", "cat", "dog", "eel", "fox"];
const TYPES: [ScalarType; 5] = [
    ScalarType::Uint,
    ScalarType::Iint,
    ScalarType::String,
    ScalarType::Bool,
    ScalarType::Char,
];

fn random_type(rng: &mut SplitMix64) -> ScalarType {
    TYPES[rng.below(TYPES.len() as u64) as usize]
}

/// A random value of type `ty`. Strings occasionally fall outside the
/// stored vocabulary so unknown literals occur.
fn random_value(rng: &mut SplitMix64, ty: ScalarType) -> Value {
    match ty {
        ScalarType::Uint => Value::Uint(rng.below(DOMAIN)),
        ScalarType::Iint => Value::Iint(rng.below(DOMAIN) as i64 - 4),
        ScalarType::String => {
            if rng.below(12) == 0 {
                Value::from("unknown")
            } else {
                Value::from(WORDS[rng.below(WORDS.len() as u64) as usize])
            }
        }
        ScalarType::Bool => Value::Bool(rng.below(2) == 1),
        ScalarType::Char => Value::Char(char::from(b'a' + rng.below(4) as u8)),
    }
}

/// Random catalog: 1..=4 relations, arity 1..=3 with random column types,
/// 0..=20 rows each (zero-row relations included deliberately).
fn random_catalog(rng: &mut SplitMix64) -> (Catalog, Vec<(String, Schema)>) {
    let mut cat = Catalog::new();
    let mut rels = Vec::new();
    for r in 0..1 + rng.below(4) {
        let arity = 1 + rng.below(3) as usize;
        let schema =
            Schema::new((0..arity).map(|c| Column::new(format!("c{c}"), random_type(rng))));
        let rows = rng.below(21);
        let rows: Vec<Vec<Value>> = (0..rows)
            .map(|_| {
                schema
                    .types()
                    .into_iter()
                    .map(|ty| random_value(rng, ty))
                    .collect()
            })
            .collect();
        let name = format!("R{r}");
        cat.insert_rows(name.clone(), schema.clone(), rows).unwrap();
        rels.push((name, schema));
    }
    (cat, rels)
}

/// Random query: 1..=4 atoms over the given relations, a typed pool of up
/// to 4 variables, 25% literal terms. A column takes a variable of its
/// type when the pool has one, otherwise a literal. Returns `None` for the
/// rare all-literal draw, which leaves no variable for the head.
fn random_query(rng: &mut SplitMix64, rels: &[(String, Schema)]) -> Option<Query> {
    // Pool types come from actual columns, so most columns find a match.
    let pool: Vec<ScalarType> = (0..1 + rng.below(4))
        .map(|_| {
            let (_, schema) = &rels[rng.below(rels.len() as u64) as usize];
            schema.column_type(rng.below(schema.arity() as u64) as usize)
        })
        .collect();
    let mut atoms = Vec::new();
    for _ in 0..1 + rng.below(4) {
        let (name, schema) = &rels[rng.below(rels.len() as u64) as usize];
        let terms = schema
            .types()
            .into_iter()
            .map(|ty| {
                let candidates: Vec<usize> = (0..pool.len()).filter(|&v| pool[v] == ty).collect();
                if candidates.is_empty() || rng.below(4) == 0 {
                    Term::Lit(random_value(rng, ty))
                } else {
                    Term::Var(candidates[rng.below(candidates.len() as u64) as usize])
                }
            })
            .collect();
        atoms.push(Atom {
            relation: name.clone(),
            terms,
        });
    }

    // Renumber the variables that actually occur to a dense 0..n range,
    // since `Query::validate` rejects declared-but-unused variables.
    let mut remap: Vec<Option<usize>> = vec![None; pool.len()];
    let mut next = 0;
    for atom in &mut atoms {
        for term in &mut atom.terms {
            if let Term::Var(v) = term {
                let id = *remap[*v].get_or_insert_with(|| {
                    next += 1;
                    next - 1
                });
                *term = Term::Var(id);
            }
        }
    }
    if next == 0 {
        return None;
    }

    // Head: a random non-empty subset of the variables, in random order.
    let mut vars: Vec<usize> = (0..next).collect();
    for i in (1..vars.len()).rev() {
        vars.swap(i, rng.below(i as u64 + 1) as usize);
    }
    vars.truncate(1 + rng.below(next as u64) as usize);

    Some(Query {
        var_names: (0..next).map(|v| format!("v{v}")).collect(),
        atoms,
        head: vars,
    })
}

#[test]
fn random_queries_agree_with_oracle() {
    let mut ran = 0;
    let mut non_empty = 0;
    let mut string_results = 0;
    for case in 0..CASES {
        let mut rng = SplitMix64::new(case);
        let (cat, rels) = random_catalog(&mut rng);
        let Some(query) = random_query(&mut rng, &rels) else {
            continue;
        };
        let oracle = reference::execute(&query, &cat)
            .unwrap_or_else(|e| panic!("case {case}: oracle failed: {e}\n{query:#?}"));
        let binary = binary_join::execute(&query, &cat).unwrap();
        let generic = generic_join::execute(&query, &cat).unwrap();
        assert_eq!(
            oracle, binary,
            "case {case}: binary join disagrees with oracle\n{query:#?}\n{cat:#?}"
        );
        assert_eq!(
            oracle, generic,
            "case {case}: generic join disagrees with oracle\n{query:#?}\n{cat:#?}"
        );
        // Every result decodes under the catalog's dictionary and schema.
        for i in 0..oracle.len() {
            let row = oracle.row_values(i, cat.dictionary()).unwrap();
            assert_eq!(
                row.iter().map(Value::scalar_type).collect::<Vec<_>>(),
                oracle.schema.types(),
                "case {case}: decoded row does not match the result schema"
            );
        }
        ran += 1;
        if !oracle.is_empty() {
            non_empty += 1;
            if oracle.schema.types().contains(&ScalarType::String) {
                string_results += 1;
            }
        }
    }
    // Guard against a degenerate generator: it must neither skip most
    // cases nor produce only trivial ones.
    assert!(ran >= CASES * 8 / 10, "only {ran}/{CASES} cases ran");
    assert!(
        non_empty >= CASES / 10,
        "only {non_empty} non-empty results"
    );
    assert!(
        string_results >= CASES / 40,
        "only {string_results} non-empty results with string columns"
    );
}
