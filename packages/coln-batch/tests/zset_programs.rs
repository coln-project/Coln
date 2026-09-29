// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Z-set semantics of programs, on random programs over random input:
//! stored rows with any weights and in several copies, derived relations
//! that start from initial facts, rules that subtract or count twice,
//! relations declared distinct, and recursion within one relation and
//! across several.
//!
//! Every strategy and executor is held against the definition in `common`,
//! refusals included, and against laws that need no definition: the order
//! of the rules does not matter, a rule may be split into two whose
//! weights add up to its own, declaring a relation distinct that nothing
//! reads only reduces it to a set, the way rows are stored does not
//! matter, and scaling positive input leaves every recursive relation as
//! it is. Each case seeds its own [`SplitMix64`], so a failing case number
//! reproduces in isolation.

mod common;

use std::collections::BTreeMap;

use coln_batch::fixpoint::{self, Exec, FixpointResult};
use coln_batch::query::{Atom, Catalog, Term};
use coln_batch::relation::Relation;
use coln_batch::rng::SplitMix64;
use coln_batch::rule::{Program, Rule};
use coln_batch::types::{Schema, Weight};
use coln_batch::{binary_join, generic_join, reference};

use common::{Outcome, Storage, ZSet};

const CASES: u64 = 1000;

/// A fixpoint strategy, `semi_naive` or `naive`.
type Strategy = fn(&Program, &Catalog, Exec) -> anyhow::Result<FixpointResult>;

const RUNS: [(&str, Strategy, Exec); 5] = [
    (
        "semi-naive, generic join",
        fixpoint::semi_naive as Strategy,
        generic_join::execute as Exec,
    ),
    (
        "semi-naive, binary join",
        fixpoint::semi_naive as Strategy,
        binary_join::execute as Exec,
    ),
    (
        "semi-naive, oracle",
        fixpoint::semi_naive as Strategy,
        reference::execute as Exec,
    ),
    (
        "naive, generic join",
        fixpoint::naive as Strategy,
        generic_join::execute as Exec,
    ),
    (
        "naive, binary join",
        fixpoint::naive as Strategy,
        binary_join::execute as Exec,
    ),
];

/// The derived relations a run produced, decoded, or its error message.
type Got = Result<BTreeMap<String, ZSet>, String>;

fn got(result: anyhow::Result<FixpointResult>, derived: &[String]) -> Got {
    let catalog = result.map_err(|e| e.to_string())?.catalog;
    Ok(derived
        .iter()
        .map(|name| {
            let rel = catalog.get(name).unwrap();
            assert!(rel.is_consolidated(), "{name} is not in normal form");
            (name.clone(), common::zset_of(rel, catalog.dictionary()))
        })
        .collect())
}

/// The main run: semi-naive over the generic join.
fn evaluate(program: &Program, edb: &Catalog, derived: &[String]) -> Got {
    got(
        fixpoint::semi_naive(program, edb, generic_join::execute as Exec),
        derived,
    )
}

fn random_case(case: u64) -> (SplitMix64, common::ProgramCase) {
    let mut rng = SplitMix64::new(case);
    let signed = rng.below(2) == 0;
    let program_case = common::program_case(&mut rng, signed);
    (rng, program_case)
}

/// How much of the space of programs a run of cases covered.
#[derive(Default)]
struct Coverage {
    evaluated: usize,
    refused: usize,
    recursion: usize,
    negative: usize,
    distinct: usize,
    mixed: usize,
}

