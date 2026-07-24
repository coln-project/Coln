// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    context::InterpreterContext,
    dbsp::{OrdIndexedNestedStream, RootCircuit},
    error::EngineError,
    expr::{
        AliasExpr, AntiJoinExpr, AssignExpr, BinaryExpr, CallExpr, CartesianProductExpr,
        DifferenceExpr, DistinctExpr, EquiJoinExpr, Expr, ExprVisitor, FixedPointIterExpr,
        FunctionExpr, GetIndexExpr, GroupingExpr, LiteralExpr, ProjectionExpr, SelectionExpr,
        TupleExpr, UnaryExpr, UnionExpr, VarExpr,
    },
    function::new_function,
    operator::Operator,
    operators::{
        coalesce::coalesce_helper,
        projection::{ProjectionStrategy, projection_helper},
        reindex::reindex_helper,
    },
    relation::{Relation, RelationRef, SchemaTuple, TupleKey, TupleValue, new_relation},
    stmt::{BlockStmt, ExprStmt, Stmt, StmtVisitor, VarStmt},
    tuple::Tuple,
    variable::{Environment, Value},
};
use std::{cell::Ref, rc::Rc};

pub struct Interpreter {
    root_circuit: Option<RootCircuit>,
}

impl Interpreter {
    pub fn new(root_circuit: Option<RootCircuit>) -> Self {
        Self { root_circuit }
    }
    pub fn interpret<'a>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a Stmt>,
        ctx: &mut InterpreterContext,
    ) -> Result<Option<Value>, EngineError> {
        // Ensure we have a global scope before interpreting.
        debug_assert!(ctx.environment.just_global());
        // We do not call `visit_block` here because the root scope is created
        // in the `Environment` constructor and should remain intact across
        // multiple calls to `interpret`.
        let ret = self.visit_stmts(stmts, ctx);
        // Ensure we have a global scope after interpreting.
        debug_assert!(ctx.environment.just_global());
        ret
    }
    pub fn evaluate(&mut self, expr: &Expr, ctx: VisitorCtx) -> Result<Value, EngineError> {
        self.visit_expr(expr, ctx)
    }
    fn visit_stmts<'a>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a Stmt>,
        ctx: VisitorCtx,
    ) -> Result<Option<Value>, EngineError> {
        // Functional programming can be so beautiful. Return the last value
        // if any.
        stmts
            .into_iter()
            .try_fold(None, |_prev, stmt| self.visit_stmt(stmt, ctx))
    }
    fn visit_block<'a, F: FnOnce(&mut Environment)>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a Stmt>,
        ctx: VisitorCtx,
        environment: F,
    ) -> Result<Option<Value>, EngineError> {
        ctx.environment.begin_scope();
        environment(ctx.environment);
        let ret = self.visit_stmts(stmts, ctx);
        ctx.environment.end_scope();
        ret
    }
}

macro_rules! comparison_helper {
    ($left:expr, $right:expr, $op:tt, $($variant:path),*) => {{
        match (&$left, &$right) {
            $(
                ($variant(left), $variant(right)) => Ok(Value::Bool(left $op right)),
            )*
            _ => Err(EngineError::new(
                format!("expected comparable type, got: {:?} and {:?}", $left, $right),
            )),
        }
    }}
}

macro_rules! arithmetic_helper {
    ($left:expr, $right:expr, $op:tt, $($variant:path),*) => {{
        match (&$left, &$right) {
            $(
                ($variant(left), $variant(right)) => Ok($variant(left $op right)),
            )*
            _ => Err(EngineError::new(
                format!("expected number type, got: {:?} and {:?}", $left, $right),
            )),
        }
    }}
}

macro_rules! assert_type {
    ($value:expr, $variant:path) => {
        match $value {
            $variant(inner) => Ok(inner),
            _ => Err(EngineError::new(format!(
                "expected {} type, got: {:?}",
                stringify!($variant:path), $value
            ))),
        }
    };
}

