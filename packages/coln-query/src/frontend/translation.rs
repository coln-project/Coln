// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Translation of a [`LogicalProgram`](super::LogicalProgram) into the query
//! engine's IR.
//!
//! # Naming rules in the IR
//!
//! Each rule becomes a [`VarStmt`] of its own, and the predicate's statement,
//! emitted right after, unions those variables. So a rule's name has to serve
//! as an IR variable name, and [`Identifiable::id`] is not enough on its own:
//! **two rules of one predicate may share a name**, because a frontend that
//! does not name rules apart from predicates hands the predicate's name to
//! every rule defining it.
//!
//! That case is broken, and silently so. Re-declaring a name shadows it, so
//! `var r = …; var r = …; var p = Union(r, r)` resolves cleanly with _both_
//! union operands bound to the second `r`. The first rule is emitted and then
//! never read, and the predicate quietly loses it.
//!
//! Translation therefore makes each rule's name unique within its predicate,
//! suffixing a counter where it has to. A frontend whose rule names are already
//! distinct (e.g. FLIR's rules) keeps them untouched.
//!
//! The predicate's own name needs no such treatment, and a single-rule
//! predicate may share its rule's name: the predicate's statement comes after
//! the rule's, and [`Resolver`](crate::host::resolver) resolves an initializer
//! before declaring the name it binds, so `var path = …; var path =
//! Output(Union(path))` reads the rule and only then shadows it, which is fine.

use super::{
    AggregateRules, Atom, Bind, Component, Cond, Identifiable, Identifier, Lit, Predicate, Rule,
    TypedVar,
};
use crate::{
    error::SyntaxError,
    frontend::{ExecutionOrder, analysis::PredicateComponent},
    host::{
        QueryIr,
        expr::{BinaryExpr, Expr, LiteralExpr, VarExpr},
        operator::Operator,
        stmt::{BlockStmt, ExprStmt, Stmt, VarStmt},
    },
    relational::{
        expr::{
            FixedPointIterExpr, JoinVariable, MultiWayEquiJoinExpr, OutputExpr, OutputKind,
            ProjectionExpr, RelationIdx, SelectionExpr, SinkId, SourceExpr, UnionExpr,
        },
        schema::Column,
    },
};
use indexmap::IndexMap;
use std::collections::{HashMap, hash_map::Entry};

/// The rule type of a program's predicates, and the pieces hanging off it. Named
/// because the paths through the associated types are otherwise unreadable.
type RuleOf<P> = <P as AggregateRules>::Rule;
type AtomOf<P> = <RuleOf<P> as Rule>::Atom;
type CondOf<P> = <RuleOf<P> as Rule>::Cond;

pub(super) struct Translator<'a, P: Predicate> {
    /// The IDB, keyed the way an atom looks a relation up. Consulted before the
    /// EDB, which is what lets a predicate shadow a base relation of the same
    /// name.
    predicates: IndexMap<&'a P::Identifier, &'a P>,
    ir: QueryIr,
}

impl<'a, P: Predicate> Translator<'a, P> {
    pub(super) fn new() -> Self {
        Self {
            predicates: IndexMap::new(),
            ir: QueryIr::default(),
        }
    }

    /// Lowers `program` into the query engine's IR.
    pub(super) fn run(
        mut self,
        program: &'a ExecutionOrder<'a, P>,
    ) -> Result<QueryIr, SyntaxError> {
        for component in program.iter() {
            self.component(component)?;
        }
        Ok(self.ir)
    }

