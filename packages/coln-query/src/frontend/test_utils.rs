// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Test scaffolding for this layer: a [`LogicalProgram`] implemented over plain
//! strings, plus builders that let a test read as the Datalog it stands for.
//!
//! Shared by the tests of [`super`] and of [`super::translation`], which is why
//! it sits in a module of its own rather than inside either one's `mod tests`.

use super::*;
use crate::{error::SyntaxError, test_utils::table_schema};
use std::collections::HashMap;

impl Identifier for String {}

/// A predicate named `name`, defined by one rule per entry in `rules`,
/// each rule listing the names its atoms reference. A name that is not a
/// predicate of the program stands for a base table from the EDB.
pub(super) fn pred(name: &str, rules: &[&[&str]]) -> TestPredicate {
    TestPredicate {
        name: name.to_owned(),
        columns: columns(),
        rules: rules
            .iter()
            .enumerate()
            .map(|(idx, atoms)| rule(&format!("{name}#{idx}"), name, atoms))
            .collect(),
    }
}

/// A rule called `name`, deriving the atom `head` from the body `atoms`.
///
/// Each is an [`atom`] spec, so `rule("r0", "path(x, y)", &["edge(x, y)"])`
/// reads as the rule it stands for. A bare name binds nothing, which is all
/// the tests that only look at the reference graph need.
pub(super) fn rule(name: &str, head: &str, atoms: &[&str]) -> TestRule {
    TestRule {
        name: name.to_owned(),
        head: atom(head),
        atoms: atoms.iter().map(|atom_spec| atom(atom_spec)).collect(),
    }
}

/// An atom written as `name(term, term, …)`, or as a bare `name` when it
/// binds nothing.
///
/// A term that parses as a number is a literal, `_` leaves that position
/// unbound (the sparseness [`Atom::bindings`] allows), and anything else is
/// a variable.
pub(super) fn atom(spec: &str) -> TestAtom {
    let (name, terms) = spec.split_once('(').map_or((spec, ""), |(name, terms)| {
        (name, terms.trim_end_matches(')'))
    });
    let bindings = terms
        .split(',')
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .enumerate()
        .filter(|(_, term)| *term != "_")
        .map(|(position, term)| {
            let bind = match term.parse::<u64>() {
                Ok(value) => Bind::Lit(TestLit(Literal::Uint(value))),
                Err(_) => Bind::Var(TestTypedVar {
                    name: term.to_owned(),
                }),
            };
            (position, bind)
        })
        .collect();
    TestAtom {
        name: name.trim().to_owned(),
        bindings,
    }
}

/// A rule paired with the definand it derives, the shape
/// [`RulePredicate::group`] takes. The rule heads an atom named after the
/// definand, which is the ordinary case; pass an explicit pair where the
/// two have to differ.
pub(super) fn derives(head: &str, name: &str, atoms: &[&str]) -> (String, TestRule) {
    (atom(head).name, rule(name, head, atoms))
}

/// A program whose EDB holds every name the predicates reference but do not
/// define, so that it is free of dangling references by construction.
pub(super) fn program(predicates: Vec<TestPredicate>) -> TestLogicalProgram {
    let defined: HashSet<&str> = predicates
        .iter()
        .map(|predicate| predicate.name.as_str())
        .collect();
    let base_relations = predicates
        .iter()
        .flat_map(|predicate| predicate.rules.iter())
        .flat_map(|rule| rule.atoms.iter())
        .map(|atom| atom.name.as_str())
        .filter(|name| !defined.contains(name))
        .map(edb_entry)
        .collect();
    TestLogicalProgram {
        predicates,
        base_relations,
    }
}

/// A program whose EDB holds exactly `base_relations`, for the cases where
/// that matters.
pub(super) fn program_over(
    predicates: Vec<TestPredicate>,
    base_relations: &[&str],
) -> TestLogicalProgram {
    TestLogicalProgram {
        predicates,
        base_relations: base_relations.iter().copied().map(edb_entry).collect(),
    }
}

/// A base relation paired with its schema. The columns play no part in
/// grouping, resolution or stratification, so one keyed column stands in
/// for whatever shape a translation test will want later.
pub(super) fn edb_entry(name: &str) -> (String, TableSchema) {
    (
        name.to_owned(),
        table_schema(name, [("x", ScalarType::Uint)], ["x"]),
    )
}

/// The declared columns of a test predicate, matching [`edb_entry`]'s
/// single column so heads and base relations line up.
pub(super) fn columns() -> Vec<Column> {
    vec![Column::new("x", ScalarType::Uint)]
}

/// A declaration for each `name`, all sharing [`columns`].
pub(super) fn declarations(names: &[&str]) -> Vec<(String, Vec<Column>)> {
    names
        .iter()
        .map(|name| ((*name).to_owned(), columns()))
        .collect()
}

/// The names of the predicates per clique, in execution order.
pub(super) fn cliques_of(program: &TestLogicalProgram) -> Vec<Vec<String>> {
    program
        .cliques()
        .into_iter()
        .map(|clique| clique.members().map(|member| member.id().clone()).collect())
        .collect()
}

/// The names of the non-recursive and the recursive rules of every clique,
/// in execution order.
pub(super) fn rule_split_of(program: &TestLogicalProgram) -> Vec<(Vec<String>, Vec<String>)> {
    program
        .cliques()
        .into_iter()
        .map(|clique| {
            let names = |rules: &mut dyn Iterator<Item = &TestRule>| {
                rules.map(|rule| rule.id().clone()).collect()
            };
            (
                names(&mut clique.non_rec_rules()),
                names(&mut clique.rec_rules()),
            )
        })
        .collect()
}

