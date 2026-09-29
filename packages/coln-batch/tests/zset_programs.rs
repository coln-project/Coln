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
//! of the rules does not matter, a rule may be split into two whose weights
//! add up to its own, declaring a relation distinct that nothing reads only
//! reduces it to a set, the way rows are stored does not matter, and
//! scaling positive input leaves every recursion as it is. Each case seeds
//! its own [`SplitMix64`], so a failing case number reproduces in
//! isolation.

mod common;

use std::collections::BTreeMap;
use std::ops::Range;

use coln_batch::fixpoint::{self, Exec, FixpointResult};
use coln_batch::query::Catalog;
use coln_batch::relation::Relation;
use coln_batch::rng::SplitMix64;
use coln_batch::rule::Program;
use coln_batch::types::Weight;
use coln_batch::{binary_join, fixtures, generic_join, reference};

use common::{Outcome, ProgramCase, Storage, ZSet, entries, rule, stored, weighing};

const CASES: u64 = 1000;

/// A fixpoint strategy, `semi_naive` or `naive`.
type Strategy = fn(&Program, &Catalog, Exec) -> anyhow::Result<FixpointResult>;

const RUNS: [(&str, Strategy, Exec); 5] = [
    (
        "semi-naive, generic join",
        fixpoint::semi_naive,
        generic_join::execute,
    ),
    (
        "semi-naive, binary join",
        fixpoint::semi_naive,
        binary_join::execute,
    ),
    (
        "semi-naive, oracle",
        fixpoint::semi_naive,
        reference::execute,
    ),
    (
        "naive, generic join",
        fixpoint::naive,
        generic_join::execute,
    ),
    ("naive, binary join", fixpoint::naive, binary_join::execute),
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
        fixpoint::semi_naive(program, edb, generic_join::execute),
        derived,
    )
}

/// Run `check` on every case in `cases`, with the case's generator for
/// further choices.
fn each_case(cases: Range<u64>, mut check: impl FnMut(u64, &mut SplitMix64, &ProgramCase)) {
    for case in cases {
        let mut rng = SplitMix64::new(case);
        let signed = rng.below(2) == 0;
        let c = common::program_case(&mut rng, signed);
        check(case, &mut rng, &c);
    }
}

/// Two outcomes agree: the same values, or both refused.
fn assert_same(a: &Got, b: &Got, what: &str) {
    match (a, b) {
        (Ok(a), Ok(b)) => assert_eq!(a, b, "{what}"),
        (Err(_), Err(_)) => {}
        _ => panic!("{what}: {a:?} against {b:?}"),
    }
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

fn against_the_definition(cases: Range<u64>) -> Coverage {
    let mut seen = Coverage::default();
    each_case(cases, |case, _, c| {
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
                        let set = result[relation].values().all(|&w| w == 1);
                        assert!(set, "case {case}: {name}: {relation} is not a set");
                    }
                }
            }
        }
        let Outcome::Derived(values) = &expected else {
            seen.refused += 1;
            return;
        };
        let non_empty = |name: &String| !values[name].is_empty();
        let recursion = recursive.iter().any(non_empty);
        seen.evaluated += 1;
        seen.recursion += usize::from(recursion);
        seen.negative += usize::from(values.values().any(|z| z.values().any(|&w| w < 0)));
        seen.distinct += usize::from(c.program.distinct.iter().any(non_empty));
        seen.mixed += usize::from(
            recursion
                && c.derived
                    .iter()
                    .any(|d| !recursive.contains(d) && non_empty(d)),
        );
    });
    seen
}

#[test]
fn random_programs_match_the_definition() {
    let seen = against_the_definition(0..CASES);
    // Guard against a generator that stopped producing what the suite is for.
    let cases = CASES as usize;
    assert!(
        seen.evaluated >= cases * 2 / 5,
        "{} evaluated",
        seen.evaluated
    );
    assert!(seen.refused >= cases / 20, "{} refused", seen.refused);
    assert!(
        seen.recursion >= cases / 10,
        "{} recursions",
        seen.recursion
    );
    assert!(seen.negative >= cases / 20, "{} negative", seen.negative);
    assert!(seen.distinct >= cases / 20, "{} distinct", seen.distinct);
    assert!(seen.mixed >= cases / 40, "{} mixed", seen.mixed);
}

/// The same check on 50 000 further cases.
/// Run with: cargo test -p coln-batch --release -- --include-ignored
#[test]
#[ignore = "large; run explicitly (use --release)"]
fn random_programs_match_the_definition_at_scale() {
    let seen = against_the_definition(CASES..CASES + 50_000);
    assert!(seen.recursion >= 5_000, "{} recursions", seen.recursion);
}

