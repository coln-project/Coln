// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A named, typed relation stored as normalized keys.
//!
//! `Relation` is the engine's plain in-memory interchange type: a
//! [`Schema`] plus one `Vec<Key>` per column. Values are encoded on the
//! way in ([`Relation::from_rows`]) and decoded on the way out
//! ([`Relation::row_values`]), see [`crate::types`] for the encoding;
//! everything in between compares keys. Arrow `RecordBatch` is the
//! serialization boundary (see [`crate::io`]).

use std::cmp::Ordering;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use arrow::array::{
    Array, ArrayRef, BooleanArray, Int64Array, StringArray, UInt32Array, UInt64Array,
};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow::record_batch::RecordBatch;

use crate::types::{Column, Dictionary, Key, ScalarType, Schema, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub name: String,
    pub schema: Schema,
    /// Column-major keys; all columns have the same length.
    pub cols: Vec<Vec<Key>>,
}

impl Relation {
    /// A relation of unsigned integer columns. Keys and values coincide
    /// for this type, so `cols` holds the values themselves.
    pub fn new(
        name: impl Into<String>,
        col_names: impl IntoIterator<Item = impl Into<String>>,
        cols: Vec<Vec<Key>>,
    ) -> Self {
        Self::with_schema(name, Schema::uint(col_names), cols)
    }

    /// A relation over already encoded keys.
    pub fn with_schema(name: impl Into<String>, schema: Schema, cols: Vec<Vec<Key>>) -> Self {
        assert_eq!(schema.arity(), cols.len(), "one column per schema entry");
        if let Some(first) = cols.first() {
            assert!(
                cols.iter().all(|c| c.len() == first.len()),
                "all columns must have the same length"
            );
        }
        Self {
            name: name.into(),
            schema,
            cols,
        }
    }

    /// An empty relation with the given schema.
    pub fn empty(name: impl Into<String>, schema: Schema) -> Self {
        let cols = vec![Vec::new(); schema.arity()];
        Self::with_schema(name, schema, cols)
    }

    /// Encode typed rows. Every row must have one value per column, of
    /// the column's type; strings are interned into `dict`.
    pub fn from_rows(
        name: impl Into<String>,
        schema: Schema,
        rows: impl IntoIterator<Item = Vec<Value>>,
        dict: &mut Dictionary,
    ) -> Result<Self> {
        let name = name.into();
        let mut cols: Vec<Vec<Key>> = vec![Vec::new(); schema.arity()];
        for (i, row) in rows.into_iter().enumerate() {
            if row.len() != schema.arity() {
                bail!(
                    "relation {name}, row {i}: {} values for {} columns",
                    row.len(),
                    schema.arity()
                );
            }
            for (c, value) in row.iter().enumerate() {
                let expected = schema.column_type(c);
                if value.scalar_type() != expected {
                    bail!(
                        "relation {name}, row {i}, column {}: expected {expected}, got {value}",
                        schema.name(c)
                    );
                }
                cols[c].push(value.to_key(dict));
            }
        }
        Ok(Self::with_schema(name, schema, cols))
    }

    /// Number of columns.
    pub fn arity(&self) -> usize {
        self.cols.len()
    }

