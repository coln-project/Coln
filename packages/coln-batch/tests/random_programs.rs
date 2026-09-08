// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Randomized differential testing for recursion: generated Datalog
//! programs over generated data, evaluated with every strategy and
//! executor combination. All four runs must agree on every derived
//! relation. Termination is guaranteed by the tiny value domains.
//!
//! Relations carry random column types and rules are generated
//! well-typed; every derived relation is declared in the catalog (an
//! empty relation with a schema), as a plan would declare it, because a
//! random program may define a relation only in terms of itself.
//!
//! Same reproducibility scheme as `tests/random_queries.rs`: each case
//! seeds its own [`SplitMix64`], so a failing case number reproduces in
//! isolation.

use coln_batch::fixpoint::{self, Exec, FixpointResult};
use coln_batch::query::{Atom, Catalog, Term};
use coln_batch::relation::Relation;
use coln_batch::rng::SplitMix64;
use coln_batch::rule::{Program, Rule};
use coln_batch::types::{Column, ScalarType, Schema, Value};
use coln_batch::{binary_join, generic_join};

const CASES: u64 = 150;
/// Unsigned values are drawn from `0..DOMAIN`, small so joins hit often
/// and every closure stays finite and small.
const DOMAIN: u64 = 6;
const WORDS: [&str; 4] = ["ant", "bee", "cat", "dog"];
const TYPES: [ScalarType; 5] = [
    ScalarType::Uint,
    ScalarType::Iint,
    ScalarType::String,
    ScalarType::Bool,
    ScalarType::Char,
];

fn random_value(rng: &mut SplitMix64, ty: ScalarType) -> Value {
    match ty {
        ScalarType::Uint => Value::Uint(rng.below(DOMAIN)),
        ScalarType::Iint => Value::Iint(rng.below(DOMAIN) as i64 - 3),
        ScalarType::String => Value::from(WORDS[rng.below(WORDS.len() as u64) as usize]),
        ScalarType::Bool => Value::Bool(rng.below(2) == 1),
        ScalarType::Char => Value::Char(char::from(b'a' + rng.below(3) as u8)),
    }
}

/// A random schema of `arity` columns whose first column is `uint`, so
/// every relation offers a variable for a rule head.
fn random_schema(rng: &mut SplitMix64, arity: usize) -> Schema {
    Schema::new((0..arity).map(|c| {
        let ty = if c == 0 {
            ScalarType::Uint
        } else {
            TYPES[rng.below(TYPES.len() as u64) as usize]
        };
        Column::new(format!("c{c}"), ty)
    }))
}

fn random_rows(rng: &mut SplitMix64, schema: &Schema, max_rows: u64) -> Vec<Vec<Value>> {
    (0..rng.below(max_rows + 1))
        .map(|_| {
            schema
                .types()
                .into_iter()
                .map(|ty| random_value(rng, ty))
                .collect()
        })
        .collect()
}

/// 1..=2 stored relations, arity 1..=2, 0..=10 rows each.
fn random_edb(rng: &mut SplitMix64) -> (Catalog, Vec<(String, Schema)>) {
    let mut cat = Catalog::new();
    let mut rels = Vec::new();
    for r in 0..1 + rng.below(2) {
        let arity = 1 + rng.below(2) as usize;
        let schema = random_schema(rng, arity);
        let rows = random_rows(rng, &schema, 10);
        let name = format!("E{r}");
        cat.insert_rows(name.clone(), schema.clone(), rows).unwrap();
        rels.push((name, schema));
    }
    (cat, rels)
}