    /// Emits one component: a statement per rule, then the statement binding the
    /// predicate to the union of them, wrapped in a fixed point if the component
    /// is recursive.
    fn component(&mut self, component: &'a PredicateComponent<'a, P>) -> Result<(), SyntaxError> {
        let mut members = component.members();
        let predicate = members.next().expect("a component has at least one member");
        if members.next().is_some() {
            todo!("mutual recursion in query IR")
        }

        // Named and split in one pass, so that the names stay aligned with the
        // rules they belong to. See the module docs for why a rule's own
        // identifier is not always usable as an IR variable name.
        let mut names = Names::default();
        let (non_rec, rec): (Vec<_>, Vec<_>) = component
            .rules()
            .map(|rule| (names.mint(rule.id()), rule))
            .partition(|(_, rule)| !component.references_member(rule));

        if non_rec.is_empty() {
            return Err(SyntaxError::new(format!(
                "Predicate '{}' has no base case: every rule of it reads the component \
                 it is part of, so nothing ever seeds the iteration",
                predicate.id(),
            )));
        }

        let (base, base_stmts) = self.rules(predicate, &non_rec)?;
        self.ir.extend(base_stmts);

        let relation = match rec.is_empty() {
            true => base,
            false => {
                let (step, mut step_stmts) = self.rules(predicate, &rec)?;
                // The step block evaluates to its last expression, which is the
                // union of the recursive rules.
                step_stmts.push(Stmt::from(ExprStmt { expr: step }));
                Expr::from(FixedPointIterExpr {
                    accumulator: (predicate.id().to_string(), base),
                    step: BlockStmt { stmts: step_stmts },
                })
            }
        };

        self.predicates.insert(predicate.id(), predicate);
        self.ir.push(Stmt::from(VarStmt {
            name: predicate.id().to_string(),
            initializer: Some(Expr::from(OutputExpr {
                id: SinkId::from(predicate.id().to_string()),
                kind: OutputKind::Channel,
                relation,
            })),
        }));
        Ok(())
    }

    /// Binds each rule to a statement of its own and unions those bindings.
    ///
    /// The union is what the predicate evaluates to; the statements have to be
    /// emitted ahead of it, since it references them by name.
    fn rules(
        &self,
        predicate: &P,
        rules: &[(String, &P::Rule)],
    ) -> Result<(Expr, Vec<Stmt>), SyntaxError> {
        let mut relations: Vec<Expr> = Vec::with_capacity(rules.len());
        let mut stmts: Vec<Stmt> = Vec::with_capacity(rules.len());
        for (name, rule) in rules {
            let initializer = self.rule(predicate, rule)?;
            relations.push(Expr::from(VarExpr::new(name.clone())));
            stmts.push(Stmt::from(VarStmt {
                name: name.clone(),
                initializer: Some(initializer),
            }));
        }
        let combined = match relations.len() {
            // A lone rule is its own union, and `UnionExpr` wants two or more.
            1 => relations.pop().expect("length checked"),
            _ => Expr::from(UnionExpr { relations }),
        };
        Ok((combined, stmts))
    }

    /// One rule: its body as a conjunctive query, projected onto the columns
    /// its predicate declares.
    fn rule(&self, predicate: &P, rule: &P::Rule) -> Result<Expr, SyntaxError> {
        let relation = self.conjunctive_query(rule)?;
        let columns: Vec<&Column> = predicate.columns().collect();

        // A head is dense over its predicate's columns: every column filled,
        // and none of them twice.
        let mut terms: Vec<Option<Expr>> = (0..columns.len()).map(|_| None).collect();
        for (position, bind) in rule.head().bindings() {
            let slot = terms.get_mut(position).ok_or_else(|| {
                SyntaxError::new(format!(
                    "Rule '{}' fills column {position} of predicate '{}', which has {} column(s)",
                    rule.id(),
                    predicate.id(),
                    columns.len(),
                ))
            })?;
            if slot.is_some() {
                return Err(SyntaxError::new(format!(
                    "Rule '{}' fills column {position} of predicate '{}' more than once",
                    rule.id(),
                    predicate.id(),
                )));
            }
            *slot = Some(bound_expr(bind));
        }

        let attributes = columns
            .iter()
            .zip(terms)
            .enumerate()
            .map(|(position, (column, term))| match term {
                Some(term) => Ok((column.name().to_string(), term)),
                None => Err(SyntaxError::new(format!(
                    "Rule '{}' leaves column {position} ('{}') of predicate '{}' unfilled",
                    rule.id(),
                    column.name(),
                    predicate.id(),
                ))),
            })
            .collect::<Result<Vec<_>, SyntaxError>>()?;

        Ok(Expr::from(ProjectionExpr {
            relation,
            attributes,
        }))
    }