    /// Number of rows.
    pub fn len(&self) -> usize {
        self.cols.first().map_or(0, Vec::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn col_names(&self) -> Vec<String> {
        self.schema.names()
    }

    /// The `i`-th row as keys.
    pub fn row(&self, i: usize) -> Vec<Key> {
        self.cols.iter().map(|c| c[i]).collect()
    }

    /// One cell, decoded.
    pub fn value(&self, row: usize, col: usize, dict: &Dictionary) -> Result<Value> {
        Value::from_key(self.schema.column_type(col), self.cols[col][row], dict)
            .with_context(|| format!("relation {}, column {}", self.name, self.schema.name(col)))
    }

    /// The `i`-th row, decoded.
    pub fn row_values(&self, i: usize, dict: &Dictionary) -> Result<Vec<Value>> {
        (0..self.arity()).map(|c| self.value(i, c, dict)).collect()
    }

    fn cmp_rows(&self, a: usize, b: usize) -> Ordering {
        for c in &self.cols {
            match c[a].cmp(&c[b]) {
                Ordering::Equal => continue,
                other => return other,
            }
        }
        Ordering::Equal
    }

    /// Sort rows lexicographically by key (all columns, left to right)
    /// and drop duplicate rows. Relations are sets; generators may emit
    /// duplicates.
    pub fn sorted_dedup(self) -> Self {
        let mut idx: Vec<usize> = (0..self.len()).collect();
        idx.sort_unstable_by(|&a, &b| self.cmp_rows(a, b));
        idx.dedup_by(|a, b| self.cmp_rows(*a, *b) == Ordering::Equal);
        let cols = self
            .cols
            .iter()
            .map(|c| idx.iter().map(|&i| c[i]).collect())
            .collect();
        Self {
            name: self.name,
            schema: self.schema,
            cols,
        }
    }

    /// Set union of two relations with the same arity. Both inputs must be
    /// sorted and deduplicated (as produced by [`Self::sorted_dedup`]); the
    /// result keeps this relation's name and schema and is sorted and
    /// deduplicated again.
    pub fn union(&self, other: &Relation) -> Relation {
        assert_eq!(self.arity(), other.arity(), "union needs equal arity");
        let mut cols: Vec<Vec<Key>> = (0..self.arity())
            .map(|_| Vec::with_capacity(self.len() + other.len()))
            .collect();
        let mut push = |rel: &Relation, row: usize| {
            for (c, col) in cols.iter_mut().enumerate() {
                col.push(rel.cols[c][row]);
            }
        };
        let (mut i, mut j) = (0, 0);
        while i < self.len() && j < other.len() {
            match cmp_row_pair(self, i, other, j) {
                Ordering::Less => {
                    push(self, i);
                    i += 1;
                }
                Ordering::Greater => {
                    push(other, j);
                    j += 1;
                }
                Ordering::Equal => {
                    push(self, i);
                    i += 1;
                    j += 1;
                }
            }
        }
        while i < self.len() {
            push(self, i);
            i += 1;
        }
        while j < other.len() {
            push(other, j);
            j += 1;
        }
        Relation::with_schema(self.name.clone(), self.schema.clone(), cols)
    }

    /// Rows of this relation that are absent from `other` (set difference).
    /// Both inputs must be sorted and deduplicated.
    pub fn minus(&self, other: &Relation) -> Relation {
        assert_eq!(self.arity(), other.arity(), "minus needs equal arity");
        let mut cols: Vec<Vec<Key>> = (0..self.arity()).map(|_| Vec::new()).collect();
        let (mut i, mut j) = (0, 0);
        while i < self.len() {
            let keep = loop {
                if j == other.len() {
                    break true;
                }
                match cmp_row_pair(self, i, other, j) {
                    Ordering::Less => break true,
                    Ordering::Equal => break false,
                    Ordering::Greater => j += 1,
                }
            };
            if keep {
                for (c, col) in cols.iter_mut().enumerate() {
                    col.push(self.cols[c][i]);
                }
            }
            i += 1;
        }
        Relation::with_schema(self.name.clone(), self.schema.clone(), cols)
    }

    /// Build a relation from row-major flat keys (`schema.arity()` values
    /// per row).
    pub fn from_flat_rows(name: impl Into<String>, schema: Schema, flat: &[Key]) -> Self {
        let width = schema.arity();
        assert!(width > 0, "from_flat_rows needs at least one column");
        assert_eq!(flat.len() % width, 0, "flat data must be whole rows");
        let n = flat.len() / width;
        let mut cols: Vec<Vec<Key>> = (0..width).map(|_| Vec::with_capacity(n)).collect();
        for row in flat.chunks_exact(width) {
            for (c, &x) in row.iter().enumerate() {
                cols[c].push(x);
            }
        }
        Self::with_schema(name, schema, cols)
    }

    /// Decode into an Arrow batch: `Uint` becomes `UInt64`, `Iint`
    /// `Int64`, `Bool` `Boolean`, `Char` `UInt32` (the scalar value) and
    /// `String` `Utf8`.
    pub fn to_record_batch(&self, dict: &Dictionary) -> Result<RecordBatch> {
        let mut fields = Vec::with_capacity(self.arity());
        let mut arrays: Vec<ArrayRef> = Vec::with_capacity(self.arity());
        for (c, col) in self.schema.columns().iter().enumerate() {
            let keys = &self.cols[c];
            let (data_type, array): (DataType, ArrayRef) = match col.ty {
                ScalarType::Uint => (DataType::UInt64, Arc::new(UInt64Array::from(keys.clone()))),
                ScalarType::Iint => (
                    DataType::Int64,
                    Arc::new(Int64Array::from(
                        keys.iter()
                            .map(|&k| match Value::from_key(ScalarType::Iint, k, dict) {
                                Ok(Value::Iint(i)) => i,
                                _ => unreachable!("every key decodes as iint"),
                            })
                            .collect::<Vec<i64>>(),
                    )),
                ),
                ScalarType::Bool => (
                    DataType::Boolean,
                    Arc::new(BooleanArray::from(
                        keys.iter()
                            .map(|&k| match Value::from_key(ScalarType::Bool, k, dict)? {
                                Value::Bool(b) => Ok(b),
                                _ => unreachable!(),
                            })
                            .collect::<Result<Vec<bool>>>()?,
                    )),
                ),
                ScalarType::Char => (
                    DataType::UInt32,
                    Arc::new(UInt32Array::from(
                        keys.iter()
                            .map(|&k| match Value::from_key(ScalarType::Char, k, dict)? {
                                Value::Char(ch) => Ok(u32::from(ch)),
                                _ => unreachable!(),
                            })
                            .collect::<Result<Vec<u32>>>()?,
                    )),
                ),
                ScalarType::String => (
                    DataType::Utf8,
                    Arc::new(StringArray::from(
                        keys.iter()
                            .map(|&k| {
                                dict.string(k).map(str::to_owned).with_context(|| {
                                    format!(
                                        "relation {}, column {}: key {k} is not a dictionary code",
                                        self.name, col.name
                                    )
                                })
                            })
                            .collect::<Result<Vec<String>>>()?,
                    )),
                ),
            };
            fields.push(Field::new(&col.name, data_type, false));
            arrays.push(array);
        }
        RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), arrays)
            .with_context(|| format!("building record batch for relation {}", self.name))
    }

