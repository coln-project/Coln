// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt;

use super::ValidationError;
use crate::commit::hash::CommitHash;
use crate::ir::{BuiltinTy, ColType};
use crate::value::Value;

/// The unique id that identifies each row in a table.
///
/// It is managed by the database and read-only for the user.
#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash, Serialize, Deserialize, Type,
)]
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

// TODO consider generateing this via macro
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
