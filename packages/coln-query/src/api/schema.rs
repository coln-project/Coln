// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module provides implementations for passing references to tables
//! ([TableRef]) and communicating a schema of a table ([TableSchema]).

use std::fmt::Display;

use crate::{host::expr::Literal, scalarial::ScalarType};

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

impl From<&ir::Path> for TableRef {
    fn from(value: &ir::Path) -> Self {
        TableRef {
            inner: value.to_string(),
        }
    }
}

pub struct TableSchema {
    /// The table's unique identifier/name.
    name: TableRef,
    /// All fields of the table in their physical order.
    columns: Vec<Column>,
    /// The list of (possibly compound) primary keys into the table, specified
    /// as indexes into the schema's [`columns`](Self::columns).
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

impl From<&ir::Lit> for Literal {
    fn from(value: &ir::Lit) -> Self {
        match value {
            ir::Lit::Int { value } => Literal::Iint(*value),
            ir::Lit::String { value } => Literal::String(value.clone()),
        }
    }
}
