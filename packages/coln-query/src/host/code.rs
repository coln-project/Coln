// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The host-language program.

use super::{print, stmt::Stmt};
use std::ops::{Deref, DerefMut};

/// A host-language program: the statements a `coln-flir` lowering emits, before
/// any static pass has run over them.
///
/// A newtype rather than an alias for `Vec<Stmt>`, so that the crate's central
/// type carries its own methods — [`to_tree`](Self::to_tree) needs no trait in
/// scope — and so that an arbitrary vector of statements cannot be passed where
/// a whole program is meant. What a program is *about* (its base tables and
/// their schemas) lives one layer up, next to it, in the api layer's
/// `QueryProgram`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Code(Vec<Stmt>);

impl Code {
    pub fn new(stmts: Vec<Stmt>) -> Self {
        Self(stmts)
    }

    /// This program rendered as an indented node tree, e.g.
    /// `println!("{}", code.to_tree())`. See [`super::print`].
    ///
    /// Inherent rather than a trait method, so that reaching for it costs no
    /// import. [`print::to_tree`] is the same rendering for a bare `[Stmt]`,
    /// which is what a sub-forest — a fixed point's step body, a function's
    /// body — actually is.
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

impl From<Vec<Stmt>> for Code {
    fn from(stmts: Vec<Stmt>) -> Self {
        Self(stmts)
    }
}

impl From<Code> for Vec<Stmt> {
    fn from(code: Code) -> Self {
        code.0
    }
}

impl FromIterator<Stmt> for Code {
    fn from_iter<I: IntoIterator<Item = Stmt>>(stmts: I) -> Self {
        Self(stmts.into_iter().collect())
    }
}

/// Derefs to the *slice*, exactly as [`Vec`] itself does, rather than to the
/// `Vec`: a pass gets `iter`, `iter_mut`, `len` and indexing, while `clear`,
/// `truncate` and `drain` stay off a type whose point is to be a complete
/// program. Appending goes through [`Code::push`].
impl Deref for Code {
    type Target = [Stmt];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Code {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Deref coercion does not apply at a generic bound, so a `&Code` handed to
/// something taking `impl IntoIterator<Item = &Stmt>` — as the interpreter does
/// — would not compile without this.
impl<'a> IntoIterator for &'a Code {
    type Item = &'a Stmt;
    type IntoIter = std::slice::Iter<'a, Stmt>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut Code {
    type Item = &'a mut Stmt;
    type IntoIter = std::slice::IterMut<'a, Stmt>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl IntoIterator for Code {
    type Item = Stmt;
    type IntoIter = std::vec::IntoIter<Stmt>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
