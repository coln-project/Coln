// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::ops::Range;

use coln_flir_rs::engine::{
    packed::StoreScalarValue,
    table::{ColIdx, RowIdx, SortedTable},
};

use crate::table::TableHandle;

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
    type Value = StoreScalarValue;

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
        if col == 0 {
            Some(StoreScalarValue::RowId(
                self.table.inner().row_id_by_idx(row)?,
            ))
        } else {
            self.table.inner().cell_by_idx(row, col - 1)
        }
    }

    // Note assuming that col[depth] is totally sorted, otherwise UB.
    fn lower_bound(&self, depth: usize, v: &Self::Value, lo: ColIdx, hi: ColIdx) -> Option<RowIdx> {
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

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn primary_keys(&self) -> &[&[ColIdx]] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
    use crate::store::Store;
    use crate::store::txn::ReadTxnable;
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

        let read_txn = store.begin_read();
        let t = read_txn.table(oid).expect("table exists").sorted_copies();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].sort_order(), &[0]);
        assert_eq!(t[1].sort_order(), &[1, 2]);
    }
}
