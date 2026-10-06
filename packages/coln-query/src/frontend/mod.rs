// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod analysis;
mod display;
mod graph_utils;
#[cfg(test)]
mod test_utils;
mod translation;

use crate::{
    error::SyntaxError,
    frontend::{
        analysis::{ExecutionOrder, static_analysis_pipeline},
        display::{
            AtomDisplay, CondDisplay, PredicateDisplay, ProgramDisplay, RuleDisplay, separated,
        },
        translation::Translator,
    },
    host::{QueryIr, expr::Literal, operator::Operator},
    relational::schema::Column,
    scalarial::ScalarType,
};
use indexmap::IndexMap;
use std::fmt;

pub trait Identifier: Clone + fmt::Debug + fmt::Display + PartialEq + Eq + std::hash::Hash {}

pub trait Identifiable {
    type Identifier: Identifier;

    fn id(&self) -> &Self::Identifier;
}

/// The identifier of something [`Identifiable`]. Named because the path is
/// unreadable wherever the identifiable type is itself an associated type.
pub type IdOf<T> = <T as Identifiable>::Identifier;

/// The identifier naming a predicate, spelled as the rules referencing it spell
/// it: an [`Atom`]'s identifier _is_ a predicate reference, which makes the
/// rule's atoms the origin of this space rather than the predicates themselves.
///
/// Distinct from `IdOf<R>`, which names the rule itself. A frontend keeping
/// rule names apart from predicate names instantiates the two differently;
/// plain Datalog may pass the same type for both.
pub type PredIdOf<R> = IdOf<<R as Rule>::Atom>;

pub trait LogicalProgram {
    type Predicate: Predicate;

    /// All predicates of the program, each exactly once and in no particular
    /// order. Together they form the IDB.
    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate>;

    fn verify<'a>(&'a self) -> Result<ExecutionOrder<'a, Self::Predicate>, SyntaxError>
    where
        Self: Sized,
    {
        // println!("{}", self.display());
        println!("{:#}", self.display());
        static_analysis_pipeline(self).map_err(|e| SyntaxError::new(e.to_string()))
    }

    fn prepare<'a>(
        &'a self,
        exec_order: ExecutionOrder<'a, Self::Predicate>,
    ) -> Result<QueryIr, SyntaxError> {
        Translator::new().run(&exec_order)
    }

    /// The program in Datalog notation. See [`display`] for the notation.
    fn display(&self) -> ProgramDisplay<'_, Self>
    where
        Self: Sized,
    {
        ProgramDisplay(self)
    }
}

pub trait AggregateRules {
    type Rule: Rule;

    /// All rules that contribute to the definition of this [`Predicate`], or,
    /// for a [`Component`], of all of its members.
    ///
    /// Whether a rule is recursive is deliberately not asked here: it is a
    /// property of the [`Component`] a predicate ends up in, not of the predicate
    /// itself. See [`Component::rec_rules`].
    fn rules(&self) -> impl Iterator<Item = &Self::Rule>;

    /// If any rule references the `identifier`, which names a predicate and is
    /// hence not the space a rule's own [`id`](Identifiable::id) lives in.
    fn references(&self, identifier: &PredIdOf<Self::Rule>) -> bool {
        self.rules().any(|rule| rule.references(identifier))
    }
}

/// A component contains one or multiple [`Predicate`]s. In the latter case,
/// the predicates reference each other (mutual recursion) and therefore form
/// a component.
trait Component: AggregateRules {
    type Predicate: Predicate<Rule = Self::Rule>;

    fn members(&self) -> impl Iterator<Item = &Self::Predicate>;

    /// Returns `Some(Predicate)` if the component contains an EDB predicate.
    /// Otherwise, `None` is returned. A component representing an EDB predicate
    /// must only contain a single [predicate](Self::members) for which
    /// [Predicate::is_edb_predicate()] is `true`. If these conditions are met,
    /// this function returns exactly that predicate.
    fn is_edb_component(&self) -> Option<&Self::Predicate> {
        let mut iter = self.members();
        if let Some(predicate) = iter.next()
            && predicate.is_edb_predicate()
            && iter.next().is_none()
        {
            Some(predicate)
        } else {
            None
        }
    }

