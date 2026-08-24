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
    error::SyntaxError,
    host::{expr::Expr, stmt::BlockStmt},
    relational::RelationSchema,
    util::MemAddr,
};
use std::collections::HashSet;

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
    MultiWayEquiJoin(Box<MultiWayEquiJoinExpr>),
    AntiJoin(Box<AntiJoinExpr>),
    FixedPointIter(Box<FixedPointIterExpr>),
}

/// Generates `From<XxxExpr> for RelExpr` (boxing into the given variant) and the
/// composed `From<XxxExpr> for Expr` (via [`Expr::Relational`]) so that
/// constructing a host expression from a relational operator stays a single
/// `Expr::from(..)`/`.into()` call, exactly as before the host/relational split.
///
/// Each also comes in a `Box<XxxExpr>` flavour, which reuses the allocation the
/// caller already holds. That is what an owned rewriting pass rebuilds an
/// untouched node with — see [`RelExprVisitorOwn`] — and since
/// [`Expr::Relational`] does not box what it wraps, that route allocates
/// nothing at all.
macro_rules! impl_rel_and_expr_from {
    ($(($variant:path, $expr:ty)),* $(,)?) => {
        $(
            impl From<$expr> for RelExpr {
                fn from(value: $expr) -> Self {
                    $variant(Box::new(value))
                }
            }
            impl From<Box<$expr>> for RelExpr {
                fn from(value: Box<$expr>) -> Self {
                    $variant(value)
                }
            }
            impl From<$expr> for Expr {
                fn from(value: $expr) -> Self {
                    Expr::Relational(RelExpr::from(value))
                }
            }
            impl From<Box<$expr>> for Expr {
                fn from(value: Box<$expr>) -> Self {
                    Expr::Relational(RelExpr::from(value))
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
    (RelExpr::MultiWayEquiJoin, MultiWayEquiJoinExpr),
    (RelExpr::AntiJoin, AntiJoinExpr),
    (RelExpr::FixedPointIter, FixedPointIterExpr),
}

/// The single bridge from the relational layer back into the host layer: a
/// relational operator is *also* a host expression. Free of charge, since
/// [`Expr::Relational`] does not box what it wraps.
impl From<RelExpr> for Expr {
    fn from(value: RelExpr) -> Self {
        Expr::Relational(value)
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

/// The position of a relation within [`MultiWayEquiJoinExpr::relations`].
pub type RelationIdx = usize;

/// One equality class of a [`MultiWayEquiJoinExpr`]: every listed occurrence
/// must produce the same value for a tuple to enter the join's output.
///
/// A variable bound by only *one* relation is deliberately not representable
/// here: It constrains nothing, so it is not part of a join condition. Such a
/// variable still reaches the output, carried by its relation's schema like any
/// other non-join attribute. Keeping them out is what makes
/// [`MultiWayEquiJoinExpr::on`]`.is_empty()` an exact test for "nothing to join
/// on".
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JoinVariable {
    /// The name the joined attribute carries in the output schema.
    ///
    /// The lowering from coln's FLIR projects every atom onto the names of the
    /// variables it binds, so there the occurrences are plain column picks that
    /// already agree on this name, and the schema fold described on
    /// [`MultiWayEquiJoinExpr::on`] keeps exactly one active copy of it. When
    /// the occurrences do *not* agree on a name (`l.a = r.b`), producing this
    /// name is the job of whoever lowers the join.
    pub name: String,
    /// Which relations bind this variable, and how: the [`RelationIdx`] indexes
    /// into [`MultiWayEquiJoinExpr::relations`], and the [`Expr`] is evaluated
    /// in the context of that relation.
    ///
    /// Invariants, enforced by [`MultiWayEquiJoinExpr::new`]: at least two
    /// occurrences, every index in bounds, indices pairwise distinct, and
    /// ordered by index.
    pub occurrences: Vec<(RelationIdx, Expr)>,
}

/// An equijoin involving `N >= 2` relations. A better input than a folded
/// sequence of [binary `EquiJoin`s](EquiJoinExpr) for worst-case optimal join
/// algorithms (such as the leapfrog triejoin), which are variable-oriented:
/// they iterate a variable ordering, which is what [`on`](Self::on) spells out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiWayEquiJoinExpr {
    /// The `N >= 2` relations which participate in the join. Each [`Expr`] must
    /// evaluate to a relation.
    pub relations: Vec<Expr>,
    /// The join condition, as one [`JoinVariable`] per equality class of
    /// attributes that have to agree.
    ///
    /// If `on` is empty, a [`CartesianProduct`](CartesianProductExpr) over
    /// [`relations`](Self::relations) is computed. Since a variable bound by a
    /// single relation cannot be a [`JoinVariable`], that test is exact rather
    /// than approximate.
    ///
    /// **Output schema.** Joining folds
    /// [`RelationSchema::join`](crate::relational::RelationSchema::join) left to
    /// right, which deactivates an attribute of a later relation when an earlier
    /// one already contributes an active attribute of the same name. A join
    /// variable whose occurrences agree on their name therefore appears **once**
    /// in the output, carried by the first relation that binds it — no
    /// de-duplicating projection is required, and no join column is silently
    /// duplicated.
    pub on: Vec<JoinVariable>,
    /// An optional projection step. See documentation of [`ProjectionExpr`].
    pub attributes: Option<Vec<(String, Expr)>>,
}

impl MultiWayEquiJoinExpr {
    /// The only constructor that cannot produce a malformed join: it normalizes
    /// each [`JoinVariable`]'s occurrences into relation order and then applies
    /// [`validate`](Self::validate).
    pub fn new(
        relations: Vec<Expr>,
        on: Vec<JoinVariable>,
        attributes: Option<Vec<(String, Expr)>>,
    ) -> Result<Self, SyntaxError> {
        let mut joined = Self {
            relations,
            on,
            attributes,
        };
        for variable in &mut joined.on {
            variable.occurrences.sort_by_key(|(relation, _)| *relation);
        }
        joined.validate()?;
        Ok(joined)
    }

    /// Checks the invariants documented on [`Self::relations`] and
    /// [`JoinVariable::occurrences`]. [`Self::new`] applies this to everything
    /// it builds; the resolver re-applies it because the fields are public and
    /// a plan may also be assembled or rewritten by hand.
    pub fn validate(&self) -> Result<(), SyntaxError> {
        if self.relations.len() < 2 {
            return Err(SyntaxError::new(format!(
                "A multi way equi join requires at least two relations, got {}",
                self.relations.len()
            )));
        }
        let mut names = HashSet::with_capacity(self.on.len());
        for variable in &self.on {
            if !names.insert(&variable.name) {
                return Err(SyntaxError::new(format!(
                    "Join variable '{}' is declared twice",
                    variable.name
                )));
            }
            if variable.occurrences.len() < 2 {
                return Err(SyntaxError::new(format!(
                    "Join variable '{}' has {} occurrence(s): below two it constrains \
                     nothing, and a variable bound by a single relation reaches the \
                     output through that relation's schema instead",
                    variable.name,
                    variable.occurrences.len()
                )));
            }
            let mut relations = HashSet::with_capacity(variable.occurrences.len());
            for (relation, _) in &variable.occurrences {
                if *relation >= self.relations.len() {
                    return Err(SyntaxError::new(format!(
                        "Join variable '{}' refers to relation {relation} but the join \
                         has only {} relations",
                        variable.name,
                        self.relations.len()
                    )));
                }
                if !relations.insert(relation) {
                    return Err(SyntaxError::new(format!(
                        "Join variable '{}' occurs twice in relation {relation}: a \
                         variable repeated within one relation is a local equality \
                         condition on that relation, not a join condition",
                        variable.name
                    )));
                }
            }
        }
        Ok(())
    }

    /// Every [`Expr`] nested in the join condition, in [`on`](Self::on) order.
    /// Each one is evaluated in the context of *its own* relation, so a consumer
    /// that needs to know which relation must iterate [`on`](Self::on) directly.
    pub fn on_exprs(&self) -> impl Iterator<Item = &Expr> {
        self.on
            .iter()
            .flat_map(|variable| variable.occurrences.iter().map(|(_, expr)| expr))
    }

    /// The [`on_exprs`](Self::on_exprs) counterpart for rewriting passes.
    /// Handing out `&mut Expr` cannot break any invariant, as those constrain
    /// the arity and the relation indices rather than the expressions.
    pub fn on_exprs_mut(&mut self) -> impl Iterator<Item = &mut Expr> {
        self.on
            .iter_mut()
            .flat_map(|variable| variable.occurrences.iter_mut().map(|(_, expr)| expr))
    }
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
    /// The attributes the two relations are compared on: a `left` row is
    /// suppressed exactly when some `right` row agrees with it on all of them.
    /// The first element of any pair is evaluated in the context of the left
    /// relation, the second in the context of the right one, and each pair
    /// should produce the same type.
    ///
    /// Note that this is the key to match *on*, in the same sense as
    /// [`EquiJoinExpr::on`] — the columns that survive into the output are not
    /// expressed here at all, since the output carries the left relation's
    /// schema unchanged.
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
    fn visit_multi_way_equi_join_expr(&mut self, expr: &MultiWayEquiJoinExpr, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: &AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: &FixedPointIterExpr, ctx: C) -> T;
}

/// Annotating visitor. See [`RelExprVisitorOwn`].
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
    fn visit_multi_way_equi_join_expr(&mut self, expr: &mut MultiWayEquiJoinExpr, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: &mut AntiJoinExpr, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: &mut FixedPointIterExpr, ctx: C) -> T;
}

/// Restructuring visitor for the relational layer, and the family a
/// backend-specific *lowering* pass lives in — see
/// [`ExprVisitorOwn`](crate::host::expr::ExprVisitorOwn) for the rule that
/// decides between the three families, and for why the payloads arrive boxed.
pub trait RelExprVisitorOwn<T, C> {
    fn visit_rel(&mut self, expr: RelExpr, ctx: C) -> T {
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
    fn visit_source_expr(&mut self, expr: Box<SourceExpr>, ctx: C) -> T;
    fn visit_output_expr(&mut self, expr: Box<OutputExpr>, ctx: C) -> T;
    fn visit_alias_expr(&mut self, expr: Box<AliasExpr>, ctx: C) -> T;
    fn visit_distinct_expr(&mut self, expr: Box<DistinctExpr>, ctx: C) -> T;
    fn visit_union_expr(&mut self, expr: Box<UnionExpr>, ctx: C) -> T;
    fn visit_difference_expr(&mut self, expr: Box<DifferenceExpr>, ctx: C) -> T;
    fn visit_selection_expr(&mut self, expr: Box<SelectionExpr>, ctx: C) -> T;
    fn visit_projection_expr(&mut self, expr: Box<ProjectionExpr>, ctx: C) -> T;
    fn visit_cartesian_product_expr(&mut self, expr: Box<CartesianProductExpr>, ctx: C) -> T;
    fn visit_equi_join_expr(&mut self, expr: Box<EquiJoinExpr>, ctx: C) -> T;
    fn visit_multi_way_equi_join_expr(&mut self, expr: Box<MultiWayEquiJoinExpr>, ctx: C) -> T;
    fn visit_anti_join_expr(&mut self, expr: Box<AntiJoinExpr>, ctx: C) -> T;
    fn visit_fixed_point_iter_expr(&mut self, expr: Box<FixedPointIterExpr>, ctx: C) -> T;
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
impl MemAddr for MultiWayEquiJoinExpr {}
impl MemAddr for AntiJoinExpr {}
impl MemAddr for ThetaJoinExpr {}
impl MemAddr for FixedPointIterExpr {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::expr::VarExpr;

    /// A stand-in relation operand. [`MultiWayEquiJoinExpr::validate`] only ever
    /// counts these, so their content is irrelevant.
    fn relations(count: usize) -> Vec<Expr> {
        (0..count)
            .map(|idx| Expr::from(VarExpr::new(format!("r{idx}"))))
            .collect()
    }

    fn join_variable(name: &str, occurrences: &[RelationIdx]) -> JoinVariable {
        JoinVariable {
            name: name.to_string(),
            occurrences: occurrences
                .iter()
                .map(|relation| (*relation, Expr::from(VarExpr::new(name))))
                .collect(),
        }
    }

    #[test]
    fn accepts_a_join_variable_shared_by_two_relations() {
        let joined =
            MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0, 1])], None)
                .expect("A variable bound by two relations is a join variable");
        assert_eq!(joined.on.len(), 1);
        assert_eq!(joined.on_exprs().count(), 2);
    }

    #[test]
    fn accepts_an_empty_join_condition_as_a_cartesian_product() {
        let joined = MultiWayEquiJoinExpr::new(relations(3), vec![], None)
            .expect("An empty join condition is a cartesian product, not an error");
        assert!(joined.on.is_empty());
    }

    #[test]
    fn rejects_fewer_than_two_relations() {
        for count in 0..2 {
            assert!(
                MultiWayEquiJoinExpr::new(relations(count), vec![], None).is_err(),
                "A join over {count} relation(s) should be rejected"
            );
        }
    }

    #[test]
    fn rejects_a_single_occurrence_because_it_constrains_nothing() {
        // The whole point of the `on` representation: a variable bound by only
        // one relation is not an equality class. It reaches the output through
        // that relation's schema instead, which is why rejecting it here is safe
        // and keeps `on.is_empty()` an exact cartesian-product test.
        let error = MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0])], None)
            .expect_err("A single occurrence must not be representable");
        assert!(error.to_string().contains("occurrence"));
    }

    #[test]
    fn rejects_an_out_of_bounds_relation_index() {
        assert!(
            MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0, 2])], None)
                .is_err()
        );
    }

    #[test]
    fn rejects_a_variable_occurring_twice_in_one_relation() {
        // Such a repetition is a local equality condition on that one relation,
        // so it belongs in a `SelectionExpr` beneath the join.
        assert!(
            MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0, 0])], None)
                .is_err()
        );
    }

    #[test]
    fn rejects_two_join_variables_claiming_the_same_output_name() {
        assert!(
            MultiWayEquiJoinExpr::new(
                relations(3),
                vec![join_variable("x", &[0, 1]), join_variable("x", &[1, 2])],
                None
            )
            .is_err()
        );
    }

    #[test]
    fn normalizes_occurrences_into_relation_order() {
        // Plans have to be reproducible: the occurrence order must not depend on
        // the order the producer happened to discover the occurrences in.
        let joined =
            MultiWayEquiJoinExpr::new(relations(3), vec![join_variable("x", &[2, 0, 1])], None)
                .expect("Out-of-order occurrences are normalized, not rejected");
        let order: Vec<RelationIdx> = joined.on[0]
            .occurrences
            .iter()
            .map(|(relation, _)| *relation)
            .collect();
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn validate_agrees_with_new_on_hand_assembled_joins() {
        // The fields are public, so a hand-built or rewritten plan can violate
        // the invariants; the resolver relies on `validate` catching that.
        let malformed = MultiWayEquiJoinExpr {
            relations: relations(2),
            on: vec![join_variable("x", &[0])],
            attributes: None,
        };
        assert!(malformed.validate().is_err());
    }
}
