// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Z-set operations on relations, on random relations of arity 0 to 3
//! with weights of both signs, held against a plain map from rows to
//! weights and against the laws of Z-set arithmetic. Also: sorted tables
//! and Arrow batches and files keep every row with its weight, and weights
//! that leave `i64` panic instead of wrapping around.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::{Int64Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow::ipc::writer::FileWriter;
use arrow::record_batch::RecordBatch;
use coln_batch::io::{load_relation, save_relation};
use coln_batch::query::Catalog;
use coln_batch::relation::{Relation, WEIGHT_COLUMN};
use coln_batch::rng::SplitMix64;
use coln_batch::table::{ArrowSortedTable, SortedTable, check_contract};
use coln_batch::types::{Dictionary, Key, Schema, Weight};

use common::Storage;

const CASES: u64 = 500;

/// A relation of uint columns as stored: up to 10 rows from a small domain
/// (plus `u64::MAX`), in any order, rows repeated, weights from -3 to 3.
fn raw(rng: &mut SplitMix64, arity: usize) -> Relation {
    let rows = rng.below(11) as usize;
    let key = |rng: &mut SplitMix64| {
        if rng.below(10) == 0 {
            u64::MAX
        } else {
            rng.below(3)
        }
    };
    let cols = (0..arity)
        .map(|_| (0..rows).map(|_| key(rng)).collect())
        .collect();
    let weights = (0..rows).map(|_| rng.below(7) as Weight - 3).collect();
    let schema = Schema::uint((0..arity).map(|c| format!("c{c}")));
    Relation::with_weights("r", schema, cols, weights)
}

/// The model of a relation: every row with the sum of its weights, rows of
/// sum 0 left out.
fn model(rel: &Relation) -> BTreeMap<Vec<Key>, i128> {
    let mut m = BTreeMap::new();
    for i in 0..rel.len() {
        *m.entry(rel.row(i)).or_insert(0) += i128::from(rel.weight(i));
    }
    m.retain(|_, w| *w != 0);
    m
}

fn entries(rel: &Relation) -> BTreeMap<Vec<Key>, i128> {
    assert!(rel.is_consolidated(), "not in normal form: {rel:?}");
    (0..rel.len())
        .map(|i| (rel.row(i), i128::from(rel.weight(i))))
        .collect()
}

fn model_plus(
    a: &BTreeMap<Vec<Key>, i128>,
    b: &BTreeMap<Vec<Key>, i128>,
    sign: i128,
) -> BTreeMap<Vec<Key>, i128> {
    let mut m = a.clone();
    for (row, w) in b {
        *m.entry(row.clone()).or_insert(0) += sign * w;
    }
    m.retain(|_, w| *w != 0);
    m
}

/// Case `case`: three random relations of one random arity.
fn three(case: u64) -> (SplitMix64, [Relation; 3]) {
    let mut rng = SplitMix64::new(case);
    let arity = rng.below(4) as usize;
    let rels = [0, 1, 2].map(|_| raw(&mut rng, arity));
    (rng, rels)
}

#[test]
fn consolidate_matches_the_model() {
    for case in 0..CASES {
        let (mut rng, [r, _, _]) = three(case);
        let normal = r.clone().consolidate();
        assert_eq!(entries(&normal), model(&r), "case {case}");
        assert_eq!(
            normal.clone().consolidate(),
            normal,
            "case {case}: not stable"
        );
        assert_eq!(normal.name, r.name);
        assert_eq!(normal.schema, r.schema);

        // Neither the order of the rows nor how a weight is split over
        // copies changes the normal form.
        let mut rows: Vec<(Vec<Key>, Weight)> =
            (0..r.len()).map(|i| (r.row(i), r.weight(i))).collect();
        let mut more = Vec::new();
        for (row, w) in &mut rows {
            if rng.below(2) == 0 {
                more.push((row.clone(), 5));
                *w -= 5;
            }
        }
        rows.extend(more);
        common::shuffle(&mut rng, &mut rows);
        let cols = (0..r.arity())
            .map(|c| rows.iter().map(|(row, _)| row[c]).collect())
            .collect();
        let weights = rows.iter().map(|(_, w)| *w).collect();
        let other = Relation::with_weights("r", r.schema.clone(), cols, weights);
        assert_eq!(other.consolidate(), normal, "case {case}");
    }
}

