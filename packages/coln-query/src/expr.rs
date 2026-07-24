// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::operator::Operator;
use crate::{
    impl_from_auto_box,
    relation::Relation,
    stmt::BlockStmt,
    util::{MemAddr, Named, Resolvable},
    variable::VariableSlot,
};
use std::fmt::{self, Debug, Display};

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
    Alias(Box<AliasExpr>),
    Distinct(Box<DistinctExpr>),
    Union(Box<UnionExpr>),
    Difference(Box<DifferenceExpr>),
    Selection(Box<SelectionExpr>),
    Projection(Box<ProjectionExpr>),
    CartesianProduct(Box<CartesianProductExpr>),
    EquiJoin(Box<EquiJoinExpr>),
    AntiJoin(Box<AntiJoinExpr>),
    FixedPointIter(Box<FixedPointIterExpr>),
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
    (Expr::Function, FunctionExpr),
    (Expr::Alias, AliasExpr),
    (Expr::Distinct, DistinctExpr),
    (Expr::Union, UnionExpr),
    (Expr::Difference, DifferenceExpr),
    (Expr::Selection, SelectionExpr),
    (Expr::Projection, ProjectionExpr),
    (Expr::CartesianProduct, CartesianProductExpr),
    (Expr::EquiJoin, EquiJoinExpr),
    (Expr::AntiJoin, AntiJoinExpr),
    (Expr::FixedPointIter, FixedPointIterExpr)
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AliasExpr {
    pub relation: Expr,
    pub alias: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistinctExpr {
    pub relation: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnionExpr {
    /// All `Expr`s must evaluate to a relation and have a compatible schema,
    /// that is, the same order and arity of attributes with same types, respectively.
    pub relations: Vec<Expr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DifferenceExpr {
    /// All `Expr`s must evaluate to a relation and have a compatible schema,
    /// that is, the same order and arity of attributes with same types, respectively.
    pub left: Expr,
    pub right: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionExpr {
    /// Must evaluate to a relation.
    pub relation: Expr,
    pub condition: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionExpr {
    /// Must evaluate to a relation.
    pub relation: Expr,
    /// The attributes to map over. The first element `String` is the name
    /// of the attribute. The second element `Expr` is the expression
    /// which produces the new value of the attribute.
    ///
    /// In case the `Expr` is just a `VarExpr` referencing a **tuple** variable,
    /// the interpreter is not run to evaluate the expression but instead only
    /// the schema is changed.
    pub attributes: Vec<(String, Expr)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CartesianProductExpr {
    /// We delegate to an [`EquiJoinExpr`] with an empty `on` clause.
    pub inner: EquiJoinExpr,
}

impl CartesianProductExpr {
    pub fn new(left: Expr, right: Expr, attributes: Option<Vec<(String, Expr)>>) -> Self {
        Self {
            inner: EquiJoinExpr {
                left,
                right,
                on: vec![],
                attributes,
            },
        }
    }
}

/// An equijoin is a join that exclusively uses equality of attribute(s).
/// [More information on join classifications](https://stackoverflow.com/a/7870216).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EquiJoinExpr {
    /// Must evaluate to a relation.
    pub left: Expr,
    /// Must evaluate to a relation.
    pub right: Expr,
    /// The attributes to join on. The first element of any pair belongs to the
    /// left relation, and the second element of any pair belongs to right relation.
    /// Each attribute pair should produce the same type.
    pub on: Vec<(Expr, Expr)>,
    /// An optional projection step. See documentation of [`ProjectionExpr`].
    pub attributes: Option<Vec<(String, Expr)>>,
}

/// This is not a commutative operation, that is, swapping the `left` and `right`
/// relations may alter the result. This computes `left` setminus `right` while
/// only considering the columns specified in `on`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AntiJoinExpr {
    /// Must evaluate to a relation.
    pub left: Expr,
    /// Must evaluate to a relation.
    pub right: Expr,
    /// The attributes to _not_ join on. The first element of any pair belongs to the
    /// left relation, and the second element of any pair belongs to right relation.
    /// Each attribute pair should produce the same type.
    pub on: Vec<(Expr, Expr)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThetaJoinExpr {
    /// Must evaluate to a relation.
    pub left: Expr,
    /// Must evaluate to a relation.
    pub right: Expr,
    /// An arbitrary join condition that is evaluated for each pair of tuples
    /// (cartesian product of both relations) in the context containing
    /// the attributes of both relations.
    /// If the condition evaluates to false, the output does not contain that
    /// pair. If the condition evaluates to true, the output contains the pair.
    pub on: Expr,
    /// An optional projection step. See documentation of [`ProjectionExpr`].
    pub attributes: Option<Vec<(String, Expr)>>,
}

/// Evaluates to a relation/stream again.
#[derive(Clone, Debug)]
pub struct FixedPointIterExpr {
    /// The streams from the root circuit to make available in the nested circuit.
    /// The relations are available as variables named according to the first respective
    /// tuple element in the context of the child circuit, that is,
    /// within the the context of the [`step`](FixedPointIterExpr.step) statements.
    /// The second tuple elements must evaluate to a relation, respectively.
    pub imports: Vec<(String, Expr)>,
    /// The accumulator is available as a variable named according to the first
    /// tuple element in the context of the child circuit, that is,
    /// within the the context of the [`step`](FixedPointIterExpr.step) statements.
    /// The second tuple element must evaluate to a relation.
    /// The accumulator also defines the schema of the fixed point computation.
    pub accumulator: (String, Expr),
    /// What to do in each iteration. Runs in the context of the child circuit.
    /// The value the last statement evaluates to becomes the accumulator of
    /// the next iteration.
    pub step: BlockStmt,
}

impl Eq for FixedPointIterExpr {}

impl PartialEq for FixedPointIterExpr {
    fn eq(&self, other: &Self) -> bool {
        self.imports == other.imports
            && self.accumulator == other.accumulator
            && self.step == other.step
    }
}

#[derive(Clone, Debug)]
pub enum Literal {
    /// String.
    String(String),
    /// Unsigned integer value of 64 bits.
    Uint(u64),
    /// Signed integer value of 64 bits.
    Iint(i64),
    /// Boolean.
    Bool(bool),
    /// Null.
    // The `Null` variant carries the unit type to align its field-arity with
    // other variants. That eases the definition of macros operating on the enum.
    Null(()),
    /// Relation (an extensional database predicate).
    // With a textual representation of the language, this may be just an
    // identifier and the stream with its schema is defined elsewhere.
    Relation(Relation),
}

impl Eq for Literal {}

impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Literal::String(a), Literal::String(b)) => a == b,
            (Literal::Uint(a), Literal::Uint(b)) => a == b,
            (Literal::Iint(a), Literal::Iint(b)) => a == b,
            (Literal::Bool(a), Literal::Bool(b)) => a == b,
            (Literal::Null(()), Literal::Null(())) => true,
            _ => false,
        }
    }
}

impl From<String> for Literal {
    fn from(value: String) -> Self {
        Literal::String(value)
    }
}

impl From<&str> for Literal {
    fn from(value: &str) -> Self {
        Literal::String(value.to_string())
    }
}

impl From<u64> for Literal {
    fn from(value: u64) -> Self {
        Literal::Uint(value)
    }
}

impl From<i64> for Literal {
    fn from(value: i64) -> Self {
        Literal::Iint(value)
    }
}

impl From<bool> for Literal {
    fn from(value: bool) -> Self {
        Literal::Bool(value)
    }
}

impl From<()> for Literal {
    fn from(_: ()) -> Self {
        Literal::Null(())
    }
}

impl Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::String(value) => write!(f, "{value}"),
            Literal::Uint(value) => write!(f, "{value}"),
            Literal::Iint(value) => write!(f, "{value}"),
            Literal::Bool(value) => write!(f, "{value}"),
            Literal::Null(()) => write!(f, "null"),
            Literal::Relation(value) => write!(f, "relation {}", value.schema.name),
        }
    }
}

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
            Expr::Alias(expr) => self.visit_alias_expr(expr, ctx),
            Expr::Distinct(expr) => self.visit_distinct_expr(expr, ctx),
            Expr::Union(expr) => self.visit_union_expr(expr, ctx),
            Expr::Difference(expr) => self.visit_difference_expr(expr, ctx),
            Expr::Selection(expr) => self.visit_selection_expr(expr, ctx),
            Expr::Projection(expr) => self.visit_projection_expr(expr, ctx),
            Expr::CartesianProduct(expr) => self.visit_cartesian_product_expr(expr, ctx),
            Expr::EquiJoin(expr) => self.visit_equi_join_expr(expr, ctx),
            Expr::AntiJoin(expr) => self.visit_anti_join_expr(expr, ctx),
            Expr::FixedPointIter(expr) => self.visit_fixed_point_iter_expr(expr, ctx),
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
    fn visit_alias_expr(&mut self, expr: &AliasExpr, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: &DistinctExpr, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: &UnionExpr, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: &DifferenceExpr, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: &SelectionExpr, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: &ProjectionExpr, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: &CartesianProductExpr, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: &EquiJoinExpr, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: &AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: &FixedPointIterExpr, ctx: C) -> T;
}

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
            Expr::Alias(expr) => self.visit_alias_expr(expr, ctx),
            Expr::Distinct(expr) => self.visit_distinct_expr(expr, ctx),
            Expr::Union(expr) => self.visit_union_expr(expr, ctx),
            Expr::Difference(expr) => self.visit_difference_expr(expr, ctx),
            Expr::Selection(expr) => self.visit_selection_expr(expr, ctx),
            Expr::Projection(expr) => self.visit_projection_expr(expr, ctx),
            Expr::CartesianProduct(expr) => self.visit_cartesian_product_expr(expr, ctx),
            Expr::EquiJoin(expr) => self.visit_equi_join_expr(expr, ctx),
            Expr::AntiJoin(expr) => self.visit_anti_join_expr(expr, ctx),
            Expr::FixedPointIter(expr) => self.visit_fixed_point_iter_expr(expr, ctx),
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
    fn visit_alias_expr(&mut self, expr: &mut AliasExpr, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: &mut DistinctExpr, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: &mut UnionExpr, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: &mut DifferenceExpr, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: &mut SelectionExpr, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: &mut ProjectionExpr, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: &mut CartesianProductExpr, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: &mut EquiJoinExpr, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: &mut AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: &mut FixedPointIterExpr, ctx: C) -> T;
}