/// Hold every strategy and executor against the definition on the given
/// cases, refusals included.
fn against_the_definition(cases: std::ops::Range<u64>) -> Coverage {
    let mut seen = Coverage::default();
    for case in cases {
        let (_, c) = random_case(case);
        let expected = common::program_definition(&c.program, &common::stored_catalog(&c.edb));
        let recursive = common::recursive_relations(&c.program);
        for (name, strategy, exec) in RUNS {
            let result = got(strategy(&c.program, &c.edb, exec), &c.derived);
            match &expected {
                Outcome::Refused(phrase) => {
                    let err = result.expect_err(&format!("case {case}: {name} must refuse"));
                    assert!(err.contains(phrase), "case {case}: {name}: {err}");
                }
                Outcome::Derived(values) => {
                    let result = result.unwrap_or_else(|e| {
                        panic!("case {case}: {name} failed: {e}\n{:#?}", c.program)
                    });
                    assert_eq!(
                        &result, values,
                        "case {case}: {name} disagrees with the definition\n{:#?}\n{:#?}",
                        c.program, c.edb
                    );
                    for relation in &recursive {
                        assert!(
                            result[relation].values().all(|&w| w == 1),
                            "case {case}: {name}: the recursive {relation} is not a set"
                        );
                    }
                }
            }
        }
        match &expected {
            Outcome::Refused(_) => seen.refused += 1,
            Outcome::Derived(values) => {
                seen.evaluated += 1;
                let non_empty = |name: &String| !values[name].is_empty();
                seen.recursion += usize::from(recursive.iter().any(non_empty));
                seen.negative += usize::from(values.values().any(|z| z.values().any(|&w| w < 0)));
                seen.distinct += usize::from(c.program.distinct.iter().any(non_empty));
                seen.mixed += usize::from(
                    recursive.iter().any(non_empty)
                        && c.derived
                            .iter()
                            .any(|d| !recursive.contains(d) && non_empty(d)),
                );
            }
        }
    }
    seen
}

#[test]
fn random_programs_match_the_definition() {
    let seen = against_the_definition(0..CASES);
    // Guard against a generator that stopped producing what the suite is for.
    let cases = CASES as usize;
    assert!(
        seen.evaluated >= cases * 2 / 5,
        "only {} evaluated",
        seen.evaluated
    );
    assert!(seen.refused >= cases / 20, "only {} refused", seen.refused);
    assert!(
        seen.recursion >= cases / 10,
        "only {} recursions",
        seen.recursion
    );
    assert!(
        seen.negative >= cases / 20,
        "only {} negative results",
        seen.negative
    );
    assert!(
        seen.distinct >= cases / 20,
        "only {} distinct relations",
        seen.distinct
    );
    assert!(
        seen.mixed >= cases / 40,
        "only {} mixed programs",
        seen.mixed
    );
}

/// The same check on 50 000 further cases.
/// Run with: cargo test -p coln-batch --release -- --include-ignored
#[test]
#[ignore = "large; run explicitly (use --release)"]
fn random_programs_match_the_definition_at_scale() {
    let seen = against_the_definition(CASES..CASES + 50_000);
    assert!(
        seen.recursion >= 5_000,
        "only {} recursions",
        seen.recursion
    );
}

/// Two outcomes agree: the same values, or both refused.
fn assert_same(a: &Got, b: &Got, what: &str) {
    match (a, b) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "{what}"),
        (Err(_), Err(_)) => {}
        _ => panic!("{what}: {a:?} against {b:?}"),
    }
}

#[test]
fn the_order_of_the_rules_does_not_matter() {
    for case in 0..CASES {
        let (mut rng, c) = random_case(case);
        let mut shuffled = c.program.clone();
        common::shuffle(&mut rng, &mut shuffled.rules);
        assert_same(
            &evaluate(&c.program, &c.edb, &c.derived),
            &evaluate(&shuffled, &c.edb, &c.derived),
            &format!("case {case}"),
        );
    }
}

#[test]
fn a_rule_splits_into_two_whose_weights_add_up_to_its_own() {
    let mut checked = 0;
    for case in 0..CASES {
        let (mut rng, c) = random_case(case);
        let recursive = common::recursive_relations(&c.program);
        let candidates: Vec<usize> = (0..c.program.rules.len())
            .filter(|&i| !recursive.contains(&c.program.rules[i].head.relation))
            .collect();
        if candidates.is_empty() {
            continue;
        }
        let i = common::pick(&mut rng, &candidates);
        let rule = c.program.rules[i].clone();
        let part: Weight = if rule.weight == -1 { 1 } else { -1 };
        let mut split = c.program.clone();
        split.rules[i].weight = rule.weight - part;
        split.rules.push(Rule {
            weight: part,
            ..rule
        });
        assert_same(
            &evaluate(&c.program, &c.edb, &c.derived),
            &evaluate(&split, &c.edb, &c.derived),
            &format!("case {case}, rule {i}"),
        );
        checked += 1;
    }
    assert!(
        checked >= CASES as usize * 2 / 5,
        "only {checked} rules split"
    );
}