#[test]
fn the_order_of_the_rules_does_not_matter() {
    each_case(0..CASES, |case, rng, c| {
        let mut shuffled = c.program.clone();
        common::shuffle(rng, &mut shuffled.rules);
        assert_same(
            &evaluate(&c.program, &c.edb, &c.derived),
            &evaluate(&shuffled, &c.edb, &c.derived),
            &format!("case {case}"),
        );
    });
}

#[test]
fn a_rule_splits_into_two_whose_weights_add_up_to_its_own() {
    let mut checked = 0;
    each_case(0..CASES, |case, rng, c| {
        // Only outside a recursion, where rules may subtract.
        let recursive = common::recursive_relations(&c.program);
        let candidates: Vec<usize> = (0..c.program.rules.len())
            .filter(|&i| !recursive.contains(&c.program.rules[i].head.relation))
            .collect();
        if candidates.is_empty() {
            return;
        }
        let i = common::pick(rng, &candidates);
        let rule = c.program.rules[i].clone();
        let part: Weight = if rule.weight == -1 { 1 } else { -1 };
        let mut split = c.program.clone();
        split.rules[i].weight = rule.weight - part;
        split.rules.push(weighing(part, rule));
        assert_same(
            &evaluate(&c.program, &c.edb, &c.derived),
            &evaluate(&split, &c.edb, &c.derived),
            &format!("case {case}, rule {i}"),
        );
        checked += 1;
    });
    assert!(checked >= CASES as usize * 2 / 5, "{checked} rules split");
}

#[test]
fn declaring_a_relation_distinct_that_nothing_reads_reduces_it_to_a_set() {
    let mut checked = 0;
    each_case(0..CASES, |case, _, c| {
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
            let expected = before.clone().map(|mut values| {
                let set = common::support(&values[sink]);
                values.insert(sink.clone(), set);
                values
            });
            let after = evaluate(&declared, &c.edb, &c.derived);
            assert_same(&after, &expected, &format!("case {case}, {sink}"));
            checked += 1;
        }
    });
    assert!(
        checked >= CASES as usize / 4,
        "{checked} relations declared"
    );
}

#[test]
fn how_the_input_is_stored_does_not_matter() {
    each_case(0..CASES, |case, rng, c| {
        let mut other = Catalog::new();
        for name in c.edb.names() {
            let rel = c.edb.get(name).unwrap();
            let rows = (0..rel.len())
                .map(|i| {
                    (
                        rel.row_values(i, c.edb.dictionary()).unwrap(),
                        rel.weight(i),
                    )
                })
                .collect();
            let storage = common::pick(
                rng,
                &[Storage::Consolidated, Storage::Copies, Storage::Units],
            );
            common::store(rng, &mut other, name, &rel.schema, rows, storage);
        }
        let a = evaluate(&c.program, &c.edb, &c.derived);
        let b = evaluate(&c.program, &other, &c.derived);
        assert_same(&a, &b, &format!("case {case}"));
    });
}

#[test]
fn scaling_positive_input_leaves_every_recursion_as_it_is() {
    let mut checked = 0;
    for case in 0..CASES {
        let c = common::program_case(&mut SplitMix64::new(case), false);
        if c.program.rules.iter().any(|r| r.weight < 0) {
            continue;
        }
        let mut scaled = c.edb.clone();
        for name in c
            .edb
            .names()
            .into_iter()
            .filter(|n| !c.derived.iter().any(|d| d == n))
        {
            let mut rel = c.edb.get(name).unwrap().clone();
            for w in &mut rel.weights {
                *w *= 3;
            }
            scaled.insert(rel);
        }
        let before = evaluate(&c.program, &c.edb, &c.derived).unwrap();
        let after = evaluate(&c.program, &scaled, &c.derived).unwrap();
        let recursive = common::recursive_relations(&c.program);
        for relation in &recursive {
            assert_eq!(before[relation], after[relation], "case {case}, {relation}");
        }
        checked += usize::from(!recursive.is_empty());
    }
    assert!(
        checked >= CASES as usize / 10,
        "{checked} recursions checked"
    );
}

// ---------------------------------------------------------------------------
// Hand-picked cases

fn catalog(relations: impl IntoIterator<Item = Relation>) -> Catalog {
    let mut cat = Catalog::new();
    for rel in relations {
        cat.insert(rel);
    }
    cat
}