impl Interpreter {
    fn visit_lazy_binary_expr(&mut self, expr: &BinaryExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let left = is_truthy(&self.visit_expr(&expr.left, ctx)?);

        if let Operator::And | Operator::Or = expr.operator {
            if expr.operator == Operator::And && left || expr.operator == Operator::Or && !left {
                let right = is_truthy(&self.visit_expr(&expr.right, ctx)?);
                Ok(Value::Bool(right))
            } else {
                Ok(Value::Bool(left))
            }
        } else {
            Err(EngineError::new(format!(
                "unsupported (lazy) binary operator: {:?}",
                expr.operator
            )))
        }
    }
    fn visit_eager_binary_expr(&mut self, expr: &BinaryExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let left = self.visit_expr(&expr.left, ctx)?;
        let right = self.visit_expr(&expr.right, ctx)?;

        match expr.operator {
            Operator::Equal => {
                comparison_helper!(left, right, ==, Value::Iint, Value::Uint, Value::Bool, Value::String, Value::Null)
            }
            Operator::NotEqual => {
                comparison_helper!(left, right, !=, Value::Iint, Value::Uint, Value::Bool, Value::String, Value::Null)
            }
            Operator::Less => {
                comparison_helper!(left, right, <, Value::Iint, Value::Uint, Value::Bool, Value::String)
            }
            Operator::LessEqual => {
                comparison_helper!(left, right, <=, Value::Iint, Value::Uint, Value::Bool, Value::String)
            }
            Operator::Greater => {
                comparison_helper!(left, right, >, Value::Iint, Value::Uint, Value::Bool, Value::String)
            }
            Operator::GreaterEqual => {
                comparison_helper!(left, right, >=, Value::Iint, Value::Uint, Value::Bool, Value::String)
            }
            Operator::Addition => {
                if let (Value::String(left), Value::String(right)) = (&left, &right) {
                    return Ok(Value::String(format!("{left}{right}")));
                }
                arithmetic_helper!(left, right, +, Value::Iint, Value::Uint)
            }
            Operator::Subtraction => {
                arithmetic_helper!(left, right, -, Value::Iint, Value::Uint)
            }
            Operator::Multiplication => {
                arithmetic_helper!(left, right, *, Value::Iint, Value::Uint)
            }
            Operator::Division => {
                arithmetic_helper!(left, right, /, Value::Iint, Value::Uint)
            }
            _ => Err(EngineError::new(format!(
                "unsupported (eager) binary operator: {:?}",
                expr.operator
            ))),
        }
    }
}

type VisitorCtx<'a, 'b> = &'a mut InterpreterContext<'b>;

type ExprVisitorResult = Result<Value, EngineError>;

impl ExprVisitor<ExprVisitorResult, VisitorCtx<'_, '_>> for Interpreter {
    fn visit_literal_expr(&mut self, expr: &LiteralExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        // Maybe make values reference counted instead of cloning here?
        let literal = expr.value.clone();
        Ok(Value::from(literal))
    }

    fn visit_tuple_expr(&mut self, expr: &TupleExpr, ctx: VisitorCtx<'_, '_>) -> ExprVisitorResult {
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
        ctx: VisitorCtx<'_, '_>,
    ) -> ExprVisitorResult {
        let target = self.visit_expr(&expr.target, ctx)?;
        let index = self.visit_expr(&expr.index, ctx)?;

        if let Value::Uint(idx) = index {
            match target {
                Value::Tuple(tuple) => {
                    let idx = idx as usize;
                    let tuple_len = tuple.len();
                    if idx < tuple_len {
                        Ok(tuple.get(idx).clone())
                    } else {
                        Err(EngineError::new(format!(
                            "{} at index {idx} is out of bounds.",
                            tuple.display_compact()
                        )))
                    }
                }
                // Extend here to handle arrays, string indexing, etc.
                _ =>
                // If the type checker is used this case is actually unreachable.
                {
                    Err(EngineError::new(format!(
                        "Invalid index operation on {target}."
                    )))
                }
            }
        } else {
            Err(EngineError::new(format!(
                "Index {index} does not evaluate to an uint"
            )))
        }
    }

    fn visit_grouping_expr(&mut self, expr: &GroupingExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        self.visit_expr(&expr.expr, ctx)
    }

    fn visit_binary_expr(&mut self, expr: &BinaryExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        if let Operator::And | Operator::Or = expr.operator {
            self.visit_lazy_binary_expr(expr, ctx)
        } else {
            self.visit_eager_binary_expr(expr, ctx)
        }
    }

    fn visit_unary_expr(&mut self, expr: &UnaryExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let operand = self.visit_expr(&expr.operand, ctx)?;

        match expr.operator {
            Operator::Subtraction => match operand {
                Value::Iint(value) => Ok(Value::Iint(-value)),
                _ => Err(EngineError::new(format!(
                    "expected signed int, got: {operand:?}",
                ))),
            },
            Operator::Not => Ok(Value::Bool(!is_truthy(&operand))),
            _ => Err(EngineError::new(format!(
                "unsupported unary operator: {:?}",
                expr.operator
            ))),
        }
    }

