// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! What a frontend hands the [`Pipeline`](crate::pipeline::Pipeline).
//!
//! A query program is two things: the [`QueryIr`] to compile, and the
//! [`Catalog`] describing the extensional relations its
//! [`SourceExpr`](crate::relational::expr::SourceExpr) leaves name. This module
//! is deliberately *not* part of [`api`](crate::api) because that module is
//! coln's FLIR frontend, and a second frontend (a Datalog one, say) must be
//! able to implement [`QueryProgram`] independently.

use crate::{
    host::{QueryIr, print},
    relational::catalog::Catalog,
};

/// A compilable query program: the [`QueryIr`] the pipeline rewrites, plus (via
/// [`Catalog`]) what its source leaves refer to.
///
/// Each frontend implements this over its own program type, keeping whatever
/// else it needs — coln's FLIR frontend also tracks the derived view per rule —
/// beside the two things the pipeline asks for.
pub trait QueryProgram: Catalog {
    /// The program's statements, for inspection.
    fn code(&self) -> &QueryIr;

    /// Hand the code over to be rewritten, leaving the catalog behind.
    ///
    /// Every stage of the pipeline *consumes* the code: The optimizer, the
    /// backend's lowering and the resolver each take a [`QueryIr`] and return
    /// the rewritten one, while all of them may still need to ask the catalog
    /// what a source means. Moving the code out rather than borrowing it is
    /// what lets both hold: afterwards this program describes its sources as
    /// before, and [`code`](Self::code) reports the empty program that is left.
    fn take_code(&mut self) -> QueryIr;

    /// This program rendered as an indented node tree, with each source leaf
    /// described by *this* program's catalog. The counterpart of
    /// [`QueryIr::to_tree`], which has no catalog to consult and so can only
    /// name the leaves.
    fn to_tree(&self) -> String
    where
        Self: Sized,
    {
        print::to_tree_with(self.code(), self)
    }
}
