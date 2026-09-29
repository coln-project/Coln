// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A named, typed Z-set stored as normalized keys.
//!
//! `Relation` is the engine's plain in-memory interchange type: a
//! [`Schema`], one `Vec<Key>` per column and one [`Weight`] per row that
//! says how often the row is present (1 everywhere in a set). Values are
//! encoded on the way in ([`Relation::from_rows`]) and decoded on the way
//! out ([`Relation::row_values`]), see [`crate::types`] for the encoding;
//! everything in between compares keys. Arrow `RecordBatch` is the
//! serialization boundary (see [`crate::io`]).
//!
//! **Normal form** ([`Relation::consolidate`]): rows sorted by key, equal
//! rows merged with their weights added, rows of weight 0 dropped. The
//! executors return it; [`Relation::plus`] and [`Relation::minus`] need
//! it.

use std::cmp::Ordering;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use arrow::array::{
    Array, ArrayRef, BooleanArray, Int64Array, StringArray, UInt32Array, UInt64Array,
};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use arrow::record_batch::RecordBatch;

use crate::table::SortedTable;
use crate::types::{
    Column, Dictionary, Key, ScalarType, Schema, Value, Weight, add_weights, mul_weights,
};

/// The name of the Arrow column that carries the row weights, after the
/// value columns (see [`Relation::to_record_batch`]).
pub const WEIGHT_COLUMN: &str = "__weight";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub name: String,
    pub schema: Schema,
    /// Column-major keys; all columns have the same length.
    pub cols: Vec<Vec<Key>>,
    /// One weight per row, in row order.
    pub weights: Vec<Weight>,
}

impl Relation {
    /// A relation of unsigned integer columns, every row with weight 1.
    /// Keys and values coincide for this type, so `cols` holds the values
    /// themselves.
    pub fn new(
        name: impl Into<String>,
        col_names: impl IntoIterator<Item = impl Into<String>>,
        cols: Vec<Vec<Key>>,
    ) -> Self {
        Self::with_schema(name, Schema::uint(col_names), cols)
    }

    /// A relation over already encoded keys, every row with weight 1.
    pub fn with_schema(name: impl Into<String>, schema: Schema, cols: Vec<Vec<Key>>) -> Self {
        let rows = cols.first().map_or(0, Vec::len);
        Self::with_weights(name, schema, cols, vec![1; rows])
    }

    /// A relation over already encoded keys with one weight per row.
    pub fn with_weights(
        name: impl Into<String>,
        schema: Schema,
        cols: Vec<Vec<Key>>,
        weights: Vec<Weight>,
    ) -> Self {
        assert_eq!(schema.arity(), cols.len(), "one column per schema entry");
        assert!(
            cols.iter().all(|c| c.len() == weights.len()),
            "every column must have one key per weight"
        );
        Self {
            name: name.into(),
            schema,
            cols,
            weights,
        }
    }

    /// Copy the rows of a sorted table with their weights, keys as they are
    /// and in its sort order.
    pub fn from_table(
        name: impl Into<String>,
        schema: Schema,
        table: &(impl SortedTable + ?Sized),
    ) -> Result<Self> {
        let name = name.into();
        if table.arity() != schema.arity() {
            bail!(
                "{name}: the table has {} columns, the schema has {}",
                table.arity(),
                schema.arity()
            );
        }
        let cols = (0..table.arity())
            .map(|c| (0..table.len()).map(|r| table.value(r, c)).collect())
            .collect();
        let weights = (0..table.len()).map(|r| table.weight(r)).collect();
        Ok(Self::with_weights(name, schema, cols, weights))
    }

    /// An empty relation with the given schema.
    pub fn empty(name: impl Into<String>, schema: Schema) -> Self {
        let cols = vec![Vec::new(); schema.arity()];
        Self::with_schema(name, schema, cols)
    }

    /// Encode typed rows, every row with weight 1. Every row must have one
    /// value per column, of the column's type; strings are interned into
    /// `dict`.
    pub fn from_rows(
        name: impl Into<String>,
        schema: Schema,
        rows: impl IntoIterator<Item = Vec<Value>>,
        dict: &mut Dictionary,
    ) -> Result<Self> {
        Self::from_weighted_rows(name, schema, rows.into_iter().map(|row| (row, 1)), dict)
    }

    /// Encode typed rows with their weights, checked like
    /// [`Self::from_rows`].
    pub fn from_weighted_rows(
        name: impl Into<String>,
        schema: Schema,
        rows: impl IntoIterator<Item = (Vec<Value>, Weight)>,
        dict: &mut Dictionary,
    ) -> Result<Self> {
        let name = name.into();
        let mut cols: Vec<Vec<Key>> = vec![Vec::new(); schema.arity()];
        let mut weights = Vec::new();
        for (i, (row, weight)) in rows.into_iter().enumerate() {
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
            weights.push(weight);
        }
        Ok(Self::with_weights(name, schema, cols, weights))
    }

