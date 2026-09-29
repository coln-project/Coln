// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The engine over storage it does not own: tables served through the
//! `Tables` trait by code outside the crate, the way a store serves them.
//!
//! One store implements only the four required `SortedTable` methods, so
//! every row weighs 1 and every search is the trait's default: a store of
//! sets that knows nothing of weights. The other keeps every row in
//! several copies whose weights add up to the row's, and searches with a
//! galloping search of its own. Queries and programs over either must give
//! what they give over an in-memory catalog.

mod common;

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use coln_batch::fixpoint;
use coln_batch::query::{Catalog, Tables};
use coln_batch::rng::SplitMix64;
use coln_batch::table::{ColId, RowIdx, SortedTable, check_contract};
use coln_batch::types::{Dictionary, Key, Schema, Weight};

use common::EXECUTORS;

const CASES: u64 = 300;

type Rows = Vec<(Vec<Key>, Weight)>;

/// `rows` sorted by the columns of `order`, then by all columns, so that
/// copies of a row end up next to each other.
fn sorted_by(mut rows: Rows, order: &[ColId]) -> Rows {
    rows.sort_by(|(a, _), (b, _)| {
        let key = |row: &Vec<Key>| order.iter().map(|&c| row[c]).collect::<Vec<_>>();
        key(a).cmp(&key(b)).then_with(|| a.cmp(b))
    });
    rows
}

/// A table that implements only what `SortedTable` requires.
struct MinimalTable {
    arity: usize,
    order: Vec<ColId>,
    rows: Vec<Vec<Key>>,
}

impl SortedTable for MinimalTable {
    fn arity(&self) -> usize {
        self.arity
    }

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn sort_order(&self) -> &[ColId] {
        &self.order
    }

    fn value(&self, row: RowIdx, col: ColId) -> Key {
        self.rows[row][col]
    }
}

/// A table that holds rows in copies and searches by galloping.
struct CopiesTable {
    arity: usize,
    order: Vec<ColId>,
    rows: Rows,
}

impl CopiesTable {
    /// The first position in `lo..hi` whose value in sort column `depth`
    /// fails `below`: doubling steps find a window, a binary search the
    /// position in it.
    fn gallop(&self, depth: usize, lo: RowIdx, hi: RowIdx, below: impl Fn(Key) -> bool) -> RowIdx {
        let col = self.order[depth];
        let mut step = 1;
        while lo + step <= hi && below(self.value(lo + step - 1, col)) {
            step *= 2;
        }
        let (mut a, mut b) = (lo + step / 2, (lo + step).min(hi));
        while a < b {
            let mid = a + (b - a) / 2;
            if below(self.value(mid, col)) {
                a = mid + 1;
            } else {
                b = mid;
            }
        }
        a
    }
}

impl SortedTable for CopiesTable {
    fn arity(&self) -> usize {
        self.arity
    }

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn sort_order(&self) -> &[ColId] {
        &self.order
    }

    fn value(&self, row: RowIdx, col: ColId) -> Key {
        self.rows[row].0[col]
    }

    fn weight(&self, row: RowIdx) -> Weight {
        self.rows[row].1
    }

    fn lower_bound(&self, depth: usize, v: Key, lo: RowIdx, hi: RowIdx) -> RowIdx {
        self.gallop(depth, lo, hi, |x| x < v)
    }

    fn upper_bound(&self, depth: usize, v: Key, lo: RowIdx, hi: RowIdx) -> RowIdx {
        self.gallop(depth, lo, hi, |x| x <= v)
    }
}

/// Which kind of table a [`Store`] serves.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Minimal,
    Copies,
}

/// Relations outside any catalog, served as sorted tables on request.
struct Store {
    kind: Kind,
    schemas: BTreeMap<String, Schema>,
    rows: BTreeMap<String, Rows>,
    dict: Dictionary,
}

impl Store {
    /// The relations of `cat` with its dictionary. A minimal store holds
    /// the rows of positive weight once, as a store of sets would; a store
    /// of copies holds every row, its weight split into up to three copies.
    fn of(cat: &Catalog, kind: Kind, rng: &mut SplitMix64) -> Store {
        let mut schemas = BTreeMap::new();
        let mut rows = BTreeMap::new();
        for name in cat.names() {
            let rel = cat.get(name).unwrap();
            let normal = match kind {
                Kind::Minimal => rel.clone().distinct(),
                Kind::Copies => rel.clone().consolidate(),
            };
            let mut stored = Vec::new();
            for i in 0..normal.len() {
                let (row, w) = (normal.row(i), normal.weight(i));
                let copies = match (kind, rng.below(3)) {
                    (Kind::Copies, 0) => {
                        let part = if w == 2 { 3 } else { 2 };
                        vec![part, w - part]
                    }
                    (Kind::Copies, 1) => vec![w + 1, -2, 1],
                    _ => vec![w],
                };
                let copies = copies.into_iter().filter(|&c| c != 0);
                stored.extend(copies.map(|c| (row.clone(), c)));
            }
            schemas.insert(name.to_owned(), rel.schema.clone());
            rows.insert(name.to_owned(), stored);
        }
        Store {
            kind,
            schemas,
            rows,
            dict: cat.dictionary().clone(),
        }
    }
}

