// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    error::SyntaxError,
    host::{
        QueryIr,
        expr::{
            AssignExpr, BinaryExpr, CallExpr, Expr, ExprVisitorMut, FunctionExpr, GetIndexExpr,
            GroupingExpr, LiteralExpr, TupleExpr, UnaryExpr, VarExpr,
        },
        stmt::{BlockStmt, ExprStmt, Stmt, StmtVisitorMut, VarStmt},
        variable::SCOPES_CAPACITY,
    },
    relational::expr::{
        AliasExpr, AntiJoinExpr, CartesianProductExpr, DifferenceExpr, DistinctExpr, EquiJoinExpr,
        FixedPointIterExpr, MultiWayEquiJoinExpr, OutputExpr, ProjectionExpr, RelExpr,
        RelExprVisitorMut, SelectionExpr, SourceExpr, UnionExpr,
    },
};
use std::collections::HashMap;

pub trait Resolvable {
    fn set_resolved(&mut self, resolved: super::variable::VariableSlot);
}

pub trait Named {
    fn name(&self) -> &str;
}

#[derive(Clone, Copy, Debug)]
struct VariableMeta {
    slot: usize,
}

/// One scope's variables, plus how many declarations it has handed slots to.
struct Scope<T> {
    vars: HashMap<String, T>,
    declared: usize,
}

impl<T> Scope<T> {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
            declared: 0,
        }
    }
}

pub struct ScopeStack<T> {
    inner: Vec<Scope<T>>,
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
        self.inner.push(Scope::new());
    }
    pub fn end_scope(&mut self) {
        self.inner.pop();
    }
    /// Hands out the next slot of the innermost scope, counting _declarations_
    /// rather than distinct names.
    ///
    /// Shadowing a name replaces its map entry without growing the map, while
    /// the interpreter pushes a fresh slot for every declaration it executes
    /// ([`Environment::define_var`](super::variable::Environment::define_var)).
    /// Deriving slots from the map's length would therefore drift apart from
    /// the runtime layout at the first shadowed name, and alias every variable
    /// declared after it in that scope.
    pub fn next_slot(&mut self) -> Option<usize> {
        self.inner.last_mut().map(|scope| {
            let slot = scope.declared;
            scope.declared += 1;
            slot
        })
    }
    pub fn innermost(&self) -> Option<&HashMap<String, T>> {
        self.inner.last().map(|scope| &scope.vars)
    }
    pub fn innermost_mut(&mut self) -> Option<&mut HashMap<String, T>> {
        self.inner.last_mut().map(|scope| &mut scope.vars)
    }
    /// Iterates from innermost to outermost scope.
    pub fn iter(&self) -> impl Iterator<Item = &HashMap<String, T>> {
        self.inner.iter().rev().map(|scope| &scope.vars)
    }
    /// Iterates from innermost to outermost scope while returning indexes that
    /// work from left to right.
    pub fn indexed_iter(&self) -> impl Iterator<Item = (usize, &HashMap<String, T>)> {
        self.inner
            .iter()
            .enumerate()
            .rev()
            .map(|(idx, scope)| (idx, &scope.vars))
    }
}

/// A plan that has been through a static resolve pass. Only
/// [`ResolvedCode::from`] mints one, so a backend cannot be handed an
/// unprocessed plan.
#[derive(Clone)]
pub struct ResolvedCode(QueryIr);

impl ResolvedCode {
    /// Run the static pipeline over a raw plan and resolve variable slots.
    pub fn from(code: impl Into<QueryIr>) -> Result<Self, SyntaxError> {
        let mut code = code.into();
        let mut scopes = ScopeStack::new();
        let mut ctx = ResolverContext::new(&mut scopes);
        Resolver::new().resolve(code.iter_mut(), &mut ctx)?;
        Ok(Self(code))
    }
    pub fn as_code(&self) -> &QueryIr {
        &self.0
    }
    pub fn into_code(self) -> QueryIr {
        self.0
    }
}

struct Resolver {}

