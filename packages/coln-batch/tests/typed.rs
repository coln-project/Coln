// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Type transport: the same queries and programs, run over every scalar
//! type the engine knows.
//!
//! A `u64` instance (relations, literals, expected results) is mapped
//! cell by cell through an injective function into each type. The typed
//! run must then produce exactly the image of the `u64` result under the
//! same function, on every executor and every fixpoint strategy. Beyond
//! that, one query mixes three types across its columns, and the edge
//! values of every type (extremes, empty and non-ASCII strings) go
//! through a join instead of only through a round trip.

use coln_batch::fixpoint::{self, Exec, FixpointResult};
use coln_batch::query::{Atom, Catalog, Query, Term};
use coln_batch::relation::Relation;
use coln_batch::rule::Program;
use coln_batch::types::{Column, ScalarType, Schema, Value};
use coln_batch::{binary_join, fixtures, generate, generic_join, reference};

/// A fixpoint strategy, `semi_naive` or `naive`.
type Strategy = fn(&Program, &Catalog, Exec) -> anyhow::Result<FixpointResult>;

const TYPES: [ScalarType; 5] = [
    ScalarType::Uint,
    ScalarType::Iint,
    ScalarType::Bool,
    ScalarType::Char,
    ScalarType::String,
];

/// An injective map from the small integers the base instances use into
/// `ty`. Deliberately not the identity for any type, so decoding is
/// exercised too: unsigned values count down from `u64::MAX`, signed
/// ones start negative, characters are non-ASCII, strings carry a
/// non-ASCII letter. Booleans have two values, so their instances draw
/// from `0..2` only.
fn lift(ty: ScalarType, x: u64) -> Value {
    match ty {
        ScalarType::Uint => Value::Uint(u64::MAX - x),
        ScalarType::Iint => Value::Iint(x as i64 - 5),
        ScalarType::Bool => {
            assert!(x < 2, "boolean instances use the domain 0..2");
            Value::Bool(x == 1)
        }
        ScalarType::Char => Value::Char(char::from_u32(0x1F600 + x as u32).expect("emoji range")),
        ScalarType::String => Value::String(format!("wert {x} ß")),
    }
}

/// The domain a base instance may use for `ty`.
fn domain(ty: ScalarType) -> u64 {
    if ty == ScalarType::Bool { 2 } else { 6 }
}

fn lift_row(row: &[u64], types: &[ScalarType]) -> Vec<Value> {
    row.iter().zip(types).map(|(&x, &ty)| lift(ty, x)).collect()
}

/// Every relation of a `u64` catalog, lifted column by column into the
/// types `types_of` assigns to it.
fn lift_catalog(base: &Catalog, types_of: impl Fn(&str, usize) -> ScalarType) -> Catalog {
    let mut lifted = Catalog::new();
    for name in base.names() {
        let rel = base.get(name).unwrap();
        let types: Vec<ScalarType> = (0..rel.arity()).map(|c| types_of(name, c)).collect();
        let schema = Schema::new(
            rel.schema
                .columns()
                .iter()
                .zip(&types)
                .map(|(column, &ty)| Column::new(column.name.clone(), ty)),
        );
        let rows: Vec<Vec<Value>> = (0..rel.len())
            .map(|i| lift_row(&rel.row(i), &types))
            .collect();
        lifted.insert_rows(name, schema, rows).unwrap();
    }
    lifted
}

/// A query's unsigned literals lifted into `ty`.
fn lift_query(query: &Query, ty: ScalarType) -> Query {
    let mut lifted = query.clone();
    for atom in &mut lifted.atoms {
        for term in &mut atom.terms {
            if let Term::Lit(Value::Uint(x)) = term {
                *term = Term::Lit(lift(ty, *x));
            }
        }
    }
    lifted
}

/// The rows of a `u64` relation lifted into `types`, sorted by value.
fn lifted_rows(rel: &Relation, types: &[ScalarType]) -> Vec<Vec<Value>> {
    let mut rows: Vec<Vec<Value>> = (0..rel.len())
        .map(|i| lift_row(&rel.row(i), types))
        .collect();
    rows.sort();
    rows
}

/// A typed relation decoded, sorted by value.
fn decoded(rel: &Relation, catalog: &Catalog) -> Vec<Vec<Value>> {
    let mut rows: Vec<Vec<Value>> = (0..rel.len())
        .map(|i| rel.row_values(i, catalog.dictionary()).unwrap())
        .collect();
    rows.sort();
    rows
}

fn atom(relation: &str, terms: Vec<Term>) -> Atom {
    Atom {
        relation: relation.into(),
        terms,
    }
}

