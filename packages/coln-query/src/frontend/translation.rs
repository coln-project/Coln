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
//! No rule is ever bound under its predicate's own name either, as the same
//! shadowing strikes inside a recursive predicate's fixed point: its step reads
//! the accumulator under the predicate's name, so in `var path = …; var path#1
//! = … path …` the second rule reads the first rule's result rather than the
//! accumulator. The predicate's name is therefore taken before any rule is
//! named, and a rule carrying it gets a suffix like any other duplicate.
//!
//! Outside a fixed point, sharing the name would be harmless: the predicate's
//! statement comes after its rules', and [`Resolver`](crate::host::resolver)
//! resolves an initializer before declaring the name it binds. Taking the name
//! regardless keeps one rule for both cases.

use super::{
    AggregateRules, Atom, Bind, Component, Cond, IdOf, Identifiable, Identifier, Lit, Predicate,
    Rule, TypedVar,
};
use crate::{
    error::{Frame, SyntaxError},
    frontend::{ExecutionOrder, analysis::PredicateComponent},
    host::{
        QueryIr,
        expr::{BinaryExpr, Expr, LiteralExpr, VarExpr},
        operator::Operator,
        stmt::{BlockStmt, ExprStmt, Stmt, VarStmt},
    },
    relational::{
        expr::{
            AntiJoinExpr, FixedPointIterExpr, JoinVariable, MultiWayEquiJoinExpr, OutputExpr,
            OutputKind, ProjectionExpr, RelationIdx, SelectionExpr, SinkId, SourceExpr, UnionExpr,
        },
        schema::Column,
    },
};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet, hash_map::Entry};

/// Code translated from a program that passed
/// [verification](super::Analysis::verify), as
/// [`LogicalProgram::prepare`](super::LogicalProgram::prepare) hands it out.
///
/// Its field is private, so this module's translator is the only source of
/// one: whatever takes a `Prepared` rather than a bare [`QueryIr`] takes code
/// of a verified program. Which program, the type does not say.
#[derive(Debug)]
pub struct Prepared(QueryIr);

impl Prepared {
    pub fn into_code(self) -> QueryIr {
        self.0
    }
}

/// Reading the code certifies nothing, so that is free.
impl std::ops::Deref for Prepared {
    type Target = QueryIr;

    fn deref(&self) -> &QueryIr {
        &self.0
    }
}

/// The rule type of a program's predicates, and the pieces hanging off it. Named
/// because the paths through the associated types are otherwise unreadable.
type RuleOf<P> = <P as AggregateRules>::Rule;
type AtomOf<P> = <RuleOf<P> as Rule>::Atom;
type CondOf<P> = <RuleOf<P> as Rule>::Cond;
/// The identifier naming a variable, which is a space of its own: a predicate
/// is named across rules, a variable only within one.
type VarIdOf<P> = IdOf<<AtomOf<P> as Atom>::Var>;

pub(super) struct Translator<'a, P: Predicate> {
    /// The IDB, keyed the way an atom looks a relation up. Consulted before the
    /// EDB, which is what lets a predicate shadow a base relation of the same
    /// name.
    predicates: IndexMap<&'a P::Identifier, &'a P>,
    /// The predicates whose relation is bound to an output sink. Any other is
    /// bound to a plain host variable only.
    outputs: HashSet<&'a P::Identifier>,
    ir: QueryIr,
}

impl<'a, P: Predicate> Translator<'a, P> {
    pub(super) fn new(outputs: HashSet<&'a P::Identifier>) -> Self {
        Self {
            predicates: IndexMap::new(),
            outputs,
            ir: QueryIr::default(),
        }
    }

    /// Lowers `program` into the query engine's IR.
    pub(super) fn run(
        mut self,
        program: &'a ExecutionOrder<'a, P>,
    ) -> Result<Prepared, SyntaxError> {
        for component in program.iter() {
            self.component(component)?;
        }
        Ok(Prepared(self.ir))
    }