    /// Encode an Arrow batch. Accepts the column types
    /// [`Self::to_record_batch`] produces, all non-nullable; strings are
    /// interned into `dict`.
    pub fn from_record_batch(
        name: impl Into<String>,
        batch: &RecordBatch,
        dict: &mut Dictionary,
    ) -> Result<Self> {
        let name = name.into();
        let mut columns = Vec::new();
        let mut cols = Vec::new();
        for (field, array) in batch.schema().fields().iter().zip(batch.columns()) {
            if array.null_count() > 0 {
                bail!(
                    "relation {name}, column {}: nulls not supported",
                    field.name()
                );
            }
            let (ty, keys): (ScalarType, Vec<Key>) = match field.data_type() {
                DataType::UInt64 => (
                    ScalarType::Uint,
                    downcast::<UInt64Array>(array, &name, field.name())?
                        .values()
                        .to_vec(),
                ),
                DataType::Int64 => (
                    ScalarType::Iint,
                    downcast::<Int64Array>(array, &name, field.name())?
                        .values()
                        .iter()
                        .map(|&i| Value::Iint(i).to_key(dict))
                        .collect(),
                ),
                DataType::Boolean => (
                    ScalarType::Bool,
                    downcast::<BooleanArray>(array, &name, field.name())?
                        .iter()
                        .map(|b| u64::from(b.expect("no nulls")))
                        .collect(),
                ),
                DataType::UInt32 => {
                    let array = downcast::<UInt32Array>(array, &name, field.name())?;
                    let mut keys = Vec::with_capacity(array.len());
                    for &raw in array.values().iter() {
                        let Some(ch) = char::from_u32(raw) else {
                            bail!(
                                "relation {name}, column {}: {raw} is not a character",
                                field.name()
                            );
                        };
                        keys.push(Value::Char(ch).to_key(dict));
                    }
                    (ScalarType::Char, keys)
                }
                DataType::Utf8 => (
                    ScalarType::String,
                    downcast::<StringArray>(array, &name, field.name())?
                        .iter()
                        .map(|s| dict.intern(s.expect("no nulls")))
                        .collect(),
                ),
                other => bail!(
                    "relation {name}, column {}: unsupported Arrow type {other}",
                    field.name()
                ),
            };
            columns.push(Column::new(field.name().clone(), ty));
            cols.push(keys);
        }
        Ok(Self::with_schema(name, Schema::new(columns), cols))
    }
}

fn downcast<'a, T: Array + 'static>(
    array: &'a ArrayRef,
    relation: &str,
    column: &str,
) -> Result<&'a T> {
    array.as_any().downcast_ref::<T>().with_context(|| {
        format!(
            "relation {relation}, column {column}: array does not match its declared type {}",
            array.data_type()
        )
    })
}

