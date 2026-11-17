// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::operator::Operator;
use super::resolver::{Named, Resolvable};
use crate::{
    host::stmt::BlockStmt, host::variable::VariableSlot, impl_from_auto_box,
    relational::expr::RelExpr, scalarial::ScalarTypedValue,
};
use std::fmt::Debug;

/// Host-language expression. Evaluates to a `Value` (scalar / relation /
/// function / tuple). Backend-agnostic — relations flow through as opaque values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Expr {
    Literal(Box<LiteralExpr>),
    Tuple(Box<TupleExpr>),
    GetIndex(Box<GetIndexExpr>),
    Grouping(Box<GroupingExpr>),
    Binary(Box<BinaryExpr>),
    Unary(Box<UnaryExpr>),
    Var(Box<VarExpr>),
    Assign(Box<AssignExpr>),
    Call(Box<CallExpr>),
    Function(Box<FunctionExpr>),
    /// The single bridge into the relational layer: a relational operator is
    /// *also* a host expression (bindable to a var, placeable in a tuple, …).
    ///
    /// The only variant that does not box its payload, because there is nothing
    /// to gain by it: [`RelExpr`] boxes each of *its* payloads, so it is already
    /// a discriminant plus a pointer, and rustc packs this enum's discriminant
    /// into the unused values of that one. `Expr` is the same size either way,
    /// while every relational node saves an allocation and an indirection.
    Relational(RelExpr),
}

