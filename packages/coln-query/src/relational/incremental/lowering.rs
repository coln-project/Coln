// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The DBSP backend's [lowering](crate::relational::Backend::lower) pass: it
//! folds every [`MultiWayEquiJoinExpr`] into a left-deep chain of binary
//! [`EquiJoinExpr`]s, which is the only join shape a DBSP circuit can be built
//! from.
//!
//! # Why a pass and not a step of the interpreter
//!
//! The two join nodes are not the same operator at two arities. A
//! [`MultiWayEquiJoinExpr`] is *variable-oriented*: its condition is a set of
//! equality classes, each with one output name, which is what a worst-case
//! optimal join algorithm iterates and what coln's FLIR naturally lowers to. An
//! [`EquiJoinExpr`] is *pair-oriented*: an ordered left and right, joined on
//! arbitrary pairs of expressions, both sides surviving into the output. Going
//! from the first to the second means *choosing a join order* and *naming the
//! accumulated key*, so it is a real translation between two vocabularies —
//! worth doing once, over a plan that can be printed and tested, rather than
//! inside the circuit-building fold of
//! [`DbspInterpreter`](super::interpreter::DbspInterpreter).
//!
//! Running before the [`Resolver`](crate::host::resolver) is what lets this pass
//! mint nodes freely: the [`VarExpr`]s it creates for the accumulated join keys
//! are resolved along with everything else afterwards.
//!
//! # How the fold names its keys
//!
//! Step `k` of the fold joins the accumulated relation with the next relation
//! in the order. For a [`JoinVariable`] bound by the incoming relation and by
//! some relation already in the accumulator, the pair is
//! `(VarExpr(variable.name), <the incoming relation's occurrence>)`: the left
//! side addresses the accumulator's single surviving copy of that column, which
//! [`RelationSchema::join`](crate::relational::RelationSchema::join) guarantees
//! is the one contributed by the first relation to bind it.
//!
//! That copy is only addressable by [`JoinVariable::name`] if the *first*
//! occurrence in fold order is a plain pick of a column already carrying that
//! name — which is exactly what the FLIR lowering emits, since it projects every
//! atom onto the names of the variables it binds. When it is not (an aliased
//! pick, a computed expression, or occurrences that disagree on a name), the
//! accumulated relation has no such column and the pass reports it instead of
//! silently building a circuit that joins on the wrong thing. Renaming into
//! agreement is the job of whoever builds the join, as documented on
//! [`JoinVariable::name`].

use std::collections::HashMap;

use crate::{
    error::LoweringError,
    host::{
        Code,
        expr::{
            AssignExpr, BinaryExpr, CallExpr, Expr, ExprVisitorOwn, FunctionExpr, GetIndexExpr,
            GroupingExpr, LiteralExpr, TupleExpr, UnaryExpr, VarExpr,
        },
        stmt::{BlockStmt, ExprStmt, Stmt, StmtVisitorOwn, VarStmt},
    },
    relational::expr::{
        AliasExpr, AntiJoinExpr, CartesianProductExpr, DifferenceExpr, DistinctExpr, EquiJoinExpr,
        FixedPointIterExpr, JoinVariable, MultiWayEquiJoinExpr, OutputExpr, ProjectionExpr,
        RelExpr, RelExprVisitorOwn, RelationIdx, SelectionExpr, SourceExpr, UnionExpr,
    },
};

/// Rewrite `plan` so that no [`MultiWayEquiJoinExpr`] remains, leaving a plan
/// the DBSP backend can build a circuit from. See the [module docs](self).
pub fn fold_multi_way_joins(plan: Code) -> Result<Code, LoweringError> {
    let mut fold = MultiWayJoinFold;
    plan.into_iter()
        .map(|stmt| fold.visit_stmt(stmt, ()))
        .collect()
}

/// The order the relations of a join are folded into the chain in, as positions
/// into [`MultiWayEquiJoinExpr::relations`]. Contract: a permutation of
/// `0..relations.len()`.
///
/// This is the seam a cost-based optimizer plugs into, and the reason it is a
/// step of its own: *which* order is fastest is a cardinality question, and
/// therefore an optimizer's business, while turning a chosen order into a chain
/// of binary joins is mechanical and belongs here. With nothing to inform the
/// choice yet, the order the plan already states is as good a guess as any.
fn join_order(join: &MultiWayEquiJoinExpr) -> Vec<RelationIdx> {
    (0..join.relations.len()).collect()
}

