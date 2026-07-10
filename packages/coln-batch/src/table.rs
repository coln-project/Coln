// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The engine's window onto stored data: sorted tables.
//!
//! One [`SortedTable`] is one **index**: the rows of a relation, totally
//! ordered by the lexicographic order of a declared column permutation.
//! All join executors in this crate are written purely against this trait.
//! Anything that can answer its five required methods — the in-memory
//! [`ArrowSortedTable`] below today, a Hexane-backed B-tree index later —
//! can back the engine unchanged.
//!
//! The full contract and rationale live in `docs/sorted-table-api.md`.

use std::cmp::Ordering;
use std::ops::Range;

use anyhow::{Result, bail};
use arrow::record_batch::RecordBatch;

use crate::relation::Relation;

/// Column id, in *schema* order (position in the relation's column list).
pub type ColId = usize;

pub trait SortedTable {
    /// Number of columns of the underlying relation.
    fn arity(&self) -> usize;

    /// Number of rows.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The column order rows are sorted by: a permutation of `0..arity()`.
    /// `sort_order()[0]` is the major sort column.
    fn sort_order(&self) -> &[ColId];

    /// Cell access. `row` is a position in *sorted* order (`0..len()`);
    /// `col` is a column id in *schema* order.
    fn value(&self, row: usize, col: ColId) -> u64;

