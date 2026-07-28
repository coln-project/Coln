// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! So far just stubs for a columnar scalar engine.

use crate::{
    error::{BuildError, RuntimeError},
    host::expr::Expr,
};

/// Vectorized scalar evaluation: a column batch in, a column / selection mask out.
///
/// PROVISIONAL: there is no implementor yet, and [`ColumnBatch`] / [`Column`] are
/// placeholders whose real shape lands with the vectorized data path. It is
/// declared now only to mark the seam symmetric to [`super::RowScalarEngine`].
///
/// Two constraints shape any implementor, and are recorded here so they are not
/// rediscovered later:
///
/// - **Selection vectors, not jumps.** Short-circuit `and` / `or`, conditionals,
///   and null-skipping cannot branch, because a batch holds rows that disagree
///   about which way to go. A vectorized walk instead recurses into a subtree
///   carrying a *narrowed* set of live row indices — evaluate the left operand,
///   collect the rows still in play, and evaluate the right operand for those
///   only. [`ColumnBatch`] is where that carrier lives.
/// - **Intermediates get materialized.** Every node writes an `N`-element buffer,
///   whereas a row engine keeps intermediates in registers, so vectorization
///   trades interpretive overhead for memory traffic. Note that a bytecode VM
///   does *not* address this — only operator fusion or compilation does — so the
///   materialization cost is an argument for a JIT, never an argument for
///   bytecode on top of a vectorized walk.
pub trait ColumnScalarEngine {
    type Program;

    fn compile(&self, expr: &Expr) -> Result<Self::Program, BuildError>;

    fn run(
        &self,
        program: &Self::Program,
        batch: &mut ColumnBatch<'_>,
    ) -> Result<Column, RuntimeError>;
}

#[derive(Clone, Copy, Default)]
pub struct VectorizedScalarEngine {}

impl ColumnScalarEngine for VectorizedScalarEngine {
    type Program = Expr;

    fn compile(&self, expr: &Expr) -> Result<Self::Program, BuildError> {
        todo!()
    }

    fn run(
        &self,
        program: &Self::Program,
        batch: &mut ColumnBatch<'_>,
    ) -> Result<Column, RuntimeError> {
        todo!()
    }
}

/// PROVISIONAL placeholder — a batch of columnar tuple data fed to a
/// [`ColumnScalarEngine`]. Its real shape (Arrow-style arrays, selection vectors,
/// …) lands with the vectorized data path.
pub struct ColumnBatch<'a> {
    _marker: std::marker::PhantomData<&'a ()>,
}

/// PROVISIONAL placeholder — a column / selection mask produced by vectorized
/// evaluation.
pub struct Column;
