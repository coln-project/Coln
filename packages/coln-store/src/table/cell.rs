// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::fmt;

use crate::column_map::ColIndex;
use crate::commit::hash::CommitHash;
use crate::commit::hash_dict::HashMapper;
use crate::ir::{BuiltinTy, ColType};

use super::ValidationError;

/// The unique id that identifies each row in a table.
///
/// It is managed by the database and read-only for the user.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct RowId {
    pub commit: CommitHash,
    pub counter: u32,
}

impl fmt::Display for RowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.commit.0[..6] {
            write!(f, "{byte:02x}")?;
        }
        write!(f, ":{}", self.counter)
    }
}

/// One cell in columnar storage: an entity id or a primitive value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CellValue {
    Id(RowId),
    Int(i64),
    Str(String),
}

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

impl From<&CellValue> for CellKind {
    fn from(value: &CellValue) -> Self {
        match value {
            CellValue::Id(_) => CellKind::RowId,
            CellValue::Int(_) => CellKind::Int,
            CellValue::Str(_) => CellKind::Str,
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

impl CellValue {
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

impl fmt::Display for CellValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CellValue::Id(id) => write!(f, "#{id}"),
            CellValue::Int(value) => write!(f, "{value}"),
            CellValue::Str(value) => write!(f, "{value:?}"),
        }
    }
}

/// A compact [`RowId`] representation that dictionary-encodes commit hashes.
///
/// This is only meaningful together with the store-wide [`HashMapper`] that
/// produced it, so it never crosses the store boundary. Packed ids order by
/// `(commit_idx, counter)`, which depends on dictionary insertion order.
/// Deterministic ordering across stores must compare unpacked [`RowId`]s.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub(crate) struct PackedRowId {
    pub(super) commit_idx: u32,
    pub(super) counter: u32,
}

impl PackedRowId {
    /// Pack `id`, interning its commit hash in `dict` if it is new.
    pub(crate) fn pack(id: RowId, dict: &mut HashMapper) -> Self {
        PackedRowId {
            commit_idx: dict.insert(id.commit),
            counter: id.counter,
        }
    }

    /// Pack without interning.
    ///
    /// Returns `None` when the commit hash is not in `dict`, which means no
    /// stored row can carry `id`.
    pub(crate) fn lookup(id: RowId, dict: &HashMapper) -> Option<Self> {
        Some(PackedRowId {
            commit_idx: dict.index(id.commit)?,
            counter: id.counter,
        })
    }

    pub(crate) fn unpack(self, dict: &HashMapper) -> RowId {
        RowId {
            commit: dict
                .hash_at(self.commit_idx)
                .expect("packed row id commit hash was interned on insert"),
            counter: self.counter,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PackedCell {
    Id(PackedRowId),
    Int(i64),
    Str(String),
}

impl PackedCell {
    pub(super) fn pack_cell(value: &CellValue, dict: &mut HashMapper) -> Self {
        match value {
            CellValue::Id(id) => PackedCell::Id(PackedRowId::pack(*id, dict)),
            CellValue::Int(value) => PackedCell::Int(*value),
            CellValue::Str(value) => PackedCell::Str(value.clone()),
        }
    }

    /// Packs without modifying the dictionary.
    ///
    /// Returns `None` when an id's commit hash is missing.
    pub(super) fn try_pack_cell(value: &CellValue, dict: &HashMapper) -> Option<Self> {
        Some(match value {
            CellValue::Id(id) => PackedCell::Id(PackedRowId::lookup(*id, dict)?),
            CellValue::Int(value) => PackedCell::Int(*value),
            CellValue::Str(value) => PackedCell::Str(value.clone()),
        })
    }

    pub(crate) fn unpack_cell(&self, dict: &HashMapper) -> CellValue {
        match self {
            PackedCell::Id(id) => CellValue::Id(id.unpack(dict)),
            PackedCell::Int(value) => CellValue::Int(*value),
            PackedCell::Str(value) => CellValue::Str(value.clone()),
        }
    }
}