    /// The rules referencing a member of this component. Those are what make
    /// the component recursive, whether a rule references the very predicate it
    /// defines (self recursion) or another member (mutual recursion).
    ///
    /// [Self::rec_rules] and [Self::non_rec_rules] partition all of
    /// the [`Component`]s rules into two partitions.
    fn rec_rules(&self) -> impl Iterator<Item = &Self::Rule> {
        self.rules().filter(|rule| self.references_member(rule))
    }

    /// The rules referencing no member of this component. They only read
    /// predicates of earlier components and base tables from the EDB, all of
    /// which are fully computed by the time this component runs.
    ///
    /// [Self::non_rec_rules] and [Self::rec_rules] partition all of
    /// the [`Component`]s rules into two partitions.
    fn non_rec_rules(&self) -> impl Iterator<Item = &Self::Rule> {
        self.rules().filter(|rule| !self.references_member(rule))
    }

    /// If the component has to be evaluated recursively at all. True for every
    /// component of more than one member, and for a single member that is
    /// self-recursive.
    fn is_recursive(&self) -> bool {
        self.rec_rules().next().is_some()
    }

    fn contains_self_recursion(&self) -> bool {
        self.members()
            .any(|predicate| predicate.is_self_recursive())
    }

    fn references_member(&self, rule: &Self::Rule) -> bool {
        self.members().any(|member| rule.references(member.id()))
    }
}

/// A predicate is either defined by one or multiple rules (part of the IDB),
/// or it is given externally (part of the EDB).
///
/// A predicate is named in the space its rules' atoms reference it by, which is
/// what lets a rule reference a predicate at all: [`is_self_recursive`] hands
/// this predicate's own [`id`](Identifiable::id) to
/// [`AggregateRules::references`], and a [`Component`] hands it its members'.
/// Note that this is [`PredIdOf`] and not `IdOf<Self::Rule>`: a rule's own name
/// is a space of its own.
///
/// [`is_self_recursive`]: Self::is_self_recursive
pub trait Predicate: Identifiable<Identifier = PredIdOf<Self::Rule>> + AggregateRules {
    /// The columns this predicate's relation exposes, in order. Every rule's
    /// head fills exactly these, positionally.
    ///
    /// Declared rather than derived: a predicate defined by several rules
    /// becomes a union of their projections, and union branches have to agree
    /// on column names, while variable names are rule-local. Deriving names
    /// from one rule's head would silently impose that rule's naming on all the
    /// others.
    fn columns(&self) -> impl Iterator<Item = &Column>;

    /// A predicate is a predicate of the EDB (base table, externally given)
    /// if it does contain only a single rule with no atoms in its body.
    fn is_edb_predicate(&self) -> bool {
        let mut iter = self.rules();
        let first_is_edb = iter
            .next()
            .is_some_and(|rule| rule.atoms().next().is_none());
        let is_only = iter.next().is_none();
        is_only && first_is_edb
    }

    /// A predicate is a predicate of the IDB (derived view, defined by rules)
    /// if it does contain some rules.
    fn is_idb_predicate(&self) -> bool {
        !self.is_edb_predicate()
    }

    /// If the predicate is self-recursive, that is, referencing itself in some
    /// of its rules. Unlike mutual recursion, this is visible from the
    /// predicate alone and needs no [`Component`] computation.
    fn is_self_recursive(&self) -> bool {
        self.references(self.id())
    }

    /// The predicate in Datalog notation. See [`display`] for the notation.
    fn display(&self) -> PredicateDisplay<'_, Self>
    where
        Self: Sized,
    {
        PredicateDisplay(self)
    }
}

#[derive(Debug)]
pub struct DatalogProgram<R: Rule> {
    predicates: Vec<RulePredicate<R>>,
}

impl<R: Rule> DatalogProgram<R> {
    pub fn new(predicates: Vec<RulePredicate<R>>) -> Self {
        Self { predicates }
    }
}

impl<R: Rule> LogicalProgram for DatalogProgram<R> {
    type Predicate = RulePredicate<R>;

    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate> {
        self.predicates.iter()
    }
}