/// Folds one join, whose operands have already been lowered.
fn fold(join: MultiWayEquiJoinExpr) -> Result<Expr, LoweringError> {
    // The fields are public, so this plan may have been assembled or rewritten
    // by hand; the fold relies on the invariants (two relations or more, in
    // bounds and pairwise distinct occurrences) rather than re-deriving them.
    join.validate()?;

    let order = join_order(&join);
    let position: HashMap<RelationIdx, usize> = order
        .iter()
        .enumerate()
        .map(|(position, relation)| (*relation, position))
        .collect();

    let MultiWayEquiJoinExpr {
        relations,
        on,
        attributes,
    } = join;
    let mut keys = keys_by_relation(on, &position)?;
    // Taken one by one, in fold order, rather than in relation order.
    let mut relations: Vec<Option<Expr>> = relations.into_iter().map(Some).collect();
    let mut take = |relation: RelationIdx| {
        relations[relation]
            .take()
            .expect("A join order names every relation exactly once")
    };

    // The last relation is split off rather than folded with the others so that
    // the projection can operate on the outermost join alone: an inner step that
    // projected would drop columns a later one still has to join on or include
    // columns that a later one still has to produce.
    let folded = order
        .split_last()
        .map(|(last, order)| {
            let mut order = order.iter();
            let first = order
                .next()
                .expect("A validated join has at least two relations");
            let acc = order.fold(take(*first), |acc, relation| {
                Expr::from(EquiJoinExpr {
                    left: acc,
                    right: take(*relation),
                    // No keys for this relation makes the step a cartesian
                    // product, which is what an empty `on` already means for an
                    // `EquiJoinExpr`. So the chain stays one node kind instead
                    // of switching to `CartesianProductExpr`, a newtype over it.
                    on: keys.remove(relation).unwrap_or_default(),
                    attributes: None,
                })
            });
            Expr::from(EquiJoinExpr {
                left: acc,
                right: take(*last),
                on: keys.remove(last).unwrap_or_default(),
                attributes,
            })
        })
        .expect("A validated join has at least two relations");

    Ok(folded)
}

/// Distributes the join condition over the fold: which key pairs each relation
/// contributes when it enters the chain.
///
/// A variable's first occurrence *in fold order* produces no pair — it only
/// carries the column into the accumulator, so there is nothing to constrain it
/// against yet. Every later occurrence is compared against that copy by name.
fn keys_by_relation(
    on: Vec<JoinVariable>,
    position: &HashMap<RelationIdx, usize>,
) -> Result<HashMap<RelationIdx, Vec<(Expr, Expr)>>, LoweringError> {
    let mut keys: HashMap<RelationIdx, Vec<(Expr, Expr)>> = HashMap::new();

    for variable in on {
        let JoinVariable {
            name,
            mut occurrences,
        } = variable;
        // `MultiWayEquiJoinExpr` normalizes into relation order, which is not
        // the order the relations enter the chain in.
        occurrences.sort_by_key(|(relation, _)| position[relation]);

        let mut occurrences = occurrences.into_iter();
        let (carrier, carried) = occurrences
            .next()
            .expect("A validated join variable has at least two occurrences");
        if !matches!(&carried, Expr::Var(var) if var.name == name) {
            return Err(LoweringError::new(format!(
                "Cannot lower join variable '{name}': the first relation to bind it \
                 (relation {carrier}) does not do so as a plain pick of a column named \
                 '{name}', so the accumulated relation of the binary join chain has no \
                 such column to compare later occurrences against. Project the relation \
                 onto '{name}' beneath the join"
            )));
        }

        for (relation, occurrence) in occurrences {
            keys.entry(relation)
                .or_default()
                .push((Expr::from(VarExpr::new(name.clone())), occurrence));
        }
    }

    Ok(keys)
}

/// Walks the plan, rewriting nothing but [`MultiWayEquiJoinExpr`]. Every other
/// node is carried through in the allocation it arrived in.
struct MultiWayJoinFold;

type Lowered<T> = Result<T, LoweringError>;