impl_from_auto_box! {
    Expr,
    (Expr::Literal, LiteralExpr),
    (Expr::Tuple, TupleExpr),
    (Expr::GetIndex, GetIndexExpr),
    (Expr::Grouping, GroupingExpr),
    (Expr::Binary, BinaryExpr),
    (Expr::Unary, UnaryExpr),
    (Expr::Var, VarExpr),
    (Expr::Assign, AssignExpr),
    (Expr::Call, CallExpr),
    (Expr::Function, FunctionExpr)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiteralExpr {
    pub value: Literal,
}

impl<T: Into<Literal>> From<T> for LiteralExpr {
    fn from(value: T) -> Self {
        Self {
            value: value.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TupleExpr {
    pub elements: Vec<Expr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetIndexExpr {
    pub target: Expr,
    pub index: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupingExpr {
    pub expr: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BinaryExpr {
    pub operator: Operator,
    pub left: Expr,
    pub right: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnaryExpr {
    pub operator: Operator,
    pub operand: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VarExpr {
    pub name: String,
    pub resolved: Option<VariableSlot>,
}

impl VarExpr {
    pub fn new<T: Into<String>>(name: T) -> Self {
        Self {
            name: name.into(),
            resolved: None,
        }
    }
}

impl Resolvable for VarExpr {
    fn set_resolved(&mut self, info: VariableSlot) {
        self.resolved = Some(info);
    }
}

impl Named for VarExpr {
    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignExpr {
    pub name: String,
    pub value: Expr,
    pub resolved: Option<VariableSlot>,
}

impl AssignExpr {
    pub fn new<T: Into<String>>(name: T, value: Expr) -> Self {
        Self {
            name: name.into(),
            value,
            resolved: None,
        }
    }
}

impl Resolvable for AssignExpr {
    fn set_resolved(&mut self, info: VariableSlot) {
        self.resolved = Some(info);
    }
}

impl Named for AssignExpr {
    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionExpr {
    pub parameters: Vec<String>,
    pub body: BlockStmt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallExpr {
    pub callee: Expr,
    pub arguments: Vec<Expr>,
}

/// A literal is any scalar value, written down in an expression.
pub type Literal = ScalarTypedValue;

/// Read-only visitor. See [`ExprVisitorOwn`] for which of the three families a
/// given pass belongs in.
pub trait ExprVisitor<T, C> {
    fn visit_expr(&mut self, expr: &Expr, ctx: C) -> T {
        match expr {
            Expr::Literal(expr) => self.visit_literal_expr(expr, ctx),
            Expr::Tuple(expr) => self.visit_tuple_expr(expr, ctx),
            Expr::GetIndex(expr) => self.visit_get_index_expr(expr, ctx),
            Expr::Grouping(expr) => self.visit_grouping_expr(expr, ctx),
            Expr::Binary(expr) => self.visit_binary_expr(expr, ctx),
            Expr::Unary(expr) => self.visit_unary_expr(expr, ctx),
            Expr::Var(expr) => self.visit_var_expr(expr, ctx),
            Expr::Assign(expr) => self.visit_assign_expr(expr, ctx),
            Expr::Function(expr) => self.visit_function_expr(expr, ctx),
            Expr::Call(expr) => self.visit_call_expr(expr, ctx),
            Expr::Relational(expr) => self.visit_relational_expr(expr, ctx),
        }
    }
    fn visit_literal_expr(&mut self, expr: &LiteralExpr, ctx: C) -> T;
    fn visit_tuple_expr(&mut self, expr: &TupleExpr, ctx: C) -> T;
    fn visit_get_index_expr(&mut self, expr: &GetIndexExpr, ctx: C) -> T;
    fn visit_grouping_expr(&mut self, expr: &GroupingExpr, ctx: C) -> T;
    fn visit_binary_expr(&mut self, expr: &BinaryExpr, ctx: C) -> T;
    fn visit_unary_expr(&mut self, expr: &UnaryExpr, ctx: C) -> T;
    fn visit_var_expr(&mut self, expr: &VarExpr, ctx: C) -> T;
    fn visit_assign_expr(&mut self, expr: &AssignExpr, ctx: C) -> T;
    fn visit_function_expr(&mut self, expr: &FunctionExpr, ctx: C) -> T;
    fn visit_call_expr(&mut self, expr: &CallExpr, ctx: C) -> T;
    /// Bridge into the relational layer. A backend that also implements
    /// [`RelExprVisitor`](crate::relational::expr::RelExprVisitor) delegates
    /// here to its `visit_rel` router.
    fn visit_relational_expr(&mut self, expr: &RelExpr, ctx: C) -> T;
}

/// Annotating visitor. See [`ExprVisitorOwn`] for which of the three families a
/// given pass belongs in.
pub trait ExprVisitorMut<T, C> {
    fn visit_expr(&mut self, expr: &mut Expr, ctx: C) -> T {
        match expr {
            Expr::Literal(expr) => self.visit_literal_expr(expr, ctx),
            Expr::Tuple(expr) => self.visit_tuple_expr(expr, ctx),
            Expr::GetIndex(expr) => self.visit_get_index_expr(expr, ctx),
            Expr::Grouping(expr) => self.visit_grouping_expr(expr, ctx),
            Expr::Binary(expr) => self.visit_binary_expr(expr, ctx),
            Expr::Unary(expr) => self.visit_unary_expr(expr, ctx),
            Expr::Var(expr) => self.visit_var_expr(expr, ctx),
            Expr::Assign(expr) => self.visit_assign_expr(expr, ctx),
            Expr::Function(expr) => self.visit_function_expr(expr, ctx),
            Expr::Call(expr) => self.visit_call_expr(expr, ctx),
            Expr::Relational(expr) => self.visit_relational_expr(expr, ctx),
        }
    }
    fn visit_literal_expr(&mut self, expr: &mut LiteralExpr, ctx: C) -> T;
    fn visit_tuple_expr(&mut self, expr: &mut TupleExpr, ctx: C) -> T;
    fn visit_get_index_expr(&mut self, expr: &mut GetIndexExpr, ctx: C) -> T;
    fn visit_grouping_expr(&mut self, expr: &mut GroupingExpr, ctx: C) -> T;
    fn visit_binary_expr(&mut self, expr: &mut BinaryExpr, ctx: C) -> T;
    fn visit_unary_expr(&mut self, expr: &mut UnaryExpr, ctx: C) -> T;
    fn visit_var_expr(&mut self, expr: &mut VarExpr, ctx: C) -> T;
    fn visit_assign_expr(&mut self, expr: &mut AssignExpr, ctx: C) -> T;
    fn visit_function_expr(&mut self, expr: &mut FunctionExpr, ctx: C) -> T;
    fn visit_call_expr(&mut self, expr: &mut CallExpr, ctx: C) -> T;
    /// Bridge into the relational layer. See [`ExprVisitor::visit_relational_expr`].
    fn visit_relational_expr(&mut self, expr: &mut RelExpr, ctx: C) -> T;
}

/// Restructuring visitor: it consumes the tree and produces a new one.
///
/// # Which visitor family a pass belongs in
///
/// The three families differ in what a pass is allowed to *do*, not merely in
/// how it borrows:
///
/// - [`ExprVisitor`] (`&`) — **read**. The pass derives something from the tree
///   and leaves it untouched (type resolution, printing, interpretation).
/// - [`ExprVisitorMut`] (`&mut`) — **annotate**. The pass fills fields in place
///   but never changes the tree's *shape*; every node stays the node it was
///   (the [`Resolver`](crate::host::resolver) filling variable slots).
/// - [`ExprVisitorOwn`] (owned) — **restructure**. A node may be replaced by a
///   differently shaped one, or become the child of a node that did not exist
///   before (lowering a multi-way join into a fold of binary ones).
///
/// The rule follows from what Rust permits. Restructuring means moving children
/// out of their parent and re-parenting them, and one cannot move out of a
/// `&mut`. A `&mut` pass would have to leave a placeholder behind for every
/// child it takes, which needs a dummy node the AST does not have, and which
/// leaves a half-rewritten tree behind when the pass fails part-way through. An
/// owned pass just moves values, and a failure drops the partial result.
///
/// # Why the payloads arrive boxed
///
/// Every [`Expr`] variant boxes its payload, so unboxing here would make a pass
/// pay a deallocation plus an allocation for each node it walks over — including
/// the overwhelming majority it does not rewrite at all. Handing out the `Box`
/// instead lets an unchanged node go straight back into its enum
/// (`Ok(expr.into())`, via the `From<Box<XxxExpr>>` impls) for free, while a
/// pass that *does* consume a node still writes `let XxxExpr { .. } = *expr;`
/// exactly as it would have otherwise.
///
/// Recursing into a child keeps the box, too, because a field can be moved out
/// of a `Box`'s contents and written back:
///
/// ```ignore
/// fn visit_grouping_expr(&mut self, mut expr: Box<GroupingExpr>, ctx: C) -> T {
///     expr.expr = self.visit_expr(expr.expr, ctx)?;
///     Ok(expr.into())
/// }
/// ```
pub trait ExprVisitorOwn<T, C> {
    fn visit_expr(&mut self, expr: Expr, ctx: C) -> T {
        match expr {
            Expr::Literal(expr) => self.visit_literal_expr(expr, ctx),
            Expr::Tuple(expr) => self.visit_tuple_expr(expr, ctx),
            Expr::GetIndex(expr) => self.visit_get_index_expr(expr, ctx),
            Expr::Grouping(expr) => self.visit_grouping_expr(expr, ctx),
            Expr::Binary(expr) => self.visit_binary_expr(expr, ctx),
            Expr::Unary(expr) => self.visit_unary_expr(expr, ctx),
            Expr::Var(expr) => self.visit_var_expr(expr, ctx),
            Expr::Assign(expr) => self.visit_assign_expr(expr, ctx),
            Expr::Function(expr) => self.visit_function_expr(expr, ctx),
            Expr::Call(expr) => self.visit_call_expr(expr, ctx),
            Expr::Relational(expr) => self.visit_relational_expr(expr, ctx),
        }
    }
    fn visit_literal_expr(&mut self, expr: Box<LiteralExpr>, ctx: C) -> T;
    fn visit_tuple_expr(&mut self, expr: Box<TupleExpr>, ctx: C) -> T;
    fn visit_get_index_expr(&mut self, expr: Box<GetIndexExpr>, ctx: C) -> T;
    fn visit_grouping_expr(&mut self, expr: Box<GroupingExpr>, ctx: C) -> T;
    fn visit_binary_expr(&mut self, expr: Box<BinaryExpr>, ctx: C) -> T;
    fn visit_unary_expr(&mut self, expr: Box<UnaryExpr>, ctx: C) -> T;
    fn visit_var_expr(&mut self, expr: Box<VarExpr>, ctx: C) -> T;
    fn visit_assign_expr(&mut self, expr: Box<AssignExpr>, ctx: C) -> T;
    fn visit_function_expr(&mut self, expr: Box<FunctionExpr>, ctx: C) -> T;
    fn visit_call_expr(&mut self, expr: Box<CallExpr>, ctx: C) -> T;
    /// Bridge into the relational layer. See [`ExprVisitor::visit_relational_expr`].
    ///
    /// The one payload that does not arrive boxed, because [`Expr::Relational`]
    /// does not box it either.
    fn visit_relational_expr(&mut self, expr: RelExpr, ctx: C) -> T;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_a_relational_operator_in_an_expr_is_free() {
        // Why `Relational` is the one variant that does not box its payload:
        // `RelExpr` is a discriminant plus a pointer, and rustc packs `Expr`'s
        // discriminant into the values that one does not use. Inlining it costs
        // no space and saves an allocation per relational node. If this ever
        // stops holding, the trade-off is worth revisiting rather than keeping
        // by habit.
        assert_eq!(size_of::<Expr>(), size_of::<RelExpr>());
        assert_eq!(size_of::<Expr>(), size_of::<Box<LiteralExpr>>() * 2);
    }
}
