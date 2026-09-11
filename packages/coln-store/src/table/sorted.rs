// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::ops::Range;

use crate::table::{self, TableHandle};

pub type RowIdx = usize;
pub type ColIdx = usize;

// @Jan, I think this is best defined in coln-integrator and you can code against
// this interface which coln-integrator provides for you no matter if the data
// your queries operate upon comes from coln-store or from coln-query.
/// A read API for some snapshot of _sorted_, _column-oriented_ data.
pub trait SortedTable {
    type Value: PartialOrd;

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
    // TODO should this come from the IR?
    fn primary_keys(&self) -> &[&[ColIdx]] {
        &[]
    }

    // TODO maybe expose an iterator data structure to avoid lookup every time?
    /// Cell access. `row` is a position in *sorted* order (`0..len()`);
    /// `col` is a column position in *schema* order.
    fn value(&self, row: RowIdx, col: ColIdx) -> Option<Self::Value>;

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
    fn lower_bound(&self, depth: usize, v: &Self::Value, lo: RowIdx, hi: RowIdx) -> Option<RowIdx> {
        let col = self.sort_order()[depth];
        let (mut lo, mut hi) = (lo, hi);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.value(mid, col)? < *v {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Some(lo)
    }

    /// First position in `lo..hi` whose value in sort column `depth` is
    /// `> v`. Same precondition as [`Self::lower_bound`].
    fn upper_bound(&self, depth: usize, v: &Self::Value, lo: RowIdx, hi: RowIdx) -> Option<RowIdx> {
        let col = self.sort_order()[depth];
        let (mut lo, mut hi) = (lo, hi);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.value(mid, col)? <= *v {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        Some(lo)
    }

    /// The contiguous range of positions in `lo..hi` whose sort column
    /// `depth` equals `v` (empty, positioned at the insertion point, if
    /// `v` is absent). Same precondition as [`Self::lower_bound`].
    fn equal_range(
        &self,
        depth: usize,
        v: &Self::Value,
        lo: RowIdx,
        hi: RowIdx,
    ) -> Option<Range<RowIdx>> {
        let start = self.lower_bound(depth, v, lo, hi)?;
        let end = self.upper_bound(depth, v, start, hi)?;
        Some(start..end)
    }
}

pub struct SortedCopy<'a> {
    table: TableHandle<'a>,
    sort_order: Vec<ColIdx>,
}

impl<'a> TableHandle<'a> {
    pub fn sorted_copies(self) -> Vec<SortedCopy<'a>> {
        let s1 = SortedCopy {
            table: self,
            sort_order: vec![0],
        };
        let s2 = SortedCopy {
            table: self,
            sort_order: (1..self.inner().schema().columns.len() + 1).collect(),
        };
        vec![s1, s2]
    }
}

impl<'a> SortedTable for SortedCopy<'a> {
    type Value = table::PackedValue;

    /// Number of columns, including rowid column
    fn arity(&self) -> usize {
        self.table.col_count()
    }

    fn len(&self) -> usize {
        self.table.row_count()
    }

    fn sort_order(&self) -> &[ColIdx] {
        &self.sort_order
    }

    fn value(&self, row: RowIdx, col: ColIdx) -> Option<Self::Value> {
        self.table.inner().cell_by_idx(row, col)
    }

    // Note assuming that col[depth] is totally sorted, otherwise UB.
    fn lower_bound(&self, depth: usize, v: &Self::Value, lo: RowIdx, hi: RowIdx) -> Option<RowIdx> {
        let col = self.sort_order()[depth];
        let r = self.table.inner().cols.get(col)?.scope_to_value(v, lo..hi);
        Some(r.start)
    }

    fn upper_bound(&self, depth: usize, v: &Self::Value, lo: RowIdx, hi: RowIdx) -> Option<RowIdx> {
        let col = self.sort_order()[depth];
        let r = self.table.inner().cols.get(col)?.scope_to_value(v, lo..hi);
        Some(r.end)
    }

    fn equal_range(
        &self,
        depth: usize,
        v: &Self::Value,
        lo: RowIdx,
        hi: RowIdx,
    ) -> Option<Range<RowIdx>> {
        let col = self.sort_order()[depth];
        let r = self.table.inner().cols.get(col)?.scope_to_value(v, lo..hi);
        Some(r)
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
    use crate::store::Store;
    use crate::table::sorted::SortedTable;

    // TODO: once we can distinguish tables that need rebuild from those that
    // do not, assert that only rebuild tables expose the structural index
    // snapshot (rid + pk + structural = 3) while non-rebuild PK tables expose
    // two (rid + pk).

    #[test]
    fn table_with_primary_key_returns_three_sorted_snapshots() {
        let path = Path::from("T");
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![
                ColumnEntry {
                    path: Path::from("c0"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                },
                ColumnEntry {
                    path: Path::from("c1"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                },
            ],
            primary_key: Some(vec![0]),
        };
        let mut store = Store::new();
        let oid = store.create_table(path, schema).expect("create test table");

        let t = store.table(oid).expect("table exists").sorted_copies();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].sort_order(), &[0]);
        assert_eq!(t[1].sort_order(), &[1, 2]);
    }
}