/// The offending names, in the order they are reported.
pub(super) fn dangling_names(program: &TestLogicalProgram) -> Vec<String> {
    program
        .dangling_references()
        .iter()
        .map(|reference| reference.identifier.clone())
        .collect()
}

/// The predicate names, and per predicate its rule names.
pub(super) fn grouping_of(predicates: &[TestPredicate]) -> Vec<(&str, Vec<&str>)> {
    predicates
        .iter()
        .map(|predicate| {
            (
                predicate.id().as_str(),
                predicate.rules().map(|rule| rule.id().as_str()).collect(),
            )
        })
        .collect()
}

pub(super) struct TestLogicalProgram {
    predicates: Vec<TestPredicate>,
    base_relations: HashMap<String, TableSchema>,
}

impl LogicalProgram for TestLogicalProgram {
    type Identifier = String;
    type Predicate = TestPredicate;

    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate> {
        self.predicates.iter()
    }

    fn base_relation_schema(&self, identifier: &Self::Identifier) -> Option<Cow<'_, TableSchema>> {
        self.base_relations.get(identifier).map(Cow::Borrowed)
    }
}

/// The tests use [`RulePredicate`] itself as their [`Predicate`], so the
/// scaffolding stops at the rule level.
pub(super) type TestPredicate = RulePredicate<String, TestRule>;

#[derive(Debug)]
pub(super) struct TestRule {
    name: String,
    head: TestAtom,
    atoms: Vec<TestAtom>,
}

impl Identifiable<String> for TestRule {
    fn id(&self) -> &String {
        &self.name
    }
}

impl Rule for TestRule {
    type Identifier = String;
    type Atom = TestAtom;
    type Cond = TestCond;

    fn head(&self) -> &Self::Atom {
        &self.head
    }

    fn atoms(&self) -> impl Iterator<Item = &Self::Atom> {
        self.atoms.iter()
    }

    fn conditions(&self) -> impl Iterator<Item = &Self::Cond> {
        std::iter::empty()
    }
}

#[derive(Debug)]
pub(super) struct TestAtom {
    name: String,
    bindings: Vec<(usize, Bind<TestTypedVar, TestLit>)>,
}

impl Identifiable<String> for TestAtom {
    fn id(&self) -> &String {
        &self.name
    }
}

impl Atom for TestAtom {
    type Identifier = String;
    type Var = TestTypedVar;
    type Lit = TestLit;

    fn bindings(&self) -> impl Iterator<Item = (usize, Bind<&Self::Var, &Self::Lit>)> {
        self.bindings
            .iter()
            .map(|(position, bind)| (*position, borrow(bind)))
    }

    fn vars(&self) -> impl Iterator<Item = &Self::Var> {
        self.bindings.iter().filter_map(|(_, bind)| match bind {
            Bind::Var(var) => Some(var),
            Bind::Lit(_) => None,
        })
    }
}

#[derive(Debug)]
pub(super) struct TestTypedVar {
    name: String,
}

impl Identifiable<String> for TestTypedVar {
    fn id(&self) -> &String {
        &self.name
    }
}

impl TypedVar for TestTypedVar {
    type Identifier = String;

    fn ty(&self) -> ScalarType {
        ScalarType::Uint
    }
}

#[derive(Debug)]
pub(super) struct TestLit(Literal);

impl Lit for TestLit {
    fn to_literal(&self) -> Literal {
        self.0.clone()
    }
}

/// Conditions play no part in the reference graph, so the test programs
/// carry none and this type exists only to satisfy [`Rule::Cond`].
#[derive(Debug)]
pub(super) struct TestCond {
    operator: Operator,
    left: Bind<TestTypedVar, TestLit>,
    right: Bind<TestTypedVar, TestLit>,
}

impl Cond for TestCond {
    type Identifier = String;
    type Var = TestTypedVar;
    type Lit = TestLit;

    fn operator(&self) -> impl Into<Operator> {
        self.operator
    }

    fn left(&self) -> Bind<&Self::Var, &Self::Lit> {
        borrow(&self.left)
    }

    fn right(&self) -> Bind<&Self::Var, &Self::Lit> {
        borrow(&self.right)
    }
}

pub(super) fn borrow<Var, Lit>(bind: &Bind<Var, Lit>) -> Bind<&Var, &Lit> {
    match bind {
        Bind::Var(var) => Bind::Var(var),
        Bind::Lit(lit) => Bind::Lit(lit),
    }
}

/// The EDB every translation test runs against.
pub(super) fn edb() -> HashMap<String, TableSchema> {
    [
        (
            "edge",
            vec![("from", ScalarType::Uint), ("to", ScalarType::Uint)],
        ),
        ("node", vec![("id", ScalarType::Uint)]),
    ]
    .into_iter()
    .map(|(name, columns)| (name.to_owned(), table_schema(name, columns, [])))
    .collect()
}

/// A predicate declaration, every column of it a `Uint`.
pub(super) fn declare(name: &str, columns: &[&str]) -> (String, Vec<Column>) {
    (
        name.to_owned(),
        columns
            .iter()
            .map(|column| Column::new(*column, ScalarType::Uint))
            .collect(),
    )
}

/// The IR a program of `declarations` and `rules` lowers to, rendered.
pub(super) fn translated(
    declarations: Vec<(String, Vec<Column>)>,
    rules: Vec<(String, TestRule)>,
) -> Result<String, SyntaxError> {
    let predicates = RulePredicate::group(declarations, rules).expect("every definand is declared");
    let program = TestLogicalProgram {
        predicates,
        base_relations: edb(),
    };
    translation::translate(&program).map(|ir| ir.to_tree())
}