impl MultiWayJoinFold {
    fn exprs(&mut self, exprs: Vec<Expr>) -> Lowered<Vec<Expr>> {
        exprs
            .into_iter()
            .map(|expr| self.visit_expr(expr, ()))
            .collect()
    }

    fn stmts(&mut self, stmts: Vec<Stmt>) -> Lowered<Vec<Stmt>> {
        stmts
            .into_iter()
            .map(|stmt| self.visit_stmt(stmt, ()))
            .collect()
    }

    fn pairs(&mut self, pairs: Vec<(Expr, Expr)>) -> Lowered<Vec<(Expr, Expr)>> {
        pairs
            .into_iter()
            .map(|(left, right)| Ok((self.visit_expr(left, ())?, self.visit_expr(right, ())?)))
            .collect()
    }

    fn attributes(&mut self, attributes: Vec<(String, Expr)>) -> Lowered<Vec<(String, Expr)>> {
        attributes
            .into_iter()
            .map(|(name, expr)| Ok((name, self.visit_expr(expr, ())?)))
            .collect()
    }

    fn optional_attributes(
        &mut self,
        attributes: Option<Vec<(String, Expr)>>,
    ) -> Lowered<Option<Vec<(String, Expr)>>> {
        attributes
            .map(|attributes| self.attributes(attributes))
            .transpose()
    }

    fn join_variables(&mut self, on: Vec<JoinVariable>) -> Lowered<Vec<JoinVariable>> {
        on.into_iter()
            .map(|variable| {
                Ok(JoinVariable {
                    name: variable.name,
                    occurrences: variable
                        .occurrences
                        .into_iter()
                        .map(|(relation, expr)| Ok((relation, self.visit_expr(expr, ())?)))
                        .collect::<Lowered<_>>()?,
                })
            })
            .collect()
    }
}

impl StmtVisitorOwn<Lowered<Stmt>, ()> for MultiWayJoinFold {
    fn visit_var_stmt(&mut self, mut stmt: Box<VarStmt>, _ctx: ()) -> Lowered<Stmt> {
        stmt.initializer = stmt
            .initializer
            .map(|initializer| self.visit_expr(initializer, ()))
            .transpose()?;
        Ok(stmt.into())
    }

    fn visit_expr_stmt(&mut self, mut stmt: Box<ExprStmt>, _ctx: ()) -> Lowered<Stmt> {
        stmt.expr = self.visit_expr(stmt.expr, ())?;
        Ok(stmt.into())
    }

    fn visit_block_stmt(&mut self, mut stmt: Box<BlockStmt>, _ctx: ()) -> Lowered<Stmt> {
        stmt.stmts = self.stmts(stmt.stmts)?;
        Ok(stmt.into())
    }
}

impl ExprVisitorOwn<Lowered<Expr>, ()> for MultiWayJoinFold {
    fn visit_literal_expr(&mut self, expr: Box<LiteralExpr>, _ctx: ()) -> Lowered<Expr> {
        Ok(expr.into())
    }

    fn visit_tuple_expr(&mut self, mut expr: Box<TupleExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.elements = self.exprs(expr.elements)?;
        Ok(expr.into())
    }

    fn visit_get_index_expr(&mut self, mut expr: Box<GetIndexExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.target = self.visit_expr(expr.target, ())?;
        expr.index = self.visit_expr(expr.index, ())?;
        Ok(expr.into())
    }

    fn visit_grouping_expr(&mut self, mut expr: Box<GroupingExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.expr = self.visit_expr(expr.expr, ())?;
        Ok(expr.into())
    }

    fn visit_binary_expr(&mut self, mut expr: Box<BinaryExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.left = self.visit_expr(expr.left, ())?;
        expr.right = self.visit_expr(expr.right, ())?;
        Ok(expr.into())
    }

    fn visit_unary_expr(&mut self, mut expr: Box<UnaryExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.operand = self.visit_expr(expr.operand, ())?;
        Ok(expr.into())
    }

    fn visit_var_expr(&mut self, expr: Box<VarExpr>, _ctx: ()) -> Lowered<Expr> {
        Ok(expr.into())
    }

