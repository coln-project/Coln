// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The host-language evaluator, shared by every eval backend.
//!
//! Evaluating the host layer (literals, arithmetic, variables, tuples,
//! functions, blocks) to a [`Value`] is entirely backend-agnostic — it only
//! touches the [`InterpreterContext`] and recurses; it never looks at streams,
//! Z-sets, or a scalar engine. The one backend-specific host node is the bridge
//! into the relational layer ([`Expr::Relational`]).
//!
//! So the host evaluation lives here once, as the provided methods of
//! [`HostInterpreter`], and each eval backend supplies only the single required
//! [`visit_relational_expr`](HostInterpreter::visit_relational_expr) bridge
//! (typically `self.visit_rel(..)` against its own
//! [`RelExprVisitor`](crate::relational::expr::RelExprVisitor) impl). A pure
//! scalar context uses [`ScalarHost`], whose bridge is `unreachable!` because —
//! by the host/relational split invariant — a scalar fragment never contains a
//! relational operator.
//!
//! This is deliberately a *separate* trait from the generic
//! [`ExprVisitor`](crate::host::expr::ExprVisitor) dispatch contract: baking eval
//! bodies onto that generic trait would pin its `T` to `Value` and lock out the
//! SQL transpiler (whose `T = Sql`). Here `T` is fixed to `Value` on purpose.

use super::{
    expr::{
        AssignExpr, BinaryExpr, CallExpr, Expr, FunctionExpr, GetIndexExpr, GroupingExpr,
        LiteralExpr, TupleExpr, UnaryExpr, VarExpr,
    },
    function::new_function,
    operator::Operator,
    stmt::{BlockStmt, ExprStmt, Stmt, VarStmt},
    tuple::Tuple,
    variable::{Environment, Value, all_values},
};
use crate::{error::BuildError, relational::expr::RelExpr, scalarial::ScalarTypedValue};
use std::{cmp::Ordering, collections::HashMap};

pub type EvalResult = Result<Value, BuildError>;
pub type StmtResult = Result<Option<Value>, BuildError>;

/// Orders two values of the same scalar type, or `None` for two nulls, which
/// are equal to each other but unordered. Any other pair is incomparable.
fn compare(left: &Value, right: &Value) -> Result<Option<Ordering>, BuildError> {
    Ok(Some(match (left, right) {
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Uint(a), Value::Uint(b)) => a.cmp(b),
        (Value::Iint(a), Value::Iint(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Char(a), Value::Char(b)) => a.cmp(b),
        (Value::Null(()), Value::Null(())) => return Ok(None),
        // Not a wildcard, so a new variant fails to compile here until it is
        // decided whether it is comparable.
        (all_values!(), _) => {
            return Err(BuildError::new(format!(
                "expected comparable type, got: {left:?} and {right:?}"
            )));
        }
    }))
}

/// Like [`compare`], but for the operators that require an order, which
/// nulls lack.
fn order(left: &Value, right: &Value) -> Result<Ordering, BuildError> {
    compare(left, right)?.ok_or_else(|| {
        BuildError::new(format!(
            "expected ordered type, got: {left:?} and {right:?}"
        ))
    })
}

/// Applies the checked integer method `$op` (`checked_add`, `checked_div`, …)
/// to two numbers of the same type. Evaluates to an [`EvalResult`], which is
/// an error for operands that are no numbers of the same type, and for an
/// overflow or a division by zero, where `$op` yields `None`.
macro_rules! arithmetic {
    ($left:expr, $right:expr, $op:ident) => {{
        let (left, right): (&Value, &Value) = ($left, $right);
        let result: Result<Option<Value>, BuildError> = match (left, right) {
            (Value::Iint(a), Value::Iint(b)) => Ok(a.$op(*b).map(Value::Iint)),
            (Value::Uint(a), Value::Uint(b)) => Ok(a.$op(*b).map(Value::Uint)),
            // Not a wildcard, so a new variant fails to compile here until
            // it is decided whether it is a number.
            (all_values!(), _) => Err(BuildError::new(format!(
                "expected number type, got: {left:?} and {right:?}"
            ))),
        };
        result.and_then(|value| {
            value.ok_or_else(|| {
                BuildError::new(format!(
                    "arithmetic overflow or division by zero in {}: {left:?} and {right:?}",
                    stringify!($op)
                ))
            })
        })
    }};
}

