// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    context::ResolverContext,
    error::SyntaxError,
    expr::{
        AliasExpr, AntiJoinExpr, AssignExpr, BinaryExpr, CallExpr, CartesianProductExpr,
        DifferenceExpr, DistinctExpr, EquiJoinExpr, Expr, ExprVisitorMut, FixedPointIterExpr,
        FunctionExpr, GroupingExpr, LiteralExpr, ProjectionExpr, SelectionExpr, UnaryExpr,
        UnionExpr, VarExpr,
    },
    stmt::{BlockStmt, ExprStmt, Stmt, StmtVisitorMut, VarStmt},
    util::{Named, Resolvable},
    variable::SCOPES_CAPACITY,
};
use std::{collections::HashMap, iter};

#[derive(Clone, Copy, Debug)]
pub struct VariableMeta {
    initialized: bool,
    slot: usize,
}

impl VariableMeta {
    fn new(slot: usize) -> Self {
        Self {
            initialized: false,
            slot,
        }
    }
}

pub struct ScopeStack<T> {
    inner: Vec<HashMap<String, T>>,
}

impl<T> Default for ScopeStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> ScopeStack<T> {
    pub fn new() -> Self {
        let mut scope_stack = Self {
            inner: Vec::with_capacity(SCOPES_CAPACITY),
        };
        // Create the global scope.
        scope_stack.begin_scope();
        scope_stack
    }
    pub fn just_global(&self) -> bool {
        self.inner.len() == 1
    }
    pub fn begin_scope(&mut self) {
        self.inner.push(HashMap::new());
    }
    pub fn end_scope(&mut self) {
        self.inner.pop();
    }
    pub fn innermost(&self) -> Option<&HashMap<String, T>> {
        self.inner.last()
    }
    pub fn innermost_mut(&mut self) -> Option<&mut HashMap<String, T>> {
        self.inner.last_mut()
    }
    /// Iterates from innermost to outermost scope.
    pub fn iter(&self) -> impl Iterator<Item = &HashMap<String, T>> {
        self.inner.iter().rev()
    }
    /// Iterates from innermost to outermost scope while returning indexes that
    /// work from left to right.
    pub fn indexed_iter(&self) -> impl Iterator<Item = (usize, &HashMap<String, T>)> {
        self.inner.iter().enumerate().rev()
    }
}

pub struct Resolver {}

impl Resolver {
    pub fn new() -> Self {
        Self {}
    }
    pub fn resolve<'a>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a mut Stmt>,
        ctx: VisitorCtx,
    ) -> Result<(), SyntaxError> {
        // Ensure we have a global scope before resolving.
        debug_assert!(ctx.scopes.just_global());
        // We do not call `visit_block` here because the root scope is created
        // in the `ScopeStack` constructor and should remain intact across
        // multiple calls to `resolve`.
        let ret = self.visit_stmts(stmts, ctx);
        // Ensure we have a global scope after resolving.
        debug_assert!(ctx.scopes.just_global());
        ret
    }
    // declare in Lox
    fn declare_var(&mut self, name: &str, ctx: VisitorCtx) -> Result<(), SyntaxError> {
        match ctx.scopes.innermost_mut() {
            Some(scope) => {
                scope.insert(name.to_string(), VariableMeta::new(scope.len()));
                Ok(())
            }
            None => Err(SyntaxError::new("No scope to declare variable in")),
        }
    }
    // define in Lox
    fn define_var(&mut self, name: &String, ctx: VisitorCtx) -> Result<(), SyntaxError> {
        match ctx.scopes.innermost_mut() {
            Some(scope) => match scope.get_mut(name) {
                Some(var) => {
                    var.initialized = true;
                    Ok(())
                }
                None => Err(SyntaxError::new("Variable not declared in innermost scope")),
            },
            None => Err(SyntaxError::new("No scope to find variable to assign to")),
        }
    }
    // resolveLocal in Lox
    fn resolve_var<T: Resolvable + Named>(
        &mut self,
        expr: &mut T,
        ctx: VisitorCtx,
    ) -> Result<(), SyntaxError> {
        for (scope_idx, scope) in ctx.scopes.indexed_iter() {
            if let Some(var) = scope.get(expr.name()) {
                let slot_idx = var.slot;
                expr.set_resolved((scope_idx, slot_idx));
                return Ok(());
            }
        }
        // We have to tolerate unresolved variables because we are in a tuple context.
        // Later on, we can also do static analysis to determine if it is a valid
        // reference to a tuple variable by tracking a relation's schema.
        if ctx.is_tuple_context {
            Ok(())
        } else {
            Err(SyntaxError::new(format!(
                "Variable '{}' not declared",
                expr.name()
            )))
        }
    }
    fn visit_stmts<'a>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a mut Stmt>,
        ctx: VisitorCtx,
    ) -> VisitorResult {
        for stmt in stmts {
            self.visit_stmt(stmt, ctx)?;
        }
        Ok(())
    }
    fn visit_block<'a, F>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a mut Stmt>,
        ctx: VisitorCtx,
        after_new_scope_actions: F,
    ) -> Result<(), SyntaxError>
    where
        F: FnOnce(&mut Self, VisitorCtx) -> Result<(), SyntaxError>,
    {
        ctx.scopes.begin_scope();
        after_new_scope_actions(self, ctx)?;
        self.visit_stmts(stmts, ctx)?;
        ctx.scopes.end_scope();
        Ok(())
    }
}

