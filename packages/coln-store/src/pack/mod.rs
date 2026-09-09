// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod id_packer;
pub(crate) use id_packer::{IdPacker, IdPackerSnapshot};

use crate::{column_map::ColIndex, value::Value};

/// A compact [`RowId`] representation that dictionary-encodes commit hashes.
///
/// This is only meaningful together with the store-wide
/// [`IdPacker`](crate::id_packer::IdPacker) that produced it, so it never
/// crosses the store boundary. Packed ids order by `(commit_idx, counter)`,
/// which depends on dictionary insertion order. Deterministic ordering across
/// stores must compare unpacked [`RowId`]s.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct PackedRowId {
    pub commit_idx: u32,
    pub counter: u32,
}

impl ColIndex for PackedRowId {
    type Columns = (hexane::Column<u32>, hexane::Column<u32>);
    type Ref<'a> = PackedRowId;

    fn new_columns() -> Self::Columns {
        (hexane::Column::new(), hexane::Column::new())
    }

    fn scope(
        columns: &Self::Columns,
        key: Self::Ref<'_>,
        range: std::ops::Range<usize>,
    ) -> std::ops::Range<usize> {
        let range = columns.0.scope_to_value(key.commit_idx, range);
        columns.1.scope_to_value(key.counter, range)
    }

    fn iter_range(
        columns: &Self::Columns,
        range: std::ops::Range<usize>,
    ) -> impl Iterator<Item = Self::Ref<'_>> {
        columns
            .0
            .iter_range(range.clone())
            .zip(columns.1.iter_range(range))
            .map(|(commit_idx, counter)| PackedRowId {
                commit_idx,
                counter,
            })
    }

    fn len(columns: &Self::Columns) -> usize {
        columns.0.len()
    }

    fn insert(columns: &mut Self::Columns, index: usize, key: Self::Ref<'_>) {
        columns.0.insert(index, key.commit_idx);
        columns.1.insert(index, key.counter);
    }

    fn remove(columns: &mut Self::Columns, index: usize) {
        columns.0.remove(index);
        columns.1.remove(index);
    }
}

pub type PackedValue = Value<PackedRowId>;

/// Packed representation of an operation staged for a table.
#[derive(Debug)]
pub(crate) enum PackedOp {
    Add {
        row_id: PackedRowId,
        values: Vec<PackedValue>,
    },
    Delete {
        row_id: PackedRowId,
    },
}

pub(crate) struct PackedRowView {
    pub row_id: PackedRowId,
    pub values: Vec<PackedValue>,
}
