// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared machinery for the static passes that *rewrite* a plan: the
//! [optimizers](crate::optimizer::Optimizer) and the backend
//! [lowerings](crate::relational::Backend::lower).
//!
//! # What this exists for
//!
//! A rewriting pass is one interesting function surrounded by a traversal. The
//! traversal is the same for every pass: Recurse into the children and rebuild
//! the node. Implementing a folding walk is 27 methods long, one per node kind
//! across the three [owned visitor](crate::host::expr::ExprVisitorOwn)
//! families. Writing those per pass means writing the same 26 uninteresting
//! ones again each time. So they are written *here*, once, and a pass
//! contributes only the interesting part: a [`TransformationRule`].
//!
//! Note what this is **not** an argument for. Walking the plan once per rule
//! instead of once per pass saves nothing worth having: a plan is tens to
//! hundreds of nodes, walked at compile time, not per row. Bundle rules into
//! one [`RewriteDriver`] because you want them to see each other's output,
//! and split them across several because you do not but never for saving the
//! performance cost of the walk.
//!
//! # Rules see relational nodes
//!
//! [`TransformationRule::apply`] is offered [`RelExpr`] nodes, since that is
//! the vocabulary plan rewrites are phrased in. Scalar rewrites (constant
//! folding in a join condition, say) would want a sibling trait offered
//! [`Expr`] nodes; the traversal below already reaches them, so adding one is a
//! matter of a second hook rather than a second walk.
//!
//! # Scheduling
//!
//! [`RewriteDriver::run`] walks the plan repeatedly until a full walk fires
//! nothing. That is all the scheduling there is, and it is enough for one kind
//! of dependency: if rule `A` *enables* rule `B`, `B` fires on a later round,
//! and neither rule has to know the other exists.
//!
//! The other kind — `B` undoes what `A` did — is not a scheduling problem but a
//! modelling error, and no ordering fixes it. So instead of pretending to solve
//! it, the driver bounds the rounds and, on exhausting them, **names the rules
//! that were still firing**. A cycle then shows up as a legible error during
//! development rather than as a plan that quietly stopped halfway. Rules that
//! genuinely pull in opposite directions belong in separate [`RewriteDriver`]s,
//! run in an order you choose.
//!
//! One consequence worth stating: a *mandatory* rule set (a backend lowering,
//! whose whole point is a post-condition) and an *optional* one (the optimizer,
//! which may always decline) should not share a [`RewriteDriver`] pass.
//! Otherwise a correctness guarantee comes to depend on the scheduler,
//! which is the same mistake as folding
//! [`Backend::lower`](crate::relational::Backend::lower) into the
//! [`Optimizer`](crate::optimizer::Optimizer). Share the machinery,
//! not the rule set, and have the mandatory side *verify* its post-condition
//! afterwards (a [`walk`](mod@crate::host::walk) scan) rather than trust that
//! the rules were scheduled right.

use crate::{
    error::RewriteError,
    host::{
        QueryIr,
        expr::{
            AssignExpr, BinaryExpr, CallExpr, Expr, ExprVisitorOwn, FunctionExpr, GetIndexExpr,
            GroupingExpr, LiteralExpr, TupleExpr, UnaryExpr, VarExpr,
        },
        stmt::{BlockStmt, ExprStmt, Stmt, StmtVisitorOwn, VarStmt},
    },
    relational::expr::{
        AliasExpr, AntiJoinExpr, CartesianProductExpr, DifferenceExpr, DistinctExpr, EquiJoinExpr,
        FixedPointIterExpr, JoinVariable, MultiWayEquiJoinExpr, OutputExpr, ProjectionExpr,
        RelExpr, RelExprVisitorOwn, RelKind, SelectionExpr, SourceExpr, UnionExpr,
    },
};

/// When in the traversal a rule is offered a node.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Direction {
    /// Before the node's children are rewritten. What a rule that pushes
    /// something *into* a subtree needs (predicate pushdown has to see the
    /// selection before descending past it).
    TopDown,
    /// After the node's children are rewritten, so the rule sees operands that
    /// are already in their final form. What canonicalization and lowering
    /// need, and the default.
    BottomUp,
}

/// The outcome of offering a node to a rule.
///
/// A rule that declines hands the node back rather than answering a separate
/// "would you fire?" question first. That keeps the structural precondition
/// ([`TransformationRule::interest`]) apart from the semantic one without
/// making the rule take the node apart twice, and it makes "declined" a state
/// the type system tracks.
pub enum Rewritten {
    /// The rule fired; this replaces the node.
    Changed(Expr),
    /// The rule declined; this is the node it was handed, untouched.
    Unchanged(RelExpr),
}