#[test]
fn declaring_a_relation_distinct_that_nothing_reads_reduces_it_to_a_set() {
    let mut checked = 0;
    for case in 0..CASES {
        let (_, c) = random_case(case);
        let read: Vec<&str> = c
            .program
            .rules
            .iter()
            .flat_map(|r| &r.body)
            .map(|a| a.relation.as_str())
            .collect();
        let before = evaluate(&c.program, &c.edb, &c.derived);
        for sink in c.derived.iter().filter(|d| !read.contains(&d.as_str())) {
            let mut declared = c.program.clone();
            declared.distinct.insert(sink.clone());
            let after = evaluate(&declared, &c.edb, &c.derived);
            let expected = before.clone().map(|mut values| {
                let set = common::support(&values[sink]);
                values.insert(sink.clone(), set);
                values
            });
            assert_same(&after, &expected, &format!("case {case}, {sink}"));
            checked += 1;
        }
    }
    assert!(
        checked >= CASES as usize / 4,
        "only {checked} relations declared"
    );
}

/// `c.edb` with every relation stored anew in a random form.
fn restored(rng: &mut SplitMix64, edb: &Catalog) -> Catalog {
    let mut cat = Catalog::new();
    for name in edb.names() {
        let rel = edb.get(name).unwrap();
        let rows = (0..rel.len())
            .map(|i| (rel.row_values(i, edb.dictionary()).unwrap(), rel.weight(i)))
            .collect();
        let storage = common::pick(
            rng,
            &[Storage::Consolidated, Storage::Copies, Storage::Units],
        );
        common::store(rng, &mut cat, name, &rel.schema, rows, storage);
    }
    cat
}

#[test]
fn how_the_input_is_stored_does_not_matter() {
    for case in 0..CASES {
        let (mut rng, c) = random_case(case);
        let other = restored(&mut rng, &c.edb);
        let a = evaluate(&c.program, &c.edb, &c.derived);
        let b = evaluate(&c.program, &other, &c.derived);
        assert_same(&a, &b, &format!("case {case}"));
    }
}

#[test]
fn scaling_positive_input_leaves_every_recursion_as_it_is() {
    let mut checked = 0;
    for case in 0..CASES {
        let mut rng = SplitMix64::new(case);
        let c = common::program_case(&mut rng, false);
        if c.program.rules.iter().any(|r| r.weight < 0) {
            continue;
        }
        let mut scaled = c.edb.clone();
        for name in c.edb.names() {
            if c.derived.iter().any(|d| d == name) {
                continue;
            }
            let mut rel = c.edb.get(name).unwrap().clone();
            for w in &mut rel.weights {
                *w *= 3;
            }
            scaled.insert(rel);
        }
        let recursive = common::recursive_relations(&c.program);
        let before = evaluate(&c.program, &c.edb, &c.derived).unwrap();
        let after = evaluate(&c.program, &scaled, &c.derived).unwrap();
        for relation in &recursive {
            assert_eq!(before[relation], after[relation], "case {case}, {relation}");
        }
        checked += usize::from(!recursive.is_empty());
    }
    assert!(
        checked >= CASES as usize / 10,
        "only {checked} recursions checked"
    );
}

// ---------------------------------------------------------------------------
// Hand-picked cases

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

fn weighing(weight: Weight, rule: Rule) -> Rule {
    Rule { weight, ..rule }
}

/// A relation of uint columns with given rows and weights, stored as is.
fn stored(name: &str, arity: usize, rows: &[(&[u64], Weight)]) -> Relation {
    let cols = (0..arity)
        .map(|c| rows.iter().map(|(row, _)| row[c]).collect())
        .collect();
    let schema = Schema::uint((0..arity).map(|c| format!("c{c}")));
    Relation::with_weights(name, schema, cols, rows.iter().map(|&(_, w)| w).collect())
}

