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
//! Translation therefore makes each rule's name unique within its component,
//! suffixing a counter where it has to. A frontend whose rule names are already
//! distinct (e.g. FLIR's rules) keeps them untouched. Unique within the
//! predicate would not do: mutually recursive predicates iterate in one fixed
//! point, whose step binds the recursive rules of all of them side by side.
//!
//! No rule is ever bound under the name of a predicate of its component either,
//! as the same shadowing can happen inside a fixed point: its step reads each
//! accumulator under its predicate's name, so in `var path = …; var path#1 = …
//! path …` the second rule reads the first rule's result rather than the
//! accumulator. The predicates' names are therefore allocated before any rule
//! is named, and a rule carrying one gets a suffix like any other duplicate.
//!
//! Outside a fixed point, sharing the name would be harmless: the predicate's
//! statement comes after its rules', and [`Resolver`](crate::host::resolver)
//! resolves an initializer before declaring the name it binds. Taking the name
//! regardless keeps one rule for both cases.
//!
//! A fixed point evaluates to a tuple of its accumulators' relations, which is
//! bound to a variable of its own, named after the component, e.g.
//! `fixpoint(red, blue)`, before each predicate is bound to its element of it.

use super::{
    AggregateRules, Atom, Bind, Component, Cond, IdOf, Identifiable, Identifier, Lit, Predicate,
    Rule, TypedVar,
};
use crate::{
    frontend::{ExecutionOrder, analysis::PredicateComponent},
    host::{
        QueryIr,
        expr::{BinaryExpr, Expr, GetIndexExpr, LiteralExpr, TupleExpr, VarExpr},
        operator::Operator,
        stmt::{BlockStmt, ExprStmt, Stmt, VarStmt},
    },
    relational::{
        expr::{
            AntiJoinExpr, ConstantExpr, FixedPointIterExpr, JoinVariable, MultiWayEquiJoinExpr,
            OutputExpr, OutputKind, ProjectionExpr, RelationIdx, SelectionExpr, SinkId, SourceExpr,
            UnionExpr,
        },
        schema::{Column, EntityRef, TableSchema},
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
    ///
    /// Infallible: the execution order is only handed out by a
    /// [verified](super::Analysis::verify) analysis, which rejects every
    /// program this could not translate. What this relies on is asserted
    /// where it is relied on.
    pub(super) fn run(mut self, program: &'a ExecutionOrder<'a, P>) -> Prepared {
        program
            .iter()
            .for_each(|component| self.component(component));
        Prepared(self.ir)
    }

    /// Emits one component. Each member's rules are split into its base rules,
    /// which read no member of the component, and its recursive rules, which do.
    ///
    /// A non-recursive component is a single predicate: a statement per rule,
    /// then the statement binding the predicate to the union of them. A
    /// recursive one iterates all of its members to their joint fixed point
    /// instead, see [`Self::fixed_point`].
    fn component(&mut self, component: &'a PredicateComponent<'a, P>) {
        // A component's members are in scope for its own rules: inside the
        // fixed point, each name reads the accumulator bound under it.
        self.predicates
            .extend(component.members().map(|member| (member.id(), member)));

        if component.is_edb_component().is_some() {
            // An EDB predicate needs no translation but only the registration
            // above in `self.predicates`.
            return;
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
        let members: Vec<MemberRules<'a, P>> = component
            .members()
            .map(|predicate| {
                let (base, rec) = predicate
                    .rules()
                    .map(|rule| (names.mint(rule.id()), rule))
                    .partition(|(_, rule)| !component.references_member(rule));
                MemberRules {
                    predicate,
                    base,
                    rec,
                }
            })
            .collect();

        debug_assert!(
            members.iter().any(|member| !member.base.is_empty()),
            "verification rejects a component without a base case"
        );

        if members.iter().all(|member| member.rec.is_empty()) {
            // Only a single predicate can read no member of its own component.
            let [member] = members.as_slice() else {
                unreachable!("a component of several predicates is recursive");
            };
            let (relation, stmts) = self.rules(member.predicate, &member.base);
            self.ir.extend(stmts);
            self.bind(member.predicate, relation);
            return;
        }

        self.fixed_point(&members, &mut names);
    }

    /// Emits a recursive component as one fixed point with an accumulator per
    /// member, so that mutually recursive predicates iterate together. Each
    /// accumulator is named after its member and seeded with the union of the
    /// member's base rules, which are emitted ahead of the fixed point.
    /// A member without any base rule starts out empty, as only the other
    /// members' rules can derive its rows.
    ///
    /// The step binds every recursive rule of every member and evaluates to a
    /// tuple of each member's union of them. The fixed point evaluates to a
    /// tuple of each member's final relation in turn, which is bound to a
    /// variable of its own, so that each member can be bound under its name.
    fn fixed_point(&mut self, members: &[MemberRules<'a, P>], names: &mut Names) {
        let accumulators = members
            .iter()
            .map(|member| {
                let seed = match member.base.is_empty() {
                    true => Expr::from(ConstantExpr::empty(table_schema(member.predicate))),
                    false => {
                        let (relation, stmts) = self.rules(member.predicate, &member.base);
                        self.ir.extend(stmts);
                        relation
                    }
                };
                (member.predicate.id().to_string(), seed)
            })
            .collect();

        let (steps, step_stmts): (Vec<Expr>, Vec<Vec<Stmt>>) = members
            .iter()
            .map(|member| {
                // Every member of a recursive component reads one: A predicate
                // reads itself, and in a component of several, each member is
                // reached from the others only through its own rules.
                assert!(
                    !member.rec.is_empty(),
                    "every member of a recursive component has a recursive rule"
                );
                self.rules(member.predicate, &member.rec)
            })
            .unzip();
        let mut step_stmts: Vec<Stmt> = step_stmts.into_iter().flatten().collect();
        // The step block evaluates to its last expression, which holds the
        // union of each member's recursive rules in a tuple.
        step_stmts.push(Stmt::from(ExprStmt {
            expr: Expr::from(TupleExpr { elements: steps }),
        }));

        let ids: Vec<String> = members
            .iter()
            .map(|member| member.predicate.id().to_string())
            .collect();
        let fixed_point = names.mint(&format!("fixpoint({})", ids.join(", ")));
        self.ir.push(Stmt::from(VarStmt {
            name: fixed_point.clone(),
            initializer: Some(Expr::from(FixedPointIterExpr {
                accumulators,
                step: BlockStmt { stmts: step_stmts },
            })),
        }));
        members.iter().enumerate().for_each(|(index, member)| {
            let relation = Expr::from(GetIndexExpr {
                target: Expr::from(VarExpr::new(fixed_point.clone())),
                index: Expr::from(LiteralExpr::from(index as u64)),
            });
            self.bind(member.predicate, relation);
        });
    }

    /// Binds `predicate` to `relation` under its own name, which is how the
    /// rules of later components read it, tapping it as an output if the
    /// program exposes it.
    fn bind(&mut self, predicate: &P, relation: Expr) {
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
    }

    /// Binds each rule to a statement of its own and unions those bindings.
    ///
    /// The union is what the predicate evaluates to; the statements have to be
    /// emitted ahead of it, since it references them by name.
    fn rules(&self, predicate: &P, rules: &[(String, &P::Rule)]) -> (Expr, Vec<Stmt>) {
        let mut relations: Vec<Expr> = Vec::with_capacity(rules.len());
        let mut stmts: Vec<Stmt> = Vec::with_capacity(rules.len());
        for (name, rule) in rules {
            relations.push(Expr::from(VarExpr::new(name.clone())));
            stmts.push(Stmt::from(VarStmt {
                name: name.clone(),
                initializer: Some(self.rule(predicate, rule)),
            }));
        }
        let combined = match relations.len() {
            // A lone rule is its own union, and `UnionExpr` wants two or more.
            1 => relations.pop().expect("length checked"),
            _ => Expr::from(UnionExpr { relations }),
        };
        (combined, stmts)
    }

    /// One rule: its body as a conjunctive query, projected onto the columns
    /// its predicate declares.
    fn rule(&self, predicate: &P, rule: &P::Rule) -> Expr {
        let relation = self.conjunctive_query(rule);

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

        Expr::from(ProjectionExpr {
            relation,
            attributes,
        })
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
    fn conjunctive_query(&self, rule: &P::Rule) -> Expr {
        let conjunctive = self.conjunctive_fragment(rule.positive_atoms());
        let conjunctive = rule.negated_atoms().fold(conjunctive, |acc, negated_atom| {
            let negated_atom = self.atom(negated_atom);
            Expr::from(AntiJoinExpr {
                on: antijoin_variables(&negated_atom.variables),
                left: acc,
                right: negated_atom.relation,
            })
        });

        // All conditions become one condition by ANDing them, and a rule with
        // none keeps `conjunctive` unwrapped rather than gaining a vacuous selection.
        rule.conditions()
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
            })
    }

    /// A conjunctive fragment joins `atoms` on the variables they share. A
    /// rule's positive atoms form one; each negated atom stands on its own.
    ///
    /// No atoms at all join to the [unit](ConstantExpr::unit) relation, the
    /// join's neutral element, just as an empty conjunction is `true`. That is
    /// the body of a rule with an empty antecedent, like `m() :- !c().`, which
    /// holds as long as `c` is empty.
    /// [Static program verification](mod@super::analysis) guarantees that such
    /// a rule does not contain a variable, as otherwise the range restriction
    /// or negation safety would not hold.
    fn conjunctive_fragment<'r>(&self, atoms: impl IntoIterator<Item = &'r AtomOf<P>>) -> Expr
    where
        AtomOf<P>: 'r,
    {
        let plans: Vec<_> = atoms.into_iter().map(|atom| self.atom(atom)).collect();

        let on = join_variables(&plans);
        let mut relations: Vec<Expr> = plans.into_iter().map(|plan| plan.relation).collect();
        match relations.len() {
            0 => Expr::from(ConstantExpr::unit()),
            // A single atom has nothing to join against, and the join operators
            // require at least two relations.
            1 => relations.pop().expect("length checked"),
            // Well-formed by construction: there are two relations or more,
            // `join_variables` keeps a variable only if two of them bind it,
            // indexes them by their position among `plans`, and groups each
            // variable's occurrences under one entry, of which an atom
            // contributes at most one (see `AtomBinder::bind`).
            _ => Expr::from(
                MultiWayEquiJoinExpr::new(relations, on, None)
                    .expect("join variables are well-formed by construction"),
            ),
        }
    }

    /// One atom: the relation it names, filtered by what it pins down locally
    /// and projected onto the variables it brings into scope.
    fn atom<'r>(&self, atom: &'r AtomOf<P>) -> AtomPlan<'r, VarIdOf<P>> {
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

        AtomPlan {
            relation: Expr::from(ProjectionExpr {
                relation: filtered,
                attributes: binder.attributes,
            }),
            variables: binder.variables,
        }
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

/// One member of a component, with its rules split as
/// [`Translator::component`] splits them, each under the IR variable name
/// minted for it.
struct MemberRules<'a, P: Predicate> {
    predicate: &'a P,
    /// The rules reading no member of the component.
    base: Vec<(String, &'a P::Rule)>,
    /// The rules reading some member of the component.
    rec: Vec<(String, &'a P::Rule)>,
}

/// The schema of `predicate`'s relation, with no key, as its rules derive it.
fn table_schema<P: Predicate>(predicate: &P) -> TableSchema {
    TableSchema::new(
        EntityRef::from(predicate.id().to_string()),
        predicate.columns().cloned().collect(),
        Vec::new(),
    )
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
        host::{
            QueryIr,
            expr::{Expr, Literal},
            stmt::Stmt,
            walk::{Node, pre_order},
        },
        relational::expr::{FixedPointIterExpr, RelExpr, RelKind},
    };

    #[test]
    fn an_empty_antecedent_antijoins_from_the_unit_relation() {
        // `open` holds exactly while `blocked` is empty: with no positive atom
        // to join, the body is the unit relation, which the negation antijoins
        // on nothing.
        let predicates = RulePredicate::group(
            [declare("blocked", &[]), declare("open", &[])],
            [
                rule("b0", "blocked()", &[]),
                rule("r0", "open()", &["!blocked()"]),
            ],
        )
        .expect("every definand names a declared predicate");
        let program = program(predicates);
        let ir = program.prepare(program.verify().expect("the program is valid"));

        let antijoin = pre_order(&ir)
            .filter_map(Node::as_rel)
            .find_map(|rel_expr| match rel_expr {
                RelExpr::AntiJoin(antijoin) => Some(antijoin),
                _ => None,
            })
            .expect("the negation becomes an antijoin");
        assert!(
            matches!(&antijoin.left, Expr::Relational(RelExpr::Constant(constant)) if constant.is_unit()),
            "the antijoin's left operand is not the unit relation:\n{}",
            ir.to_tree()
        );
        assert!(antijoin.on.is_empty(), "{}", ir.to_tree());
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
        // Translating panics if the atom does not find the predicate.
        program.prepare(program.verify().expect("the program is stratifiable"));
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
        let ir = program.prepare(program.verify().expect("the program is stratifiable"));

        let fixed_point = pre_order(&ir)
            .filter_map(Node::as_rel)
            .find_map(|rel_expr| match rel_expr {
                RelExpr::FixedPointIter(fixed_point) => Some(fixed_point),
                _ => None,
            })
            .expect("a recursive predicate iterates to a fixed point");
        assert_no_accumulator_is_shadowed(fixed_point, &ir);
    }

    /// Fails if a statement of the fixed point's step rebinds one of its
    /// accumulators, which hides it from every statement after.
    fn assert_no_accumulator_is_shadowed(fixed_point: &FixedPointIterExpr, ir: &QueryIr) {
        for (accumulator, _) in &fixed_point.accumulators {
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
    }

    /// The single fixed point `ir` contains.
    fn sole_fixed_point(ir: &QueryIr) -> &FixedPointIterExpr {
        let mut fixed_points = pre_order(ir)
            .filter_map(Node::as_rel)
            .filter_map(|rel_expr| match rel_expr {
                RelExpr::FixedPointIter(fixed_point) => Some(&**fixed_point),
                _ => None,
            });
        let fixed_point = fixed_points
            .next()
            .unwrap_or_else(|| panic!("there is no fixed point:\n{}", ir.to_tree()));
        assert!(
            fixed_points.next().is_none(),
            "there is more than one fixed point:\n{}",
            ir.to_tree()
        );
        fixed_point
    }

    /// The red/blue reachability of a bipartite graph: `red(x)` holds for the
    /// red nodes reachable from a red seed, alternating colour with every
    /// edge. `red` and `blue` read each other, so they form one component.
    fn red_blue(blue_has_base_case: bool) -> Vec<TestPredicate> {
        let mut rules = vec![
            rule("e0", "edge(x, y)", &[]),
            rule("s0", "seed(x)", &[]),
            rule("red_base", "red(x)", &["seed(x)"]),
            rule("red_step", "red(y)", &["blue(x)", "edge(x, y)"]),
            rule("blue_step", "blue(y)", &["red(x)", "edge(x, y)"]),
        ];
        if blue_has_base_case {
            rules.push(rule("blue_base", "blue(x)", &["seed(x)"]));
        }
        RulePredicate::group(
            [
                declare("edge", &["x", "y"]),
                declare("seed", &["x"]),
                declare("red", &["x"]),
                declare("blue", &["x"]),
            ],
            rules,
        )
        .expect("every definand names a declared predicate")
    }

    #[test]
    fn mutually_recursive_predicates_iterate_in_one_fixed_point() {
        let program = program(red_blue(true));
        let ir = program.prepare(program.verify().expect("the program is stratifiable"));

        let fixed_point = sole_fixed_point(&ir);
        let accumulators: Vec<&str> = fixed_point
            .accumulators
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(accumulators, ["red", "blue"], "{}", ir.to_tree());
        assert_no_accumulator_is_shadowed(fixed_point, &ir);

        // The step yields one relation per accumulator.
        let Some(Stmt::Expr(last)) = fixed_point.step.stmts.last() else {
            panic!("the step does not end in an expression:\n{}", ir.to_tree());
        };
        assert!(
            matches!(&last.expr, Expr::Tuple(tuple) if tuple.elements.len() == 2),
            "the step does not yield a pair:\n{}",
            ir.to_tree()
        );

        // Each member is bound under its own name to its relation of the
        // fixed point's tuple, and tapped as an output.
        for (index, member) in ["red", "blue"].into_iter().enumerate() {
            let initializer = ir
                .iter()
                .find_map(|stmt| match stmt {
                    Stmt::Var(var) if var.name == member => var.initializer.as_ref(),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("'{member}' is not bound:\n{}", ir.to_tree()));
            let Expr::Relational(RelExpr::Output(output)) = initializer else {
                panic!("'{member}' is not an output:\n{}", ir.to_tree());
            };
            assert!(
                matches!(
                    &output.relation,
                    Expr::GetIndex(get_index) if matches!(
                        &get_index.index,
                        Expr::Literal(literal) if literal.value == Literal::Uint(index as u64)
                    )
                ),
                "'{member}' does not read element {index} of the fixed point:\n{}",
                ir.to_tree()
            );
        }
    }

    #[test]
    fn a_member_without_a_base_case_starts_out_empty() {
        // Only `red` has a base rule; `blue` is derived from `red` alone.
        let program = program(red_blue(false));
        let ir = program.prepare(
            program
                .verify()
                .expect("a component needs a base case in one member only"),
        );

        let fixed_point = sole_fixed_point(&ir);
        let [(_, red), (blue_name, blue)] = fixed_point.accumulators.as_slice() else {
            panic!("there are not exactly two accumulators:\n{}", ir.to_tree());
        };
        assert_eq!(blue_name, "blue");
        assert!(
            matches!(blue, Expr::Relational(RelExpr::Constant(constant)) if constant.is_empty()),
            "'blue' is not seeded with the empty relation:\n{}",
            ir.to_tree()
        );
        assert!(
            !matches!(red, Expr::Relational(RelExpr::Constant(_))),
            "'red' is not seeded with its base rule:\n{}",
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
        let ir = program.prepare(program.verify().expect("the program is stratifiable"));

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
        program.prepare(program.verify().expect("the program is stratifiable"));
    }
}
