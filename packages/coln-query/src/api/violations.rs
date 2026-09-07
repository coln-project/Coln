// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module is concerned with violations and how to report them. Many things
//! are still TBD.
//!
//! Violations come in two [interpretations](Interpretation), and they are two
//! types rather than one: a [`ViolationsSet`] says which violations *are*, a
//! [`ViolationsDelta`] says how they *changed*. The rows are identical either
//! way — the same [`TableDelta`]s of ±weighted counterexamples — so nothing but
//! the type keeps a consumer from reading one as the other, and reading a delta
//! as a set is the mistake worth preventing: it reports a repaired constraint as
//! a broken one.
//!
//! Which interpretation a rule's violations carry follows from what the engine
//! does when one occurs. See
//! [`TxOutcome`](super::transaction::TxOutcome), whose two violation arms are
//! where this distinction comes from.

use super::deltas::TableDelta;
use std::marker::PhantomData;

/// Violations that exist: the whole set of them, as of now.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Set;

/// A change in the violations, relative to whatever previous transactions left
/// behind. A positive [`ZWeight`](super::deltas::ZWeight) is a violation that
/// appeared, a negative one a violation that was resolved.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Delta;

/// What a [`Violations`] is a statement about. Implemented by [`Set`] and
/// [`Delta`]; there is no third case.
pub trait Interpretation {
    /// How this interpretation names itself when violations are printed. The
    /// rows alone do not say which one they are, so the label is the only thing
    /// that tells a reader whether a `-1` in the output is bad news or good.
    const LABEL: &'static str;
}

impl Interpretation for Set {
    const LABEL: &'static str = "Violations";
}

impl Interpretation for Delta {
    const LABEL: &'static str = "Violations Delta";
}

pub type ViolationsSet = Violations<Set>;
pub type ViolationsDelta = Violations<Delta>;

/// For each query which is checking a constraint, this reports back identified
/// counterexamples.
///
/// Always named through [`ViolationsSet`] or [`ViolationsDelta`]; the parameter
/// carries no data and exists only so the two cannot be mixed up. See the
/// [module docs](self).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Violations<I: Interpretation> {
    /// Contains the counter examples for each unmet constraint. Note that
    /// [`EntityRef`](crate::relational::schema::EntityRef) refers to a derived
    /// view (defined through a query) rather than a physical base table here.
    inner: Vec<TableDelta>,
    interpretation: PhantomData<I>,
}

impl<I: Interpretation> Violations<I> {
    /// Report no violations.
    pub fn empty() -> Self {
        Self {
            inner: Vec::new(), // Does not allocate.
            interpretation: PhantomData,
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
    pub fn extend<T: IntoIterator<Item = TableDelta>>(&mut self, deltas: T) {
        self.inner.extend(deltas);
    }
}

impl<I: Interpretation> IntoIterator for Violations<I> {
    type Item = TableDelta;
    type IntoIter = std::vec::IntoIter<TableDelta>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, I: Interpretation> IntoIterator for &'a Violations<I> {
    type Item = &'a TableDelta;
    type IntoIter = std::slice::Iter<'a, TableDelta>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<I: Interpretation> std::fmt::Display for Violations<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            I::LABEL,
            if self.is_empty() { " <empty>" } else { "\n" }
        )?;
        for violation in &self.inner {
            write!(f, "{}", violation)?;
        }
        Ok(())
    }
}