impl Resolver {
    /// A helper method to visit projection attributes.
    fn visit_projection_attributes(
        &mut self,
        attributes: Option<&mut Vec<(String, Expr)>>,
        ctx: VisitorCtx,
    ) -> VisitorResult {
        ctx.begin_tuple_context();
        let ret = attributes
            .map(|attributes| {
                attributes
                    .iter_mut()
                    .try_for_each(|attribute| self.visit_expr(&mut attribute.1, ctx))
            })
            .unwrap_or(Ok(()));
        ctx.end_tuple_context();
        ret
    }
}

type VisitorResult = Result<(), SyntaxError>;
type VisitorCtx<'a, 'b> = &'a mut ResolverContext<'b>;

impl ExprVisitorMut<VisitorResult, VisitorCtx<'_, '_>> for Resolver {
    fn visit_binary_expr(&mut self, expr: &mut BinaryExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.left, ctx)
            .and_then(|()| self.visit_expr(&mut expr.right, ctx))
    }

    fn visit_unary_expr(&mut self, expr: &mut UnaryExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.operand, ctx)
    }

    fn visit_grouping_expr(&mut self, expr: &mut GroupingExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.expr, ctx)
    }

    fn visit_var_expr(&mut self, expr: &mut VarExpr, ctx: VisitorCtx) -> VisitorResult {
        if let Some(var) = ctx
            .scopes
            .innermost()
            .and_then(|scope| scope.get(&expr.name))
            && !var.initialized
        {
            return Err(SyntaxError::new(
                "Variable referenced in its own initializer",
            ));
        }
        // `resolve_var` returns an error if the variable is not declared.
        self.resolve_var(expr, ctx)
    }

    fn visit_assign_expr(&mut self, expr: &mut AssignExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.value, ctx)?;
        // `resolve_var` returns an error if the variable is not declared.
        self.resolve_var(expr, ctx)
    }

    fn visit_literal_expr(&mut self, expr: &mut LiteralExpr, ctx: VisitorCtx) -> VisitorResult {
        Ok(())
    }

    fn visit_function_expr(&mut self, expr: &mut FunctionExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_block(&mut expr.body.stmts, ctx, |resolver, ctx| {
            for parameter in &expr.parameters {
                resolver.declare_var(parameter, ctx)?;
                resolver.define_var(parameter, ctx)?;
            }
            Ok(())
        })
    }

    fn visit_call_expr(&mut self, expr: &mut CallExpr, ctx: VisitorCtx) -> VisitorResult {
        // TODO: check for arity here just once statically.
        self.visit_expr(&mut expr.callee, ctx)?;
        for arg in &mut expr.arguments {
            self.visit_expr(arg, ctx)?;
        }
        Ok(())
    }

    fn visit_alias_expr(&mut self, expr: &mut AliasExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.relation, ctx)
    }

    fn visit_distinct_expr(&mut self, expr: &mut DistinctExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.relation, ctx)
    }

    fn visit_union_expr(&mut self, expr: &mut UnionExpr, ctx: VisitorCtx) -> VisitorResult {
        // TODO: Typecheck: A union is valid if the column types match and
        // the amount of columns is the same.
        if expr.relations.len() < 2 {
            return Err(SyntaxError::new("Union requires at least two relations"));
        }
        expr.relations
            .iter_mut()
            .try_for_each(|relation| self.visit_expr(relation, ctx))
    }

    fn visit_difference_expr(
        &mut self,
        expr: &mut DifferenceExpr,
        ctx: VisitorCtx,
    ) -> VisitorResult {
        self.visit_expr(&mut expr.right, ctx)
            .and_then(|()| self.visit_expr(&mut expr.left, ctx))
    }

    fn visit_selection_expr(&mut self, expr: &mut SelectionExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.relation, ctx).and_then(|()| {
            ctx.begin_tuple_context();
            let ret = self.visit_expr(&mut expr.condition, ctx);
            ctx.end_tuple_context();
            ret
        })
    }

    fn visit_projection_expr(
        &mut self,
        expr: &mut ProjectionExpr,
        ctx: VisitorCtx,
    ) -> VisitorResult {
        // TODO: statically check that the listed attributes are valid.
        // Implement through returning type information through `VisitorResult`.
        self.visit_expr(&mut expr.relation, ctx)
            .and_then(|()| self.visit_projection_attributes(Some(&mut expr.attributes), ctx))
    }

    fn visit_cartesian_product_expr(
        &mut self,
        expr: &mut CartesianProductExpr,
        ctx: VisitorCtx,
    ) -> VisitorResult {
        self.visit_equi_join_expr(&mut expr.inner, ctx)
    }

    fn visit_equi_join_expr(&mut self, expr: &mut EquiJoinExpr, ctx: VisitorCtx) -> VisitorResult {
        // Maybe: statically check that the listed attributes are valid.
        // Could be implemented through returning type information through `VisitorResult`.
        self.visit_expr(&mut expr.left, ctx)
            .and_then(|()| self.visit_expr(&mut expr.right, ctx))
            .and_then(|()| {
                expr.on.iter_mut().try_for_each(|(left, right)| {
                    ctx.begin_tuple_context();
                    let ret = self
                        .visit_expr(left, ctx)
                        .and_then(|()| self.visit_expr(right, ctx));
                    ctx.end_tuple_context();
                    ret
                })
            })
            .and_then(|()| self.visit_projection_attributes(expr.attributes.as_mut(), ctx))
    }

    fn visit_anti_join_expr(&mut self, expr: &mut AntiJoinExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.left, ctx)
            .and_then(|()| self.visit_expr(&mut expr.right, ctx))
            .and_then(|()| {
                expr.on.iter_mut().try_for_each(|(left, right)| {
                    ctx.begin_tuple_context();
                    let ret = self
                        .visit_expr(left, ctx)
                        .and_then(|()| self.visit_expr(right, ctx));
                    ctx.end_tuple_context();
                    ret
                })
            })
    }

    fn visit_fixed_point_iter_expr(
        &mut self,
        expr: &mut FixedPointIterExpr,
        ctx: VisitorCtx,
    ) -> VisitorResult {
        let exprs = iter::once(&mut expr.accumulator)
            .chain(expr.imports.iter_mut())
            .try_for_each(|variable| self.visit_expr(&mut variable.1, ctx));
        self.visit_block(&mut expr.step.stmts, ctx, |resolver, ctx| {
            iter::once(&expr.accumulator)
                .chain(expr.imports.iter())
                .try_for_each(|variable| {
                    resolver.declare_var(&variable.0, ctx)?;
                    resolver.define_var(&variable.0, ctx)?;
                    Ok(())
                })
        })
    }
}

impl StmtVisitorMut<VisitorResult, VisitorCtx<'_, '_>> for Resolver {
    fn visit_var_stmt(&mut self, stmt: &mut VarStmt, ctx: VisitorCtx) -> VisitorResult {
        self.declare_var(&stmt.name, ctx)
            .and_then(|()| {
                if let Some(expr) = &mut stmt.initializer {
                    self.visit_expr(expr, ctx)
                } else {
                    Ok(())
                }
            })
            .and_then(|()| self.define_var(&stmt.name, ctx))
    }

    fn visit_expr_stmt(&mut self, stmt: &mut ExprStmt, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut stmt.expr, ctx)
    }

    fn visit_block_stmt(&mut self, stmt: &mut BlockStmt, ctx: VisitorCtx) -> VisitorResult {
        self.visit_block(&mut stmt.stmts, ctx, |_resolver, _ctx| Ok(()))
    }
}