macro_rules! assert_type {
    ($value:expr, $variant:path) => {
        match $value {
            $variant(inner) => Ok(inner),
            _ => Err(BuildError::new(format!(
                "expected {} type, got: {:?}",
                stringify!($variant:path),
                $value
            ))),
        }
    };
}
// Re-exported so the relational backends (which also assert `Value` variants)
// can share the exact same macro.
pub(crate) use assert_type;

/// Only `null` and `false` are falsy, everything else is truthy.
pub(crate) fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null(()) => false,
        Value::Bool(value) => *value,
        // Spelled out rather than `all_values!()`, whose `Null` and `Bool`
        // would be unreachable here, causing rustc warnings.
        Value::String(_)
        | Value::Uint(_)
        | Value::Iint(_)
        | Value::Char(_)
        | Value::Function(_)
        | Value::Relation(_)
        | Value::Tuple(_) => true,
    }
}

/// The host-language evaluator. All host expression and statement evaluation is
/// provided here; a backend supplies only [`Self::visit_relational_expr`],
/// the bridge into its relational layer.
pub trait HostInterpreter {
    /// The one required method: evaluate a relational operator. Eval backends
    /// wire this to `self.visit_rel(expr, ctx)` against their `RelExprVisitor`.
    /// Never reached from a pure scalar fragment (see [`ScalarHost`]).
    fn visit_relational_expr(&mut self, expr: &RelExpr, ctx: &mut InterpreterContext)
    -> EvalResult;