/// Every run must agree; returns the derived relation `name` as (row,
/// weight) pairs.
fn derived(program: &Program, edb: &Catalog, name: &str) -> Vec<(Vec<u64>, Weight)> {
    let results: Vec<Relation> = RUNS
        .iter()
        .map(|&(run, strategy, exec)| {
            let catalog = strategy(program, edb, exec)
                .unwrap_or_else(|e| panic!("{run} failed: {e}"))
                .catalog;
            catalog.get(name).unwrap().clone()
        })
        .collect();
    for (result, (run, ..)) in results.iter().zip(RUNS).skip(1) {
        assert_eq!(&results[0], result, "{run} disagrees on {name}");
    }
    let r = &results[0];
    (0..r.len()).map(|i| (r.row(i), r.weight(i))).collect()
}

/// Every run must refuse the program, with `phrase` in its message.
fn refused(program: &Program, edb: &Catalog, phrase: &str) {
    for (run, strategy, exec) in RUNS {
        let err = strategy(program, edb, exec)
            .err()
            .unwrap_or_else(|| panic!("{run} must refuse"))
            .to_string();
        assert!(err.contains(phrase), "{run}: {err}");
    }
}

fn chain(edges: &[(u64, u64)]) -> Relation {
    let rows: Vec<(&[u64], Weight)> = Vec::new();
    let mut rel = stored("parent", 2, &rows);
    for &(a, b) in edges {
        rel.cols[0].push(a);
        rel.cols[1].push(b);
        rel.weights.push(1);
    }
    rel
}

fn ancestor() -> Vec<Rule> {
    vec![
        rule(("ancestor", &[0, 1]), &[("parent", &[0, 1])]),
        rule(
            ("ancestor", &[0, 2]),
            &[("parent", &[0, 1]), ("ancestor", &[1, 2])],
        ),
    ]
}

#[test]
fn initial_facts_add_to_what_a_weighted_relation_derives() {
    // Two ways from 0 to 3 give (0, 3) weight 2; initial facts of weight -2
    // cancel it, and a row only among the initial facts stays.
    let mut edb = Catalog::new();
    edb.insert(chain(&[(0, 1), (0, 2), (1, 3), (2, 3)]));
    edb.insert(stored("two_hop", 2, &[(&[0, 3], -2), (&[5, 5], 4)]));
    let program = Program::new(vec![rule(
        ("two_hop", &[0, 2]),
        &[("parent", &[0, 1]), ("parent", &[1, 2])],
    )]);
    assert_eq!(derived(&program, &edb, "two_hop"), vec![(vec![5, 5], 4)]);
}

#[test]
fn initial_facts_of_a_recursive_relation_count_once() {
    let mut edb = Catalog::new();
    edb.insert(chain(&[(0, 1)]));
    edb.insert(stored("ancestor", 2, &[(&[7, 8], 2), (&[0, 1], 3)]));
    let program = Program::new(ancestor());
    assert_eq!(
        derived(&program, &edb, "ancestor"),
        vec![(vec![0, 1], 1), (vec![7, 8], 1)]
    );
}

#[test]
fn a_subtraction_anywhere_in_a_mutual_recursion_is_refused() {
    // even(y) ← succ(x, y), odd(x) subtracts; odd reads even back, so the
    // subtraction sits inside the recursion.
    let mut edb = Catalog::new();
    edb.insert(stored("succ", 2, &[(&[0, 1], 1), (&[1, 2], 1)]));
    edb.insert(stored("even", 1, &[(&[0], 1)]));
    let program = Program::new(vec![
        rule(("odd", &[1]), &[("succ", &[0, 1]), ("even", &[0])]),
        weighing(
            -1,
            rule(("even", &[1]), &[("succ", &[0, 1]), ("odd", &[0])]),
        ),
    ]);
    refused(&program, &edb, "even is recursive");
}

