// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module provides implementations for passing references to tables
//! ([TableRef]) and communicating a schema of a table ([TableSchema]).

use std::fmt::Display;

use crate::scalarial::{ScalarType, ScalarTypedValue};

/// An identifier that uniquely identifies a table (globally across the store).
#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub struct TableRef {
    inner: String,
}

impl Display for TableRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: Into<String>> From<T> for TableRef {
    fn from(value: T) -> Self {
        TableRef {
            inner: value.into(),
        }
    }
}

pub struct TableSchema {
    /// The table's unique identifier/name.
    name: TableRef,
    /// All fields of the table in their physical order.
    columns: Vec<Column>,
    /// The list of (possibly compound) primary keys into the table, specified
    /// as indices into the [`columns`](Self::columns).
    primary_keys: Vec<Vec<usize>>,
}

impl TableSchema {
    pub fn new(name: TableRef, columns: Vec<Column>, primary_keys: Vec<Vec<usize>>) -> Self {
        Self {
            name,
            columns,
            primary_keys,
        }
    }
}

pub struct Column {
    /// The column's name.
    name: String,
    /// Ihe column's (scalar) type.
    scalar_type: ScalarType,
}

use coln_flir_rs::ir::{self};

impl From<&ir::ColumnEntry> for Column {
    fn from(value: &ir::ColumnEntry) -> Self {
        // For now we use the flattened path representation in the query engine.
        let name = value.path.to_string();
        let scalar_type = ScalarType::from(&value.col_type);
        Column { name, scalar_type }
    }
}

impl From<&ir::ColType> for ScalarType {
    fn from(value: &ir::ColType) -> Self {
        match value {
            ir::ColType::BuiltinTy { builtin_ty } => ScalarType::from(*builtin_ty),
            // We assume that row ids will be sent as unsigned integers by coln-store.
            ir::ColType::RowId { path } => ScalarType::Uint,
        }
    }
}

impl From<ir::BuiltinTy> for ScalarType {
    fn from(value: ir::BuiltinTy) -> Self {
        match value {
            // TODO: Discuss scalar types and their mappings.
            ir::BuiltinTy::BuiltinInt => ScalarType::Iint,
            ir::BuiltinTy::BuiltinStr => ScalarType::String,
        }
    }
}

impl From<&ir::Lit> for ScalarTypedValue {
    fn from(value: &ir::Lit) -> Self {
        match value {
            ir::Lit::Int { value } => ScalarTypedValue::Iint(*value),
            ir::Lit::String { value } => ScalarTypedValue::String(value.clone()),
        }
    }
}
