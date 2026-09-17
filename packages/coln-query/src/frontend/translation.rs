// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Translation of a [`LogicalProgram`](super::LogicalProgram) into the query
//! engine's IR.
//!
//! # Naming rules in the IR
//!
//! Each rule becomes a [`VarStmt`](crate::host::stmt::VarStmt) of its own, and
//! the predicate's statement, emitted right after, unions those variables. So a
//! rule's name has to serve as an IR variable name, and
//! [`Identifiable::id`](super::Identifiable::id) is not enough on its own:
//! **two rules of one predicate may share a name**, because a frontend that
//! does not name rules apart from predicates hands the predicate's name to
//! every rule defining it.
//!
//! That case is broken, and silently so. Re-declaring a name shadows it, so
//! `var r = …; var r = …; var p = Union(r, r)` resolves cleanly with _both_
//! union operands bound to the second `r`. The first rule is emitted and then
//! never read, and the predicate quietly loses it.
//!
//! Translation therefore makes each rule's name unique within its predicate,
//! suffixing a counter where it has to. A frontend whose rule names are already
//! distinct — FLIR's are, for provenance — keeps them untouched.
//!
//! The predicate's own name needs no such treatment, and a single-rule
//! predicate may share its rule's name: the predicate's statement comes after
//! the rule's, and [`Resolver`](crate::host::resolver) resolves an initializer
//! before declaring the name it binds, so `var path = …; var path =
//! Output(Union(path))` reads the rule and only then shadows it.