#[test]
fn plus_and_minus_obey_the_laws_of_z_sets() {
    for case in 0..CASES {
        let (_, [a, b, c]) = three(case);
        let [a, b, c] = [a, b, c].map(Relation::consolidate);
        let empty = Relation::empty("r", a.schema.clone());

        assert_eq!(
            entries(&a.plus(&b)),
            model_plus(&model(&a), &model(&b), 1),
            "case {case}"
        );
        assert_eq!(
            entries(&a.minus(&b)),
            model_plus(&model(&a), &model(&b), -1),
            "case {case}"
        );
        assert_eq!(a.plus(&b), b.plus(&a), "case {case}: plus commutes");
        assert_eq!(
            a.plus(&b).plus(&c),
            a.plus(&b.plus(&c)),
            "case {case}: plus associates"
        );
        assert_eq!(a.plus(&empty), a, "case {case}: 0 is neutral");
        assert_eq!(empty.plus(&a), a, "case {case}: 0 is neutral");
        assert!(a.plus(&a.negate()).is_empty(), "case {case}: a + (-a) = 0");
        assert!(a.minus(&a).is_empty(), "case {case}: a - a = 0");
        assert_eq!(a.minus(&b), a.plus(&b.negate()), "case {case}");
        assert_eq!(a.plus(&b).minus(&b), a, "case {case}: (a + b) - b = a");
        assert_eq!(
            a.negate().negate(),
            a,
            "case {case}: negation is an involution"
        );
        assert_eq!(
            a.plus(&b).negate(),
            a.negate().plus(&b.negate()),
            "case {case}: negation distributes"
        );
        for r in [a.plus(&b), a.minus(&b), a.negate()] {
            assert!(r.is_consolidated(), "case {case}: {r:?}");
        }
    }
}

#[test]
fn distinct_keeps_the_rows_of_positive_weight_once() {
    for case in 0..CASES {
        let (_, [a, b, _]) = three(case);
        let d = a.clone().distinct();
        let expected: BTreeMap<Vec<Key>, i128> = model(&a)
            .into_iter()
            .filter(|(_, w)| *w > 0)
            .map(|(row, _)| (row, 1))
            .collect();
        assert_eq!(entries(&d), expected, "case {case}");
        assert_eq!(
            d.clone().distinct(),
            d,
            "case {case}: distinct is idempotent"
        );
        assert!(a.clone().negate().distinct().plus(&d).is_consolidated());

        // Over two sets, distinct of the sum is the union.
        let (x, y) = (a.distinct(), b.distinct());
        let union: BTreeMap<Vec<Key>, i128> = model(&x)
            .into_keys()
            .chain(model(&y).into_keys())
            .map(|row| (row, 1))
            .collect();
        assert_eq!(entries(&x.plus(&y).distinct()), union, "case {case}");
    }
}

#[test]
fn sorted_tables_keep_every_row_with_its_weight() {
    for case in 0..CASES {
        let (mut rng, [r, _, _]) = three(case);
        let mut order: Vec<usize> = (0..r.arity()).collect();
        common::shuffle(&mut rng, &mut order);

        // As stored, rows repeated and weights 0 included.
        let table = ArrowSortedTable::from_relation(&r, order.clone()).unwrap();
        assert_eq!(table.len(), r.len());
        let back = Relation::from_table("r", r.schema.clone(), &table).unwrap();
        assert_eq!(back.consolidate(), r.clone().consolidate(), "case {case}");

        // In normal form, a table meets the whole contract.
        let normal = r.consolidate();
        let table = ArrowSortedTable::from_relation(&normal, order).unwrap();
        check_contract(&table);
        let back = Relation::from_table("r", normal.schema.clone(), &table).unwrap();
        assert_eq!(back.consolidate(), normal, "case {case}");
    }
}