/// One semantics-preserving rewrite of the plan, independent of the traversal
/// that finds the nodes to apply it to.
pub trait TransformationRule {
    /// Identifies the rule in errors and in the driver's cycle report.
    fn name(&self) -> &'static str;

    /// The node kinds this rule could fire on. A dispatch filter, not a full
    /// precondition. Conditions that need to inspect the node's contents
    /// belong in [`apply`](Self::apply), expressed by declining.
    fn interest(&self) -> &'static [RelKind];

    /// When in the traversal this rule wants its nodes. See [`Direction`].
    fn direction(&self) -> Direction {
        Direction::BottomUp
    }

    /// Rewrite `node`, or hand it back unchanged.
    fn apply(&mut self, node: RelExpr) -> Result<Rewritten, RewriteError>;
}

/// A set of rules and the driver that runs them to a fixed point.
pub struct RewriteDriver {
    rules: Vec<Box<dyn TransformationRule>>,
    max_rounds: usize,
}

impl RewriteDriver {
    /// Enough rounds for any cascade a sane rule set produces, few enough that
    /// a cycle is reported promptly.
    pub const DEFAULT_MAX_ROUNDS: usize = 16;

    pub fn new(rules: Vec<Box<dyn TransformationRule>>) -> Self {
        Self {
            rules,
            max_rounds: Self::DEFAULT_MAX_ROUNDS,
        }
    }

    pub fn with_max_rounds(mut self, max_rounds: usize) -> Self {
        self.max_rounds = max_rounds;
        self
    }

    /// Walk `plan` until a full walk fires no rule, and return the result.
    ///
    /// Errors if the rules are still firing after
    /// [`max_rounds`](Self::with_max_rounds) walks, naming the ones that were,
    /// since that is what a pair of rules undoing each other looks like from
    /// here.
    pub fn run(&mut self, plan: QueryIr) -> Result<QueryIr, RewriteError> {
        let mut plan = plan;
        let mut fired = Vec::new();

        for _ in 0..self.max_rounds {
            let mut rewriter = Rewriter {
                rules: &mut self.rules,
                fired: Vec::new(),
            };
            plan = plan
                .into_iter()
                .map(|stmt| rewriter.visit_stmt(stmt, ()))
                .collect::<Result<QueryIr, _>>()?;
            if rewriter.fired.is_empty() {
                return Ok(plan);
            }
            fired = rewriter.fired;
        }

        fired.sort_unstable();
        fired.dedup();
        Err(RewriteError::new(format!(
            "Rewriting did not reach a fixed point after {} rounds; these rules were still \
             firing: {}. Rules that undo one another cannot be ordered apart. Run them in \
             separate rewrites, or narrow the condition under which they fire",
            self.max_rounds,
            fired.join(", ")
        )))
    }
}

/// One walk of the plan. Implements the traversal once for every rewriting
/// pass there will ever be; the passes themselves live in the
/// [rules](TransformationRule).
struct Rewriter<'r> {
    /// The rules to apply in this static pass.
    rules: &'r mut [Box<dyn TransformationRule>],
    /// The rules that fired during this walk, for the fixed-point test and for
    /// the cycle report.
    fired: Vec<&'static str>,
}

type VisitorResult<T> = Result<T, RewriteError>;

