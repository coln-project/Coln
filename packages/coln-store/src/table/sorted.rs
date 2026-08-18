// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::ops::Range;

use crate::{
    store::Store,
    table::{self, TableOid, TableRef},
};

pub type RowIdx = usize;
pub type ColIdx = usize;

// @Jan, I think this is best defined in coln-integrator and you can code against
// this interface which coln-integrator provides for you no matter if the data
// your queries operate upon comes from coln-store or from coln-query.
/// A read API for some snapshot of _sorted_, _column-oriented_ data.
pub trait SortedTableSnapshot {
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
        v: Self::Value,
        lo: RowIdx,
        hi: RowIdx,
    ) -> Option<Range<RowIdx>> {
        let start = self.lower_bound(depth, &v, lo, hi)?;
        let end = self.upper_bound(depth, &v, start, hi)?;
        Some(start..end)
    }
}

pub struct SortedTable<'a> {
    table: TableRef<'a>,
    sort_order: &'a [usize],
}

impl Store {
    /// Returns all the ways a table could be sorted by as a SortedTable
    /// which implements the `SortedTable` trait
    pub fn sorted_snapshot_of(&self, oid: TableOid) -> Vec<SortedTable<'_>> {
        let mut sorted_snapshots = vec![];
        if let Some(tr) = self.table(oid) {
            let sort_by_rid = SortedTable {
                table: tr,
                sort_order: &[0],
            };
            sorted_snapshots.push(sort_by_rid);

            for index_info in tr.indexes_meta() {
                let sort_by_idnex = SortedTable {
                    table: tr,
                    sort_order: index_info.key_cols,
                };
                sorted_snapshots.push(sort_by_idnex);
            }
        }
        sorted_snapshots
    }
}

impl<'a> SortedTableSnapshot for SortedTable<'a> {
    type Value = table::CellValue;

    /// Number of columns, including rowid column
    fn arity(&self) -> usize {
        self.table.col_count()
    }

    fn len(&self) -> usize {
        self.table.row_count()
    }

    fn sort_order(&self) -> &[ColIdx] {
        self.sort_order
    }

    fn value(&self, row: RowIdx, col: ColIdx) -> Option<Self::Value> {
        self.table.cell_at(row, col)
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
    use crate::store::Store;
    use crate::table::sorted::SortedTableSnapshot;

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
            primary_key: Some(vec![Path::from("c0")]),
        };
        let mut store = Store::new();
        let oid = store.create_table(path, schema).expect("create test table");

        let snapshots = store.sorted_snapshot_of(oid);
        assert_eq!(snapshots.len(), 3);
        // rid-order placeholder, then primary-key index, then all-columns structural index
        assert_eq!(snapshots[0].sort_order(), &[0]);
        assert_eq!(snapshots[1].sort_order(), &[0]);
        assert_eq!(snapshots[2].sort_order(), &[0, 1]);
    }
}
