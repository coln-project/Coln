// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The relational-algebra plan layer.
//!
//! [`RelExpr`] is the backend-neutral query-plan operator vocabulary (relation →
//! relation). Every backend implements the [`RelExprVisitor`] family with its
//! own return type; the plan itself is shared. Relational operators are *also*
//! host expressions (via [`Expr::Relational`]), so operands stay [`Expr`], which
//! preserves relation-valued variables, nested operators, and tuple-of-relations.

use crate::{
    host::{expr::Expr, stmt::BlockStmt},
    relational::RelationSchema,
    util::MemAddr,
};

/// Relational-algebra operator = backend-neutral query-plan vocabulary.
///
/// Operands stay [`Expr`] (a host expression that must evaluate to a relation),
/// which preserves relation-valued vars, nested ops, and tuple-of-relations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelExpr {
    /// Leaf of the relational plan: names an extensional relation without
    /// carrying its runtime representation (no stream, no table). The backend
    /// binds the [`SourceId`] to a concrete relation at execution time.
    Source(Box<SourceExpr>),
    Output(Box<OutputExpr>),
    Alias(Box<AliasExpr>),
    Distinct(Box<DistinctExpr>),
    // A union can also be expressed with a full outer join and a projection.
    Union(Box<UnionExpr>),
    // As the antijoin is a generalization of the set difference, this may be
    // removed in the future.
    Difference(Box<DifferenceExpr>),
    Selection(Box<SelectionExpr>),
    Projection(Box<ProjectionExpr>),
    CartesianProduct(Box<CartesianProductExpr>),
    EquiJoin(Box<EquiJoinExpr>),
    MultiWayEquiJoin(Box<MultiWayEquiJoin>),
    AntiJoin(Box<AntiJoinExpr>),
    FixedPointIter(Box<FixedPointIterExpr>),
}

/// Generates `From<XxxExpr> for RelExpr` (boxing into the given variant) and the
/// composed `From<XxxExpr> for Expr` (via [`Expr::Relational`]) so that
/// constructing a host expression from a relational operator stays a single
/// `Expr::from(..)`/`.into()` call, exactly as before the host/relational split.
macro_rules! impl_rel_and_expr_from {
    ($(($variant:path, $expr:ty)),* $(,)?) => {
        $(
            impl From<$expr> for RelExpr {
                fn from(value: $expr) -> Self {
                    $variant(Box::new(value))
                }
            }
            impl From<$expr> for Expr {
                fn from(value: $expr) -> Self {
                    Expr::Relational(Box::new(RelExpr::from(value)))
                }
            }
        )*
    };
}

impl_rel_and_expr_from! {
    (RelExpr::Source, SourceExpr),
    (RelExpr::Output, OutputExpr),
    (RelExpr::Alias, AliasExpr),
    (RelExpr::Distinct, DistinctExpr),
    (RelExpr::Union, UnionExpr),
    (RelExpr::Difference, DifferenceExpr),
    (RelExpr::Selection, SelectionExpr),
    (RelExpr::Projection, ProjectionExpr),
    (RelExpr::CartesianProduct, CartesianProductExpr),
    (RelExpr::EquiJoin, EquiJoinExpr),
    (RelExpr::AntiJoin, AntiJoinExpr),
    (RelExpr::FixedPointIter, FixedPointIterExpr),
}

/// The single bridge from the relational layer back into the host layer: a
/// relational operator is *also* a host expression.
impl From<RelExpr> for Expr {
    fn from(value: RelExpr) -> Self {
        Expr::Relational(Box::new(value))
    }
}

/// Backend-neutral identity of a relation source. The tree only ever *names* a
/// source; the backend maps it to a concrete relation (DBSP stream, batch Z-set,
/// or SQL table/view) at execution time.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T: Into<String>> From<T> for SourceId {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

/// Backend-neutral relation leaf. Carries a schema but no stream and no table.
/// Its identity is the schema name, so it is derived rather than stored.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceExpr {
    pub schema: RelationSchema,
}

impl SourceExpr {
    /// Build a source leaf from a schema. This is how a (circuit-free) plan names
    /// an extensional input; the backend later binds its [`id`](Self::to_id) to a
    /// concrete relation.
    pub fn new(schema: RelationSchema) -> Self {
        Self { schema }
    }

    pub fn as_id(&self) -> &str {
        &self.schema.name
    }

    /// The extensional input this leaf names. Derived from the schema name, so it
    /// can never disagree with the schema.
    pub fn to_id(&self) -> String {
        self.schema.name.clone()
    }
}

/// Backend-neutral identity of a query output. Mirrors [`SourceId`] on the input
/// side: the plan only ever *names* a sink; the backend maps the name to a live
/// destination (a read handle, a CLI printer, …) at execution time. Replaces the
/// old positional `OutputId` so outputs are addressed by name.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SinkId(pub String);

impl SinkId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T: Into<String>> From<T> for SinkId {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

/// Where an [`OutputExpr`] sends the rows it taps. Pure data — the backend binds
/// each variant to a concrete destination; nothing runtime-stateful lives in the
/// plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum OutputKind {
    /// Print the (intermediate) rows to the CLI for debugging. The rows still
    /// flow downstream unchanged.
    Cli,
    /// Expose the rows as a named runtime output channel the driver reads via
    /// `Runtime::output`.
    Channel,
}