/// Hand-built base instances with literals, over the domain `0..2` so
/// they work for booleans as well.
fn literal_cases() -> Vec<(&'static str, Query, Catalog)> {
    let mut cat = Catalog::new();
    cat.insert(Relation::new(
        "R",
        ["a", "b"],
        vec![vec![0, 1, 1], vec![1, 1, 0]],
    ));
    cat.insert(Relation::new("U", ["a"], vec![vec![1]]));
    let (x, y) = (0, 1);
    vec![
        (
            "literal filter",
            Query {
                var_names: vec!["y".into()],
                atoms: vec![atom("R", vec![Term::lit(1u64), Term::Var(0)])],
                head: vec![0],
            },
            cat.clone(),
        ),
        (
            "self join with a filter",
            Query {
                var_names: vec!["x".into(), "y".into()],
                atoms: vec![
                    atom("R", vec![Term::Var(x), Term::Var(y)]),
                    atom("R", vec![Term::Var(y), Term::lit(0u64)]),
                    atom("U", vec![Term::Var(y)]),
                ],
                head: vec![x, y],
            },
            cat.clone(),
        ),
        (
            "repeated variable",
            Query {
                var_names: vec!["x".into()],
                atoms: vec![atom("R", vec![Term::Var(0), Term::Var(0)])],
                head: vec![0],
            },
            cat,
        ),
    ]
}

/// The generated fixtures, sized to the domain of `ty`.
fn fixture_cases(ty: ScalarType) -> Vec<(&'static str, Query, Catalog)> {
    let n = domain(ty);
    vec![
        (
            "triangle",
            fixtures::triangle_query(),
            fixtures::triangle_catalog(n, 30, 6, 1),
        ),
        (
            "f(a, g(a))",
            fixtures::fg_query(),
            fixtures::fg_catalog(n, 30, 6, 2),
        ),
    ]
}

#[test]
fn same_query_every_type() {
    for ty in TYPES {
        let mut cases = literal_cases();
        cases.extend(fixture_cases(ty));
        for (name, query, base) in cases {
            let expected_u64 = reference::execute(&query, &base).unwrap();
            let types = vec![ty; expected_u64.arity()];
            let expected = lifted_rows(&expected_u64, &types);

            let lifted = lift_catalog(&base, |_, _| ty);
            let lifted_query = lift_query(&query, ty);
            for (executor, exec) in [
                ("oracle", reference::execute as Exec),
                ("binary join", binary_join::execute as Exec),
                ("generic join", generic_join::execute as Exec),
            ] {
                let result = exec(&lifted_query, &lifted)
                    .unwrap_or_else(|e| panic!("{name} over {ty}: {executor} failed: {e}"));
                assert_eq!(
                    result.schema.types(),
                    types,
                    "{name} over {ty}: {executor} result schema"
                );
                assert_eq!(
                    decoded(&result, &lifted),
                    expected,
                    "{name} over {ty}: {executor} result"
                );
            }
        }
    }
}

#[test]
fn same_program_every_type() {
    for ty in TYPES {
        let n = domain(ty);
        let program = fixtures::ancestor_program();
        // A chain plus one initial fact that is not derivable from it.
        let mut base = fixtures::ancestor_chain_catalog(n);
        base.insert(Relation::new(
            "ancestor",
            ["x", "y"],
            vec![vec![n - 1], vec![0]],
        ));
        let expected_u64 = fixpoint::semi_naive(&program, &base, generic_join::execute as Exec)
            .unwrap()
            .catalog;
        let expected = lifted_rows(expected_u64.get("ancestor").unwrap(), &[ty, ty]);
        assert!(expected.len() > 1, "the instance must derive something");

        let lifted = lift_catalog(&base, |_, _| ty);
        for (name, strategy) in [
            ("semi-naive", fixpoint::semi_naive as Strategy),
            ("naive", fixpoint::naive as Strategy),
        ] {
            for (executor, exec) in [
                ("binary join", binary_join::execute as Exec),
                ("generic join", generic_join::execute as Exec),
            ] {
                let result = strategy(&program, &lifted, exec)
                    .unwrap_or_else(|e| panic!("{name} + {executor} over {ty}: {e}"));
                let ancestor = result.catalog.get("ancestor").unwrap();
                assert_eq!(ancestor.schema.types(), vec![ty, ty]);
                assert_eq!(
                    decoded(ancestor, &result.catalog),
                    expected,
                    "{name} + {executor} over {ty}"
                );
            }
        }
    }
}

#[test]
fn mixed_types_in_one_query() {
    // The triangle with x unsigned, y a string and z signed: every atom
    // mixes two types, and every variable keeps one.
    let types_of = |relation: &str, column: usize| match (relation, column) {
        ("R_f", 0) | ("R_h", 1) => ScalarType::Uint,
        ("R_f", 1) | ("R_g", 0) => ScalarType::String,
        ("R_g", 1) | ("R_h", 0) => ScalarType::Iint,
        _ => unreachable!(),
    };
    let query = fixtures::triangle_query();
    let base = fixtures::triangle_catalog(6, 30, 6, 9);
    let expected_u64 = reference::execute(&query, &base).unwrap();
    let head_types = [ScalarType::Uint, ScalarType::String, ScalarType::Iint];
    let expected = lifted_rows(&expected_u64, &head_types);
    assert!(!expected.is_empty());

    let lifted = lift_catalog(&base, types_of);
    for (executor, exec) in [
        ("oracle", reference::execute as Exec),
        ("binary join", binary_join::execute as Exec),
        ("generic join", generic_join::execute as Exec),
    ] {
        let result = exec(&query, &lifted).unwrap();
        assert_eq!(result.schema.types(), head_types, "{executor}");
        assert_eq!(decoded(&result, &lifted), expected, "{executor}");
    }
}