    /// Number of columns.
    pub fn arity(&self) -> usize {
        self.cols.len()
    }

    /// Number of rows.
    pub fn len(&self) -> usize {
        self.weights.len()
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

    /// The weight of the `i`-th row.
    pub fn weight(&self, i: usize) -> Weight {
        self.weights[i]
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

    /// The rows at positions `rows`, in that order, with the given weights.
    fn pick(self, rows: &[usize], weights: Vec<Weight>) -> Self {
        let cols = self
            .cols
            .iter()
            .map(|c| rows.iter().map(|&i| c[i]).collect())
            .collect();
        Self {
            name: self.name,
            schema: self.schema,
            cols,
            weights,
        }
    }

    /// Bring the relation into normal form (see the module docs); rows are
    /// sorted lexicographically, all columns left to right.
    pub fn consolidate(self) -> Self {
        let mut idx: Vec<usize> = (0..self.len()).collect();
        idx.sort_unstable_by(|&a, &b| self.cmp_rows(a, b));
        let (rows, weights): (Vec<usize>, Vec<Weight>) = idx
            .chunk_by(|&a, &b| self.cmp_rows(a, b) == Ordering::Equal)
            .map(|copies| {
                let weight = copies.iter().map(|&r| self.weights[r]).fold(0, add_weights);
                (copies[0], weight)
            })
            .filter(|&(_, weight)| weight != 0)
            .unzip();
        self.pick(&rows, weights)
    }

    /// The Z-set `distinct`: the rows of positive weight, each with weight
    /// 1, in normal form.
    pub fn distinct(self) -> Self {
        let rel = self.consolidate();
        let rows: Vec<usize> = (0..rel.len()).filter(|&i| rel.weights[i] > 0).collect();
        let weights = vec![1; rows.len()];
        rel.pick(&rows, weights)
    }

    /// Whether the relation is in normal form (see [`Self::consolidate`]):
    /// rows strictly ascending by key, no weight 0.
    pub fn is_consolidated(&self) -> bool {
        (1..self.len()).all(|i| self.cmp_rows(i - 1, i) == Ordering::Less)
            && self.weights.iter().all(|&w| w != 0)
    }

    /// Every weight negated.
    pub fn negate(&self) -> Relation {
        let mut rel = self.clone();
        for w in &mut rel.weights {
            *w = mul_weights(*w, -1);
        }
        rel
    }

    /// The Z-set sum: weights of equal rows added, rows that cancel out
    /// dropped. For two sets this is their union, shared rows with weight
    /// 2. Both inputs must be in normal form and of equal arity; the result
    /// is in normal form, with this relation's name and schema.
    pub fn plus(&self, other: &Relation) -> Relation {
        self.merge(other, 1)
    }

    /// The Z-set difference, `self` plus `other` negated. Same requirements
    /// as [`Self::plus`].
    pub fn minus(&self, other: &Relation) -> Relation {
        self.merge(other, -1)
    }

    /// `self + sign · other`, merging the two sorted row lists.
    fn merge(&self, other: &Relation, sign: Weight) -> Relation {
        assert_eq!(self.arity(), other.arity(), "adding needs equal arity");
        debug_assert!(
            self.is_consolidated() && other.is_consolidated(),
            "adding needs the normal form"
        );
        let mut cols: Vec<Vec<Key>> = vec![Vec::new(); self.arity()];
        let mut weights = Vec::new();
        let (mut i, mut j) = (0, 0);
        while i < self.len() || j < other.len() {
            let order = if j == other.len() {
                Ordering::Less
            } else if i == self.len() {
                Ordering::Greater
            } else {
                cmp_row_pair(self, i, other, j)
            };
            let (rel, row, weight) = match order {
                Ordering::Less => (self, i, self.weights[i]),
                Ordering::Greater => (other, j, mul_weights(sign, other.weights[j])),
                Ordering::Equal => (
                    self,
                    i,
                    add_weights(self.weights[i], mul_weights(sign, other.weights[j])),
                ),
            };
            if weight != 0 {
                for (col, source) in cols.iter_mut().zip(&rel.cols) {
                    col.push(source[row]);
                }
                weights.push(weight);
            }
            i += usize::from(order != Ordering::Greater);
            j += usize::from(order != Ordering::Less);
        }
        Relation::with_weights(self.name.clone(), self.schema.clone(), cols, weights)
    }

    /// Build a relation from row-major flat keys (`schema.arity()` values
    /// per row) and one weight per row.
    pub fn from_flat_rows(
        name: impl Into<String>,
        schema: Schema,
        flat: &[Key],
        weights: Vec<Weight>,
    ) -> Self {
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
        Self::with_weights(name, schema, cols, weights)
    }

    /// Decode into an Arrow batch: `Uint` becomes `UInt64`, `Iint`
    /// `Int64`, `Bool` `Boolean`, `Char` `UInt32` (the scalar value) and
    /// `String` `Utf8`. The weights follow as a last `Int64` column named
    /// [`WEIGHT_COLUMN`].
    pub fn to_record_batch(&self, dict: &Dictionary) -> Result<RecordBatch> {
        let mut fields = Vec::with_capacity(self.arity() + 1);
        let mut arrays: Vec<ArrayRef> = Vec::with_capacity(self.arity() + 1);
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
        fields.push(Field::new(WEIGHT_COLUMN, DataType::Int64, false));
        arrays.push(Arc::new(Int64Array::from(self.weights.clone())));
        RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), arrays)
            .with_context(|| format!("building record batch for relation {}", self.name))
    }

