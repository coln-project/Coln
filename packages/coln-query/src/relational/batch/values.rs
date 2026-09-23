// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Conversions between the pipeline's scalar vocabulary and the batch
//! engine's. The two agree on every type except `Null`, which the engine
//! does not store: a schema or row carrying it is rejected at the
//! boundary instead of being mapped to something else.

use anyhow::{Result, bail};
use coln_batch::types::{Column as BatchColumn, ScalarType as BatchType, Schema, Value};

use crate::host::expr::Literal;
use crate::relational::schema::TableSchema;
use crate::scalarial::{ScalarType, ScalarTypedValue};

/// The engine's type for a plan type.
pub(crate) fn batch_type(ty: ScalarType) -> Result<BatchType> {
    Ok(match ty {
        ScalarType::Uint => BatchType::Uint,
        ScalarType::Iint => BatchType::Iint,
        ScalarType::Bool => BatchType::Bool,
        ScalarType::Char => BatchType::Char,
        ScalarType::String => BatchType::String,
        ScalarType::Null => bail!("the batch backend does not store null values"),
    })
}

/// The plan type for an engine type.
pub(crate) fn plan_type(ty: BatchType) -> ScalarType {
    match ty {
        BatchType::Uint => ScalarType::Uint,
        BatchType::Iint => ScalarType::Iint,
        BatchType::Bool => ScalarType::Bool,
        BatchType::Char => ScalarType::Char,
        BatchType::String => ScalarType::String,
    }
}

/// A base table's schema in the engine's vocabulary.
pub(crate) fn batch_schema(schema: &TableSchema) -> Result<Schema> {
    schema
        .columns()
        .iter()
        .map(|column| {
            Ok(BatchColumn::new(
                column.name(),
                batch_type(column.scalar_type())?,
            ))
        })
        .collect()
}

/// A plan literal as an engine value.
pub(crate) fn literal_value(literal: &Literal) -> Result<Value> {
    Ok(match literal {
        Literal::String(s) => Value::String(s.clone()),
        Literal::Uint(u) => Value::Uint(*u),
        Literal::Iint(i) => Value::Iint(*i),
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Null(()) => bail!("the batch backend does not support null literals"),
    })
}

/// A fed cell as an engine value.
pub(crate) fn engine_value(value: &ScalarTypedValue) -> Result<Value> {
    Ok(match value {
        ScalarTypedValue::String(s) => Value::String(s.clone()),
        ScalarTypedValue::Uint(u) => Value::Uint(*u),
        ScalarTypedValue::Iint(i) => Value::Iint(*i),
        ScalarTypedValue::Bool(b) => Value::Bool(*b),
        ScalarTypedValue::Char(c) => Value::Char(*c),
        ScalarTypedValue::Null(()) => bail!("the batch backend does not store null values"),
    })
}

/// An engine value as a pipeline cell.
pub(crate) fn pipeline_value(value: Value) -> ScalarTypedValue {
    match value {
        Value::String(s) => ScalarTypedValue::String(s),
        Value::Uint(u) => ScalarTypedValue::Uint(u),
        Value::Iint(i) => ScalarTypedValue::Iint(i),
        Value::Bool(b) => ScalarTypedValue::Bool(b),
        Value::Char(c) => ScalarTypedValue::Char(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_maps_both_ways_except_null() {
        for ty in [
            ScalarType::Uint,
            ScalarType::Iint,
            ScalarType::Bool,
            ScalarType::Char,
            ScalarType::String,
        ] {
            assert_eq!(plan_type(batch_type(ty).unwrap()), ty);
        }
        assert!(batch_type(ScalarType::Null).is_err());
    }

    #[test]
    fn values_round_trip() {
        let values = [
            ScalarTypedValue::Uint(7),
            ScalarTypedValue::Iint(-7),
            ScalarTypedValue::Bool(true),
            ScalarTypedValue::Char('x'),
            ScalarTypedValue::String("seven".into()),
        ];
        for value in values {
            assert_eq!(pipeline_value(engine_value(&value).unwrap()), value);
        }
        assert!(engine_value(&ScalarTypedValue::Null(())).is_err());
        assert!(literal_value(&Literal::Null(())).is_err());
        assert_eq!(literal_value(&Literal::Iint(-1)).unwrap(), Value::Iint(-1));
    }
}
