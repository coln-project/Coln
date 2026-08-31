// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! How coln's FLIR frontend's vocabulary maps onto the backend-neutral one.
//!
//! The schema types themselves live in
//! [`relational::schema`](crate::relational::schema), one layer down, because
//! [`Catalog`](crate::relational::catalog::Catalog) answers in them. What
//! belongs here is only the frontend half of the translation: FLIR paths
//! becoming [`TableRef`]s, FLIR literals becoming [`Literal`]s, and each
//! engine's scalar types collapsing into the query engine's [`ScalarType`].

use crate::relational::schema::EntityRef;
use crate::{host::expr::Literal, scalarial::ScalarType};
use coln_flir_rs::ir::{self};
use coln_flir_rs::schema::{NativeScalarType, QueryEngineScalarType};

impl From<&ir::Path> for EntityRef {
    fn from(value: &ir::Path) -> Self {
        EntityRef::from(value.to_string())
    }
}

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