    /// Interpret a sequence of statements at the top level (the root scope is
    /// created by the `Environment` constructor and kept across calls, so this
    /// does not open a new scope).
    fn interpret<'a>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a Stmt>,
        ctx: &mut InterpreterContext,
    ) -> StmtResult {
        debug_assert!(ctx.environment.just_global());
        let ret = self.visit_stmts(stmts, ctx);
        debug_assert!(ctx.environment.just_global());
        ret
    }

    /// Evaluate a single (scalar) expression.
    fn evaluate(&mut self, expr: &Expr, ctx: &mut InterpreterContext) -> EvalResult {
        self.visit_expr(expr, ctx)
    }

    fn visit_expr(&mut self, expr: &Expr, ctx: &mut InterpreterContext) -> EvalResult {
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

    fn visit_literal_expr(
        &mut self,
        expr: &LiteralExpr,
        _ctx: &mut InterpreterContext,
    ) -> EvalResult {
        Ok(Value::from(expr.value.clone()))
    }

    fn visit_tuple_expr(&mut self, expr: &TupleExpr, ctx: &mut InterpreterContext) -> EvalResult {
        let values = expr
            .elements
            .iter()
            .map(|expr| self.visit_expr(expr, ctx))
            .collect::<Result<Vec<Value>, _>>()?;
        Ok(Value::from(Tuple::from(values)))
    }

    fn visit_get_index_expr(
        &mut self,
        expr: &GetIndexExpr,
        ctx: &mut InterpreterContext,
    ) -> EvalResult {
        let target = self.visit_expr(&expr.target, ctx)?;
        let index = self.visit_expr(&expr.index, ctx)?;

        if let Value::Uint(idx) = index {
            match target {
                Value::Tuple(tuple) => {
                    let idx = idx as usize;
                    if idx < tuple.len() {
                        Ok(tuple.get(idx).clone())
                    } else {
                        Err(BuildError::new(format!(
                            "{} at index {idx} is out of bounds.",
                            tuple.display_compact()
                        )))
                    }
                }
                _ => Err(BuildError::new(format!(
                    "Invalid index operation on {target}."
                ))),
            }
        } else {
            Err(BuildError::new(format!(
                "Index {index} does not evaluate to an uint"
            )))
        }
    }

    fn visit_grouping_expr(
        &mut self,
        expr: &GroupingExpr,
        ctx: &mut InterpreterContext,
    ) -> EvalResult {
        self.visit_expr(&expr.expr, ctx)
    }

    fn visit_binary_expr(&mut self, expr: &BinaryExpr, ctx: &mut InterpreterContext) -> EvalResult {
        if let Operator::And | Operator::Or = expr.operator {
            let left = is_truthy(&self.visit_expr(&expr.left, ctx)?);
            if expr.operator == Operator::And && left || expr.operator == Operator::Or && !left {
                let right = is_truthy(&self.visit_expr(&expr.right, ctx)?);
                Ok(Value::Bool(right))
            } else {
                Ok(Value::Bool(left))
            }
        } else {
            let left = self.visit_expr(&expr.left, ctx)?;
            let right = self.visit_expr(&expr.right, ctx)?;
            match expr.operator {
                Operator::Equal => Ok(Value::Bool(
                    compare(&left, &right)?.is_none_or(Ordering::is_eq),
                )),
                Operator::NotEqual => Ok(Value::Bool(
                    !compare(&left, &right)?.is_none_or(Ordering::is_eq),
                )),
                Operator::Less => Ok(Value::Bool(order(&left, &right)?.is_lt())),
                Operator::LessEqual => Ok(Value::Bool(order(&left, &right)?.is_le())),
                Operator::Greater => Ok(Value::Bool(order(&left, &right)?.is_gt())),
                Operator::GreaterEqual => Ok(Value::Bool(order(&left, &right)?.is_ge())),
                Operator::Addition => {
                    if let (Value::String(left), Value::String(right)) = (&left, &right) {
                        return Ok(Value::String(format!("{left}{right}")));
                    }
                    arithmetic!(&left, &right, checked_add)
                }
                Operator::Subtraction => arithmetic!(&left, &right, checked_sub),
                Operator::Multiplication => arithmetic!(&left, &right, checked_mul),
                Operator::Division => arithmetic!(&left, &right, checked_div),
                _ => Err(BuildError::new(format!(
                    "unsupported (eager) binary operator: {:?}",
                    expr.operator
                ))),
            }
        }
    }

    fn visit_unary_expr(&mut self, expr: &UnaryExpr, ctx: &mut InterpreterContext) -> EvalResult {
        let operand = self.visit_expr(&expr.operand, ctx)?;
        match expr.operator {
            Operator::Subtraction => match operand {
                Value::Iint(value) => Ok(Value::Iint(-value)),
                _ => Err(BuildError::new(format!(
                    "expected signed int, got: {operand:?}",
                ))),
            },
            Operator::Not => Ok(Value::Bool(!is_truthy(&operand))),
            _ => Err(BuildError::new(format!(
                "unsupported unary operator: {:?}",
                expr.operator
            ))),
        }
    }

    fn visit_var_expr(&mut self, expr: &VarExpr, ctx: &mut InterpreterContext) -> EvalResult {
        let name = &expr.name;
        ctx.tuple_vars.get(name).map_or_else(
            || {
                let resolved = expr
                    .resolved
                    .as_ref()
                    .unwrap_or_else(|| panic!("Unresolved variable '{name}'."));
                Ok(ctx.environment.lookup_var(resolved).clone())
            },
            |value| Ok(Value::from(value.clone())),
        )
    }

    fn visit_assign_expr(&mut self, expr: &AssignExpr, ctx: &mut InterpreterContext) -> EvalResult {
        let name = &expr.name;
        self.visit_expr(&expr.value, ctx).inspect(|value| {
            let resolved = expr
                .resolved
                .as_ref()
                .unwrap_or_else(|| panic!("Unresolved variable '{name}'."));
            ctx.environment.assign_var(resolved, value.clone());
        })
    }

    fn visit_function_expr(
        &mut self,
        expr: &FunctionExpr,
        ctx: &mut InterpreterContext,
    ) -> EvalResult {
        Ok(Value::Function(new_function(
            None,
            expr.clone(),
            ctx.environment.clone(),
        )))
    }

    fn visit_call_expr(&mut self, expr: &CallExpr, ctx: &mut InterpreterContext) -> EvalResult {
        let callee = self
            .visit_expr(&expr.callee, ctx)
            .and_then(|value| assert_type!(value, Value::Function))?;
        let mut callee = callee.borrow_mut();

        if expr.arguments.len() != callee.arity() {
            return Err(BuildError::new(format!(
                "expected exactly {} arguments, but got {}",
                callee.arity(),
                expr.arguments.len()
            )));
        }

        let args = expr
            .arguments
            .iter()
            .map(|arg| self.visit_expr(arg, ctx))
            .collect::<Result<Vec<_>, _>>()?;

        let body: &Vec<Stmt> =
            // Safe: `body` and `environment` are disjoint borrows of `callee`.
            unsafe { &*std::ptr::from_ref(&callee.declaration().body.stmts) as &Vec<Stmt> };
        let mut fn_ctx = InterpreterContext::new(&mut callee.environment);

        self.visit_block(body, &mut fn_ctx, move |environment| {
            for arg in args.into_iter() {
                environment.define_var(arg);
            }
        })
        .map(|value| value.unwrap_or_default())
    }

    fn visit_stmt(&mut self, stmt: &Stmt, ctx: &mut InterpreterContext) -> StmtResult {
        match stmt {
            Stmt::Var(stmt) => self.visit_var_stmt(stmt, ctx),
            Stmt::Expr(stmt) => self.visit_expr_stmt(stmt, ctx),
            Stmt::Block(stmt) => self.visit_block_stmt(stmt, ctx),
        }
    }

    fn visit_var_stmt(&mut self, stmt: &VarStmt, ctx: &mut InterpreterContext) -> StmtResult {
        stmt.initializer
            .as_ref()
            .map_or_else(
                || Ok(Value::default()),
                |expr| {
                    self.visit_expr(expr, ctx).inspect(|val| {
                        if let Value::Function(function) = val {
                            function.borrow_mut().name = Some(stmt.name.clone());
                        }
                    })
                },
            )
            .map(|value| {
                ctx.environment.define_var(value.clone());
                Some(value)
            })
    }

    fn visit_expr_stmt(&mut self, stmt: &ExprStmt, ctx: &mut InterpreterContext) -> StmtResult {
        self.visit_expr(&stmt.expr, ctx).map(Some)
    }

    fn visit_block_stmt(&mut self, stmt: &BlockStmt, ctx: &mut InterpreterContext) -> StmtResult {
        self.visit_block(&stmt.stmts, ctx, |_env| ())
    }

    fn visit_stmts<'a>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a Stmt>,
        ctx: &mut InterpreterContext,
    ) -> StmtResult {
        stmts
            .into_iter()
            .try_fold(None, |_prev, stmt| self.visit_stmt(stmt, ctx))
    }

    fn visit_block<'a, F: FnOnce(&mut Environment)>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a Stmt>,
        ctx: &mut InterpreterContext,
        after_new_scope: F,
    ) -> StmtResult {
        ctx.environment.begin_scope();
        after_new_scope(ctx.environment);
        let ret = self.visit_stmts(stmts, ctx);
        ctx.environment.end_scope();
        ret
    }
}

