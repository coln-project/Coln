// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module provides implementations for passing references to tables
//! ([TableRef]) and communicating a schema of a table ([TableSchema]).

use std::fmt::Display;

use crate::{host::expr::Literal, relational::expr::SourceId, scalarial::ScalarType};

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

/// The other direction of the same identity: a plan's
/// [`SourceExpr`](crate::relational::expr::SourceExpr) leaf names its base table
/// by a [`SourceId`] built from that table's `ir::Path`, so a `SourceId` and the
/// `TableRef` of the table it names hold the same string. This is what lets a
/// [`Catalog`](crate::relational::catalog::Catalog) lookup reach a
/// `TableRef`-keyed map.
impl From<&SourceId> for TableRef {
    fn from(value: &SourceId) -> Self {
        TableRef {
            inner: value.as_str().to_string(),
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

impl TableSchema {
    pub fn name(&self) -> &TableRef {
        &self.name
    }
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }
}

pub struct Column {
    /// The column's name.
    name: String,
    /// Ihe column's (scalar) type.
    scalar_type: ScalarType,
}

impl Column {
    pub fn new<T: Into<String>>(name: T, scalar_type: ScalarType) -> Self {
        Self {
            name: name.into(),
            scalar_type,
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn scalar_type(&self) -> ScalarType {
        self.scalar_type
    }
}

use coln_flir_rs::ir::{self};
use coln_flir_rs::schema::{NativeScalarType, QueryEngineScalarType};

impl From<&ir::Lit> for Literal {
    fn from(value: &ir::Lit) -> Self {
        match value {
            ir::Lit::Int { value } => Literal::Iint(*value),
            ir::Lit::String { value } => Literal::String(value.clone()),
        }
    }
}

impl From<NativeScalarType> for ScalarType {
    fn from(value: NativeScalarType) -> Self {
        match value {
            NativeScalarType::Iint => ScalarType::Iint,
            NativeScalarType::Uint => ScalarType::Uint,
            NativeScalarType::String => ScalarType::String,
        }
    }
}

impl From<QueryEngineScalarType> for ScalarType {
    fn from(value: QueryEngineScalarType) -> Self {
        match value {
            // A row id's two halves reach the query engine as plain unsigned
            // integers, so every query-engine type is a native one by this
            // point.
            QueryEngineScalarType::Native(native) => ScalarType::from(native),
        }
    }
}