    /// A rule's body: its atoms joined on the variables they share, filtered by
    /// its conditions.
    pub fn conjunctive_query(&self, rule: &P::Rule) -> Result<Expr, SyntaxError> {
        let plans = rule
            .atoms()
            .map(|atom| self.atom(atom))
            .collect::<Result<Vec<_>, SyntaxError>>()?;
        if plans.is_empty() {
            return Err(SyntaxError::new(format!(
                "Rule '{}' has an empty body, so there is no relation to derive from",
                rule.id(),
            )));
        }

        let on = join_variables(&plans);
        let mut relations: Vec<Expr> = plans.into_iter().map(|plan| plan.relation).collect();
        let joined = match relations.len() {
            // A single atom has nothing to join against, and the join operators
            // require at least two relations.
            1 => relations.pop().expect("length checked"),
            _ => Expr::from(MultiWayEquiJoinExpr::new(relations, on, None)?),
        };

        // All conditions become one condition by ANDing them, and a rule with
        // none keeps `joined` unwrapped rather than gaining a vacuous selection.
        Ok(rule
            .conditions()
            .map(condition)
            .reduce(|left, right| {
                Expr::from(BinaryExpr {
                    operator: Operator::And,
                    left,
                    right,
                })
            })
            .into_iter()
            .fold(joined, |relation, condition| {
                Expr::from(SelectionExpr {
                    relation,
                    condition,
                })
            }))
    }

    /// One atom: the relation it names, filtered by what it pins down locally
    /// and projected onto the variables it brings into scope.
    fn atom<'r>(&self, atom: &'r AtomOf<P>) -> Result<AtomPlan<'r, P::Identifier>, SyntaxError> {
        let name = atom.id();
        // The IDB is consulted first: a predicate shadows a base relation of
        // the same name. A derived relation is bound to a host variable rather
        // than being a source leaf, because its rows are computed, not fed in.
        let (relation, columns): (Expr, Vec<Column>) = match self.predicates.get(name) {
            Some(predicate) => {
                let columns: Vec<Column> = predicate.columns().cloned().collect();
                let expr = if predicate.is_edb_predicate() {
                    Expr::from(SourceExpr::new(name.to_string()))
                } else {
                    Expr::from(VarExpr::new(name.to_string()))
                };
                (expr, columns)
            }
            None => {
                // TODO: Should not return an Err but something like
                // expect("program not in execution order or predicate has not been registered")
                return Err(SyntaxError::new(format!(
                    "Atom references undeclared entity '{name}'"
                )));
            }
        };

        let mut binder = AtomBinder::new();
        for (position, bind) in atom.bindings() {
            let column = columns.get(position).ok_or_else(|| {
                SyntaxError::new(format!(
                    "Atom '{name}' binds position {position} of a relation with {} column(s)",
                    columns.len(),
                ))
            })?;
            match bind {
                // A literal in an atom pins that column down: a local selection
                // on this one relation rather than anything the join sees.
                Bind::Lit(lit) => binder.conditions.push(equals(
                    Expr::from(VarExpr::new(column.name())),
                    Expr::from(LiteralExpr::from(lit.to_literal())),
                )),
                Bind::Var(var) => binder.bind(var.name(), column),
            }
        }

        let filtered = binder
            .conditions
            .into_iter()
            .reduce(|left, right| {
                Expr::from(BinaryExpr {
                    operator: Operator::And,
                    left,
                    right,
                })
            })
            .into_iter()
            .fold(relation, |relation, condition| {
                Expr::from(SelectionExpr {
                    relation,
                    condition,
                })
            });

        Ok(AtomPlan {
            relation: Expr::from(ProjectionExpr {
                relation: filtered,
                attributes: binder.attributes,
            }),
            variables: binder.variables,
        })
    }
}

/// The relational plan for one atom, plus the variables its projection exposes.
///
/// Reporting the variables is what lets the enclosing conjunctive query derive
/// its join condition without re-deriving it from the projection just built.
struct AtomPlan<'r, I> {
    relation: Expr,
    /// In the order the atom binds them, each exposed under its own name.
    variables: Vec<&'r I>,
}

/// Accumulates what one atom contributes while its bindings are walked.
struct AtomBinder<'r, I> {
    /// Conditions local to this atom: the literals it pins a column to, plus
    /// the equalities a variable repeated within this one atom gives rise to.
    conditions: Vec<Expr>,
    /// The projection, mapping each bound variable onto the column carrying it.
    attributes: Vec<(String, Expr)>,
    variables: Vec<&'r I>,
    /// The column each variable was *first* bound to in this atom, so a
    /// repeated occurrence can be turned into an equality against it.
    bound: HashMap<&'r I, String>,
}