/// Every run must agree on `program`; returns `name`'s entries.
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
    entries(&results[0])
}

/// Every run must refuse `program`, with `phrase` in its message.
fn refused(program: &Program, edb: &Catalog, phrase: &str) {
    for (run, strategy, exec) in RUNS {
        let err = strategy(program, edb, exec)
            .err()
            .unwrap_or_else(|| panic!("{run} must refuse"))
            .to_string();
        assert!(err.contains(phrase), "{run}: {err}");
    }
}

fn edges(pairs: &[[u64; 2]]) -> Relation {
    let rows: Vec<(&[u64], Weight)> = pairs.iter().map(|pair| (&pair[..], 1)).collect();
    stored("parent", 2, &rows)
}

#[test]
fn a_weighted_relation_adds_up_its_rules_and_distinct_keeps_each_row_once() {
    // Two ways from 0 to 3, over 1 and over 2, and a second rule that
    // derives (0, 3) once more.
    let edb = catalog([
        edges(&[[0, 1], [0, 2], [1, 3], [2, 3]]),
        stored("hub", 2, &[(&[0, 3], 1)]),
    ]);
    let mut program = Program::new(vec![
        rule(
            ("two_hop", &[0, 2]),
            &[("parent", &[0, 1]), ("parent", &[1, 2])],
        ),
        rule(("two_hop", &[0, 1]), &[("hub", &[0, 1])]),
    ]);
    assert_eq!(derived(&program, &edb, "two_hop"), vec![(vec![0, 3], 3)]);
    program.distinct.insert("two_hop".into());
    assert_eq!(derived(&program, &edb, "two_hop"), vec![(vec![0, 3], 1)]);
}

#[test]
fn initial_facts_add_to_what_a_weighted_relation_derives() {
    // (0, 3) derived twice, and initial facts of weight -2 cancel it; a row
    // only among the initial facts stays.
    let edb = catalog([
        edges(&[[0, 1], [0, 2], [1, 3], [2, 3]]),
        stored("two_hop", 2, &[(&[0, 3], -2), (&[5, 5], 4)]),
    ]);
    let program = Program::new(vec![rule(
        ("two_hop", &[0, 2]),
        &[("parent", &[0, 1]), ("parent", &[1, 2])],
    )]);
    assert_eq!(derived(&program, &edb, "two_hop"), vec![(vec![5, 5], 4)]);
}

#[test]
fn a_derived_relation_may_hold_nothing_but_initial_facts() {
    let edb = catalog([
        stored("empty", 1, &[]),
        stored("seen", 1, &[(&[1], 2), (&[2], -1)]),
    ]);
    let program = Program::new(vec![rule(("seen", &[0]), &[("empty", &[0])])]);
    assert_eq!(
        derived(&program, &edb, "seen"),
        vec![(vec![1], 2), (vec![2], -1)]
    );
}

#[test]
fn a_rule_of_weight_minus_one_subtracts() {
    // diff = a - b: a row only in b comes out negative.
    let edb = catalog([
        stored("a", 2, &[(&[1, 1], 1), (&[2, 2], 1)]),
        stored("b", 2, &[(&[2, 2], 1), (&[3, 3], 1)]),
    ]);
    let program = Program::new(vec![
        rule(("diff", &[0, 1]), &[("a", &[0, 1])]),
        weighing(-1, rule(("diff", &[0, 1]), &[("b", &[0, 1])])),
    ]);
    assert_eq!(
        derived(&program, &edb, "diff"),
        vec![(vec![1, 1], 1), (vec![3, 3], -1)]
    );
}

#[test]
fn an_anti_join_subtracts_a_distinct_semi_join() {
    // leaf(x) ← node(x), minus node(x), has_child(x): the nodes without a
    // child. 0 has two children, so has_child must be distinct, or leaf
    // would take 0 away twice.
    let edb = catalog([
        edges(&[[0, 1], [0, 2], [1, 2], [2, 3]]),
        stored("node", 1, &[(&[0], 1), (&[1], 1), (&[2], 1), (&[3], 1)]),
    ]);
    let mut program = Program::new(vec![
        rule(("has_child", &[0]), &[("parent", &[0, 1])]),
        rule(("leaf", &[0]), &[("node", &[0])]),
        weighing(
            -1,
            rule(("leaf", &[0]), &[("node", &[0]), ("has_child", &[0])]),
        ),
    ]);
    assert_eq!(
        derived(&program, &edb, "leaf"),
        vec![(vec![0], -1), (vec![3], 1)]
    );
    program.distinct.insert("has_child".into());
    assert_eq!(derived(&program, &edb, "leaf"), vec![(vec![3], 1)]);
}