/// A host evaluator for pure scalar fragments — selection conditions, projection
/// attributes, join keys — which, by the host/relational split invariant, never
/// contain a relational operator. Backend-neutral: it has no circuit, no source
/// registry, and no scalar engine.
pub struct ScalarHost;

impl HostInterpreter for ScalarHost {
    fn visit_relational_expr(
        &mut self,
        _expr: &RelExpr,
        _ctx: &mut InterpreterContext,
    ) -> EvalResult {
        unreachable!("a scalar fragment never contains a relational operator")
    }
}

#[derive(Debug)]
pub struct InterpreterContext<'a> {
    pub environment: &'a mut Environment,
    /// The fields of the row a backend is currently processing, each accessible
    /// as a variable. The host layer only reads this map; filling it from a row
    /// is the backend's job, since only the backend knows how its rows are laid
    /// out (for instance, see
    /// [`DbspTupleContext`](crate::relational::incremental::schema::DbspTupleContext)).
    // No need to wrap it in an Option because HashMap::new() does not allocate!
    pub tuple_vars: HashMap<String, ScalarTypedValue>,
    /// Stores the most recent alias for a relation.
    alias: Option<String>,
}

impl InterpreterContext<'_> {
    pub fn new(environment: &mut Environment) -> InterpreterContext<'_> {
        InterpreterContext {
            environment,
            tuple_vars: HashMap::new(),
            alias: None,
        }
    }
    pub fn set_alias(&mut self, alias: String) {
        self.alias = Some(alias);
    }
    pub fn consume_alias(&mut self) -> Option<String> {
        self.alias.take()
    }
    pub fn clear_tuple_ctx(&mut self) {
        self.tuple_vars.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{
        QueryIr,
        expr::{BinaryExpr, LiteralExpr, VarExpr},
        resolver::ResolvedCode,
        stmt::VarStmt,
        variable::Environment,
    };

    fn bind(name: &str, initializer: Expr) -> Stmt {
        Stmt::from(VarStmt {
            name: name.to_owned(),
            initializer: Some(initializer),
        })
    }

    fn add(left: Expr, right: Expr) -> Expr {
        Expr::from(BinaryExpr {
            operator: Operator::Addition,
            left,
            right,
        })
    }

    /// Resolves `stmts` and runs them, yielding the last statement's value.
    fn run(stmts: Vec<Stmt>) -> Value {
        let resolved = ResolvedCode::from(QueryIr::new(stmts)).expect("resolves");
        let mut environment = Environment::default();
        let mut ctx = InterpreterContext::new(&mut environment);
        ScalarHost
            .interpret(resolved.as_code().iter(), &mut ctx)
            .expect("interprets")
            .expect("the last statement yields a value")
    }

    #[test]
    fn a_shadowing_initializer_reads_the_previous_binding() {
        // var x = 1; var x = x + 1; var x = x + 1;
        // Each initializer sees the binding it shadows, so this counts up.
        let value = run(vec![
            bind("x", Expr::from(LiteralExpr::from(1u64))),
            bind(
                "x",
                add(
                    Expr::from(VarExpr::new("x")),
                    Expr::from(LiteralExpr::from(1u64)),
                ),
            ),
            bind(
                "x",
                add(
                    Expr::from(VarExpr::new("x")),
                    Expr::from(LiteralExpr::from(1u64)),
                ),
            ),
        ]);
        assert_eq!(value, Value::Uint(3));
    }

    #[test]
    fn a_variable_declared_after_a_shadowing_is_not_aliased() {
        // var x = 1; var x = 2; var y = 3; var z = y;
        // `y` must survive the shadowed `x`, which it would not if slots were
        // counted by distinct names rather than by declarations.
        let value = run(vec![
            bind("x", Expr::from(LiteralExpr::from(1u64))),
            bind("x", Expr::from(LiteralExpr::from(2u64))),
            bind("y", Expr::from(LiteralExpr::from(3u64))),
            bind("z", Expr::from(VarExpr::new("y"))),
        ]);
        assert_eq!(value, Value::Uint(3));
    }

    #[test]
    fn shadowing_keeps_the_latest_binding_visible() {
        // var x = 1; var x = 2; var y = x;
        let value = run(vec![
            bind("x", Expr::from(LiteralExpr::from(1u64))),
            bind("x", Expr::from(LiteralExpr::from(2u64))),
            bind("y", Expr::from(VarExpr::new("x"))),
        ]);
        assert_eq!(value, Value::Uint(2));
    }

    #[test]
    fn chars_compare_by_value() {
        let (a, b) = (Value::Char('a'), Value::Char('b'));
        assert_eq!(compare(&a, &a).unwrap(), Some(Ordering::Equal));
        assert_eq!(order(&a, &b).unwrap(), Ordering::Less);
    }

    #[test]
    fn nulls_are_equal_but_unordered() {
        let null = Value::Null(());
        assert_eq!(compare(&null, &null).unwrap(), None);
        assert!(order(&null, &null).is_err());
    }

    #[test]
    fn values_of_different_types_are_incomparable() {
        assert!(compare(&Value::Uint(1), &Value::Iint(1)).is_err());
        assert!(arithmetic!(&Value::Uint(1), &Value::Iint(1), checked_add).is_err());
    }

    #[test]
    fn arithmetic_reports_overflow_and_division_by_zero() {
        assert!(arithmetic!(&Value::Uint(u64::MAX), &Value::Uint(1), checked_add).is_err());
        assert!(arithmetic!(&Value::Iint(1), &Value::Iint(0), checked_div).is_err());
        assert_eq!(
            arithmetic!(&Value::Iint(-6), &Value::Iint(3), checked_div).unwrap(),
            Value::Iint(-2)
        );
    }
}
