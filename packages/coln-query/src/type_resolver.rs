// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    error::SyntaxError,
    expr::{
        AliasExpr, AntiJoinExpr, AssignExpr, BinaryExpr, CallExpr, CartesianProductExpr,
        DifferenceExpr, DistinctExpr, EquiJoinExpr, Expr, ExprVisitor, FixedPointIterExpr,
        FunctionExpr, GroupingExpr, Literal, LiteralExpr, ProjectionExpr, SelectionExpr, TupleExpr,
        UnaryExpr, UnionExpr, VarExpr,
    },
    operator::Operator,
    resolver::ScopeStack,
    stmt::{BlockStmt, ExprStmt, Stmt, StmtVisitor, VarStmt},
    tuple::TupleType,
};
pub use crate::{function::FunctionType, relation::RelationType, scalar::ScalarType};
use std::collections::HashMap;

macro_rules! assert_type {
    ($value:expr, $variant:path) => {
        match $value {
            $variant(inner) => Ok(inner),
            _ => Err(SyntaxError::new(format!(
                "expected {} type, got: {:?}",
                stringify!($variant:path), $value
            ))),
        }
    };
}

/// This is "get me the type but don't check it".
#[derive(Default)]
pub struct TypeResolver {}

impl TypeResolver {
    pub fn resolve<'a>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a Stmt>,
        ctx: VisitorCtx,
    ) -> Result<Option<ExprType>, SyntaxError> {
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
    pub fn resolve_expr(&mut self, expr: &Expr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(expr, ctx)
    }
    pub fn resolve_stmt(&mut self, stmt: &Stmt, ctx: VisitorCtx) -> VisitorResult {
        self.visit_stmt(stmt, ctx)
    }
    fn visit_stmts<'a>(
        &mut self,
        stmts: impl IntoIterator<Item = &'a Stmt>,
        ctx: VisitorCtx,
    ) -> Result<Option<ExprType>, SyntaxError> {
        stmts
            .into_iter()
            .try_fold(None, |_prev, stmt| Ok(Some(self.resolve_stmt(stmt, ctx)?)))
    }
}

type VisitorCtx<'a, 'b> = &'a mut TypeResolverContext<'b>;
type VisitorResult = Result<ExprType, SyntaxError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprType {
    Scalar(ScalarType),
    Relation(RelationType),
    Function(FunctionType),
    Tuple(TupleType),
}

pub struct TypeResolverContext<'a> {
    scopes: ScopeStack<ExprType>,
    tuple_vars: &'a mut HashMap<String, ScalarType>,
}

impl TypeResolverContext<'_> {
    pub fn new<'a>(tuple_vars: &'a mut HashMap<String, ScalarType>) -> TypeResolverContext<'a> {
        TypeResolverContext {
            scopes: ScopeStack::new(),
            tuple_vars,
        }
    }
    fn begin_tuple_context(&mut self, relation_type: RelationType) {
        *self.tuple_vars = relation_type.into_tuple_vars();
    }
    fn get_type(&self, of: &str) -> Option<ExprType> {
        self.tuple_vars
            .get(of)
            .map(|scalar_type| ExprType::Scalar(*scalar_type))
            .or_else(|| {
                for scope in self.scopes.iter() {
                    if let Some(var_type) = scope.get(of) {
                        return Some(var_type.clone());
                    }
                }
                None
            })
    }
    fn end_tuple_context(&mut self) {
        self.tuple_vars.clear();
    }
}

impl TypeResolver {
    /// A helper method to deal with projections.
    fn visit_projection_attributes(
        &mut self,
        relation_type: RelationType,
        attributes: Option<&Vec<(String, Expr)>>,
        ctx: VisitorCtx,
    ) -> VisitorResult {
        let relation_type = if let Some(attributes) = attributes {
            ctx.begin_tuple_context(relation_type);
            let relation_type = attributes
                .iter()
                .map(|(name, expr)| {
                    self.visit_expr(expr, ctx).and_then(|expr_type| {
                        Ok((name, assert_type!(expr_type, ExprType::Scalar)?))
                    })
                })
                .collect::<Result<RelationType, _>>()?;
            ctx.end_tuple_context();
            relation_type
        } else {
            relation_type
        };
        Ok(ExprType::Relation(relation_type))
    }
}

impl ExprVisitor<VisitorResult, VisitorCtx<'_, '_>> for TypeResolver {
    fn visit_literal_expr(&mut self, expr: &LiteralExpr, ctx: VisitorCtx) -> VisitorResult {
        Ok(ExprType::from(&expr.value))
    }

    fn visit_tuple_expr(&mut self, expr: &TupleExpr, ctx: VisitorCtx<'_, '_>) -> VisitorResult {
        let element_types = expr
            .elements
            .iter()
            .map(|expr| self.visit_expr(expr, ctx))
            .collect::<Result<Vec<ExprType>, _>>()?;
        Ok(ExprType::Tuple(TupleType::from(element_types)))
    }