impl Resolver {
    fn new() -> Self {
        Self {}
    }
    fn resolve<'a>(
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
    /// Binds `name` to a fresh slot of the innermost scope, shadowing whatever
    /// that name stood for before.
    ///
    /// Unlike Lox, which splits this into a declare and a define step to catch
    /// a variable read inside its own initializer, there is nothing to catch
    /// here: initializers are resolved _before_ the name they bind is declared,
    /// so a self-mention reaches the shadowed binding instead. See
    /// [`Resolver::visit_var_stmt`].
    fn declare_var(&mut self, name: &str, ctx: VisitorCtx) -> Result<(), SyntaxError> {
        let Some(slot) = ctx.scopes.next_slot() else {
            return Err(SyntaxError::new("No scope to declare variable in"));
        };
        ctx.scopes
            .innermost_mut()
            .expect("the scope that just handed out a slot")
            .insert(name.to_string(), VariableMeta { slot });
        Ok(())
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
    fn visit_literal_expr(&mut self, expr: &mut LiteralExpr, ctx: VisitorCtx) -> VisitorResult {
        Ok(())
    }

    fn visit_tuple_expr(&mut self, expr: &mut TupleExpr, ctx: VisitorCtx<'_, '_>) -> VisitorResult {
        expr.elements
            .iter_mut()
            .try_for_each(|relation| self.visit_expr(relation, ctx))
    }

    fn visit_get_index_expr(
        &mut self,
        expr: &mut GetIndexExpr,
        ctx: VisitorCtx<'_, '_>,
    ) -> VisitorResult {
        self.visit_expr(&mut expr.target, ctx)
            .and_then(|()| self.visit_expr(&mut expr.index, ctx))
    }

    fn visit_grouping_expr(&mut self, expr: &mut GroupingExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.expr, ctx)
    }