/// A [`Predicate`] assembled from the rules defining it.
///
/// Named in [`PredIdOf<R>`](PredIdOf) rather than a parameter of its own, so
/// that its name and its rules' definands share one space by construction.
#[derive(Debug)]
pub struct RulePredicate<R: Rule> {
    name: PredIdOf<R>,
    columns: Vec<Column>,
    rules: Vec<R>,
}

impl<R: Rule> RulePredicate<R> {
    /// Slots `rules` into the predicates `declarations` declares, each rule
    /// into the one its [`definand`](Rule::definand) names.
    ///
    /// The definand is read off the rule's head, never off the rule's own
    /// name. FLIR keeps the two apart, for provenance; plain Datalog does not
    /// and names the rule after its head. Either way, grouping does not care.
    ///
    /// The declarations define which predicates exist, in which order, and with
    /// which columns; a rule only says which one it contributes to. That name
    /// has to be the one _body_ atoms use to reference the predicate.
    ///
    /// The predicates come back in declaration order with their rules in
    /// arrival order, so one input always yields the same program. A declared
    /// predicate with no rules comes back empty rather than being omitted.
    ///
    /// Fails if any rule's definand matches no declaration.
    /// All offenders are collected, not just the first.
    pub fn group(
        declarations: impl IntoIterator<Item = (PredIdOf<R>, Vec<Column>)>,
        rules: impl IntoIterator<Item = R>,
    ) -> Result<Vec<Self>, UnmatchedRules<R>> {
        // Keyed by name for slotting rules in, ordered by declaration for
        // handing the predicates back.
        let mut predicates: IndexMap<PredIdOf<R>, Self> = declarations
            .into_iter()
            .map(|(name, columns)| {
                let predicate = Self {
                    name: name.clone(),
                    columns,
                    rules: Vec::new(),
                };
                (name, predicate)
            })
            .collect();
        let mut unmatched: Vec<R> = Vec::new();
        for rule in rules {
            let definand = rule.definand();
            match predicates.get_mut(definand) {
                Some(predicate) => predicate.rules.push(rule),
                None => unmatched.push(rule),
            }
        }
        match unmatched.is_empty() {
            true => Ok(predicates.into_values().collect()),
            false => Err(UnmatchedRules { rules: unmatched }),
        }
    }
}

/// The rules [`RulePredicate::group`] could not place.
#[derive(Debug)]
pub struct UnmatchedRules<R> {
    rules: Vec<R>,
}

impl<R: Rule> fmt::Display for UnmatchedRules<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let messages = self.rules.iter().map(|rule| {
            fmt::from_fn(move |f| {
                write!(
                    f,
                    "rule '{}' derives '{}', which no declaration names",
                    rule.id(),
                    rule.definand()
                )
            })
        });
        separated(f, messages, "; ")
    }
}

impl<R: Rule> Identifiable for RulePredicate<R> {
    type Identifier = PredIdOf<R>;

    fn id(&self) -> &Self::Identifier {
        &self.name
    }
}

impl<R: Rule> AggregateRules for RulePredicate<R> {
    type Rule = R;

    fn rules(&self) -> impl Iterator<Item = &Self::Rule> {
        self.rules.iter()
    }
}

impl<R: Rule> Predicate for RulePredicate<R> {
    fn columns(&self) -> impl Iterator<Item = &Column> {
        self.columns.iter()
    }
}

/// A rule contains atoms and conditions, sometimes united under the umbrella
/// term _proposition_.
///
/// [`Identifiable::id`] names the _rule_, in a space of its own: the predicates
/// a rule references are named by its [`atoms`](Self::atoms), which is
/// [`PredIdOf<Self>`](PredIdOf) and deliberately unrelated to `Self::Identifier`.
/// Nothing but diagnostics and [IR variable names] reads a rule's own name.
///
/// [`fmt::Debug`] so that anything carrying rules around — [`UnmatchedRules`],
/// say — can be unwrapped and printed without a frontend having to be asked
/// for it a second time.
///
/// [IR variable names]: super::frontend::translation
pub trait Rule: Identifiable + fmt::Debug {
    type Atom: Atom;
    /// A condition constrains a variable some atom of the same rule binds, so
    /// the two have to be the very same type. Only [`Cond::Var`] is pinned:
    /// literals name nothing and are only ever read out as a [`Literal`], so a
    /// condition is free to carry them differently than an atom does.
    type Cond: Cond<Var = <Self::Atom as Atom>::Var>;