impl Tables for Store {
    fn schema(&self, relation: &str) -> Result<&Schema> {
        self.schemas
            .get(relation)
            .with_context(|| format!("the store has no relation {relation}"))
    }

    fn sorted(&self, relation: &str, order: &[ColId]) -> Result<Box<dyn SortedTable + '_>> {
        let arity = self.schema(relation)?.arity();
        let rows = sorted_by(self.rows[relation].clone(), order);
        Ok(match self.kind {
            Kind::Minimal => Box::new(MinimalTable {
                arity,
                order: order.to_vec(),
                rows: rows.into_iter().map(|(row, _)| row).collect(),
            }),
            Kind::Copies => Box::new(CopiesTable {
                arity,
                order: order.to_vec(),
                rows,
            }),
        })
    }

    fn dictionary(&self) -> &Dictionary {
        &self.dict
    }
}

/// The catalog a store must agree with: the same relations, as sets for a
/// minimal store.
fn reference_catalog(cat: &Catalog, kind: Kind) -> Catalog {
    let mut reference = cat.clone();
    if kind == Kind::Minimal {
        for name in cat.names() {
            reference.insert(cat.get(name).unwrap().clone().distinct());
        }
    }
    reference
}

#[test]
fn queries_over_a_store_match_the_catalog() {
    let mut checked = [0, 0];
    for case in 0..CASES {
        for (k, kind) in [Kind::Minimal, Kind::Copies].into_iter().enumerate() {
            let mut rng = SplitMix64::new(case);
            let (cat, rels) = common::catalog(&mut rng, true);
            let Some(query) = common::query(&mut rng, &rels) else {
                continue;
            };
            let store = Store::of(&cat, kind, &mut rng);
            let reference = reference_catalog(&cat, kind);
            for (name, exec) in EXECUTORS {
                let over_store = exec(&query, &store).unwrap();
                let over_catalog = exec(&query, &reference).unwrap();
                assert_eq!(over_store, over_catalog, "case {case}: {name}\n{query:#?}");
            }
            // Every table the store serves meets the contract, in any order.
            for (name, schema) in &rels {
                let mut order: Vec<ColId> = (0..schema.arity()).collect();
                common::shuffle(&mut rng, &mut order);
                check_contract(&*store.sorted(name, &order).unwrap());
            }
            checked[k] += 1;
        }
    }
    assert!(
        checked.iter().all(|&n| n >= CASES as usize * 3 / 4),
        "{checked:?}"
    );
}

#[test]
fn programs_over_a_store_match_the_catalog() {
    let mut evaluated = 0;
    for case in 0..CASES {
        for kind in [Kind::Minimal, Kind::Copies] {
            let mut rng = SplitMix64::new(case);
            let signed = rng.below(2) == 0;
            let c = common::program_case(&mut rng, signed);
            let store = Store::of(&c.edb, kind, &mut rng);
            let reference = reference_catalog(&c.edb, kind);
            for (name, exec) in EXECUTORS {
                let over_store = fixpoint::semi_naive_over(&c.program, &store, exec);
                let over_catalog = fixpoint::semi_naive(&c.program, &reference, exec);
                match (over_store, over_catalog) {
                    (Ok(a), Ok(b)) => {
                        for relation in &c.derived {
                            assert_eq!(
                                common::zset_of(
                                    a.catalog.get(relation).unwrap(),
                                    a.catalog.dictionary()
                                ),
                                common::zset_of(
                                    b.catalog.get(relation).unwrap(),
                                    b.catalog.dictionary()
                                ),
                                "case {case}: {name}, {relation}"
                            );
                        }
                        // Over a store, the result holds the derived
                        // relations only.
                        assert_eq!(a.catalog.names().len(), c.derived.len());
                        evaluated += 1;
                    }
                    (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string(), "case {case}"),
                    (a, b) => panic!(
                        "case {case}: {name}: over the store {:?}, over the catalog {:?}",
                        a.err(),
                        b.err()
                    ),
                }
            }
        }
    }
    assert!(
        evaluated >= CASES as usize * 2,
        "only {evaluated} runs evaluated"
    );
}
