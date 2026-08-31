// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module is concerned with violations and how to report them. Many things
//! are still TBD.

use super::deltas::TableDelta;

/// For each query which is checking a constraint, this reports back identified
/// counterexamples.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Violations {
    /// Contains the counter examples for each unmet constraint. Note that
    /// [`TableRef`](crate::relational::schema::TableRef) refers to a derived
    /// view (defined through a query) rather than a physical base table here.
    inner: Vec<TableDelta>,
}

impl Violations {
    /// Report no violations.
    pub fn empty() -> Self {
        Self {
            inner: Vec::new(), // Does not allocate.
        }
    }
    pub fn is_empty(&self) -> bool {
        self.inner.iter().all(|table_delta| table_delta.is_empty())
    }
    pub fn iter(&self) -> impl Iterator<Item = &TableDelta> {
        self.into_iter()
    }
    pub fn into_inner(self) -> Vec<TableDelta> {
        self.inner
    }
    pub fn extend<I: IntoIterator<Item = TableDelta>>(&mut self, deltas: I) {
        self.inner.extend(deltas);
    }
}

impl IntoIterator for Violations {
    type Item = TableDelta;
    type IntoIter = std::vec::IntoIter<TableDelta>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a Violations {
    type Item = &'a TableDelta;
    type IntoIter = std::slice::Iter<'a, TableDelta>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl std::fmt::Display for Violations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Violations{}",
            if self.is_empty() { " <empty>" } else { "\n" }
        )?;
        for violation in &self.inner {
            write!(f, "{}", violation)?;
        }
        Ok(())
    }
}