impl Rewriter<'_> {
    /// The single recursion point: every child expression passes through here.
    ///
    /// Having exactly one means a top-down rule is offered each node exactly
    /// once per round, *before* that node's children are rewritten and never
    /// again on its own output within the round. Which is what keeps a rule
    /// that fires on what it just produced from recursing until the stack gives
    /// out — it gets its next turn on the next round instead, under the
    /// driver's round budget.
    fn child(&mut self, expr: Expr) -> VisitorResult<Expr> {
        let expr = self.offer(expr, Direction::TopDown)?;
        self.visit_expr(expr, ())
    }

    /// Offers a node to every rule facing `direction` whose interest covers it,
    /// stopping at the first that fires.
    fn offer(&mut self, expr: Expr, direction: Direction) -> VisitorResult<Expr> {
        let mut node = match expr {
            Expr::Relational(node) => node,
            // Rules are phrased over relational operators, so a host expression
            // is only ever passed through. See the module docs.
            other => return Ok(other),
        };
        let kind = node.kind();

        for rule in self.rules.iter_mut() {
            if rule.direction() != direction || !rule.interest().contains(&kind) {
                continue;
            }
            match rule.apply(node)? {
                Rewritten::Changed(expr) => {
                    self.fired.push(rule.name());
                    return Ok(expr);
                }
                Rewritten::Unchanged(declined) => node = declined,
            }
        }

        Ok(Expr::Relational(node))
    }

    fn children(&mut self, exprs: Vec<Expr>) -> VisitorResult<Vec<Expr>> {
        exprs.into_iter().map(|expr| self.child(expr)).collect()
    }

    fn stmts(&mut self, stmts: Vec<Stmt>) -> VisitorResult<Vec<Stmt>> {
        stmts
            .into_iter()
            .map(|stmt| self.visit_stmt(stmt, ()))
            .collect()
    }

    fn pairs(&mut self, pairs: Vec<(Expr, Expr)>) -> VisitorResult<Vec<(Expr, Expr)>> {
        pairs
            .into_iter()
            .map(|(left, right)| Ok((self.child(left)?, self.child(right)?)))
            .collect()
    }

    fn attributes(
        &mut self,
        attributes: Vec<(String, Expr)>,
    ) -> VisitorResult<Vec<(String, Expr)>> {
        attributes
            .into_iter()
            .map(|(name, expr)| Ok((name, self.child(expr)?)))
            .collect()
    }

    fn optional_attributes(
        &mut self,
        attributes: Option<Vec<(String, Expr)>>,
    ) -> VisitorResult<Option<Vec<(String, Expr)>>> {
        attributes
            .map(|attributes| self.attributes(attributes))
            .transpose()
    }

    fn join_variables(&mut self, on: Vec<JoinVariable>) -> VisitorResult<Vec<JoinVariable>> {
        on.into_iter()
            .map(|variable| {
                Ok(JoinVariable {
                    name: variable.name,
                    occurrences: variable
                        .occurrences
                        .into_iter()
                        .map(|(relation, expr)| Ok((relation, self.child(expr)?)))
                        .collect::<VisitorResult<_>>()?,
                })
            })
            .collect()
    }
}

impl StmtVisitorOwn<VisitorResult<Stmt>, ()> for Rewriter<'_> {
    fn visit_var_stmt(&mut self, mut stmt: Box<VarStmt>, _ctx: ()) -> VisitorResult<Stmt> {
        stmt.initializer = stmt
            .initializer
            .map(|initializer| self.child(initializer))
            .transpose()?;
        Ok(stmt.into())
    }

    fn visit_expr_stmt(&mut self, mut stmt: Box<ExprStmt>, _ctx: ()) -> VisitorResult<Stmt> {
        stmt.expr = self.child(stmt.expr)?;
        Ok(stmt.into())
    }

    fn visit_block_stmt(&mut self, mut stmt: Box<BlockStmt>, _ctx: ()) -> VisitorResult<Stmt> {
        stmt.stmts = self.stmts(stmt.stmts)?;
        Ok(stmt.into())
    }
}

impl ExprVisitorOwn<VisitorResult<Expr>, ()> for Rewriter<'_> {
    fn visit_literal_expr(&mut self, expr: Box<LiteralExpr>, _ctx: ()) -> VisitorResult<Expr> {
        Ok(expr.into())
    }

    fn visit_tuple_expr(&mut self, mut expr: Box<TupleExpr>, _ctx: ()) -> VisitorResult<Expr> {
        expr.elements = self.children(expr.elements)?;
        Ok(expr.into())
    }

    fn visit_get_index_expr(
        &mut self,
        mut expr: Box<GetIndexExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.target = self.child(expr.target)?;
        expr.index = self.child(expr.index)?;
        Ok(expr.into())
    }

    fn visit_grouping_expr(
        &mut self,
        mut expr: Box<GroupingExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.expr = self.child(expr.expr)?;
        Ok(expr.into())
    }

    fn visit_binary_expr(&mut self, mut expr: Box<BinaryExpr>, _ctx: ()) -> VisitorResult<Expr> {
        expr.left = self.child(expr.left)?;
        expr.right = self.child(expr.right)?;
        Ok(expr.into())
    }

    fn visit_unary_expr(&mut self, mut expr: Box<UnaryExpr>, _ctx: ()) -> VisitorResult<Expr> {
        expr.operand = self.child(expr.operand)?;
        Ok(expr.into())
    }

    fn visit_var_expr(&mut self, expr: Box<VarExpr>, _ctx: ()) -> VisitorResult<Expr> {
        Ok(expr.into())
    }

    fn visit_assign_expr(&mut self, mut expr: Box<AssignExpr>, _ctx: ()) -> VisitorResult<Expr> {
        expr.value = self.child(expr.value)?;
        Ok(expr.into())
    }

    fn visit_function_expr(
        &mut self,
        mut expr: Box<FunctionExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.body.stmts = self.stmts(expr.body.stmts)?;
        Ok(expr.into())
    }

    fn visit_call_expr(&mut self, mut expr: Box<CallExpr>, _ctx: ()) -> VisitorResult<Expr> {
        expr.callee = self.child(expr.callee)?;
        expr.arguments = self.children(expr.arguments)?;
        Ok(expr.into())
    }

    fn visit_relational_expr(&mut self, expr: RelExpr, _ctx: ()) -> VisitorResult<Expr> {
        self.visit_rel(expr, ())
    }
}