    /// First position in `lo..hi` whose value in sort column `depth`
    /// (i.e. schema column `sort_order()[depth]`) is `>= v`.
    ///
    /// Precondition: all rows in `lo..hi` agree on sort columns
    /// `0..depth`. The engine descends the sort order left to right, so
    /// this holds by construction.
    ///
    /// The default is a binary search over [`Self::value`]; back ends with
    /// better means (galloping search, block statistics, B-tree descent)
    /// should override it.
    fn lower_bound(&self, depth: usize, v: u64, lo: usize, hi: usize) -> usize {
        let col = self.sort_order()[depth];
        let (mut lo, mut hi) = (lo, hi);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.value(mid, col) < v {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// First position in `lo..hi` whose value in sort column `depth` is
    /// `> v`. Same precondition as [`Self::lower_bound`].
    fn upper_bound(&self, depth: usize, v: u64, lo: usize, hi: usize) -> usize {
        let col = self.sort_order()[depth];
        let (mut lo, mut hi) = (lo, hi);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.value(mid, col) <= v {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// The contiguous range of positions in `lo..hi` whose sort column
    /// `depth` equals `v` (empty, positioned at the insertion point, if
    /// `v` is absent). Same precondition as [`Self::lower_bound`].
    fn equal_range(&self, depth: usize, v: u64, lo: usize, hi: usize) -> Range<usize> {
        let start = self.lower_bound(depth, v, lo, hi);
        let end = self.upper_bound(depth, v, start, hi);
        start..end
    }
}

/// In-memory [`SortedTable`] built from Arrow data: sorts once at
/// construction, then serves reads from plain `u64` column vectors.
#[derive(Clone, Debug)]
pub struct ArrowSortedTable {
    name: String,
    sort_order: Vec<ColId>,
    /// Schema-ordered columns; rows are in sorted order.
    cols: Vec<Vec<u64>>,
}

impl ArrowSortedTable {
    /// Build from a [`Relation`], sorting its rows by `sort_order`
    /// (which must be a permutation of `0..relation.arity()`).
    pub fn from_relation(rel: &Relation, sort_order: Vec<ColId>) -> Result<Self> {
        let arity = rel.arity();
        let mut seen = vec![false; arity];
        if sort_order.len() != arity {
            bail!(
                "{}: sort order has {} entries, relation has arity {arity}",
                rel.name,
                sort_order.len()
            );
        }
        for &c in &sort_order {
            if c >= arity || seen[c] {
                bail!(
                    "{}: sort order {sort_order:?} is not a permutation",
                    rel.name
                );
            }
            seen[c] = true;
        }

        let mut idx: Vec<usize> = (0..rel.len()).collect();
        idx.sort_unstable_by(|&a, &b| {
            for &c in &sort_order {
                match rel.cols[c][a].cmp(&rel.cols[c][b]) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
            Ordering::Equal
        });
        let cols = (0..arity)
            .map(|c| idx.iter().map(|&i| rel.cols[c][i]).collect())
            .collect();
        Ok(Self {
            name: rel.name.clone(),
            sort_order,
            cols,
        })
    }

    /// Build directly from an Arrow batch (all columns non-nullable
    /// `UInt64`).
    pub fn from_record_batch(
        name: impl Into<String>,
        batch: &RecordBatch,
        sort_order: Vec<ColId>,
    ) -> Result<Self> {
        let rel = Relation::from_record_batch(name, batch)?;
        Self::from_relation(&rel, sort_order)
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl SortedTable for ArrowSortedTable {
    fn arity(&self) -> usize {
        self.cols.len()
    }

    fn len(&self) -> usize {
        self.cols.first().map_or(0, Vec::len)
    }

    fn sort_order(&self) -> &[ColId] {
        &self.sort_order
    }

    fn value(&self, row: usize, col: ColId) -> u64 {
        self.cols[col][row]
    }
}

/// Brute-force conformance check for [`SortedTable`] implementations.
///
/// Verifies (a) `sort_order` is a permutation, (b) rows are sorted, and
/// (c) `lower_bound`/`upper_bound`/`equal_range` agree with linear scans
/// on every prefix range — walking ranges exactly the way the generic
/// join will. Quadratic-ish; use on small instances in tests. Storage
/// implementors: point this at your index to validate the contract.
pub fn check_contract<T: SortedTable>(t: &T) {
    let arity = t.arity();
    let order = t.sort_order();
    assert_eq!(order.len(), arity, "sort order must cover all columns");
    let mut seen = vec![false; arity];
    for &c in order {
        assert!(c < arity && !seen[c], "sort order must be a permutation");
        seen[c] = true;
    }
    let sort_key = |row: usize| -> Vec<u64> { order.iter().map(|&c| t.value(row, c)).collect() };
    for r in 1..t.len() {
        assert!(
            sort_key(r - 1) <= sort_key(r),
            "rows {} and {r} out of order",
            r - 1
        );
    }
    if t.len() > 0 {
        check_range(t, 0, 0, t.len());
    }
}

fn check_range<T: SortedTable>(t: &T, depth: usize, lo: usize, hi: usize) {
    if depth == t.arity() || lo == hi {
        return;
    }
    let col = t.sort_order()[depth];
    // Probing a value below the minimum must land at `lo`.
    let min = t.value(lo, col);
    if min > 0 {
        assert_eq!(t.equal_range(depth, min - 1, lo, hi), lo..lo);
    }
    let mut pos = lo;
    while pos < hi {
        let v = t.value(pos, col);
        let range = t.equal_range(depth, v, lo, hi);
        assert_eq!(range.start, pos, "equal_range start for value {v}");
        assert!(
            range.end > pos,
            "equal_range must be non-empty for present value"
        );
        for i in range.clone() {
            assert_eq!(t.value(i, col), v, "value must be constant on equal_range");
        }
        if range.end < hi {
            assert!(t.value(range.end, col) > v, "range must be maximal");
        }
        check_range(t, depth + 1, range.start, range.end);
        pos = range.end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;

    fn table(rows: &[(u64, u64)], order: Vec<ColId>) -> ArrowSortedTable {
        let rel = Relation::new(
            "t",
            ["a", "b"],
            vec![
                rows.iter().map(|r| r.0).collect(),
                rows.iter().map(|r| r.1).collect(),
            ],
        );
        ArrowSortedTable::from_relation(&rel, order).unwrap()
    }

    #[test]
    fn rejects_bad_sort_orders() {
        let rel = Relation::new("t", ["a", "b"], vec![vec![1], vec![2]]);
        assert!(ArrowSortedTable::from_relation(&rel, vec![0]).is_err());
        assert!(ArrowSortedTable::from_relation(&rel, vec![0, 0]).is_err());
        assert!(ArrowSortedTable::from_relation(&rel, vec![0, 2]).is_err());
    }

    #[test]
    fn sorts_by_declared_order() {
        let t = table(&[(2, 10), (1, 20), (2, 5)], vec![0, 1]);
        assert_eq!(
            (0..t.len())
                .map(|r| (t.value(r, 0), t.value(r, 1)))
                .collect::<Vec<_>>(),
            vec![(1, 20), (2, 5), (2, 10)]
        );
        // Same rows, sorted by column 1 first.
        let t = table(&[(2, 10), (1, 20), (2, 5)], vec![1, 0]);
        assert_eq!(
            (0..t.len())
                .map(|r| (t.value(r, 0), t.value(r, 1)))
                .collect::<Vec<_>>(),
            vec![(2, 5), (2, 10), (1, 20)]
        );
    }

    #[test]
    fn search_on_hand_built_column() {
        // Sort column values: 1 3 3 3 7 9
        let rows = [(1, 0), (3, 0), (3, 1), (3, 2), (7, 0), (9, 0)];
        let t = table(&rows, vec![0, 1]);
        let n = t.len();
        assert_eq!(t.lower_bound(0, 3, 0, n), 1);
        assert_eq!(t.upper_bound(0, 3, 0, n), 4);
        assert_eq!(t.equal_range(0, 3, 0, n), 1..4);
        assert_eq!(t.equal_range(0, 4, 0, n), 4..4);
        assert_eq!(t.equal_range(0, 0, 0, n), 0..0);
        assert_eq!(t.equal_range(0, 9, 0, n), 5..6);
        assert_eq!(t.equal_range(0, 10, 0, n), 6..6);
        // Descend: within the a==3 range, search column b.
        assert_eq!(t.equal_range(1, 1, 1, 4), 2..3);
    }

    #[test]
    fn contract_holds_on_generated_data() {
        let [f, g, h] = generate::triangle(50, 300, 20, 7);
        for rel in [&f, &g, &h] {
            for order in [vec![0, 1], vec![1, 0]] {
                check_contract(&ArrowSortedTable::from_relation(rel, order).unwrap());
            }
        }
        let [rf, rg] = generate::f_g_pattern(40, 200, 10, 11);
        for order in [vec![0, 1, 2], vec![1, 2, 0], vec![2, 1, 0]] {
            check_contract(&ArrowSortedTable::from_relation(&rf, order).unwrap());
        }
        check_contract(&ArrowSortedTable::from_relation(&rg, vec![1, 0]).unwrap());
    }

    #[test]
    fn empty_table_is_fine() {
        let rel = Relation::new("e", ["a", "b"], vec![vec![], vec![]]);
        let t = ArrowSortedTable::from_relation(&rel, vec![0, 1]).unwrap();
        assert_eq!(t.len(), 0);
        check_contract(&t);
        assert_eq!(t.equal_range(0, 5, 0, 0), 0..0);
    }
}
