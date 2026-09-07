// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::fmt;

use crate::column_map::ColIndex;
use crate::commit::hash::CommitHash;
use crate::ir::{BuiltinTy, ColType};
use crate::value::Value;

use super::ValidationError;

/// The unique id that identifies each row in a table.
///
/// It is managed by the database and read-only for the user.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct WireRowId {
    pub commit: CommitHash,
    pub counter: u32,
}

impl fmt::Display for WireRowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.commit.0[..6] {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ":{}", self.counter)
    }
}

pub type WireValue = Value<WireRowId>;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CellKind {
    RowId,
    Int,
    Str,
}

impl From<&ColType> for CellKind {
    fn from(col_type: &ColType) -> Self {
        match col_type {
            ColType::RowId { .. } => CellKind::RowId,
            ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            } => CellKind::Int,
            ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinStr,
            } => CellKind::Str,
        }
    }
}

impl From<&WireValue> for CellKind {
    fn from(value: &WireValue) -> Self {
        match value {
            WireValue::Id(_) => CellKind::RowId,
            WireValue::Int(_) => CellKind::Int,
            WireValue::Str(_) => CellKind::Str,
        }
    }
}

impl fmt::Display for CellKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CellKind::RowId => "entity id",
            CellKind::Int => "int",
            CellKind::Str => "string",
        })
    }
}

impl WireValue {
    pub(super) fn matches_schema(
        &self,
        col_type: &ColType,
        column: usize,
    ) -> Result<(), ValidationError> {
        let expected = CellKind::from(col_type);
        let got = CellKind::from(self);
        if expected == got {
            Ok(())
        } else {
            Err(ValidationError::TypeMismatch {
                column,
                expected,
                got,
            })
        }
    }
}

impl fmt::Display for WireValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireValue::Id(id) => write!(f, "#{id}"),
            WireValue::Int(value) => write!(f, "{value}"),
            WireValue::Str(value) => write!(f, "{value:?}"),
        }
    }
}

/// A compact [`RowId`] representation that dictionary-encodes commit hashes.
///
/// This is only meaningful together with the store-wide
/// [`IdPacker`](crate::id_packer::IdPacker) that produced it, so it never
/// crosses the store boundary. Packed ids order by `(commit_idx, counter)`,
/// which depends on dictionary insertion order. Deterministic ordering across
/// stores must compare unpacked [`RowId`]s.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub(crate) struct PackedRowId {
    pub(crate) commit_idx: u32,
    pub(crate) counter: u32,
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