/// Each of these rewrites the node's children and then offers the node itself
/// to the bottom-up rules. The top-down offer already happened, in
/// [`Rewriter::child`], on the way in.
impl RelExprVisitorOwn<VisitorResult<Expr>, ()> for Rewriter<'_> {
    fn visit_source_expr(&mut self, expr: Box<SourceExpr>, _ctx: ()) -> VisitorResult<Expr> {
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_output_expr(&mut self, mut expr: Box<OutputExpr>, _ctx: ()) -> VisitorResult<Expr> {
        expr.relation = self.child(expr.relation)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_alias_expr(&mut self, mut expr: Box<AliasExpr>, _ctx: ()) -> VisitorResult<Expr> {
        expr.relation = self.child(expr.relation)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_distinct_expr(
        &mut self,
        mut expr: Box<DistinctExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.relation = self.child(expr.relation)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_union_expr(&mut self, mut expr: Box<UnionExpr>, _ctx: ()) -> VisitorResult<Expr> {
        expr.relations = self.children(expr.relations)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_difference_expr(
        &mut self,
        mut expr: Box<DifferenceExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.left = self.child(expr.left)?;
        expr.right = self.child(expr.right)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_selection_expr(
        &mut self,
        mut expr: Box<SelectionExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.relation = self.child(expr.relation)?;
        expr.condition = self.child(expr.condition)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_projection_expr(
        &mut self,
        mut expr: Box<ProjectionExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.relation = self.child(expr.relation)?;
        expr.attributes = self.attributes(expr.attributes)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_cartesian_product_expr(
        &mut self,
        mut expr: Box<CartesianProductExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.inner.left = self.child(expr.inner.left)?;
        expr.inner.right = self.child(expr.inner.right)?;
        expr.inner.attributes = self.optional_attributes(expr.inner.attributes)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_equi_join_expr(
        &mut self,
        mut expr: Box<EquiJoinExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.left = self.child(expr.left)?;
        expr.right = self.child(expr.right)?;
        expr.on = self.pairs(expr.on)?;
        expr.attributes = self.optional_attributes(expr.attributes)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_multi_way_equi_join_expr(
        &mut self,
        mut expr: Box<MultiWayEquiJoinExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.relations = self.children(expr.relations)?;
        expr.on = self.join_variables(expr.on)?;
        expr.attributes = self.optional_attributes(expr.attributes)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_anti_join_expr(
        &mut self,
        mut expr: Box<AntiJoinExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.left = self.child(expr.left)?;
        expr.right = self.child(expr.right)?;
        expr.on = self.pairs(expr.on)?;
        self.offer(expr.into(), Direction::BottomUp)
    }

    fn visit_fixed_point_iter_expr(
        &mut self,
        mut expr: Box<FixedPointIterExpr>,
        _ctx: (),
    ) -> VisitorResult<Expr> {
        expr.accumulator.1 = self.child(expr.accumulator.1)?;
        expr.step.stmts = self.stmts(expr.step.stmts)?;
        self.offer(expr.into(), Direction::BottomUp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        host::{
            expr::VarExpr,
            walk::{self, Node},
        },
        relational::expr::{DistinctExpr, OutputKind, SinkId},
    };

    fn relation(name: &str) -> Expr {
        Expr::from(VarExpr::new(name))
    }

    fn plan(expr: Expr) -> QueryIr {
        QueryIr::new(vec![Stmt::from(ExprStmt { expr })])
    }

    fn kinds(plan: &QueryIr) -> Vec<RelKind> {
        walk::pre_order(plan)
            .filter_map(Node::as_rel)
            .map(RelExpr::kind)
            .collect()
    }

    /// Wraps every [`RelKind::Distinct`] it is offered in another one, up to
    /// `budget` times in total. A rule that fires on its own output, which is
    /// what the round budget and the once-per-node offer exist for.
    struct Nest {
        budget: usize,
        direction: Direction,
    }

    impl TransformationRule for Nest {
        fn name(&self) -> &'static str {
            "nest"
        }
        fn interest(&self) -> &'static [RelKind] {
            &[RelKind::Distinct]
        }
        fn direction(&self) -> Direction {
            self.direction
        }
        fn apply(&mut self, node: RelExpr) -> Result<Rewritten, RewriteError> {
            if self.budget == 0 {
                return Ok(Rewritten::Unchanged(node));
            }
            self.budget -= 1;
            Ok(Rewritten::Changed(Expr::from(DistinctExpr {
                relation: Expr::from(node),
            })))
        }
    }

    /// Declines everything, so a walk over it must be a no-op.
    struct Inert;

    impl TransformationRule for Inert {
        fn name(&self) -> &'static str {
            "inert"
        }
        fn interest(&self) -> &'static [RelKind] {
            RelKind::ALL
        }
        fn apply(&mut self, node: RelExpr) -> Result<Rewritten, RewriteError> {
            Ok(Rewritten::Unchanged(node))
        }
    }

    #[test]
    fn a_rule_that_declines_everything_leaves_the_plan_alone() {
        let original = plan(Expr::from(OutputExpr {
            relation: Expr::from(DistinctExpr {
                relation: relation("r"),
            }),
            id: SinkId::from("out"),
            kind: OutputKind::Channel,
        }));

        let rewritten = RewriteDriver::new(vec![Box::new(Inert)])
            .run(original.clone())
            .expect("A rule that never fires cannot fail to converge");

        assert_eq!(rewritten, original);
    }

    #[test]
    fn reaches_a_fixed_point_when_the_rules_stop_firing() {
        // Two nestings, so the rule has to be offered its own output — which
        // only happens on a later round.
        let rewritten = RewriteDriver::new(vec![Box::new(Nest {
            budget: 2,
            direction: Direction::BottomUp,
        })])
        .run(plan(Expr::from(DistinctExpr {
            relation: relation("r"),
        })))
        .expect("The rule runs out of budget and the walk settles");

        assert_eq!(
            kinds(&rewritten),
            vec![RelKind::Distinct, RelKind::Distinct, RelKind::Distinct]
        );
    }

    #[test]
    fn a_top_down_rule_does_not_recurse_on_its_own_output() {
        // The same rule from the other direction. Were the replacement
        // re-offered at the node it came from, this would recurse until the
        // stack gave out instead of settling after three rounds.
        let rewritten = RewriteDriver::new(vec![Box::new(Nest {
            budget: 2,
            direction: Direction::TopDown,
        })])
        .run(plan(Expr::from(DistinctExpr {
            relation: relation("r"),
        })))
        .expect("The rule runs out of budget and the walk settles");

        assert_eq!(
            kinds(&rewritten),
            vec![RelKind::Distinct, RelKind::Distinct, RelKind::Distinct]
        );
    }

    #[test]
    fn reports_the_rules_that_kept_firing_when_it_cannot_converge() {
        // `usize::MAX` budget: the rule never stops, which is what a pair of
        // rules undoing each other looks like from the driver's side.
        let error = RewriteDriver::new(vec![Box::new(Nest {
            budget: usize::MAX,
            direction: Direction::BottomUp,
        })])
        .with_max_rounds(4)
        .run(plan(Expr::from(DistinctExpr {
            relation: relation("r"),
        })))
        .expect_err("A rule that always fires must not spin forever");

        assert!(error.message.contains("nest"), "{}", error.message);
        assert!(error.message.contains('4'), "{}", error.message);
    }

    #[test]
    fn skips_rules_whose_interest_does_not_cover_the_node() {
        // `Nest` asks for `Distinct` only, so a plan without one must come back
        // untouched however many rounds it is given.
        let original = plan(Expr::from(OutputExpr {
            relation: relation("r"),
            id: SinkId::from("out"),
            kind: OutputKind::Channel,
        }));

        let rewritten = RewriteDriver::new(vec![Box::new(Nest {
            budget: usize::MAX,
            direction: Direction::BottomUp,
        })])
        .run(original.clone())
        .expect("An uninterested rule is never offered the node");

        assert_eq!(rewritten, original);
    }
}