    /// The atom this rule derives, its _definand_. Its
    /// [`bindings`](Atom::bindings) say which term fills each of the derived
    /// predicate's [`columns`](Predicate::columns), and its identifier is
    /// what [`RulePredicate::group`] slots the rule by. Note that
    /// [`Identifiable::id`] names the _rule_, not the predicate it derives.
    fn head(&self) -> &Self::Atom;

    fn definand(&self) -> &PredIdOf<Self> {
        self.head().id()
    }

    /// The atoms of the rule's _body_. The head is _not_ among them, as
    /// otherwise, every rule would be falsely classified as self-recursive.
    fn atoms(&self) -> impl Iterator<Item = &Self::Atom>;

    /// The atoms of the rule's _body_ which are _not_ negated.
    fn positive_atoms(&self) -> impl Iterator<Item = &Self::Atom> {
        self.atoms().filter(|atom| atom.is_positive())
    }

    /// The atoms of the rule's _body_ which are negated.
    fn negative_atoms(&self) -> impl Iterator<Item = &Self::Atom> {
        self.atoms().filter(|atom| atom.is_negative())
    }

    /// The conditions of the rule's body. Conditions constrain a variable's
    /// domain.
    fn conditions(&self) -> impl Iterator<Item = &Self::Cond>;

    /// If the rule references the predicate `identifier` names. Takes a
    /// predicate name, not a rule name, which is why it is [`PredIdOf`] rather
    /// than `Self::Identifier`.
    fn references(&self, identifier: &PredIdOf<Self>) -> bool {
        self.atoms().find(|atom| atom.id() == identifier).is_some()
    }

    /// The rule in Datalog notation. See [`display`] for the notation.
    fn display(&self) -> RuleDisplay<'_, Self>
    where
        Self: Sized,
    {
        RuleDisplay(self)
    }
}

pub trait Cond: fmt::Debug {
    type Var: TypedVar;
    type Lit: Lit;

    fn operator(&self) -> impl Into<Operator>;
    fn left(&self) -> Bind<&Self::Var, &Self::Lit>;
    fn right(&self) -> Bind<&Self::Var, &Self::Lit>;

    /// The condition in Datalog notation. See [`display`] for the notation.
    fn display(&self) -> CondDisplay<'_, Self>
    where
        Self: Sized,
    {
        CondDisplay(self)
    }
}

pub trait Atom: Identifiable + fmt::Debug {
    type Var: TypedVar;
    type Lit: Lit;

    fn is_positive(&self) -> bool;
    fn is_negative(&self) -> bool {
        !self.is_positive()
    }

    /// What's in the parenthesis, as `(column position, term)` pairs.
    ///
    /// Sparse: a position the atom does not mention is simply absent. It binds
    /// no variable, so it constrains no join and reaches no projection, which
    /// is what keeps an atom over a wide relation cheap and spares Datalog's
    /// `_` any representation at all.
    ///
    /// A rule's [`head`](Rule::head) is the exception and has to be dense,
    /// covering every one of its predicate's [`columns`](Predicate::columns)
    /// exactly once: a column no head fills has nothing to project into it.
    fn bindings(&self) -> impl Iterator<Item = (usize, Bind<&Self::Var, &Self::Lit>)>;

    /// All variables brought into scope by this [`Atom`].
    fn vars(&self) -> impl Iterator<Item = &Self::Var> {
        self.bindings().filter_map(|(_, bind)| match bind {
            Bind::Var(var) => Some(var),
            Bind::Lit(_) => None,
        })
    }

    /// The atom in Datalog notation. See [`display`] for the notation.
    fn display(&self) -> AtomDisplay<'_, Self>
    where
        Self: Sized,
    {
        AtomDisplay(self)
    }
}