/// The values at the edges of every type's range.
fn extremes(ty: ScalarType) -> Vec<Value> {
    match ty {
        ScalarType::Uint => [0, 1, u64::MAX - 1, u64::MAX]
            .into_iter()
            .map(Value::Uint)
            .collect(),
        ScalarType::Iint => [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX]
            .into_iter()
            .map(Value::Iint)
            .collect(),
        ScalarType::Bool => vec![Value::Bool(false), Value::Bool(true)],
        ScalarType::Char => [
            '\0',
            'a',
            'ß',
            '\u{D7FF}',
            '\u{E000}',
            '\u{1F600}',
            char::MAX,
        ]
        .into_iter()
        .map(Value::Char)
        .collect(),
        ScalarType::String => ["", " ", "a", "ab", "b", "ß", "日本語", "\u{1F600}", "a\0b"]
            .into_iter()
            .map(Value::from)
            .chain(std::iter::once(Value::String("x".repeat(10_000))))
            .collect(),
    }
}

#[test]
fn edge_values_of_every_type_join() {
    for ty in TYPES {
        let all = extremes(ty);
        // U holds every other value; Q(x) ← T(x), U(x) must return exactly U.
        let subset: Vec<Value> = all.iter().step_by(2).cloned().collect();
        let mut cat = Catalog::new();
        let schema = || Schema::new([Column::new("x", ty)]);
        cat.insert_rows("T", schema(), all.iter().map(|v| vec![v.clone()]))
            .unwrap();
        cat.insert_rows("U", schema(), subset.iter().map(|v| vec![v.clone()]))
            .unwrap();
        let query = Query {
            var_names: vec!["x".into()],
            atoms: vec![atom("T", vec![Term::Var(0)]), atom("U", vec![Term::Var(0)])],
            head: vec![0],
        };
        let mut expected: Vec<Vec<Value>> = subset.iter().map(|v| vec![v.clone()]).collect();
        expected.sort();
        for (executor, exec) in [
            ("oracle", reference::execute as Exec),
            ("binary join", binary_join::execute as Exec),
            ("generic join", generic_join::execute as Exec),
        ] {
            let result = exec(&query, &cat).unwrap();
            assert_eq!(decoded(&result, &cat), expected, "{ty}: {executor}");
        }

        // For the ordered types, the engine's key order is the value order:
        // T sorted by key decodes in ascending value order.
        if ty != ScalarType::String {
            let t = cat.get("T").unwrap();
            let sorted_by_key = t.clone().sorted_dedup();
            let in_key_order: Vec<Value> = (0..sorted_by_key.len())
                .map(|i| sorted_by_key.value(i, 0, cat.dictionary()).unwrap())
                .collect();
            let mut in_value_order = all.clone();
            in_value_order.sort();
            assert_eq!(
                in_key_order, in_value_order,
                "{ty}: key order is value order"
            );
        }
    }
}

#[test]
fn extremes_survive_arrow_and_recursion() {
    // Every extreme value as an edge endpoint in a labeled graph, run
    // through the typed reachability program; the labels are extreme
    // strings, the weights extreme signed integers.
    let labels = extremes(ScalarType::String);
    let weights = extremes(ScalarType::Iint);
    let mut edb = Catalog::new();
    let rows: Vec<Vec<Value>> = (0..labels.len())
        .map(|i| {
            vec![
                Value::Uint(i as u64),
                Value::Uint(i as u64 + 1),
                labels[i].clone(),
                weights[i % weights.len()].clone(),
            ]
        })
        .collect();
    edb.insert_rows("edge", generate::labeled_edges_schema(), rows.clone())
        .unwrap();

    // Arrow round trip into a fresh dictionary: the same values come back.
    let edge = edb.get("edge").unwrap();
    let batch = edge.to_record_batch(edb.dictionary()).unwrap();
    let mut fresh = coln_batch::types::Dictionary::new();
    let back = Relation::from_record_batch("edge", &batch, &mut fresh).unwrap();
    let mut back_rows: Vec<Vec<Value>> = (0..back.len())
        .map(|i| back.row_values(i, &fresh).unwrap())
        .collect();
    back_rows.sort();
    let mut fed_rows = rows.clone();
    fed_rows.sort();
    assert_eq!(back_rows, fed_rows);

    let result = fixpoint::semi_naive(
        &fixtures::labeled_reach_program(),
        &edb,
        generic_join::execute as Exec,
    )
    .unwrap();
    // Every label is unique, so each edge reaches exactly its own target.
    let reach = result.catalog.get("reach").unwrap();
    let mut got = decoded(reach, &result.catalog);
    let mut expected: Vec<Vec<Value>> = rows
        .iter()
        .map(|row| vec![row[0].clone(), row[1].clone(), row[2].clone()])
        .collect();
    got.sort();
    expected.sort();
    assert_eq!(got, expected);
}