#[test]
fn a_recursive_rule_of_weight_two_still_derives_a_set() {
    let mut edb = Catalog::new();
    edb.insert(chain(&[(0, 1), (1, 2), (2, 0)]));
    let mut rules = ancestor();
    rules[1].weight = 2;
    let result = derived(&Program::new(rules), &edb, "ancestor");
    assert_eq!(result.len(), 9);
    assert!(result.iter().all(|(_, w)| *w == 1));
}

#[test]
fn a_recursion_may_read_a_distinct_relation() {
    // edge counts both directions of every parent pair; declared distinct,
    // it is a set, and reach closes over it.
    let mut edb = Catalog::new();
    edb.insert(chain(&[(0, 1), (1, 0), (1, 2)]));
    let mut program = Program::new(vec![
        rule(("edge", &[0, 1]), &[("parent", &[0, 1])]),
        rule(("edge", &[1, 0]), &[("parent", &[0, 1])]),
        rule(("reach", &[0, 1]), &[("edge", &[0, 1])]),
        rule(("reach", &[0, 2]), &[("reach", &[0, 1]), ("edge", &[1, 2])]),
    ]);
    let counted = derived(&program, &edb, "edge");
    assert!(counted.contains(&(vec![0, 1], 2)), "{counted:?}");
    program.distinct.insert("edge".into());
    let edge = derived(&program, &edb, "edge");
    assert!(edge.iter().all(|(_, w)| *w == 1));
    let reach = derived(&program, &edb, "reach");
    assert_eq!(reach.len(), 9, "0, 1 and 2 all reach each other: {reach:?}");
}

#[test]
fn weights_travel_through_a_chain_of_strata() {
    // r0(x) ← e(x), and r(i+1)(x) ← r(i)(x) with weight 2, ten times: the
    // row of weight 3 arrives with weight 3 · 2¹⁰.
    let mut edb = Catalog::new();
    edb.insert(stored("e", 1, &[(&[4], 3)]));
    let mut rules = vec![rule(("r0", &[0]), &[("e", &[0])])];
    for i in 0..10 {
        let (from, to) = (format!("r{i}"), format!("r{}", i + 1));
        rules.push(weighing(2, rule((&to, &[0]), &[(&from, &[0])])));
    }
    let program = Program::new(rules);
    assert_eq!(derived(&program, &edb, "r10"), vec![(vec![4], 3 * 1024)]);
    let stats = fixpoint::semi_naive(&program, &edb, generic_join::execute as Exec)
        .unwrap()
        .stats;
    assert_eq!(stats.rounds, 11, "one round per stratum");
    assert_eq!(stats.new_facts_per_round, vec![1; 11]);
}

#[test]
fn statistics_count_every_stratum() {
    // copy (3 rows, one round) comes first, then ancestor over copy: one
    // round per path length plus the final empty one.
    let mut edb = Catalog::new();
    edb.insert(chain(&[(0, 1), (1, 2), (2, 3)]));
    let program = Program::new(vec![
        rule(("copy", &[0, 1]), &[("parent", &[0, 1])]),
        rule(("ancestor", &[0, 1]), &[("copy", &[0, 1])]),
        rule(
            ("ancestor", &[0, 2]),
            &[("copy", &[0, 1]), ("ancestor", &[1, 2])],
        ),
    ]);
    for (run, strategy, exec) in RUNS {
        let stats = strategy(&program, &edb, exec).unwrap().stats;
        assert_eq!(stats.new_facts_per_round, vec![3, 3, 2, 1, 0], "{run}");
        assert_eq!(stats.rounds, 5, "{run}");
        assert_eq!(stats.total_new_facts(), 9, "{run}");
    }
}

#[test]
fn a_derived_relation_may_start_from_nothing_but_initial_facts() {
    // The rule reads an empty relation, so only the initial facts remain,
    // with their weights.
    let mut edb = Catalog::new();
    edb.insert(stored("empty", 1, &[]));
    edb.insert(stored("seen", 1, &[(&[1], 2), (&[2], -1)]));
    let program = Program::new(vec![rule(("seen", &[0]), &[("empty", &[0])])]);
    assert_eq!(
        derived(&program, &edb, "seen"),
        vec![(vec![1], 2), (vec![2], -1)]
    );
}