/// Binds something to either a variable or a literal value.
#[derive(Debug, Clone, Copy)]
pub enum Bind<Var, Lit> {
    Var(Var),
    Lit(Lit),
}

impl<Var, Lit> Bind<Var, Lit> {
    pub fn as_ref(&self) -> Bind<&Var, &Lit> {
        match self {
            Bind::Var(var) => Bind::Var(var),
            Bind::Lit(lit) => Bind::Lit(lit),
        }
    }
}

pub trait TypedVar: Identifiable + fmt::Debug {
    fn name(&self) -> &Self::Identifier {
        self.id()
    }

    fn ty(&self) -> ScalarType;
}

pub trait Lit: fmt::Debug {
    /// [`Atom::bindings`] and [`Cond`] hand out a `&Self`, while the IR wants an
    /// owned [`Literal`]. An implementor that has `impl From<&Self> for Literal`
    /// writes `self.into()` here.
    fn to_literal(&self) -> Literal;
}

#[cfg(test)]
mod tests {
    use super::{test_utils::*, *};

    #[test]
    fn rules_slot_into_the_predicate_their_definand_names() {
        // `r1` interleaves the two rules deriving `path`, so the grouping
        // cannot simply be a run-length split of the input.
        let predicates = RulePredicate::group(
            declarations(&["path", "reachable"]),
            vec![
                rule("r0", "path", &["edge"]),
                rule("r1", "reachable", &["path"]),
                rule("r2", "path", &["path", "edge"]),
            ],
        )
        .expect("every definand names a declared predicate");
        assert_eq!(
            grouping_of(&predicates),
            vec![("path", vec!["r0", "r2"]), ("reachable", vec!["r1"])]
        );
    }

    #[test]
    fn rules_may_carry_their_predicate_s_own_name() {
        // The Datalog convention: a frontend that does not name rules apart
        // names each after its head, so the rules of one predicate are
        // indistinguishable by name. Grouping does not care either way.
        let predicates = RulePredicate::group(
            declarations(&["path"]),
            vec![
                rule("path", "path", &["edge"]),
                rule("path", "path", &["path", "edge"]),
            ],
        )
        .expect("every definand names a declared predicate");
        assert_eq!(
            grouping_of(&predicates),
            vec![("path", vec!["path", "path"])]
        );
    }

    #[test]
    fn predicates_come_out_in_declaration_order() {
        // Declaration order decides, not the order the rules arrive in: `r0`
        // derives `reachable` but `path` was declared first.
        let predicates = RulePredicate::group(
            declarations(&["path", "reachable"]),
            vec![
                rule("r0", "reachable", &["path"]),
                rule("r1", "path", &["edge"]),
            ],
        )
        .expect("every definand names a declared predicate");
        assert_eq!(
            grouping_of(&predicates),
            vec![("path", vec!["r1"]), ("reachable", vec!["r0"])]
        );
    }

    #[test]
    fn a_declared_predicate_without_rules_stays_empty() {
        let predicates = RulePredicate::group(
            declarations(&["path", "unused"]),
            vec![rule("r0", "path", &["edge"])],
        )
        .expect("every definand names a declared predicate");
        assert_eq!(
            grouping_of(&predicates),
            vec![("path", vec!["r0"]), ("unused", vec![])]
        );
    }

    #[test]
    fn a_rule_deriving_no_declared_predicate_is_rejected() {
        // Both offenders are reported, not just the first, and each is named
        // by its definand.
        let error = RulePredicate::group(
            declarations(&["path"]),
            vec![
                rule("r0", "path", &["edge"]),
                rule("r1", "pathh", &["edge"]),
                rule("r2", "paths", &["path", "edge"]),
            ],
        )
        .expect_err("two definands name undeclared predicates");
        assert_eq!(
            error.to_string(),
            "rule 'r1' derives 'pathh', which no declaration names; \
             rule 'r2' derives 'paths', which no declaration names"
        );
    }

    #[test]
    fn grouping_nothing_yields_no_predicates() {
        let nothing = Vec::<TestRule>::new();
        let predicates =
            RulePredicate::group(declarations(&[]), nothing).expect("nothing to mismatch");
        assert!(predicates.is_empty());
    }
}