    fn visit_assign_expr(&mut self, mut expr: Box<AssignExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.value = self.visit_expr(expr.value, ())?;
        Ok(expr.into())
    }

    fn visit_function_expr(&mut self, mut expr: Box<FunctionExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.body.stmts = self.stmts(expr.body.stmts)?;
        Ok(expr.into())
    }

    fn visit_call_expr(&mut self, mut expr: Box<CallExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.callee = self.visit_expr(expr.callee, ())?;
        expr.arguments = self.exprs(expr.arguments)?;
        Ok(expr.into())
    }

    fn visit_relational_expr(&mut self, expr: RelExpr, _ctx: ()) -> Lowered<Expr> {
        self.visit_rel(expr, ())
    }
}

impl RelExprVisitorOwn<Lowered<Expr>, ()> for MultiWayJoinFold {
    fn visit_source_expr(&mut self, expr: Box<SourceExpr>, _ctx: ()) -> Lowered<Expr> {
        Ok(expr.into())
    }

    fn visit_output_expr(&mut self, mut expr: Box<OutputExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.relation = self.visit_expr(expr.relation, ())?;
        Ok(expr.into())
    }

    fn visit_alias_expr(&mut self, mut expr: Box<AliasExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.relation = self.visit_expr(expr.relation, ())?;
        Ok(expr.into())
    }

    fn visit_distinct_expr(&mut self, mut expr: Box<DistinctExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.relation = self.visit_expr(expr.relation, ())?;
        Ok(expr.into())
    }

    fn visit_union_expr(&mut self, mut expr: Box<UnionExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.relations = self.exprs(expr.relations)?;
        Ok(expr.into())
    }

    fn visit_difference_expr(&mut self, mut expr: Box<DifferenceExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.left = self.visit_expr(expr.left, ())?;
        expr.right = self.visit_expr(expr.right, ())?;
        Ok(expr.into())
    }

    fn visit_selection_expr(&mut self, mut expr: Box<SelectionExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.relation = self.visit_expr(expr.relation, ())?;
        expr.condition = self.visit_expr(expr.condition, ())?;
        Ok(expr.into())
    }

    fn visit_projection_expr(&mut self, mut expr: Box<ProjectionExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.relation = self.visit_expr(expr.relation, ())?;
        expr.attributes = self.attributes(expr.attributes)?;
        Ok(expr.into())
    }

    fn visit_cartesian_product_expr(
        &mut self,
        mut expr: Box<CartesianProductExpr>,
        _ctx: (),
    ) -> Lowered<Expr> {
        expr.inner.left = self.visit_expr(expr.inner.left, ())?;
        expr.inner.right = self.visit_expr(expr.inner.right, ())?;
        expr.inner.attributes = self.optional_attributes(expr.inner.attributes)?;
        Ok(expr.into())
    }

    fn visit_equi_join_expr(&mut self, mut expr: Box<EquiJoinExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.left = self.visit_expr(expr.left, ())?;
        expr.right = self.visit_expr(expr.right, ())?;
        expr.on = self.pairs(expr.on)?;
        expr.attributes = self.optional_attributes(expr.attributes)?;
        Ok(expr.into())
    }

    fn visit_multi_way_equi_join_expr(
        &mut self,
        mut expr: Box<MultiWayEquiJoinExpr>,
        _ctx: (),
    ) -> Lowered<Expr> {
        // The operands are lowered first: a nested multi-way join has to be
        // gone before this one is folded, because the fold moves the operands
        // into the chain as they are.
        expr.relations = self.exprs(expr.relations)?;
        expr.on = self.join_variables(expr.on)?;
        expr.attributes = self.optional_attributes(expr.attributes)?;
        fold(*expr)
    }

    fn visit_anti_join_expr(&mut self, mut expr: Box<AntiJoinExpr>, _ctx: ()) -> Lowered<Expr> {
        expr.left = self.visit_expr(expr.left, ())?;
        expr.right = self.visit_expr(expr.right, ())?;
        expr.on = self.pairs(expr.on)?;
        Ok(expr.into())
    }