    /// Emits one component: a statement per rule, then the statement binding the
    /// predicate to the union of them, wrapped in a fixed point if the component
    /// is recursive.
    fn component(&mut self, component: &'a PredicateComponent<'a, P>) -> Result<(), SyntaxError> {
        // A component's members are in scope for its own rules: inside the
        // fixed point, each name reads the accumulator bound under it.
        self.predicates
            .extend(component.members().map(|member| (member.id(), member)));

        if component.is_edb_component().is_some() {
            // An EDB predicate needs no translation but only registration above.
            return Ok(());
        }

        let mut members = component.members();
        let predicate = members.next().expect("a component has at least one member");
        if members.next().is_some() {
            todo!("mutual recursion in query IR")
        }

        // Named and split in one pass, so that the names stay aligned with the
        // rules they belong to. See the module docs for why a rule's own
        // identifier is not always usable as an IR variable name.
        let mut names = Names::default();
        // Minted (but ignored) first, so that no rule is ever bound under a
        // predicate's name. See the module docs for why.
        component.members().for_each(|member| {
            names.mint(member.id());
        });
        let (non_rec, rec): (Vec<_>, Vec<_>) = component
            .rules()
            .map(|rule| (names.mint(rule.id()), rule))
            .partition(|(_, rule)| !component.references_member(rule));

        if non_rec.is_empty() {
            return Err(SyntaxError::new(
                "no base case: every rule reads the component the predicate is part of, \
                 so nothing ever seeds the iteration",
            )
            .within(predicate_frame(predicate)));
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

        let initializer = match self.outputs.contains(predicate.id()) {
            true => Expr::from(OutputExpr {
                id: SinkId::from(predicate.id().to_string()),
                kind: OutputKind::Channel,
                relation,
            }),
            false => relation,
        };
        self.ir.push(Stmt::from(VarStmt {
            name: predicate.id().to_string(),
            initializer: Some(initializer),
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
            // The one place that knows both, so everything below reports only
            // what it knows itself.
            let initializer = self.rule(predicate, rule).map_err(|error| {
                error
                    .within(rule_frame(*rule))
                    .within(predicate_frame(predicate))
            })?;
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

        // Verification ensured that the head is dense over its predicate's
        // columns: every column filled, and none of them twice (but not
        // guaranteed to be sorted).
        let mut terms: Vec<Option<Expr>> = (0..predicate.arity()).map(|_| None).collect();
        for (position, bind) in rule.head().bindings() {
            terms[position] = Some(bound_expr(bind));
        }
        let attributes = predicate
            .columns()
            .zip(terms)
            .map(|(column, term)| {
                let term = term.expect("verified head fills every column");
                (column.name().to_string(), term)
            })
            .collect();

        Ok(Expr::from(ProjectionExpr {
            relation,
            attributes,
        }))
    }

    /// Translates a rule's body. First, [Self::conjunctive_fragment] joins
    /// the body's positive atoms on the variables they share. Then, each
    /// negated atom removes what it matches by an [`AntiJoinExpr`] of its own,
    /// on all of its variables, which the positive atoms bind. Finally, the
    /// rule's [Rule::conditions()] are applied on top of the expression as a
    /// [SelectionExpr].
    ///
    /// One antijoin per negated atom is what makes `!r(x), !s(x)` mean
    /// `¬r ∧ ¬s`, as in Datalog. Joining the negated atoms and antijoining once
    /// would compute `¬(r ∧ s)` instead.
    pub fn conjunctive_query(&self, rule: &P::Rule) -> Result<Expr, SyntaxError> {
        // Verification leaves only variable-free rules without positive atoms,
        // like `p() :- !h().`, which have nothing to antijoin against yet.
        let conjunctive = self
            .conjunctive_fragment(rule.positive_atoms())?
            .ok_or_else(|| SyntaxError::new("body has no positive atoms"))?;
        let conjunctive = rule
            .negated_atoms()
            .try_fold(conjunctive, |acc, negated_atom| {
                let negated_atom = self.atom(negated_atom)?;
                Ok(Expr::from(AntiJoinExpr {
                    on: antijoin_variables(&negated_atom.variables),
                    left: acc,
                    right: negated_atom.relation,
                }))
            })?;

        // All conditions become one condition by ANDing them, and a rule with
        // none keeps `conjunctive` unwrapped rather than gaining a vacuous selection.
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
            .fold(conjunctive, |relation, condition| {
                Expr::from(SelectionExpr {
                    relation,
                    condition,
                })
            }))
    }

    /// A conjunctive fragment joins `atoms` on the variables they share. A
    /// rule's positive atoms form one; each negated atom stands on its own.
    fn conjunctive_fragment<'r>(
        &self,
        atoms: impl IntoIterator<Item = &'r AtomOf<P>>,
    ) -> Result<Option<Expr>, SyntaxError>
    where
        AtomOf<P>: 'r,
    {
        let plans = atoms
            .into_iter()
            .map(|atom| self.atom(atom))
            .collect::<Result<Vec<_>, SyntaxError>>()?;
        if plans.is_empty() {
            return Ok(None);
        }

        let on = join_variables(&plans);
        let mut relations: Vec<Expr> = plans.into_iter().map(|plan| plan.relation).collect();
        let joined = match relations.len() {
            // A single atom has nothing to join against, and the join operators
            // require at least two relations.
            1 => relations.pop().expect("length checked"),
            _ => Expr::from(MultiWayEquiJoinExpr::new(relations, on, None)?),
        };
        Ok(Some(joined))
    }

    /// One atom: the relation it names, filtered by what it pins down locally
    /// and projected onto the variables it brings into scope.
    fn atom<'r>(&self, atom: &'r AtomOf<P>) -> Result<AtomPlan<'r, VarIdOf<P>>, SyntaxError> {
        let name = atom.id();
        // The IDB is consulted first: a predicate shadows a base relation of
        // the same name. A derived relation is bound to a host variable rather
        // than being a source leaf, because its rows are computed, not fed in.
        // Verification resolved every atom to a predicate, and the execution
        // order registers it before any rule reading it is translated.
        let predicate = self
            .predicates
            .get(name)
            .expect("verified atom references a predicate of an earlier or the same component");
        let relation = if predicate.is_edb_predicate() {
            Expr::from(SourceExpr::new(name.to_string()))
        } else {
            Expr::from(VarExpr::new(name.to_string()))
        };
        let columns: Vec<&Column> = predicate.columns().collect();

        let mut binder = AtomBinder::new();
        for (position, bind) in atom.bindings() {
            // Verification ensures each atom's bindings are within the
            // predicate's arity.
            let column = columns[position];
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
/// its join condition without re-deriving it from the [`ProjectionExpr`] just
/// built.
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

/// The variables a negated atom matches on in its antijoin, which are all of
/// its own: verification made negation safe, so all negated variables are
/// guaranteed to be bound by some positive atom.
fn antijoin_variables<I: Identifier>(negated: &[&I]) -> Vec<(Expr, Expr)> {
    negated
        .iter()
        .map(|shared_var| {
            let shared_var = Expr::from(VarExpr::new(shared_var.to_string()));
            (shared_var.clone(), shared_var)
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

fn predicate_frame<P: Predicate>(predicate: &P) -> Frame {
    Frame::Predicate {
        name: predicate.id().to_string(),
    }
}

fn rule_frame<R: Rule>(rule: &R) -> Frame {
    Frame::Rule {
        name: rule.id().to_string(),
        text: Some(rule.display().to_string()),
    }
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
    use super::super::{LogicalProgram, RulePredicate, test_utils::*};
    use crate::{
        error::Frame,
        host::{
            stmt::Stmt,
            walk::{Node, pre_order},
        },
        relational::expr::{RelExpr, RelKind},
    };

    #[test]
    fn an_error_names_the_rule_and_predicate_it_was_raised_within() {
        // The body negates its only atom. Being variable-free, the rule is
        // safe, so only its translation notices that nothing is left to
        // antijoin against.
        let predicates = RulePredicate::group(
            [declare("blocked", &[]), declare("open", &[])],
            [
                rule("b0", "blocked()", &[]),
                rule("r0", "open()", &["!blocked()"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let error = program
            .prepare(program.verify().expect("the program is valid"))
            .expect_err("the body has no positive atoms");

        assert_eq!(error.message(), "body has no positive atoms");
        assert_eq!(
            error.context().collect::<Vec<_>>(),
            [
                &Frame::Predicate {
                    name: "open".to_string()
                },
                &Frame::Rule {
                    name: "r0".to_string(),
                    text: Some("open() :- !blocked().".to_string()),
                },
            ]
        );
        assert_eq!(
            error.to_string(),
            "in predicate 'open': in rule 'r0': body has no positive atoms"
        );
        assert_eq!(
            format!("{error:#}"),
            "body has no positive atoms\n  \
             in rule 'r0': open() :- !blocked().\n  \
             in predicate 'open'"
        );
    }

    #[test]
    fn a_self_recursive_rule_reads_its_own_predicate() {
        // `r1` references `path` while `path` itself is being translated, so
        // the atom has to find the predicate before its translation finishes.
        let predicates = RulePredicate::group(
            [declare("edge", &["x", "y"]), declare("path", &["x", "y"])],
            [
                rule("e0", "edge(x, y)", &[]),
                rule("r0", "path(x, y)", &["edge(x, y)"]),
                rule("r1", "path(x, z)", &["path(x, y)", "edge(y, z)"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let execution_order = program.verify().expect("the program is stratifiable");
        if let Err(error) = program.prepare(execution_order) {
            panic!("a self-recursive predicate translates:\n{error:#}");
        }
    }

    #[test]
    fn no_rule_in_a_fixed_point_step_shadows_the_accumulator() {
        // The recursive rules are named after their predicate, as plain
        // Datalog names them, and the base rule is not, so it does not take
        // the bare name first. Were a recursive rule bound under `path` inside
        // the step, every recursive rule after it would read that rule's
        // result rather than the accumulator.
        let predicates = RulePredicate::group(
            [declare("edge", &["x", "y"]), declare("path", &["x", "y"])],
            [
                rule("e0", "edge(x, y)", &[]),
                rule("base", "path(x, y)", &["edge(x, y)"]),
                rule("path", "path(x, z)", &["path(x, y)", "edge(y, z)"]),
                rule("path", "path(x, z)", &["edge(x, y)", "path(y, z)"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let ir = program
            .prepare(program.verify().expect("the program is stratifiable"))
            .expect("a self-recursive predicate translates");

        let fixed_point = pre_order(&ir)
            .filter_map(Node::as_rel)
            .find_map(|rel_expr| match rel_expr {
                RelExpr::FixedPointIter(fixed_point) => Some(fixed_point),
                _ => None,
            })
            .expect("a recursive predicate iterates to a fixed point");
        let (accumulator, _) = &fixed_point.accumulator;
        let shadows = fixed_point.step.stmts.iter().any(|stmt| match stmt {
            Stmt::Var(var) => var.name == *accumulator,
            _ => false,
        });
        assert!(
            !shadows,
            "the step rebinds the accumulator '{accumulator}':\n{}",
            ir.to_tree()
        );
    }

    #[test]
    fn each_negated_atom_is_antijoined_on_its_own() {
        // `!r(x), !s(x)` is `¬r ∧ ¬s`, so two antijoins. Joining the negated
        // atoms first and antijoining once would compute `¬(r ∧ s)` instead.
        let predicates = RulePredicate::group(
            ["q", "r", "s", "p"].map(|name| declare(name, &["x"])),
            [
                rule("q0", "q(x)", &[]),
                rule("r0", "r(x)", &[]),
                rule("s0", "s(x)", &[]),
                rule("p0", "p(x)", &["q(x)", "!r(x)", "!s(x)"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let ir = program
            .prepare(program.verify().expect("the program is stratifiable"))
            .expect("a rule with negated atoms translates");

        let antijoins = pre_order(&ir)
            .filter_map(Node::as_rel)
            .filter(|rel_expr| rel_expr.kind() == RelKind::AntiJoin)
            .count();
        assert_eq!(antijoins, 2, "{}", ir.to_tree());
    }

    #[test]
    fn a_negated_atom_may_share_no_variable_with_the_body() {
        // `h` has no columns, so `!h()` antijoins on nothing: `p` holds
        // wherever `q` does, as long as `h` holds nowhere.
        let predicates = RulePredicate::group(
            [
                declare("q", &["x"]),
                declare("r", &["x"]),
                declare("h", &[]),
                declare("p", &["x"]),
            ],
            [
                rule("q0", "q(x)", &[]),
                rule("r0", "r(x)", &[]),
                rule("h0", "h()", &["r(y)"]),
                rule("p0", "p(x)", &["q(x)", "!h()"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        if let Err(error) = program.prepare(program.verify().expect("the program is stratifiable"))
        {
            panic!("a nullary negated atom translates:\n{error:#}");
        }
    }
}
