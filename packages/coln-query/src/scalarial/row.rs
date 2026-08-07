// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    error::{BuildError, RuntimeError},
    host::variable::Value,
    host::{HostInterpreter, InterpreterContext, ScalarHost, expr::Expr},
};

/// Row-at-a-time scalar evaluation: one tuple context in, one [`Value`] out
/// (fallibly). The protocol the DBSP and eager-batch backends feed.
///
/// The DBSP backend is bound to this protocol for more than data-shape reasons:
/// an incremental circuit pushes *deltas*, which are typically a handful of
/// tuples, and vectorization only pays off to the extent that a batch amortizes
/// dispatch over its elements. At small `N` that benefit largely disappears, so
/// row-at-a-time is the right protocol here even once a
/// [`ColumnScalarEngine`](super::ColumnScalarEngine) exists.
///
/// The `Clone + 'static` bounds (on the engine and its [`Program`](Self::Program))
/// let the eval backend capture a compiled program into the per-tuple closures it
/// stores in a circuit — so a backend only needs `E: RowScalarEngine`, never a
/// pile of closure bounds. Any real engine (a ZST tree-walker, an `Rc`-backed
/// bytecode VM) satisfies them cheaply.
pub trait RowScalarEngine: Clone + 'static {
    /// A scalar expression prepared for repeated evaluation (the AST itself for
    /// the tree-walker, a bytecode program for a future VM).
    type Program: Clone + 'static;

    /// Prepare a scalar expression. Called once, off the per-tuple hot path
    /// (at [`RelExprVisitor`](crate::relational::expr::RelExprVisitor) time).
    fn compile(&self, expr: &Expr) -> Result<Self::Program, BuildError>;

    /// Evaluate a prepared program against a single tuple's context.
    /// Called once per tuple, that is, on the hot path!
    fn run(
        &self,
        program: &Self::Program,
        ctx: &mut InterpreterContext,
    ) -> Result<Value, RuntimeError>;
}

/// The tree-walking row engine: it runs the AST directly, so a "compiled"
/// program is just the expression itself. Today's behavior, extracted verbatim
/// behind the [`RowScalarEngine`] seam.
#[derive(Clone, Copy, Default)]
pub struct TreeWalk;

impl RowScalarEngine for TreeWalk {
    type Program = Expr;

    fn compile(&self, expr: &Expr) -> Result<Self::Program, BuildError> {
        Ok(expr.clone())
    }

    fn run(
        &self,
        program: &Self::Program,
        ctx: &mut InterpreterContext,
    ) -> Result<Value, RuntimeError> {
        let mut host = ScalarHost;
        host.evaluate(program, ctx)
            // No From<BuildError> for RuntimeError as this is just due to
            // reusing the host language for the RowScalarEngine for now.
            .map_err(|build_error| RuntimeError::new(build_error.message))
    }
}