/// Lexicographic comparison of row `i` of `a` with row `j` of `b`
/// (equal arity required).
fn cmp_row_pair(a: &Relation, i: usize, b: &Relation, j: usize) -> Ordering {
    for (ca, cb) in a.cols.iter().zip(&b.cols) {
        match ca[i].cmp(&cb[j]) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_and_minus() {
        let a = Relation::new("a", ["x", "y"], vec![vec![1, 2, 4], vec![1, 2, 4]]);
        let b = Relation::new("b", ["x", "y"], vec![vec![2, 3], vec![2, 3]]);
        let u = a.union(&b);
        assert_eq!(u.len(), 4);
        assert_eq!(u.row(0), vec![1, 1]);
        assert_eq!(u.row(3), vec![4, 4]);
        let m = a.minus(&b);
        assert_eq!(m.len(), 2);
        assert_eq!(m.row(0), vec![1, 1]);
        assert_eq!(m.row(1), vec![4, 4]);
        let empty = b.minus(&u);
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn sorted_dedup_sorts_and_drops_duplicates() {
        let r = Relation::new("r", ["a", "b"], vec![vec![2, 1, 2, 1], vec![10, 20, 5, 20]]);
        let r = r.sorted_dedup();
        assert_eq!(r.len(), 3);
        assert_eq!(r.row(0), vec![1, 20]);
        assert_eq!(r.row(1), vec![2, 5]);
        assert_eq!(r.row(2), vec![2, 10]);
    }

    fn typed_schema() -> Schema {
        Schema::new([
            Column::new("id", ScalarType::Uint),
            Column::new("delta", ScalarType::Iint),
            Column::new("flag", ScalarType::Bool),
            Column::new("initial", ScalarType::Char),
            Column::new("name", ScalarType::String),
        ])
    }

    fn typed_rows() -> Vec<Vec<Value>> {
        vec![
            vec![
                1u64.into(),
                (-5i64).into(),
                true.into(),
                'a'.into(),
                "alice".into(),
            ],
            vec![
                2u64.into(),
                7i64.into(),
                false.into(),
                'b'.into(),
                "bob".into(),
            ],
            vec![
                3u64.into(),
                0i64.into(),
                true.into(),
                'a'.into(),
                "alice".into(),
            ],
        ]
    }

    #[test]
    fn typed_rows_round_trip() {
        let mut dict = Dictionary::new();
        let rel = Relation::from_rows("people", typed_schema(), typed_rows(), &mut dict).unwrap();
        assert_eq!(rel.len(), 3);
        assert_eq!(dict.len(), 2, "two distinct names");
        assert_eq!(rel.cols[4][0], rel.cols[4][2], "equal strings share a key");
        for (i, row) in typed_rows().iter().enumerate() {
            assert_eq!(&rel.row_values(i, &dict).unwrap(), row);
        }
        assert_eq!(rel.value(1, 1, &dict).unwrap(), Value::Iint(7));
    }

    #[test]
    fn from_rows_checks_shape_and_types() {
        let mut dict = Dictionary::new();
        let short = vec![vec![1u64.into()]];
        assert!(Relation::from_rows("p", typed_schema(), short, &mut dict).is_err());
        let mut wrong = typed_rows();
        wrong[0][1] = "not an int".into();
        let err = Relation::from_rows("p", typed_schema(), wrong, &mut dict).unwrap_err();
        assert!(err.to_string().contains("expected iint"), "{err}");
    }

    #[test]
    fn record_batch_roundtrip_extremes() {
        let mut dict = Dictionary::new();
        let schema = typed_schema();
        let rows = vec![
            vec![
                Value::Uint(u64::MAX),
                Value::Iint(i64::MIN),
                Value::Bool(false),
                Value::Char('\0'),
                Value::from(""),
            ],
            vec![
                Value::Uint(0),
                Value::Iint(i64::MAX),
                Value::Bool(true),
                Value::Char(char::MAX),
                Value::from("日本語 ß \u{1F600}"),
            ],
        ];
        let rel = Relation::from_rows("extremes", schema, rows.clone(), &mut dict).unwrap();
        let batch = rel.to_record_batch(&dict).unwrap();
        let mut fresh = Dictionary::new();
        let back = Relation::from_record_batch("extremes", &batch, &mut fresh).unwrap();
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(&back.row_values(i, &fresh).unwrap(), row);
        }
    }

    #[test]
    fn record_batch_roundtrip_uint() {
        let dict = Dictionary::new();
        let r = Relation::new("r", ["x", "y"], vec![vec![1, 2, 3], vec![4, 5, 6]]);
        let batch = r.to_record_batch(&dict).unwrap();
        let back = Relation::from_record_batch("r", &batch, &mut Dictionary::new()).unwrap();
        assert_eq!(r, back);
    }

    #[test]
    fn record_batch_roundtrip_typed() {
        let mut dict = Dictionary::new();
        let rel = Relation::from_rows("people", typed_schema(), typed_rows(), &mut dict).unwrap();
        let batch = rel.to_record_batch(&dict).unwrap();
        assert_eq!(batch.schema().field(1).data_type(), &DataType::Int64);
        assert_eq!(batch.schema().field(4).data_type(), &DataType::Utf8);

        // Decoding into a fresh dictionary must yield the same values,
        // even though the string codes may differ.
        let mut other = Dictionary::new();
        other.intern("zebra");
        let back = Relation::from_record_batch("people", &batch, &mut other).unwrap();
        assert_eq!(back.schema, rel.schema);
        for i in 0..rel.len() {
            assert_eq!(
                back.row_values(i, &other).unwrap(),
                rel.row_values(i, &dict).unwrap()
            );
        }
    }
}