    fn visit_fixed_point_iter_expr(
        &mut self,
        mut expr: Box<FixedPointIterExpr>,
        _ctx: (),
    ) -> Lowered<Expr> {
        expr.accumulator.1 = self.visit_expr(expr.accumulator.1, ())?;
        expr.step.stmts = self.stmts(expr.step.stmts)?;
        Ok(expr.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        host::expr::Literal,
        relational::expr::{OutputKind, SinkId},
    };

    fn relation(name: &str) -> Expr {
        Expr::from(VarExpr::new(name))
    }

    fn relations(count: usize) -> Vec<Expr> {
        (0..count).map(|idx| relation(&format!("r{idx}"))).collect()
    }

    /// A join variable bound by `occurrences`, each as the plain column pick the
    /// FLIR lowering emits.
    fn join_variable(name: &str, occurrences: &[RelationIdx]) -> JoinVariable {
        JoinVariable {
            name: name.to_string(),
            occurrences: occurrences
                .iter()
                .map(|relation| (*relation, Expr::from(VarExpr::new(name))))
                .collect(),
        }
    }

    fn lower(expr: Expr) -> Lowered<Expr> {
        let lowered = fold_multi_way_joins(Code::new(vec![Stmt::from(ExprStmt { expr })]))?;
        match lowered.into_stmts().pop() {
            Some(Stmt::Expr(stmt)) => Ok(stmt.expr),
            other => panic!("Lowering a single expression statement yielded {other:?}"),
        }
    }

    fn equi_join(expr: &Expr) -> &EquiJoinExpr {
        match expr {
            Expr::Relational(rel) => match rel {
                RelExpr::EquiJoin(join) => join,
                other => panic!("Expected an equi join, got {other:?}"),
            },
            other => panic!("Expected a relational expression, got {other:?}"),
        }
    }

    /// The pair of variable names an equi join compares on, which is all the
    /// tests need to see of a key.
    fn key_names(join: &EquiJoinExpr) -> Vec<(String, String)> {
        let name = |expr: &Expr| match expr {
            Expr::Var(var) => var.name.clone(),
            other => panic!("Expected a plain column pick, got {other:?}"),
        };
        join.on
            .iter()
            .map(|(left, right)| (name(left), name(right)))
            .collect()
    }

    #[test]
    fn folds_a_three_way_join_into_a_left_deep_chain() {
        let joined = MultiWayEquiJoinExpr::new(
            relations(3),
            vec![join_variable("x", &[0, 1]), join_variable("y", &[1, 2])],
            None,
        )
        .expect("A well-formed three way join");

        let lowered = lower(Expr::from(joined)).expect("Plain column picks are lowerable");

        // ((r0 ⋈x r1) ⋈y r2)
        let outer = equi_join(&lowered);
        assert_eq!(outer.right, relation("r2"));
        assert_eq!(key_names(outer), vec![("y".to_string(), "y".to_string())]);

        let inner = equi_join(&outer.left);
        assert_eq!(inner.left, relation("r0"));
        assert_eq!(inner.right, relation("r1"));
        assert_eq!(key_names(inner), vec![("x".to_string(), "x".to_string())]);
    }

    #[test]
    fn compares_a_later_occurrence_against_the_accumulated_copy() {
        // `x` is bound by all three relations. The chain may only ever compare
        // the incoming relation against the accumulator, never against a
        // relation that is already inside it.
        let joined =
            MultiWayEquiJoinExpr::new(relations(3), vec![join_variable("x", &[0, 1, 2])], None)
                .expect("A well-formed three way join");

        let lowered = lower(Expr::from(joined)).expect("Plain column picks are lowerable");

        let outer = equi_join(&lowered);
        assert_eq!(key_names(outer), vec![("x".to_string(), "x".to_string())]);
        assert_eq!(outer.right, relation("r2"));
        assert_eq!(
            key_names(equi_join(&outer.left)),
            vec![("x".to_string(), "x".to_string())]
        );
    }

    #[test]
    fn keeps_the_projection_on_the_outermost_join() {
        // An inner step has no business projecting: it would drop columns a
        // later step still has to join on.
        let attributes = vec![("z".to_string(), Expr::from(VarExpr::new("x")))];
        let joined = MultiWayEquiJoinExpr::new(
            relations(3),
            vec![join_variable("x", &[0, 1]), join_variable("y", &[1, 2])],
            Some(attributes.clone()),
        )
        .expect("A well-formed three way join");

        let lowered = lower(Expr::from(joined)).expect("Plain column picks are lowerable");

        let outer = equi_join(&lowered);
        assert_eq!(outer.attributes, Some(attributes));
        assert_eq!(equi_join(&outer.left).attributes, None);
    }

    #[test]
    fn folds_an_empty_join_condition_into_a_chain_of_products() {
        let joined = MultiWayEquiJoinExpr::new(relations(3), vec![], None)
            .expect("An empty join condition is a cartesian product");

        let lowered = lower(Expr::from(joined)).expect("A product needs no names");

        let outer = equi_join(&lowered);
        assert!(outer.on.is_empty());
        assert!(equi_join(&outer.left).on.is_empty());
    }

    #[test]
    fn lowers_a_join_nested_inside_another_one() {
        let inner =
            MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0, 1])], None)
                .expect("A well-formed two way join");
        let outer = MultiWayEquiJoinExpr::new(
            vec![Expr::from(inner), relation("r2")],
            vec![join_variable("x", &[0, 1])],
            None,
        )
        .expect("A join over a join");

        let lowered = lower(Expr::from(outer)).expect("Plain column picks are lowerable");

        // Both joins are gone, so the left operand of the outer chain is itself
        // a binary join rather than a multi-way one.
        let outer = equi_join(&lowered);
        assert_eq!(outer.right, relation("r2"));
        let inner = equi_join(&outer.left);
        assert_eq!(inner.left, relation("r0"));
        assert_eq!(inner.right, relation("r1"));
    }

    #[test]
    fn lowers_a_join_beneath_an_unrelated_operator() {
        let joined =
            MultiWayEquiJoinExpr::new(relations(2), vec![join_variable("x", &[0, 1])], None)
                .expect("A well-formed two way join");
        let tapped = Expr::from(OutputExpr {
            relation: Expr::from(joined),
            id: SinkId::from("out"),
            kind: OutputKind::Channel,
        });

        let lowered = lower(tapped).expect("Plain column picks are lowerable");

        match &lowered {
            Expr::Relational(rel) => match rel {
                RelExpr::Output(output) => {
                    assert_eq!(
                        key_names(equi_join(&output.relation)),
                        vec![("x".to_string(), "x".to_string())]
                    );
                }
                other => panic!("Expected the output tap to survive, got {other:?}"),
            },
            other => panic!("Expected a relational expression, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_join_variable_the_accumulator_cannot_name() {
        // `x` is carried into the accumulator by an expression rather than by a
        // column named `x`, so there is nothing for the second occurrence to be
        // compared against. Renaming beneath the join is the caller's job.
        let joined = MultiWayEquiJoinExpr::new(
            relations(2),
            vec![JoinVariable {
                name: "x".to_string(),
                occurrences: vec![
                    (0, Expr::from(LiteralExpr::from(1u64))),
                    (1, Expr::from(VarExpr::new("x"))),
                ],
            }],
            None,
        )
        .expect("Validation does not constrain the occurrence expressions");

        let error = lower(Expr::from(joined)).expect_err("An unnameable key must not be lowered");
        assert!(error.message.contains("'x'"), "{}", error.message);
    }

    #[test]
    fn rejects_a_malformed_join() {
        // Hand-assembled, so it never went through `new`.
        let joined = MultiWayEquiJoinExpr {
            relations: relations(2),
            on: vec![join_variable("x", &[0])],
            attributes: None,
        };
        assert!(lower(Expr::from(joined)).is_err());
    }

    #[test]
    fn leaves_a_plan_without_multi_way_joins_untouched() {
        let plan = Code::new(vec![Stmt::from(VarStmt {
            name: "joined".to_string(),
            initializer: Some(Expr::from(EquiJoinExpr {
                left: relation("r0"),
                right: relation("r1"),
                on: vec![(Expr::from(VarExpr::new("x")), Expr::from(VarExpr::new("y")))],
                attributes: Some(vec![(
                    "z".to_string(),
                    Expr::from(LiteralExpr {
                        value: Literal::Uint(1),
                    }),
                )]),
            })),
        })]);

        let lowered = fold_multi_way_joins(plan.clone()).expect("Nothing to lower");

        assert_eq!(lowered, plan);
    }
}