    fn visit_var_expr(&mut self, expr: &VarExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let name = &expr.name;
        ctx.tuple_vars.get(name).map_or_else(
            || {
                let resolved = expr
                    .resolved
                    .as_ref()
                    // This should never happen because the resolver should have resolved
                    // all non-tuple variables before the interpreter starts.
                    .unwrap_or_else(|| panic!("Unresolved variable '{name}'."));
                Ok(ctx.environment.lookup_var(resolved).clone())
            },
            |value| Ok(Value::from(value.clone())),
        )
    }

    fn visit_assign_expr(&mut self, expr: &AssignExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let name = &expr.name;
        self.visit_expr(&expr.value, ctx).inspect(|value| {
            let resolved = expr
                .resolved
                .as_ref()
                // This should never happen because the resolver should have resolved
                // all non-tuple variables before the interpreter starts and assigning
                // to tuple variables is not allowed.
                .unwrap_or_else(|| panic!("Unresolved variable '{name}'."));
            ctx.environment.assign_var(resolved, value.clone());
        })
    }

    fn visit_function_expr(&mut self, expr: &FunctionExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        Ok(Value::Function(new_function(
            // For now, we assume that the function is anonymous, that is, nameless.
            None,
            // We clone here to let functions own their code and thus,
            // we do not have to worry about the lifetime of their code.
            expr.clone(),
            ctx.environment.clone(), // Clone is cheap and necessary here.
        )))
    }