    fn visit_grouping_expr(&mut self, expr: &GroupingExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&expr.expr, ctx)
    }

    fn visit_binary_expr(&mut self, expr: &BinaryExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&expr.left, ctx).and_then(|left_type| {
            let right_type = self.visit_expr(&expr.right, ctx)?;
            // Maybe don't perform real type checking here, just obtain the type
            // from the left expr.
            if left_type == right_type {
                Ok(left_type)
            } else {
                Err(SyntaxError::new("Incompatible types"))
            }
        })
    }

    fn visit_unary_expr(&mut self, expr: &UnaryExpr, ctx: VisitorCtx) -> VisitorResult {
        match expr.operator {
            Operator::Not => {
                // The NOT operator always returns a boolean.
                Ok(ExprType::Scalar(ScalarType::Bool))
            }
            _ => {
                // For other operators, we just return the type of the operand.
                self.visit_expr(&expr.operand, ctx)
            }
        }
    }

    fn visit_var_expr(&mut self, expr: &VarExpr, ctx: VisitorCtx) -> VisitorResult {
        let name = &expr.name;
        ctx.get_type(name)
            .ok_or_else(|| SyntaxError::new(format!("Variable {name} not found")))
    }

    fn visit_assign_expr(&mut self, expr: &AssignExpr, ctx: VisitorCtx) -> VisitorResult {
        // We return the type of the value that is being assigned.
        self.visit_expr(&expr.value, ctx)
    }

    fn visit_function_expr(&mut self, expr: &FunctionExpr, ctx: VisitorCtx) -> VisitorResult {
        // Produces the function's type but this requires a visitor over statements, too.
        todo!("Function exprs' type");
    }

    fn visit_call_expr(&mut self, expr: &CallExpr, ctx: VisitorCtx) -> VisitorResult {
        // Produces the function's return type.
        match self.visit_expr(&expr.callee, ctx)? {
            ExprType::Function(function_type) => Ok(*function_type.return_type),
            _ => Err(SyntaxError::new("Callee has non-function type")),
        }
    }

    fn visit_alias_expr(&mut self, expr: &AliasExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&expr.relation, ctx)
    }

    fn visit_distinct_expr(&mut self, expr: &DistinctExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&expr.relation, ctx)
    }

    fn visit_union_expr(&mut self, expr: &UnionExpr, ctx: VisitorCtx) -> VisitorResult {
        let first = expr
            .relations
            .first()
            .ok_or_else(|| SyntaxError::new("Union expr with only no operands"))?;
        self.visit_expr(first, ctx)
    }

    fn visit_difference_expr(&mut self, expr: &DifferenceExpr, ctx: VisitorCtx) -> VisitorResult {
        // We just take the left's schema, even though we could take right's schema
        // equally well.
        self.visit_expr(&expr.left, ctx)
    }

    fn visit_selection_expr(&mut self, expr: &SelectionExpr, ctx: VisitorCtx) -> VisitorResult {
        // Selections don't alter the schema.
        self.visit_expr(&expr.relation, ctx)
    }

    fn visit_projection_expr(&mut self, expr: &ProjectionExpr, ctx: VisitorCtx) -> VisitorResult {
        let relation_type = self
            .visit_expr(&expr.relation, ctx)
            .and_then(|expr_type| assert_type!(expr_type, ExprType::Relation))?;
        self.visit_projection_attributes(relation_type, Some(&expr.attributes), ctx)
    }

    fn visit_cartesian_product_expr(
        &mut self,
        expr: &CartesianProductExpr,
        ctx: VisitorCtx,
    ) -> VisitorResult {
        self.visit_equi_join_expr(&expr.inner, ctx)
    }

    fn visit_equi_join_expr(&mut self, expr: &EquiJoinExpr, ctx: VisitorCtx) -> VisitorResult {
        let left_relation_type = self
            .visit_expr(&expr.left, ctx)
            .and_then(|expr_type| assert_type!(expr_type, ExprType::Relation))?;
        let right_relation_type = self
            .visit_expr(&expr.right, ctx)
            .and_then(|expr_type| assert_type!(expr_type, ExprType::Relation))?;
        let joined = left_relation_type.join(right_relation_type);
        self.visit_projection_attributes(joined, expr.attributes.as_ref(), ctx)
    }

    fn visit_anti_join_expr(&mut self, expr: &AntiJoinExpr, ctx: VisitorCtx) -> VisitorResult {
        self.visit_expr(&expr.left, ctx)
    }

    fn visit_fixed_point_iter_expr(
        &mut self,
        expr: &FixedPointIterExpr,
        ctx: VisitorCtx,
    ) -> VisitorResult {
        // We steal the relation's type from the accumulator's type.
        self.visit_expr(&expr.accumulator.1, ctx)
    }
}

impl From<&Literal> for ExprType {
    fn from(value: &Literal) -> Self {
        match value {
            Literal::String(_) => Self::Scalar(ScalarType::String),
            Literal::Iint(_) => Self::Scalar(ScalarType::Iint),
            Literal::Uint(_) => Self::Scalar(ScalarType::Uint),
            Literal::Bool(_) => Self::Scalar(ScalarType::Bool),
            Literal::Null(_) => Self::Scalar(ScalarType::Null),
            Literal::Relation(relation) => Self::Relation(RelationType::from(&relation.schema)),
        }
    }
}

impl StmtVisitor<VisitorResult, VisitorCtx<'_, '_>> for TypeResolver {
    fn visit_var_stmt(&mut self, stmt: &VarStmt, ctx: VisitorCtx) -> VisitorResult {
        todo!();
    }

    fn visit_expr_stmt(&mut self, stmt: &ExprStmt, ctx: VisitorCtx) -> VisitorResult {
        todo!();
    }

    fn visit_block_stmt(&mut self, stmt: &BlockStmt, ctx: VisitorCtx) -> VisitorResult {
        todo!();
    }
}