    /// Encode an Arrow batch. Accepts the column types
    /// [`Self::to_record_batch`] produces, all non-nullable; strings are
    /// interned into `dict`. A last `Int64` column named [`WEIGHT_COLUMN`]
    /// holds the weights; without it every row has weight 1.
    pub fn from_record_batch(
        name: impl Into<String>,
        batch: &RecordBatch,
        dict: &mut Dictionary,
    ) -> Result<Self> {
        let name = name.into();
        let schema = batch.schema();
        let mut fields: Vec<_> = schema.fields().iter().zip(batch.columns()).collect();
        let weights = match fields.last() {
            Some((field, array)) if field.name() == WEIGHT_COLUMN => {
                if array.null_count() > 0 {
                    bail!("relation {name}: weights must not be null");
                }
                let weights = downcast::<Int64Array>(array, &name, WEIGHT_COLUMN)?
                    .values()
                    .to_vec();
                fields.pop();
                weights
            }
            _ => vec![1; batch.num_rows()],
        };
        let mut columns = Vec::new();
        let mut cols = Vec::new();
        for (field, array) in fields {
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
        Ok(Self::with_weights(
            name,
            Schema::new(columns),
            cols,
            weights,
        ))
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

    fn weighted(rows: &[(u64, u64, Weight)]) -> Relation {
        Relation::with_weights(
            "r",
            Schema::uint(["x", "y"]),
            vec![
                rows.iter().map(|r| r.0).collect(),
                rows.iter().map(|r| r.1).collect(),
            ],
            rows.iter().map(|r| r.2).collect(),
        )
    }

    fn entries(rel: &Relation) -> Vec<(Vec<Key>, Weight)> {
        (0..rel.len())
            .map(|i| (rel.row(i), rel.weight(i)))
            .collect()
    }

    #[test]
    fn plus_and_minus_on_sets() {
        let a = Relation::new("a", ["x", "y"], vec![vec![1, 2, 4], vec![1, 2, 4]]);
        let b = Relation::new("b", ["x", "y"], vec![vec![2, 3], vec![2, 3]]);
        // The sum of two sets is their union, shared rows weigh 2.
        let sum = a.plus(&b);
        assert_eq!(
            entries(&sum),
            vec![
                (vec![1, 1], 1),
                (vec![2, 2], 2),
                (vec![3, 3], 1),
                (vec![4, 4], 1)
            ]
        );
        assert_eq!(sum.clone().distinct().len(), 4);
        // Taking a subset away is the set difference.
        assert_eq!(entries(&sum.minus(&b)), entries(&a));
        let two = Relation::new("t", ["x", "y"], vec![vec![2], vec![2]]);
        assert_eq!(
            entries(&a.minus(&two)),
            vec![(vec![1, 1], 1), (vec![4, 4], 1)]
        );
        // A row only in the subtrahend comes out negative.
        assert_eq!(
            entries(&b.minus(&a)),
            vec![(vec![1, 1], -1), (vec![3, 3], 1), (vec![4, 4], -1)]
        );
        assert!(a.minus(&a).is_empty());
    }

    #[test]
    fn consolidate_adds_up_equal_rows_and_distinct_keeps_positive_ones() {
        let r = weighted(&[(2, 10, 1), (1, 20, 2), (2, 5, -1), (1, 20, 3), (2, 10, -1)]);
        assert!(!r.is_consolidated());
        let r = r.consolidate();
        // (2, 10) cancels out, (1, 20) adds up.
        assert_eq!(entries(&r), vec![(vec![1, 20], 5), (vec![2, 5], -1)]);
        assert!(r.is_consolidated());
        assert_eq!(entries(&r.distinct()), vec![(vec![1, 20], 1)]);
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
    fn record_batch_carries_weights() {
        let dict = Dictionary::new();
        let r = weighted(&[(1, 4, 3), (2, 5, -2)]);
        let batch = r.to_record_batch(&dict).unwrap();
        assert_eq!(batch.schema().field(2).name(), WEIGHT_COLUMN);
        let back = Relation::from_record_batch("r", &batch, &mut Dictionary::new()).unwrap();
        assert_eq!(r, back);

        // A batch without the weight column holds a set.
        let values = batch.project(&[0, 1]).unwrap();
        let set = Relation::from_record_batch("r", &values, &mut Dictionary::new()).unwrap();
        assert_eq!(set.weights, vec![1, 1]);
        assert_eq!(set.cols, r.cols);
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