#[test]
fn weights_count_what_a_recursion_derived() {
    // ancestor is a set; projected onto the ancestor it counts the
    // descendants: 0 -> 1 -> 2 -> 3.
    let edb = fixtures::ancestor_chain_catalog(4);
    let mut program = fixtures::ancestor_program();
    program
        .rules
        .push(rule(("descendants", &[0]), &[("ancestor", &[0, 1])]));
    let ancestor = derived(&program, &edb, "ancestor");
    assert_eq!(ancestor.len(), 6);
    assert!(ancestor.iter().all(|(_, w)| *w == 1));
    assert_eq!(
        derived(&program, &edb, "descendants"),
        vec![(vec![0], 3), (vec![1], 2), (vec![2], 1)]
    );
}

#[test]
fn strata_run_in_dependency_order() {
    // reach is recursive over parent. cycle_node reads reach without
    // recursion and counts the nodes sharing a cycle with each node.
    // on_cycle is recursive again, over cycle_node: a set, whatever the
    // weights it reads. Edges: 0 -> 1 -> 2 -> 1 and 3 -> 3.
    let edb = catalog([edges(&[[0, 1], [1, 2], [2, 1], [3, 3]])]);
    let program = Program::new(vec![
        rule(("reach", &[0, 1]), &[("parent", &[0, 1])]),
        rule(
            ("reach", &[0, 2]),
            &[("reach", &[0, 1]), ("parent", &[1, 2])],
        ),
        rule(
            ("cycle_node", &[0]),
            &[("reach", &[0, 1]), ("reach", &[1, 0])],
        ),
        rule(
            ("on_cycle", &[0, 1]),
            &[("cycle_node", &[0]), ("parent", &[0, 1])],
        ),
        rule(
            ("on_cycle", &[0, 2]),
            &[("on_cycle", &[0, 1]), ("parent", &[1, 2])],
        ),
    ]);
    let set = |pairs: &[[u64; 2]]| pairs.iter().map(|p| (p.to_vec(), 1)).collect::<Vec<_>>();
    assert_eq!(
        derived(&program, &edb, "reach"),
        set(&[[0, 1], [0, 2], [1, 1], [1, 2], [2, 1], [2, 2], [3, 3]])
    );
    assert_eq!(
        derived(&program, &edb, "cycle_node"),
        vec![(vec![1], 2), (vec![2], 2), (vec![3], 1)]
    );
    assert_eq!(
        derived(&program, &edb, "on_cycle"),
        set(&[[1, 1], [1, 2], [2, 1], [2, 2], [3, 3]])
    );
}

#[test]
fn weights_travel_through_a_chain_of_strata() {
    // r0(x) ← e(x), and r(i+1)(x) ← r(i)(x) with weight 2, ten times: the
    // row of weight 3 arrives with weight 3 · 2¹⁰, one round per stratum.
    let edb = catalog([stored("e", 1, &[(&[4], 3)])]);
    let mut rules = vec![rule(("r0", &[0]), &[("e", &[0])])];
    for i in 0..10 {
        let (from, to) = (format!("r{i}"), format!("r{}", i + 1));
        rules.push(weighing(2, rule((&to, &[0]), &[(&from, &[0])])));
    }
    let program = Program::new(rules);
    assert_eq!(derived(&program, &edb, "r10"), vec![(vec![4], 3 * 1024)]);
    let stats = fixpoint::semi_naive(&program, &edb, generic_join::execute)
        .unwrap()
        .stats;
    assert_eq!((stats.rounds, stats.new_facts_per_round), (11, vec![1; 11]));
}

#[test]
fn statistics_count_every_stratum() {
    // copy (3 rows, one round) comes first, then ancestor over copy: one
    // round per path length plus the final empty one.
    let edb = catalog([edges(&[[0, 1], [1, 2], [2, 3]])]);
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
        assert_eq!((stats.rounds, stats.total_new_facts()), (5, 9), "{run}");
    }
}