pub trait ExprVisitorOwn<T, C> {
    fn visit_expr(&mut self, expr: Expr, ctx: C) -> T {
        match expr {
            Expr::Literal(expr) => self.visit_literal_expr(*expr, ctx),
            Expr::Tuple(expr) => self.visit_tuple_expr(*expr, ctx),
            Expr::GetIndex(expr) => self.visit_get_index_expr(*expr, ctx),
            Expr::Grouping(expr) => self.visit_grouping_expr(*expr, ctx),
            Expr::Binary(expr) => self.visit_binary_expr(*expr, ctx),
            Expr::Unary(expr) => self.visit_unary_expr(*expr, ctx),
            Expr::Var(expr) => self.visit_var_expr(*expr, ctx),
            Expr::Assign(expr) => self.visit_assign_expr(*expr, ctx),
            Expr::Function(expr) => self.visit_function_expr(*expr, ctx),
            Expr::Call(expr) => self.visit_call_expr(*expr, ctx),
            Expr::Alias(expr) => self.visit_alias_expr(*expr, ctx),
            Expr::Distinct(expr) => self.visit_distinct_expr(*expr, ctx),
            Expr::Union(expr) => self.visit_union_expr(*expr, ctx),
            Expr::Difference(expr) => self.visit_difference_expr(*expr, ctx),
            Expr::Selection(expr) => self.visit_selection_expr(*expr, ctx),
            Expr::Projection(expr) => self.visit_projection_expr(*expr, ctx),
            Expr::CartesianProduct(expr) => self.visit_cartesian_product_expr(*expr, ctx),
            Expr::EquiJoin(expr) => self.visit_equi_join_expr(*expr, ctx),
            Expr::AntiJoin(expr) => self.visit_anti_join_expr(*expr, ctx),
            Expr::FixedPointIter(expr) => self.visit_fixed_point_iter_expr(*expr, ctx),
        }
    }
    fn visit_literal_expr(&mut self, expr: LiteralExpr, ctx: C) -> T;
    fn visit_tuple_expr(&mut self, expr: TupleExpr, ctx: C) -> T;
    fn visit_get_index_expr(&mut self, expr: GetIndexExpr, ctx: C) -> T;
    fn visit_grouping_expr(&mut self, expr: GroupingExpr, ctx: C) -> T;
    fn visit_binary_expr(&mut self, expr: BinaryExpr, ctx: C) -> T;
    fn visit_unary_expr(&mut self, expr: UnaryExpr, ctx: C) -> T;
    fn visit_var_expr(&mut self, expr: VarExpr, ctx: C) -> T;
    fn visit_assign_expr(&mut self, expr: AssignExpr, ctx: C) -> T;
    fn visit_function_expr(&mut self, expr: FunctionExpr, ctx: C) -> T;
    fn visit_call_expr(&mut self, expr: CallExpr, ctx: C) -> T;
    fn visit_alias_expr(&mut self, expr: AliasExpr, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: DistinctExpr, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: UnionExpr, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: DifferenceExpr, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: SelectionExpr, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: ProjectionExpr, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: CartesianProductExpr, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: EquiJoinExpr, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: FixedPointIterExpr, ctx: C) -> T;
}

impl MemAddr for Expr {}
impl MemAddr for LiteralExpr {}
impl MemAddr for TupleExpr {}
impl MemAddr for GroupingExpr {}
impl MemAddr for BinaryExpr {}
impl MemAddr for UnaryExpr {}
impl MemAddr for VarExpr {}
impl MemAddr for AssignExpr {}
impl MemAddr for FunctionExpr {}
impl MemAddr for CallExpr {}
impl MemAddr for AliasExpr {}
impl MemAddr for DistinctExpr {}
impl MemAddr for UnionExpr {}
impl MemAddr for DifferenceExpr {}
impl MemAddr for SelectionExpr {}
impl MemAddr for ProjectionExpr {}
impl MemAddr for CartesianProductExpr {}
impl MemAddr for EquiJoinExpr {}
impl MemAddr for AntiJoinExpr {}
impl MemAddr for FixedPointIterExpr {}