/// Taps a relation for output. This is a **pass-through** operator: it evaluates
/// to its input [`relation`](Self::relation) unchanged, so it can sit at the root
/// of a plan or splice into the middle of one (e.g. an [`OutputKind::Cli`] tap on
/// an intermediate result). The backend discovers every `OutputExpr` by walking
/// the plan and wires a destination for its [`SinkId`], exactly as it wires a
/// [`SourceExpr`] leaf on the input side.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputExpr {
    /// The relation to tap. Returned unchanged so downstream operators are
    /// unaffected by the tap.
    pub relation: Expr,
    /// The name this output is addressed by (`Runtime::output`, CLI label).
    pub id: SinkId,
    /// Where the tapped rows go.
    pub kind: OutputKind,
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
    /// The attribute(s) to join on. The first element of any pair is evaluated
    /// in the context of the left relation, and the second element of any pair
    /// is evaluated in the context of the right relation.
    ///
    /// If `on` is empty, a [`CartesianProduct`](CartesianProductExpr) is computed.
    pub on: Vec<(Expr, Expr)>,
    /// An optional projection step. See documentation of [`ProjectionExpr`].
    pub attributes: Option<Vec<(String, Expr)>>,
}

/// An equijoin involving `N` relations. A better input than a folded sequence
/// of [binary `EquiJoin`s](EquiJoinExpr) for worst-case optimal join algorithms
/// (such as the leapfrog triejoin).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiWayEquiJoin {
    /// The `N` relations which participate in the join. Each [`Expr`] must
    /// evaluate to a relation.
    pub relations: Vec<Expr>,
    /// Each entry in the outer vector corresponds to a variable which must be
    /// equal among all its occurrences. The inner vector vector tracks the
    /// occurrences for each variable. The inner vector is _guaranteed_ to have
    /// the same arity as the [`relations`](Self::relations) vector. An entry
    /// at index `i` in the inner vector with value `None` indicates that the
    /// corresponding relation ([`relations[i]`](Self::relations)) does _not_
    /// bind the variable, whereas a value of [`Some(Expr)`](Expr) binds the
    /// variable to the value of the `Expr` evaluated in the context of the
    /// corresponding relation (which is again [`relations[i]`](Self::relations)).
    ///
    /// If `on` is empty, a [`CartesianProduct`](CartesianProductExpr) is computed.
    pub on: Vec<Vec<Option<Expr>>>,
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
    // Can be subsumed by the EquiJoin/MultiWayJoin, but better
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
///
/// The step statements may freely reference relations defined in the enclosing
/// scope as well as [`SourceExpr`] leaves; the backend is responsible for
/// bringing those into the iteration (in the DBSP backend, via `delta0`). This
/// keeps the node declarative: it describes *what* is iterated, not *how* the
/// backend must wire outer relations in.
#[derive(Clone, Debug)]
pub struct FixedPointIterExpr {
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
        self.accumulator == other.accumulator && self.step == other.step
    }
}

/// Pure dispatch contract for the relational plan layer: a provided `visit_rel`
/// router plus one required method per operator. No semantics are baked in; each
/// backend implements the per-node methods with its own return type `T`.
pub trait RelExprVisitor<T, C> {
    fn visit_rel(&mut self, expr: &RelExpr, ctx: C) -> T {
        match expr {
            RelExpr::Source(expr) => self.visit_source_expr(expr, ctx),
            RelExpr::Output(expr) => self.visit_output_expr(expr, ctx),
            RelExpr::Alias(expr) => self.visit_alias_expr(expr, ctx),
            RelExpr::Distinct(expr) => self.visit_distinct_expr(expr, ctx),
            RelExpr::Union(expr) => self.visit_union_expr(expr, ctx),
            RelExpr::Difference(expr) => self.visit_difference_expr(expr, ctx),
            RelExpr::Selection(expr) => self.visit_selection_expr(expr, ctx),
            RelExpr::Projection(expr) => self.visit_projection_expr(expr, ctx),
            RelExpr::CartesianProduct(expr) => self.visit_cartesian_product_expr(expr, ctx),
            RelExpr::EquiJoin(expr) => self.visit_equi_join_expr(expr, ctx),
            RelExpr::MultiWayEquiJoin(expr) => self.visit_multi_way_equi_join_expr(expr, ctx),
            RelExpr::AntiJoin(expr) => self.visit_anti_join_expr(expr, ctx),
            RelExpr::FixedPointIter(expr) => self.visit_fixed_point_iter_expr(expr, ctx),
        }
    }
    fn visit_source_expr(&mut self, expr: &SourceExpr, ctx: C) -> T;
    fn visit_output_expr(&mut self, expr: &OutputExpr, ctx: C) -> T;
    fn visit_alias_expr(&mut self, expr: &AliasExpr, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: &DistinctExpr, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: &UnionExpr, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: &DifferenceExpr, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: &SelectionExpr, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: &ProjectionExpr, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: &CartesianProductExpr, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: &EquiJoinExpr, ctx: C) -> T;
    fn visit_multi_way_equi_join_expr(&mut self, expr: &MultiWayEquiJoin, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: &AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: &FixedPointIterExpr, ctx: C) -> T;
}

