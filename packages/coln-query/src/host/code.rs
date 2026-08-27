// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The host-language program.

use super::{print, stmt::Stmt};
use std::ops::{Deref, DerefMut};

/// A host-language program containing multiple queries.
/// For instance, this is part of what a
/// [`coln-flir` lowering emits](crate::api::query::FlirProgram::from_flat_realm).
///
/// This encodes what a query program is _doing_. _Upon what_ it operates (base
/// tables and their schemas) is defined one layer above in
/// a [`QueryProgram`](crate::program::QueryProgram)'s
/// [`Catalog`](crate::relational::catalog::Catalog)
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueryIr(Vec<Stmt>);

impl QueryIr {
    pub fn new(stmts: Vec<Stmt>) -> Self {
        Self(stmts)
    }

    /// This program rendered as an indented node tree, e.g.
    /// `println!("{}", query_ir.to_tree())`. See [`super::print`].
    ///
    /// [`print::to_tree`] is the same rendering for a bare `[Stmt]`, which is
    /// what a sub-forest (a fixed point's step body or a function's body)
    /// actually is.
    pub fn to_tree(&self) -> String {
        print::to_tree(self)
    }

    /// Append a statement. The *only* structural mutation a program exposes:
    /// a lowering builds one up statement by statement, and nothing else has a
    /// reason to change its length.
    pub fn push(&mut self, stmt: Stmt) {
        self.0.push(stmt);
    }

    pub fn into_stmts(self) -> Vec<Stmt> {
        self.0
    }
}

impl From<Vec<Stmt>> for QueryIr {
    fn from(stmts: Vec<Stmt>) -> Self {
        Self(stmts)
    }
}

impl From<QueryIr> for Vec<Stmt> {
    fn from(code: QueryIr) -> Self {
        code.0
    }
}

impl FromIterator<Stmt> for QueryIr {
    fn from_iter<I: IntoIterator<Item = Stmt>>(stmts: I) -> Self {
        Self(stmts.into_iter().collect())
    }
}

/// Derefs to the *slice*, exactly as [`Vec`] itself does, rather than to the
/// `Vec`: a pass gets `iter`, `iter_mut`, `len` and indexing, while `clear`,
/// `truncate` and `drain` stay off a type whose point is to be a complete
/// program. Appending goes through [`QueryIr::push`].
impl Deref for QueryIr {
    type Target = [Stmt];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for QueryIr {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Deref coercion does not apply at a generic bound, so a `&QueryIr` handed to
/// something taking `impl IntoIterator<Item = &Stmt>` — as the interpreter does
/// — would not compile without this.
impl<'a> IntoIterator for &'a QueryIr {
    type Item = &'a Stmt;
    type IntoIter = std::slice::Iter<'a, Stmt>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut QueryIr {
    type Item = &'a mut Stmt;
    type IntoIter = std::slice::IterMut<'a, Stmt>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl IntoIterator for QueryIr {
    type Item = Stmt;
    type IntoIter = std::vec::IntoIter<Stmt>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