#[test]
fn a_recursion_is_a_set_whatever_its_input_weighs() {
    let all_once = |rows: Vec<(Vec<u64>, Weight)>| rows.iter().all(|(_, w)| *w == 1);
    // Every edge of the cycle is present more than once.
    let edb = catalog([stored(
        "parent",
        2,
        &[(&[0, 1], 2), (&[1, 2], 2), (&[2, 0], 3)],
    )]);
    let ancestor = derived(&fixtures::ancestor_program(), &edb, "ancestor");
    assert!(ancestor.len() == 9 && all_once(ancestor));
    // A rule that counts twice.
    let mut doubled = fixtures::ancestor_program();
    doubled.rules[1].weight = 2;
    let ancestor = derived(&doubled, &edb, "ancestor");
    assert!(ancestor.len() == 9 && all_once(ancestor));
    // Initial facts of weight 2 and 3 count once.
    let edb = catalog([
        edges(&[[0, 1]]),
        stored("ancestor", 2, &[(&[7, 8], 2), (&[0, 1], 3)]),
    ]);
    assert_eq!(
        derived(&fixtures::ancestor_program(), &edb, "ancestor"),
        vec![(vec![0, 1], 1), (vec![7, 8], 1)]
    );
}

#[test]
fn a_recursion_may_read_a_distinct_relation() {
    // edge counts both directions of every parent pair; declared distinct,
    // it is a set, and reach closes over it: 0, 1 and 2 reach each other.
    let edb = catalog([edges(&[[0, 1], [1, 0], [1, 2]])]);
    let mut program = Program::new(vec![
        rule(("edge", &[0, 1]), &[("parent", &[0, 1])]),
        rule(("edge", &[1, 0]), &[("parent", &[0, 1])]),
        rule(("reach", &[0, 1]), &[("edge", &[0, 1])]),
        rule(("reach", &[0, 2]), &[("reach", &[0, 1]), ("edge", &[1, 2])]),
    ]);
    assert!(derived(&program, &edb, "edge").contains(&(vec![0, 1], 2)));
    program.distinct.insert("edge".into());
    assert!(derived(&program, &edb, "edge").iter().all(|(_, w)| *w == 1));
    assert_eq!(derived(&program, &edb, "reach").len(), 9);
}

#[test]
fn a_recursion_cannot_subtract() {
    let mut program = fixtures::ancestor_program();
    program.rules[1].weight = -1;
    refused(
        &program,
        &fixtures::ancestor_chain_catalog(3),
        "ancestor is recursive",
    );

    // even(y) ← succ(x, y), odd(x) subtracts; odd reads even back, so the
    // subtraction sits inside a recursion of two relations.
    let edb = catalog([
        stored("succ", 2, &[(&[0, 1], 1), (&[1, 2], 1)]),
        stored("even", 1, &[(&[0], 1)]),
    ]);
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
fn a_recursion_refuses_input_of_negative_net_weight() {
    let ancestor = fixtures::ancestor_program();
    let edb = catalog([stored("parent", 2, &[(&[0, 1], 1), (&[1, 2], -1)])]);
    refused(&ancestor, &edb, "parent holds a row of weight -1");
    // Initial facts are input too.
    let edb = catalog([edges(&[[0, 1]]), stored("ancestor", 2, &[(&[7, 8], -2)])]);
    refused(&ancestor, &edb, "ancestor holds a row of weight -2");
    // So is a negative row that a subtraction feeds into the recursion.
    let edb = catalog([edges(&[[0, 1]]), stored("banned", 2, &[(&[5, 6], 1)])]);
    let program = Program::new(vec![
        rule(("allowed", &[0, 1]), &[("parent", &[0, 1])]),
        weighing(-1, rule(("allowed", &[0, 1]), &[("banned", &[0, 1])])),
        rule(("reach", &[0, 1]), &[("allowed", &[0, 1])]),
        rule(
            ("reach", &[0, 2]),
            &[("reach", &[0, 1]), ("allowed", &[1, 2])],
        ),
    ]);
    refused(&program, &edb, "allowed holds a row of weight -1");

    // What counts is the net weight of a row's copies: (0, 1) is stored
    // with 2 and -1, an edge; (1, 2) with 1 and -1, no edge at all.
    let copies = [
        (&[0, 1][..], 2),
        (&[1, 2], 1),
        (&[0, 1], -1),
        (&[1, 2], -1),
        (&[2, 3], 1),
    ];
    let edb = catalog([stored("parent", 2, &copies)]);
    assert_eq!(
        derived(&ancestor, &edb, "ancestor"),
        vec![(vec![0, 1], 1), (vec![2, 3], 1)]
    );
    let edb = catalog([stored("parent", 2, &[(&[0, 1], 1), (&[0, 1], -2)])]);
    refused(&ancestor, &edb, "parent holds a row of weight -1");
}