#[test]
fn arrow_batches_and_files_keep_every_row_with_its_weight() {
    let dir = std::env::temp_dir().join(format!("coln-batch-zset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for case in 0..CASES / 5 {
        let mut rng = SplitMix64::new(case);
        let mut cat = Catalog::new();
        let arity = rng.below(4) as usize;
        let schema = common::schema(&mut rng, arity, false);
        let rows = common::rows(&mut rng, &schema, 8, true);
        let storage = common::pick(&mut rng, &[Storage::Consolidated, Storage::Copies]);
        common::store(&mut rng, &mut cat, "t", &schema, rows, storage);
        let rel = cat.get("t").unwrap();
        let expected = common::stored_rows(rel, cat.dictionary());

        let batch = rel.to_record_batch(cat.dictionary()).unwrap();
        assert_eq!(batch.num_columns(), arity + 1);
        assert_eq!(batch.schema().field(arity).name(), WEIGHT_COLUMN);
        let mut fresh = Dictionary::new();
        let back = Relation::from_record_batch("t", &batch, &mut fresh).unwrap();
        assert_eq!(common::stored_rows(&back, &fresh), expected, "case {case}");
        assert_eq!(back.schema, rel.schema, "case {case}");

        let path = dir.join(format!("t{case}.arrow"));
        save_relation(rel, cat.dictionary(), &path).unwrap();
        let mut fresh = Dictionary::new();
        let back = load_relation("t", &path, &mut fresh).unwrap();
        assert_eq!(common::stored_rows(&back, &fresh), expected, "case {case}");
    }
}

#[test]
fn a_file_of_several_batches_loads_as_one_relation() {
    // Two batches, the second with negative weights; loading concatenates
    // rows and weights in file order.
    let schema = Arc::new(ArrowSchema::new(vec![
        Field::new("x", DataType::UInt64, false),
        Field::new(WEIGHT_COLUMN, DataType::Int64, false),
    ]));
    let batch = |xs: Vec<u64>, ws: Vec<i64>| {
        RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(UInt64Array::from(xs)),
                Arc::new(Int64Array::from(ws)),
            ],
        )
        .unwrap()
    };
    let dir = std::env::temp_dir().join(format!("coln-batch-zset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two_batches.arrow");
    let mut writer = FileWriter::try_new(std::fs::File::create(&path).unwrap(), &schema).unwrap();
    writer.write(&batch(vec![1, 2], vec![3, 1])).unwrap();
    writer.write(&batch(vec![2, 9], vec![-1, -4])).unwrap();
    writer.finish().unwrap();

    let rel = load_relation("x", &path, &mut Dictionary::new()).unwrap();
    assert_eq!(rel.cols, vec![vec![1, 2, 2, 9]]);
    assert_eq!(rel.weights, vec![3, 1, -1, -4]);
    let normal = rel.consolidate();
    assert_eq!(normal.weights, vec![3, -4], "(2) cancels out");
}

#[test]
fn a_batch_with_null_weights_is_rejected() {
    let schema = Arc::new(ArrowSchema::new(vec![
        Field::new("x", DataType::UInt64, false),
        Field::new(WEIGHT_COLUMN, DataType::Int64, true),
    ]));
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(UInt64Array::from(vec![1, 2])),
            Arc::new(Int64Array::from(vec![Some(1), None])),
        ],
    )
    .unwrap();
    let err = Relation::from_record_batch("x", &batch, &mut Dictionary::new()).unwrap_err();
    assert!(
        err.to_string().contains("weights must not be null"),
        "{err}"
    );
}

#[test]
#[should_panic(expected = "weight overflow")]
fn adding_beyond_the_largest_weight_panics_in_consolidate() {
    let schema = Schema::uint(["x"]);
    let rel = Relation::with_weights("r", schema, vec![vec![1, 1]], vec![Weight::MAX, 1]);
    let _ = rel.consolidate();
}

#[test]
#[should_panic(expected = "weight overflow")]
fn adding_beyond_the_largest_weight_panics_in_plus() {
    let schema = Schema::uint(["x"]);
    let rel = Relation::with_weights("r", schema, vec![vec![1]], vec![Weight::MAX]);
    let _ = rel.plus(&rel);
}

#[test]
#[should_panic(expected = "weight overflow")]
fn negating_the_smallest_weight_panics() {
    let schema = Schema::uint(["x"]);
    let rel = Relation::with_weights("r", schema, vec![vec![1]], vec![Weight::MIN]);
    let _ = rel.negate();
}

#[test]
#[should_panic(expected = "weight overflow")]
fn subtracting_the_smallest_weight_panics() {
    let schema = Schema::uint(["x"]);
    let zero = Relation::empty("r", schema.clone());
    let rel = Relation::with_weights("r", schema, vec![vec![1]], vec![Weight::MIN]);
    let _ = zero.minus(&rel);
}

#[test]
#[should_panic(expected = "one key per weight")]
fn a_weight_per_row_is_required() {
    let _ = Relation::with_weights("r", Schema::uint(["x"]), vec![vec![1, 2]], vec![1]);
}

#[test]
fn the_extremes_of_the_weight_range_survive_where_they_fit() {
    let schema = Schema::uint(["x"]);
    let rel = Relation::with_weights(
        "r",
        schema,
        vec![vec![1, 2, 2]],
        vec![Weight::MAX, Weight::MIN, Weight::MAX],
    );
    let normal = rel.consolidate();
    assert_eq!(normal.weights, vec![Weight::MAX, -1]);
    assert_eq!(normal.negate().weights, vec![-Weight::MAX, 1]);
}
