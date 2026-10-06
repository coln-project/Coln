// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Test scaffolding for this layer: a [`LogicalProgram`] over the three
//! identifier spaces declared below, plus builders that let a test read as the
//! Datalog it stands for.
//!
//! Shared by the tests of [`super`] and of [`super::translation`], which is why
//! it sits in a module of its own rather than inside either one's `mod tests`.

use super::*;
use crate::{relational::schema::TableSchema, test_utils::table_schema};
use std::collections::{HashMap, HashSet};

/// Declares an identifier space: a name that is a type of its own, so that
/// handing one space's name where another belongs fails to compile.
macro_rules! name_space {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub(super) struct $name(String);

        impl $name {
            pub(super) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(name: &str) -> Self {
                Self(name.to_owned())
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        impl Identifier for $name {}
    };
}

name_space! {
    /// What an [`Atom`] references and a [`Predicate`] is declared under. Names
    /// a base relation of the EDB just as well: an atom cannot tell one from a
    /// derived predicate by its name alone, which is the whole point of the
    /// shadowing [`Translator`](super::translation) resolves.
    PredicateName
}

name_space! {
    /// A rule's own name. Read by diagnostics and by the IR variable names
    /// translation mints, never to reference anything.
    RuleName
}

name_space! {
    /// A variable, in scope only within the one rule binding it.
    VarName
}

/// A predicate named `name`, defined by one rule per entry in `rules`,
/// each rule listing the names its atoms reference. A name that is not a
/// predicate of the program stands for a base table from the EDB.
pub(super) fn pred(name: &str, rules: &[&[&str]]) -> TestPredicate {
    TestPredicate {
        name: name.into(),
        columns: columns(),
        rules: rules
            .iter()
            .enumerate()
            .map(|(idx, atoms)| rule(&format!("{name}#{idx}"), name, atoms))
            .collect(),
    }
}

/// A rule called `name`, deriving the atom `head` from the `body`.
///
/// Each is an [`atom`] spec, so `rule("r0", "path(x, y)", &["edge(x, y)"])`
/// reads as the rule it stands for. A bare name binds nothing, which is all
/// the tests that only look at the reference graph need. A body spec that
/// reads as a [`cond`] is a condition instead.
pub(super) fn rule(name: &str, head: &str, body: &[&str]) -> TestRule {
    TestRule {
        name: name.into(),
        head: atom(head),
        atoms: body
            .iter()
            .filter(|spec| cond(spec).is_none())
            .map(|spec| atom(spec))
            .collect(),
        conditions: body.iter().filter_map(|spec| cond(spec)).collect(),
    }
}

/// A condition written as `term operator term`, if `spec` is one: three
/// whitespace-separated parts, the middle a comparison's symbol. The terms
/// read as in an [`atom`].
fn cond(spec: &str) -> Option<TestCond> {
    let [left, operator, right] = spec.split_whitespace().collect::<Vec<_>>()[..] else {
        return None;
    };
    let operator = [
        Operator::Equal,
        Operator::NotEqual,
        Operator::Less,
        Operator::LessEqual,
        Operator::Greater,
        Operator::GreaterEqual,
    ]
    .into_iter()
    .find(|candidate| candidate.symbol() == operator)?;
    Some(TestCond {
        operator,
        left: term(left),
        right: term(right),
    })
}

/// An atom written as `name(term, term, …)`, or as a bare `name` when it
/// binds nothing. A leading `!` negates it.
///
/// A term that parses as a number or is in double quotes is a literal, `_`
/// leaves that position unbound (the sparseness [`Atom::bindings`] allows),
/// and anything else is a variable.
pub(super) fn atom(spec: &str) -> TestAtom {
    let (spec, positive) = spec
        .strip_prefix('!')
        .map_or((spec, true), |negated| (negated, false));
    let (name, terms) = spec.split_once('(').map_or((spec, ""), |(name, terms)| {
        (name, terms.trim_end_matches(')'))
    });
    let bindings = terms
        .split(',')
        .map(str::trim)
        .filter(|spec| !spec.is_empty())
        .enumerate()
        .filter(|(_, spec)| *spec != "_")
        .map(|(position, spec)| (position, term(spec)))
        .collect();
    TestAtom {
        name: name.trim().into(),
        positive,
        bindings,
    }
}

fn term(spec: &str) -> Bind<TestTypedVar, TestLit> {
    let string = spec
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'));
    match (string, spec.parse::<u64>()) {
        (Some(string), _) => Bind::Lit(TestLit(Literal::String(string.into()))),
        (None, Ok(value)) => Bind::Lit(TestLit(Literal::Uint(value))),
        (None, Err(_)) => Bind::Var(TestTypedVar { name: spec.into() }),
    }
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
pub(super) fn declarations(names: &[&str]) -> Vec<(PredicateName, Vec<Column>)> {
    names
        .iter()
        .map(|name| ((*name).into(), columns()))
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
    type Predicate = TestPredicate;

    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate> {
        self.predicates.iter()
    }
}

/// The tests use [`RulePredicate`] itself as their [`Predicate`], so the
/// scaffolding stops at the rule level.
pub(super) type TestPredicate = RulePredicate<TestRule>;

#[derive(Debug)]
pub(super) struct TestRule {
    name: RuleName,
    head: TestAtom,
    atoms: Vec<TestAtom>,
    conditions: Vec<TestCond>,
}

impl Identifiable for TestRule {
    type Identifier = RuleName;

    fn id(&self) -> &RuleName {
        &self.name
    }
}

impl Rule for TestRule {
    type Atom = TestAtom;
    type Cond = TestCond;

    fn head(&self) -> &Self::Atom {
        &self.head
    }

    fn atoms(&self) -> impl Iterator<Item = &Self::Atom> {
        self.atoms.iter()
    }

    fn conditions(&self) -> impl Iterator<Item = &Self::Cond> {
        self.conditions.iter()
    }
}

#[derive(Debug)]
pub(super) struct TestAtom {
    name: PredicateName,
    positive: bool,
    bindings: Vec<(usize, Bind<TestTypedVar, TestLit>)>,
}

impl Identifiable for TestAtom {
    type Identifier = PredicateName;

    fn id(&self) -> &PredicateName {
        &self.name
    }
}

impl Atom for TestAtom {
    type Var = TestTypedVar;
    type Lit = TestLit;

    fn is_positive(&self) -> bool {
        self.positive
    }

    fn bindings(&self) -> impl Iterator<Item = (usize, Bind<&Self::Var, &Self::Lit>)> {
        self.bindings
            .iter()
            .map(|(position, bind)| (*position, borrow(bind)))
    }
}

#[derive(Debug)]
pub(super) struct TestTypedVar {
    name: VarName,
}

impl Identifiable for TestTypedVar {
    type Identifier = VarName;

    fn id(&self) -> &VarName {
        &self.name
    }
}

impl TypedVar for TestTypedVar {
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

/// A condition, as [`rule`] reads one off a body spec like `x < 3`.
#[derive(Debug)]
pub(super) struct TestCond {
    operator: Operator,
    left: Bind<TestTypedVar, TestLit>,
    right: Bind<TestTypedVar, TestLit>,
}

impl Cond for TestCond {
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
pub(super) fn declare(name: &str, columns: &[&str]) -> (PredicateName, Vec<Column>) {
    (
        name.into(),
        columns
            .iter()
            .map(|column| Column::new(*column, ScalarType::Uint))
            .collect(),
    )
}
