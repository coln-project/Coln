// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A named relation with `u64` columns.
//!
//! `Relation` is the engine's plain in-memory interchange type; Arrow
//! `RecordBatch` is the serialization boundary (see [`crate::io`]). All
//! values are `u64` for now: FLIR rows are row ids plus (eventually
//! dictionary-encoded) literals, and `u64` keys are all the join machinery
//! needs at this stage.

use std::cmp::Ordering;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use arrow::array::{Array, ArrayRef, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub name: String,
    pub col_names: Vec<String>,
    /// Column-major data; all columns have the same length.
    pub cols: Vec<Vec<u64>>,
}

impl Relation {
    pub fn new(
        name: impl Into<String>,
        col_names: impl IntoIterator<Item = impl Into<String>>,
        cols: Vec<Vec<u64>>,
    ) -> Self {
        let col_names: Vec<String> = col_names.into_iter().map(Into::into).collect();
        assert_eq!(col_names.len(), cols.len(), "one name per column");
        if let Some(first) = cols.first() {
            assert!(
                cols.iter().all(|c| c.len() == first.len()),
                "all columns must have the same length"
            );
        }
        Self {
            name: name.into(),
            col_names,
            cols,
        }
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

    /// The `i`-th row as a vector (test/debug helper).
    pub fn row(&self, i: usize) -> Vec<u64> {
        self.cols.iter().map(|c| c[i]).collect()
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

    /// Sort rows lexicographically (all columns, left to right) and drop
    /// duplicate rows. Relations are sets; generators may emit duplicates.
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
            col_names: self.col_names,
            cols,
        }
    }

    /// Build a relation from row-major flat data (`width` values per row).
    pub fn from_flat_rows(
        name: impl Into<String>,
        col_names: impl IntoIterator<Item = impl Into<String>>,
        width: usize,
        flat: &[u64],
    ) -> Self {
        assert!(width > 0, "from_flat_rows needs at least one column");
        assert_eq!(flat.len() % width, 0, "flat data must be whole rows");
        let n = flat.len() / width;
        let mut cols: Vec<Vec<u64>> = (0..width).map(|_| Vec::with_capacity(n)).collect();
        for row in flat.chunks_exact(width) {
            for (c, &x) in row.iter().enumerate() {
                cols[c].push(x);
            }
        }
        Self::new(name, col_names, cols)
    }

    pub fn to_record_batch(&self) -> Result<RecordBatch> {
        let fields: Vec<Field> = self
            .col_names
            .iter()
            .map(|n| Field::new(n, DataType::UInt64, false))
            .collect();
        let arrays: Vec<ArrayRef> = self
            .cols
            .iter()
            .map(|c| Arc::new(UInt64Array::from(c.clone())) as ArrayRef)
            .collect();
        RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays)
            .with_context(|| format!("building record batch for relation {}", self.name))
    }

    /// Convert an Arrow batch back into a `Relation`. All columns must be
    /// non-nullable `UInt64`.
    pub fn from_record_batch(name: impl Into<String>, batch: &RecordBatch) -> Result<Self> {
        let name = name.into();
        let mut col_names = Vec::new();
        let mut cols = Vec::new();
        for (field, array) in batch.schema().fields().iter().zip(batch.columns()) {
            let Some(array) = array.as_any().downcast_ref::<UInt64Array>() else {
                bail!(
                    "relation {name}, column {}: expected UInt64, got {}",
                    field.name(),
                    field.data_type()
                );
            };
            if array.null_count() > 0 {
                bail!(
                    "relation {name}, column {}: nulls not supported",
                    field.name()
                );
            }
            col_names.push(field.name().clone());
            cols.push(array.values().to_vec());
        }
        Ok(Self::new(name, col_names, cols))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_dedup_sorts_and_drops_duplicates() {
        let r = Relation::new("r", ["a", "b"], vec![vec![2, 1, 2, 1], vec![10, 20, 5, 20]]);
        let r = r.sorted_dedup();
        assert_eq!(r.len(), 3);
        assert_eq!(r.row(0), vec![1, 20]);
        assert_eq!(r.row(1), vec![2, 5]);
        assert_eq!(r.row(2), vec![2, 10]);
    }

    #[test]
    fn record_batch_roundtrip() {
        let r = Relation::new("r", ["x", "y"], vec![vec![1, 2, 3], vec![4, 5, 6]]);
        let batch = r.to_record_batch().unwrap();
        let back = Relation::from_record_batch("r", &batch).unwrap();
        assert_eq!(r, back);
    }
}