impl<'r, I: Identifier> AtomBinder<'r, I> {
    fn new() -> Self {
        Self {
            conditions: Vec::new(),
            attributes: Vec::new(),
            variables: Vec::new(),
            bound: HashMap::new(),
        }
    }

    fn bind(&mut self, var: &'r I, column: &Column) {
        match self.bound.entry(var) {
            Entry::Vacant(slot) => {
                self.attributes
                    .push((var.to_string(), Expr::from(VarExpr::new(column.name()))));
                self.variables.push(var);
                slot.insert(column.name().to_string());
            }
            Entry::Occupied(first) => {
                // The variable is repeated within this single atom, as in
                // `r(x, x)`. That is a local equality on this one relation
                // rather than a join condition, and the projection has to
                // expose the attribute exactly once — two attributes of the
                // same name would collide in the projected schema.
                self.conditions.push(equals(
                    Expr::from(VarExpr::new(first.get().clone())),
                    Expr::from(VarExpr::new(column.name())),
                ));
            }
        }
    }
}

/// The variables bound by more than one atom, which are exactly the ones the
/// join has to equate.
///
/// A variable bound by a single atom produces no entry: it is not an equality
/// class, and it reaches the output through that atom's schema anyway. Every
/// atom projects a variable onto the variable's own name, so the occurrences
/// agree on a name by construction.
fn join_variables<I: Identifier>(plans: &[AtomPlan<'_, I>]) -> Vec<JoinVariable> {
    let mut occurrences: IndexMap<&I, Vec<RelationIdx>> = IndexMap::new();
    for (relation, plan) in plans.iter().enumerate() {
        for variable in &plan.variables {
            occurrences.entry(variable).or_default().push(relation);
        }
    }
    occurrences
        .into_iter()
        .filter(|(_, occurrences)| occurrences.len() > 1)
        .map(|(variable, occurrences)| JoinVariable {
            name: variable.to_string(),
            occurrences: occurrences
                .into_iter()
                .map(|relation| (relation, Expr::from(VarExpr::new(variable.to_string()))))
                .collect(),
        })
        .collect()
}

fn condition<C: Cond>(condition: &C) -> Expr {
    Expr::from(BinaryExpr {
        operator: condition.operator().into(),
        left: bound_expr(condition.left()),
        right: bound_expr(condition.right()),
    })
}

/// A bound term as the expression that produces it: a variable reads the
/// attribute of its own name, a literal stands for itself.
fn bound_expr<V: TypedVar, L: Lit>(bind: Bind<&V, &L>) -> Expr {
    match bind {
        Bind::Var(var) => Expr::from(VarExpr::new(var.name().to_string())),
        Bind::Lit(lit) => Expr::from(LiteralExpr::from(lit.to_literal())),
    }
}

fn equals(left: Expr, right: Expr) -> Expr {
    Expr::from(BinaryExpr {
        operator: Operator::Equal,
        left,
        right,
    })
}

/// Hands out an IR variable name per rule, unique within the predicate.
#[derive(Default)]
struct Names {
    /// Every name handed out (including minted ones with a suffix), mapped to
    /// the suffix counter to try first the next time that same name comes
    /// in as an identifier.
    taken: HashMap<String, usize>,
}

impl Names {
    fn mint(&mut self, id: &impl std::fmt::Display) -> String {
        let name = id.to_string();
        let mut suffix = self.taken.get(&name).copied().unwrap_or(0);
        // Starting from the stored suffix rather than from zero is what keeps
        // minting O(1) amortized: The loop only runs again when a name was
        // taken by an identifier that already carried a suffix.
        let minted = loop {
            let candidate = match suffix {
                0 => name.clone(),
                suffix => format!("{name}#{suffix}"),
            };
            suffix += 1;
            if !self.taken.contains_key(&candidate) {
                break candidate;
            }
        };
        self.taken.insert(name, suffix);
        // Also mark the minted name taken.
        self.taken.entry(minted.clone()).or_insert(0);
        minted
    }
}

#[cfg(test)]
mod tests {
    // TODO:
}