    fn visit_binary_expr(&mut self, expr: &mut BinaryExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.left, ctx)
            .and_then(|()| self.visit_expr(&mut expr.right, ctx))
    }

    fn visit_unary_expr(&mut self, expr: &mut UnaryExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.operand, ctx)
    }

    fn visit_var_expr(&mut self, expr: &mut VarExpr, ctx: VisitorCtx) -> VisitorResult {
        // `resolve_var` returns an error if the variable is not declared.
        self.resolve_var(expr, ctx)
    }

    fn visit_assign_expr(&mut self, expr: &mut AssignExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut expr.value, ctx)?;
        // `resolve_var` returns an error if the variable is not declared.
        self.resolve_var(expr, ctx)
    }

    fn visit_function_expr(&mut self, expr: &mut FunctionExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_block(&mut expr.body.stmts, ctx, |resolver, ctx| {
            for parameter in &expr.parameters {
                resolver.declare_var(parameter, ctx)?;
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

    fn visit_relational_expr(&mut self, expr: &mut RelExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_rel(expr, ctx)
    }
}

impl RelExprVisitorMut<VisitorResult, VisitorCtx<'_, '_>> for Resolver {
    fn visit_source_expr(&mut self, expr: &mut SourceExpr, ctx: VisitorCtx) -> VisitorResult {
        // A source is a plan leaf that names an extensional relation; it carries
        // no variables to resolve.
        Ok(())
    }

    fn visit_output_expr(&mut self, expr: &mut OutputExpr, ctx: VisitorCtx) -> VisitorResult {
        // Output is a pass-through tap; only its inner relation carries variables
        // to resolve. Its id/kind are inert plan data.
        self.visit_expr(&mut expr.relation, ctx)
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

    fn visit_multi_way_equi_join_expr(
        &mut self,
        expr: &mut MultiWayEquiJoinExpr,
        ctx: VisitorCtx<'_, '_>,
    ) -> VisitorResult {
        // The structural invariants (arity, in-bounds and distinct relation
        // indices, at least two occurrences per join variable) are checked here
        // rather than re-derived by every consumer, because the fields are
        // public and a plan may be assembled or rewritten by hand.
        expr.validate()?;

        expr.relations
            .iter_mut()
            .try_for_each(|relation| self.visit_expr(relation, ctx))
            .and_then(|()| {
                expr.on_exprs_mut().try_for_each(|expr| {
                    ctx.begin_tuple_context();
                    let ret = self.visit_expr(expr, ctx);
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
        // The accumulator's initial value is resolved in the enclosing scope.
        self.visit_expr(&mut expr.accumulator.1, ctx)?;
        // Only the accumulator is a step-local binding. Any other relation the
        // step references (an outer variable or a `SourceExpr`) keeps its outer
        // resolution; the backend bridges it into the iteration. This is what
        // lets the step be written as a plain computation without an explicit
        // imports list.
        self.visit_block(&mut expr.step.stmts, ctx, |resolver, ctx| {
            resolver.declare_var(&expr.accumulator.0, ctx)?;
            Ok(())
        })
    }
}

impl StmtVisitorMut<VisitorResult, VisitorCtx<'_, '_>> for Resolver {
    /// Resolves the initializer _before_ declaring the name it binds, so that a
    /// mention of that name inside the initializer reaches the binding this one
    /// shadows, as in Rust and the ML family: `var x = x` rebinds `x` from its
    /// previous value rather than being rejected.
    ///
    /// This mirrors what the interpreter does anyway — it evaluates the
    /// initializer and only then pushes the new slot — so the two agree by
    /// construction instead of by a check.
    fn visit_var_stmt(&mut self, stmt: &mut VarStmt, ctx: VisitorCtx) -> VisitorResult {
        if let Some(expr) = &mut stmt.initializer {
            self.visit_expr(expr, ctx)?;
        }
        self.declare_var(&stmt.name, ctx)
    }

    fn visit_expr_stmt(&mut self, stmt: &mut ExprStmt, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&mut stmt.expr, ctx)
    }

    fn visit_block_stmt(&mut self, stmt: &mut BlockStmt, ctx: VisitorCtx) -> VisitorResult {
        self.visit_block(&mut stmt.stmts, ctx, |_resolver, _ctx| Ok(()))
    }
}

struct ResolverContext<'a> {
    scopes: &'a mut ScopeStack<VariableMeta>,
    is_tuple_context: bool,
}

impl ResolverContext<'_> {
    fn new(scopes: &mut ScopeStack<VariableMeta>) -> ResolverContext<'_> {
        ResolverContext {
            scopes,
            is_tuple_context: false,
        }
    }
    fn begin_tuple_context(&mut self) {
        self.is_tuple_context = true;
    }
    fn end_tuple_context(&mut self) {
        self.is_tuple_context = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{expr::LiteralExpr, variable::VariableSlot};

    fn bind(name: &str, initializer: Expr) -> Stmt {
        Stmt::from(VarStmt {
            name: name.to_owned(),
            initializer: Some(initializer),
        })
    }

    /// The slot a statement's initializer resolved its (sole) variable to.
    fn initializer_slot(stmt: &Stmt) -> Option<VariableSlot> {
        let Stmt::Var(var_stmt) = stmt else {
            return None;
        };
        let Some(Expr::Var(var_expr)) = var_stmt.initializer.as_ref() else {
            return None;
        };
        var_expr.resolved
    }

    #[test]
    fn an_initializer_resolves_to_the_binding_it_shadows() {
        // var x = 1; var x = x; var y = x;
        let code = QueryIr::new(vec![
            bind("x", Expr::from(LiteralExpr::from(1u64))),
            bind("x", Expr::from(VarExpr::new("x"))),
            bind("y", Expr::from(VarExpr::new("x"))),
        ]);
        let resolved = ResolvedCode::from(code).expect("shadowing resolves");
        let stmts: Vec<&Stmt> = resolved.as_code().iter().collect();
        // The shadowing initializer reads the *first* `x`, not itself.
        assert_eq!(initializer_slot(stmts[1]), Some((0, 0)));
        // And the next statement reads the *second* `x`.
        assert_eq!(initializer_slot(stmts[2]), Some((0, 1)));
    }

    #[test]
    fn shadowing_does_not_alias_later_slots() {
        // The bug a declaration counter avoids: with slots taken from the
        // scope's map length, `y` would land on the shadowed `x`'s slot,
        // because shadowing replaces a map entry without growing the map.
        // var x = 1; var x = 2; var y = x; var z = y;
        let code = QueryIr::new(vec![
            bind("x", Expr::from(LiteralExpr::from(1u64))),
            bind("x", Expr::from(LiteralExpr::from(2u64))),
            bind("y", Expr::from(VarExpr::new("x"))),
            bind("z", Expr::from(VarExpr::new("y"))),
        ]);
        let resolved = ResolvedCode::from(code).expect("shadowing resolves");
        let stmts: Vec<&Stmt> = resolved.as_code().iter().collect();
        assert_eq!(
            initializer_slot(stmts[2]),
            Some((0, 1)),
            "y reads the second x"
        );
        assert_eq!(initializer_slot(stmts[3]), Some((0, 2)), "z reads y, not x");
    }

    #[test]
    fn a_variable_that_was_never_declared_is_rejected() {
        // Including the case shadowing might be mistaken for: with the
        // initializer resolved first, `var x = x` has no earlier `x` to reach.
        let code = QueryIr::new(vec![bind("x", Expr::from(VarExpr::new("x")))]);
        let Err(error) = ResolvedCode::from(code) else {
            panic!("'x' is not declared before its own initializer");
        };
        assert_eq!(error.to_string(), "Variable 'x' not declared");
    }
}
