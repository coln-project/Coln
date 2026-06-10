// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module provides implementations for passing references to tables
//! ([TableRef]) and communicating a schema of a table ([TableSchema]).

use crate::scalar::ScalarType;

/// An identifier that uniquely identifies a table (globally across the store).
#[derive(Eq, PartialEq, Hash, Debug, Clone)]
pub struct TableRef {
    inner: String,
}

pub struct TableSchema {
    /// The table's unique identifier/name.
    name: TableRef,
    /// All fields of the table in their physical order.
    columns: Vec<Column>,
    /// The list of (possibly compound) primary keys into the table, specified
    /// as indices into the [`header`](Self::header).
    primary_keys: Vec<Vec<usize>>,
}

pub struct Column {
    /// The column's name.
    name: String,
    /// Ihe column's (scalar) type.
    scalar_type: ScalarType,
}
