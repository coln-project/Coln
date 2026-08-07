// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::ops::Range;

pub type RowIdx = usize;
pub type ColIdx = usize;

// @Jan, I think this is best defined in coln-integrator and you can code against
// this interface which coln-integrator provides for you no matter if the data
// your queries operate upon comes from coln-store or from coln-query.
/// A read API for some snapshot of _sorted_, _column-oriented_ data.
pub trait SortedTableSnapshot {
    /// Number of columns.
    fn arity(&self) -> usize;

    /// Number of rows.
    fn len(&self) -> usize;

    /// Returns `true` if the table is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The column order rows are sorted by a permutation of `0..arity()`.
    /// `sort_order()[0]` is the major sort column.
    fn sort_order(&self) -> &[ColIdx];

    /// The table's (possibly compound) unique keys, as column indexes in
    /// schema order. Defaults to empty, meaning no keys are known.
    /// Planning metadata; the executors do not rely on it yet.
    fn primary_keys(&self) -> &[&[ColIdx]] {
        &[]
    }

    /// Cell access. `row` is a position in *sorted* order (`0..len()`);
    /// `col` is a column position in *schema* order.
    fn value(&self, row: RowIdx, col: ColIdx) -> u64;

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
    fn lower_bound(&self, depth: usize, v: u64, lo: RowIdx, hi: RowIdx) -> RowIdx {
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
    fn upper_bound(&self, depth: usize, v: u64, lo: RowIdx, hi: RowIdx) -> RowIdx {
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
    fn equal_range(&self, depth: usize, v: u64, lo: RowIdx, hi: RowIdx) -> Range<RowIdx> {
        let start = self.lower_bound(depth, v, lo, hi);
        let end = self.upper_bound(depth, v, start, hi);
        start..end
    }
}
