// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Randomized differential testing for recursion: generated Datalog
//! programs over generated data, evaluated with every strategy and
//! executor combination. All four runs must agree on every derived
//! relation. Termination is guaranteed by the tiny value domain.
//!
//! Same reproducibility scheme as `tests/random_queries.rs`: each case
//! seeds its own [`SplitMix64`], so a failing case number reproduces in
//! isolation.

use coln_batch::fixpoint::{self, Exec, FixpointResult};
use coln_batch::query::{Atom, Catalog, Term};
use coln_batch::relation::Relation;
use coln_batch::rng::SplitMix64;
use coln_batch::rule::{Program, Rule};
use coln_batch::{binary_join, generic_join};

const CASES: u64 = 150;
/// Values are drawn from `0..DOMAIN`, small so joins hit often and every
/// closure stays finite and small.
const DOMAIN: u64 = 6;

/// 1..=2 stored relations, arity 1..=2, 0..=10 rows each.
fn random_edb(rng: &mut SplitMix64) -> (Catalog, Vec<(String, usize)>) {
    let mut cat = Catalog::new();
    let mut rels = Vec::new();
    for r in 0..1 + rng.below(2) {
        let arity = 1 + rng.below(2) as usize;
        let rows = rng.below(11);
        let mut cols: Vec<Vec<u64>> = vec![Vec::new(); arity];
        for _ in 0..rows {
            for col in cols.iter_mut() {
                col.push(rng.below(DOMAIN));
            }
        }
        let name = format!("E{r}");
        let col_names: Vec<String> = (0..arity).map(|c| format!("c{c}")).collect();
        cat.insert(Relation::new(name.clone(), col_names, cols));
        rels.push((name, arity));
    }
    (cat, rels)
}

/// 1..=2 derived relations with fixed arities and 2..=4 rules. Rule heads
/// cycle through the derived relations so every one of them is defined;
/// bodies mix stored and derived atoms freely.
fn random_program(rng: &mut SplitMix64, edb: &[(String, usize)]) -> Program {
    let idb: Vec<(String, usize)> = (0..1 + rng.below(2))
        .map(|i| (format!("p{i}"), 1 + rng.below(2) as usize))
        .collect();
    let n_rules = idb.len().max(2 + rng.below(3) as usize);

    let mut rules = Vec::new();
    for r in 0..n_rules {
        let (head_name, head_arity) = idb[r % idb.len()].clone();
        let var_pool = 1 + rng.below(3) as usize;

        let mut atoms = Vec::new();
        for _ in 0..1 + rng.below(3) {
            let pick = rng.below((edb.len() + idb.len()) as u64) as usize;
            let (name, arity) = if pick < edb.len() {
                edb[pick].clone()
            } else {
                idb[pick - edb.len()].clone()
            };
            let terms = (0..arity)
                .map(|_| {
                    if rng.below(5) == 0 {
                        Term::Lit(rng.below(DOMAIN))
                    } else {
                        Term::Var(rng.below(var_pool as u64) as usize)
                    }
                })
                .collect();
            atoms.push(Atom {
                relation: name,
                terms,
            });
        }

        // Renumber the used variables to a dense 0..n range, as
        // `Query::validate` requires (same trick as random_queries).
        let mut remap: Vec<Option<usize>> = vec![None; var_pool];
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
            // All-literal body: force one variable so the head has one.
            atoms[0].terms[0] = Term::Var(0);
            next = 1;
        }

        // Head of the fixed arity; the first column is always a variable
        // so the head is never all-literal.
        let head_terms = (0..head_arity)
            .map(|i| {
                if i > 0 && rng.below(5) == 0 {
                    Term::Lit(rng.below(DOMAIN))
                } else {
                    Term::Var(rng.below(next as u64) as usize)
                }
            })
            .collect();

        rules.push(Rule {
            var_names: (0..next).map(|v| format!("v{v}")).collect(),
            head: Atom {
                relation: head_name,
                terms: head_terms,
            },
            body: atoms,
        });
    }
    Program { rules }
}

#[test]
fn random_programs_all_combinations_agree() {
    let mut non_empty = 0;
    for case in 0..CASES {
        let mut rng = SplitMix64::new(case);
        let (mut edb, edb_rels) = random_edb(&mut rng);
        let program = random_program(&mut rng, &edb_rels);

        // Sometimes pre-seed the first derived relation with initial
        // facts, matching its head arity.
        if rng.below(4) == 0 {
            let name = program.rules[0].head.relation.clone();
            let arity = program.rules[0].head.terms.len();
            let rows = rng.below(4);
            let mut cols: Vec<Vec<u64>> = vec![Vec::new(); arity];
            for _ in 0..rows {
                for col in cols.iter_mut() {
                    col.push(rng.below(DOMAIN));
                }
            }
            let col_names: Vec<String> = (0..arity).map(|c| format!("c{c}")).collect();
            edb.insert(Relation::new(name, col_names, cols));
        }

        let mut idb_names: Vec<String> = program
            .rules
            .iter()
            .map(|r| r.head.relation.clone())
            .collect();
        idb_names.sort();
        idb_names.dedup();

        let run = |name: &str, r: anyhow::Result<FixpointResult>| -> Catalog {
            r.unwrap_or_else(|e| panic!("case {case}: {name} failed: {e}"))
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
        if idb_names.iter().any(|n| !base.get(n).unwrap().is_empty()) {
            non_empty += 1;
        }
    }
    // Guard against a degenerate generator.
    assert!(non_empty >= CASES / 10, "only {non_empty} non-empty cases");
}
