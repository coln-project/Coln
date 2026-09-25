// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::fmt;

use coln_flir_rs::WireValue;

use crate::ir::{BuiltinTy, ColType};

// TODO consider generating this via macro
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