pub trait RelExprVisitorMut<T, C> {
    fn visit_rel(&mut self, expr: &mut RelExpr, ctx: C) -> T {
        match expr {
            RelExpr::Source(expr) => self.visit_source_expr(expr, ctx),
            RelExpr::Output(expr) => self.visit_output_expr(expr, ctx),
            RelExpr::Alias(expr) => self.visit_alias_expr(expr, ctx),
            RelExpr::Distinct(expr) => self.visit_distinct_expr(expr, ctx),
            RelExpr::Union(expr) => self.visit_union_expr(expr, ctx),
            RelExpr::Difference(expr) => self.visit_difference_expr(expr, ctx),
            RelExpr::Selection(expr) => self.visit_selection_expr(expr, ctx),
            RelExpr::Projection(expr) => self.visit_projection_expr(expr, ctx),
            RelExpr::CartesianProduct(expr) => self.visit_cartesian_product_expr(expr, ctx),
            RelExpr::EquiJoin(expr) => self.visit_equi_join_expr(expr, ctx),
            RelExpr::MultiWayEquiJoin(expr) => self.visit_multi_way_equi_join_expr(expr, ctx),
            RelExpr::AntiJoin(expr) => self.visit_anti_join_expr(expr, ctx),
            RelExpr::FixedPointIter(expr) => self.visit_fixed_point_iter_expr(expr, ctx),
        }
    }
    fn visit_source_expr(&mut self, expr: &mut SourceExpr, ctx: C) -> T;
    fn visit_output_expr(&mut self, expr: &mut OutputExpr, ctx: C) -> T;
    fn visit_alias_expr(&mut self, expr: &mut AliasExpr, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: &mut DistinctExpr, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: &mut UnionExpr, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: &mut DifferenceExpr, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: &mut SelectionExpr, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: &mut ProjectionExpr, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: &mut CartesianProductExpr, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: &mut EquiJoinExpr, ctx: C) -> T;
    fn visit_multi_way_equi_join_expr(&mut self, expr: &mut MultiWayEquiJoin, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: &mut AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: &mut FixedPointIterExpr, ctx: C) -> T;
}

pub trait RelExprVisitorOwn<T, C> {
    fn visit_rel(&mut self, expr: RelExpr, ctx: C) -> T {
        match expr {
            RelExpr::Source(expr) => self.visit_source_expr(*expr, ctx),
            RelExpr::Output(expr) => self.visit_output_expr(*expr, ctx),
            RelExpr::Alias(expr) => self.visit_alias_expr(*expr, ctx),
            RelExpr::Distinct(expr) => self.visit_distinct_expr(*expr, ctx),
            RelExpr::Union(expr) => self.visit_union_expr(*expr, ctx),
            RelExpr::Difference(expr) => self.visit_difference_expr(*expr, ctx),
            RelExpr::Selection(expr) => self.visit_selection_expr(*expr, ctx),
            RelExpr::Projection(expr) => self.visit_projection_expr(*expr, ctx),
            RelExpr::CartesianProduct(expr) => self.visit_cartesian_product_expr(*expr, ctx),
            RelExpr::EquiJoin(expr) => self.visit_equi_join_expr(*expr, ctx),
            RelExpr::MultiWayEquiJoin(expr) => self.visit_multi_way_equi_join_expr(*expr, ctx),
            RelExpr::AntiJoin(expr) => self.visit_anti_join_expr(*expr, ctx),
            RelExpr::FixedPointIter(expr) => self.visit_fixed_point_iter_expr(*expr, ctx),
        }
    }
    fn visit_source_expr(&mut self, expr: SourceExpr, ctx: C) -> T;
    fn visit_output_expr(&mut self, expr: OutputExpr, ctx: C) -> T;
    fn visit_alias_expr(&mut self, expr: AliasExpr, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: DistinctExpr, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: UnionExpr, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: DifferenceExpr, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: SelectionExpr, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: ProjectionExpr, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: CartesianProductExpr, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: EquiJoinExpr, ctx: C) -> T;
    fn visit_multi_way_equi_join_expr(&mut self, expr: MultiWayEquiJoin, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: FixedPointIterExpr, ctx: C) -> T;
}

impl MemAddr for RelExpr {}
impl MemAddr for SourceExpr {}
impl MemAddr for OutputExpr {}
impl MemAddr for AliasExpr {}
impl MemAddr for DistinctExpr {}
impl MemAddr for UnionExpr {}
impl MemAddr for DifferenceExpr {}
impl MemAddr for SelectionExpr {}
impl MemAddr for ProjectionExpr {}
impl MemAddr for CartesianProductExpr {}
impl MemAddr for EquiJoinExpr {}
impl MemAddr for MultiWayEquiJoin {}
impl MemAddr for AntiJoinExpr {}
impl MemAddr for ThetaJoinExpr {}
impl MemAddr for FixedPointIterExpr {}