/// 1..=2 derived relations with fixed schemas and 2..=4 rules. Rule heads
/// cycle through the derived relations so every one of them is defined;
/// bodies mix stored and derived atoms freely.
fn random_program(
    rng: &mut SplitMix64,
    edb: &[(String, Schema)],
) -> (Program, Vec<(String, Schema)>) {
    let idb: Vec<(String, Schema)> = (0..1 + rng.below(2))
        .map(|i| {
            let arity = 1 + rng.below(2) as usize;
            (format!("p{i}"), random_schema(rng, arity))
        })
        .collect();
    let n_rules = idb.len().max(2 + rng.below(3) as usize);
    let all: Vec<&(String, Schema)> = edb.iter().chain(idb.iter()).collect();

    let mut rules = Vec::new();
    for r in 0..n_rules {
        let (head_name, head_schema) = idb[r % idb.len()].clone();
        // A typed pool of variables, types drawn from actual columns.
        let pool: Vec<ScalarType> = (0..1 + rng.below(3))
            .map(|_| {
                let (_, schema) = all[rng.below(all.len() as u64) as usize];
                schema.column_type(rng.below(schema.arity() as u64) as usize)
            })
            .collect();

        let mut atoms = Vec::new();
        for _ in 0..1 + rng.below(3) {
            let (name, schema) = all[rng.below(all.len() as u64) as usize];
            let terms = schema
                .types()
                .into_iter()
                .map(|ty| {
                    let candidates: Vec<usize> =
                        (0..pool.len()).filter(|&v| pool[v] == ty).collect();
                    if candidates.is_empty() || rng.below(5) == 0 {
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

        // Renumber the used variables to a dense 0..n range, as
        // `Query::validate` requires, keeping their types.
        let mut remap: Vec<Option<usize>> = vec![None; pool.len()];
        let mut var_types: Vec<ScalarType> = Vec::new();
        for atom in &mut atoms {
            for term in &mut atom.terms {
                if let Term::Var(v) = term {
                    let id = *remap[*v].get_or_insert_with(|| {
                        var_types.push(pool[*v]);
                        var_types.len() - 1
                    });
                    *term = Term::Var(id);
                }
            }
        }
        // The head's first column is uint and must be a variable: make
        // sure the body binds one (every relation's first column is uint).
        if !var_types.contains(&ScalarType::Uint) {
            atoms[0].terms[0] = Term::Var(var_types.len());
            var_types.push(ScalarType::Uint);
        }

        // Head of the fixed schema: a variable of the column's type where
        // the body binds one (always for the first column), else a
        // literal.
        let head_terms = head_schema
            .types()
            .into_iter()
            .enumerate()
            .map(|(i, ty)| {
                let candidates: Vec<usize> = (0..var_types.len())
                    .filter(|&v| var_types[v] == ty)
                    .collect();
                if candidates.is_empty() || (i > 0 && rng.below(5) == 0) {
                    Term::Lit(random_value(rng, ty))
                } else {
                    Term::Var(candidates[rng.below(candidates.len() as u64) as usize])
                }
            })
            .collect();

        rules.push(Rule {
            var_names: (0..var_types.len()).map(|v| format!("v{v}")).collect(),
            head: Atom {
                relation: head_name,
                terms: head_terms,
            },
            body: atoms,
        });
    }
    (Program { rules }, idb)
}

#[test]
fn random_programs_all_combinations_agree() {
    let mut non_empty = 0;
    for case in 0..CASES {
        let mut rng = SplitMix64::new(case);
        let (mut edb, edb_rels) = random_edb(&mut rng);
        let (program, idb) = random_program(&mut rng, &edb_rels);

        // Declare every derived relation; sometimes pre-seed the first
        // one with initial facts.
        for (i, (name, schema)) in idb.iter().enumerate() {
            if i == 0 && rng.below(4) == 0 {
                let rows = random_rows(&mut rng, schema, 3);
                edb.insert_rows(name.clone(), schema.clone(), rows).unwrap();
            } else {
                edb.insert(Relation::empty(name.clone(), schema.clone()));
            }
        }
        let idb_names: Vec<&str> = idb.iter().map(|(name, _)| name.as_str()).collect();

        let run = |name: &str, r: anyhow::Result<FixpointResult>| -> Catalog {
            r.unwrap_or_else(|e| panic!("case {case}: {name} failed: {e}\n{program:#?}"))
                .catalog
        };
        let base = run(
            "semi+generic",
            fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec),
        );
        let others = [
            (
                "semi+binary",
                run(
                    "semi+binary",
                    fixpoint::semi_naive(&program, &edb, binary_join::execute as Exec),
                ),
            ),
            (
                "naive+generic",
                run(
                    "naive+generic",
                    fixpoint::naive(&program, &edb, generic_join::execute as Exec),
                ),
            ),
            (
                "naive+binary",
                run(
                    "naive+binary",
                    fixpoint::naive(&program, &edb, binary_join::execute as Exec),
                ),
            ),
        ];
        for (name, cat) in &others {
            for idb in &idb_names {
                assert_eq!(
                    base.get(idb).unwrap().cols,
                    cat.get(idb).unwrap().cols,
                    "case {case}: {name} disagrees on {idb}"
                );
            }
        }
        for (name, schema) in &idb {
            let rel = base.get(name).unwrap();
            assert_eq!(
                &rel.schema, schema,
                "case {case}: {name} lost its declared schema"
            );
            // Every derived row decodes under the result catalog.
            base.rows_of(name)
                .unwrap_or_else(|e| panic!("case {case}: {name} does not decode: {e}"));
            if !rel.is_empty() {
                non_empty += 1;
            }
        }
    }
    // Guard against a degenerate generator.
    assert!(non_empty >= CASES / 10, "only {non_empty} non-empty cases");
}