    fn visit_call_expr(&mut self, expr: &CallExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let callee = self
            .visit_expr(&expr.callee, ctx)
            .and_then(|value| assert_type!(value, Value::Function))?;
        let mut callee = callee.borrow_mut();

        // TODO: Optimize by checking arity in resolver just _once_ statically.
        if expr.arguments.len() != callee.arity() {
            return Err(EngineError::new(format!(
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
            // This is a sin, I know, please forgive me. I bet there is a nicer solution.
            // Yet, this is safe because the the `body` and `environment` are disjoint
            // borrows from the `callee` struct.
            unsafe { &*std::ptr::from_ref(&callee.declaration().body.stmts) as &Vec<Stmt> };
        let mut fn_ctx = InterpreterContext::new(&mut callee.environment);

        self.visit_block(body, &mut fn_ctx, move |environment| {
            for arg in args.into_iter() {
                environment.define_var(arg);
            }
        })
        // We return the default value of `null` if the function does not return
        // anything.
        .map(|value| value.unwrap_or_default())
    }

    fn visit_alias_expr(&mut self, expr: &AliasExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        ctx.set_alias(expr.alias.clone());
        self.visit_expr(&expr.relation, ctx)
    }

    fn visit_distinct_expr(&mut self, expr: &DistinctExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let relation = self
            .visit_expr(&expr.relation, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))
            .map(coalesce_helper)?;
        let relation_ref = relation.borrow();

        let distincted = relation_ref.inner.distinct();

        Ok(Value::Relation(new_relation(
            relation_ref.schema.clone(),
            distincted,
        )))
    }

    fn visit_union_expr(&mut self, expr: &UnionExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let relations: Vec<RelationRef> = expr
            .relations
            .iter()
            .map(|relation| {
                self.visit_expr(relation, ctx)
                    .and_then(|value| assert_type!(value, Value::Relation))
                    .map(coalesce_helper)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let relations: Vec<Ref<'_, Relation>> =
            relations.iter().map(|relation| relation.borrow()).collect();

        let (first, others) = relations
            .split_first()
            .expect("Resolver has *not* done its job and ensured that there are at least two operands to a union!");

        let unioned = first
            .inner
            .sum(others.iter().map(|relation| &relation.inner));

        Ok(Value::Relation(new_relation(first.schema.clone(), unioned)))
    }

    fn visit_difference_expr(
        &mut self,
        expr: &DifferenceExpr,
        ctx: VisitorCtx,
    ) -> ExprVisitorResult {
        let left = self
            .visit_expr(&expr.left, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))
            .map(coalesce_helper)?;
        let right = self
            .visit_expr(&expr.right, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))
            .map(coalesce_helper)?;

        let left_ref = left.borrow();

        let differenced = left_ref.inner.minus(&right.borrow().inner);

        Ok(Value::Relation(new_relation(
            left_ref.schema.clone(),
            differenced,
        )))
    }

    fn visit_selection_expr(&mut self, expr: &SelectionExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let relation = self
            .visit_expr(&expr.relation, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;
        let relation_ref = relation.borrow();
        let relation_clone = Rc::clone(&relation);

        let condition = expr.condition.clone();
        let environment = ctx.environment.clone();
        let selected = relation_ref.inner.filter(move |(_key, tuple)| {
            // No need to run resolver here, already resolved!
            let schema = &relation_clone.borrow().schema;
            let environment = &mut environment.clone();
            let mut new_ctx = InterpreterContext::new(environment);
            new_ctx.extend_tuple_ctx(&None, &schema.tuple, tuple);
            let value = Interpreter::new(None)
                .evaluate(&condition, &mut new_ctx)
                .expect("Runtime error while interpreting selection condition");
            is_truthy(&value)
        });

        Ok(Value::Relation(new_relation(
            relation_ref.schema.select(),
            selected,
        )))
    }

    fn visit_projection_expr(
        &mut self,
        expr: &ProjectionExpr,
        ctx: VisitorCtx,
    ) -> ExprVisitorResult {
        let relation = self
            .visit_expr(&expr.relation, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;
        let relation_ref = relation.borrow();

        let (schema, projected) = match projection_helper(&expr.attributes) {
            ProjectionStrategy::Projection(projection) => {
                let (schema, projection) = projection.prepare(&relation_ref.schema);
                let projected = relation_ref.inner.map_index({
                    let relation_clone = Rc::clone(&relation);
                    let environment = ctx.environment.clone();
                    move |(key, tuple)| {
                        let schema = &relation_clone.borrow().schema;
                        let environment = &mut environment.clone();
                        let mut new_ctx = InterpreterContext::new(environment);
                        new_ctx.extend_tuple_ctx(&None, &schema.tuple, tuple);
                        projection(new_ctx)
                    }
                });
                (schema, projected)
            }
            ProjectionStrategy::Pick(pick) => {
                let schema = pick.prepare(&relation_ref.schema);
                let picked = relation_ref.inner.clone();
                (schema, picked)
            }
        };

        Ok(Value::Relation(new_relation(schema, projected)))
    }

    fn visit_cartesian_product_expr(
        &mut self,
        expr: &CartesianProductExpr,
        ctx: VisitorCtx,
    ) -> ExprVisitorResult {
        self.visit_equi_join_expr(&expr.inner, ctx)
    }

    fn visit_equi_join_expr(&mut self, expr: &EquiJoinExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let left = self
            .visit_expr(&expr.left, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;
        // Note the order here. Before we evaluate the right expression,
        // we have to consume the alias of the left relation because it is
        // replaced by the right relation's alias otherwise.
        let left_alias = ctx.consume_alias();

        let right = self
            .visit_expr(&expr.right, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;
        let right_alias = ctx.consume_alias();

        let (left_key_fields, right_key_fields): (Vec<&Expr>, Vec<&Expr>) =
            expr.on.iter().map(|(left, right)| (left, right)).unzip();

        let (left_indexed, key_fields) =
            reindex_helper(&left, left_key_fields.as_slice(), ctx.environment);
        let (right_indexed, _) =
            reindex_helper(&right, right_key_fields.as_slice(), ctx.environment);

        let joined_schema = left
            .borrow()
            .schema
            .join(&right.borrow().schema, key_fields);

        let (schema, projection) = match expr
            .attributes
            .as_ref()
            .map(|attributes| projection_helper(attributes))
        {
            Some(ProjectionStrategy::Projection(projection)) => {
                let (projected_schema, projection) = projection.prepare(&joined_schema);
                (projected_schema, Some(projection))
            }
            Some(ProjectionStrategy::Pick(pick)) => {
                let picked_schema = pick.prepare(&joined_schema);
                (picked_schema, None)
            }
            None => (joined_schema, None),
        };

        let joined = left_indexed.join_index(&right_indexed, {
            let left_rel = Rc::clone(&left);
            let right_rel = Rc::clone(&right);
            let environment = ctx.environment.clone();
            move |key: &TupleKey, left: &TupleValue, right: &TupleValue| {
                let left_schema = &left_rel.borrow().schema;
                let right_schema = &right_rel.borrow().schema;
                let joined_tuple: TupleValue = SchemaTuple::new(&left_schema.tuple, left)
                    .join(&SchemaTuple::new(&right_schema.tuple, right))
                    .collect();
                let key_tuple_pair = if let Some(projection) = &projection {
                    let environment = &mut environment.clone();
                    let mut new_ctx = InterpreterContext::new(environment);
                    new_ctx.extend_tuple_ctx(&left_alias, &left_schema.tuple, left);
                    new_ctx.extend_tuple_ctx(&right_alias, &right_schema.tuple, right);
                    projection(new_ctx)
                } else {
                    (key.clone(), joined_tuple)
                };
                Some(key_tuple_pair)
            }
        });

        Ok(Value::Relation(new_relation(schema, joined)))
    }

    fn visit_anti_join_expr(&mut self, expr: &AntiJoinExpr, ctx: VisitorCtx) -> ExprVisitorResult {
        let left = self
            .visit_expr(&expr.left, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;

        let right = self
            .visit_expr(&expr.right, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))?;

        let (left_key_fields, right_key_fields): (Vec<&Expr>, Vec<&Expr>) =
            expr.on.iter().map(|(left, right)| (left, right)).unzip();

        let (left_indexed, key_fields) =
            reindex_helper(&left, left_key_fields.as_slice(), ctx.environment);
        let (right_indexed, _) =
            reindex_helper(&right, right_key_fields.as_slice(), ctx.environment);

        let anti_joined_schema = left
            .borrow()
            .schema
            .anti_join(&right.borrow().schema, key_fields);
        let anti_joined = left_indexed.anti_join_index(&right_indexed);

        Ok(Value::Relation(new_relation(
            anti_joined_schema,
            anti_joined,
        )))
    }

    fn visit_fixed_point_iter_expr(
        &mut self,
        expr: &FixedPointIterExpr,
        ctx: VisitorCtx,
    ) -> ExprVisitorResult {
        let accumulator = self
            .visit_expr(&expr.accumulator.1, ctx)
            .and_then(|value| assert_type!(value, Value::Relation))
            .map(coalesce_helper)?;

        let imports: Vec<RelationRef> = expr
            .imports
            .iter()
            .map(|import| {
                self.visit_expr(&import.1, ctx)
                    .and_then(|value| assert_type!(value, Value::Relation))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let (accumulator_init, schema) = {
            let accumulator = accumulator.borrow();
            (accumulator.inner.clone(), accumulator.schema.clone())
        };

        let root_circuit = self
            .root_circuit
            .clone() // Can we just clone a root_circuit?
            .expect("Recursion from non-recursive context");
        let accumulated = root_circuit
            .recursive(|nested_circuit, acc: OrdIndexedNestedStream| {
                let result = self
                    .visit_block(&expr.step.stmts, ctx, move |environment| {
                        let accumulator = accumulator.borrow();
                        // delta0 does not alter the schema.
                        let schema = accumulator.schema.clone();
                        let accumulator =
                            accumulator.inner.delta0(nested_circuit).plus(&acc.into());
                        environment.define_var(new_relation(schema, accumulator));

                        for import in imports.iter() {
                            let import = import.borrow();
                            // delta0 does not alter the schema.
                            let schema = import.schema.clone();
                            let import = import.inner.delta0(nested_circuit);
                            environment.define_var(new_relation(schema, import));
                        }
                    })
                    .expect("Runtime error while interpreting fixed point iteration body")
                    .expect("Fixed point iteration body did not return a value");
                let result = assert_type!(result, Value::Relation)
                    .expect("Fixed point iteration body did not return a relation");
                // We have to coalesce the result here because the output is fed
                // into a union below, which requires the schema to be coalesced.
                let result = coalesce_helper(result);
                Ok(result.borrow().inner.expect_nested().clone())
            })
            .expect("Recursive error");

        let fixed_point = accumulator_init.plus(&accumulated.into());

        Ok(Value::Relation(new_relation(schema, fixed_point)))
    }
}

type StmtVisitorResult = Result<Option<Value>, EngineError>;

impl StmtVisitor<StmtVisitorResult, VisitorCtx<'_, '_>> for Interpreter {
    fn visit_var_stmt(&mut self, stmt: &VarStmt, ctx: VisitorCtx) -> StmtVisitorResult {
        stmt.initializer
            .as_ref()
            .map_or_else(
                // We default to null if no initializer is provided.
                || Ok(Value::default()),
                |expr| {
                    self.visit_expr(expr, ctx).inspect(|val| {
                        if let Value::Function(function) = val {
                            // Here, a function turns from anonymous to named.
                            // If the function is later aliased, that is, reassigned to another
                            // variable, we stick to this original name (NodeJS does it, too).
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

    fn visit_expr_stmt(&mut self, stmt: &ExprStmt, ctx: VisitorCtx) -> StmtVisitorResult {
        // Evaluate the expression and return the result.
        // This is the only statement that can return a value.
        self.visit_expr(&stmt.expr, ctx).map(Some)
    }

    fn visit_block_stmt(&mut self, stmt: &BlockStmt, ctx: VisitorCtx) -> StmtVisitorResult {
        self.visit_block(&stmt.stmts, ctx, |_env| ())
    }
}

/// Only `null` and `false` are falsy, everything else is truthy.
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null(()) => false,
        Value::Bool(value) => *value,
        _ => true,
    }
}
