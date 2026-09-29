// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod analysis;
mod graph_utils;
#[cfg(test)]
mod test_utils;
mod translation;

use crate::{
    error::SyntaxError,
    frontend::{
        analysis::{ExecutionOrder, static_analysis_pipeline},
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

trait LogicalProgram {
    type Predicate: Predicate;

    /// All predicates of the program, each exactly once and in no particular
    /// order. Together they form the IDB.
    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate>;

    fn verify<'a>(&'a self) -> Result<ExecutionOrder<'a, Self::Predicate>, SyntaxError>
    where
        Self: Sized,
    {
        static_analysis_pipeline(self).map_err(|e| SyntaxError::new(e.to_string()))
    }

    fn prepare<'a>(
        &'a self,
        exec_order: ExecutionOrder<'a, Self::Predicate>,
    ) -> Result<QueryIr, SyntaxError> {
        Translator::new().run(&exec_order)
    }
}

trait AggregateRules {
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
trait Predicate: Identifiable<Identifier = PredIdOf<Self::Rule>> + AggregateRules {
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
    /// if it does not contain any rule.
    fn is_edb_predicate(&self) -> bool {
        self.rules().next().is_none()
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
}

#[derive(Debug)]
pub struct DatalogProgram<I, R> {
    predicates: Vec<RulePredicate<I, R>>,
}

impl<I, R> DatalogProgram<I, R> {
    pub fn new(predicates: Vec<RulePredicate<I, R>>) -> Self {
        Self { predicates }
    }
}

impl<I: Identifier, R: Rule<Atom: Atom<Identifier = I>>> LogicalProgram for DatalogProgram<I, R> {
    type Predicate = RulePredicate<I, R>;

    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate> {
        self.predicates.iter()
    }
}

/// A [`Predicate`] assembled from the rules defining it.
#[derive(Debug)]
pub struct RulePredicate<I, R> {
    name: I,
    columns: Vec<Column>,
    rules: Vec<R>,
}

impl<I: Identifier, R: Rule> RulePredicate<I, R> {
    /// Slots `rules` into the predicates `declarations` declares, each rule
    /// paired with the definand naming the predicate it derives.
    ///
    /// The definand is always explicit, whether or not a frontend keeps it
    /// distinct from the rule's own name. FLIR does, for provenance; plain
    /// Datalog does not and passes the rule's name for both. Asking for it
    /// outright spares the layer having to tell those conventions apart, and
    /// spares a frontend from having to fit either.
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
    fn group(
        declarations: impl IntoIterator<Item = (I, Vec<Column>)>,
        rules: impl IntoIterator<Item = (I, R)>,
    ) -> Result<Vec<Self>, UnmatchedRules<I, R>> {
        // Keyed by name for slotting rules in, ordered by declaration for
        // handing the predicates back.
        let mut predicates: IndexMap<I, Self> = declarations
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
        let mut unmatched: Vec<(I, R)> = Vec::new();
        for (definand, rule) in rules {
            match predicates.get_mut(&definand) {
                Some(predicate) => predicate.rules.push(rule),
                None => unmatched.push((definand, rule)),
            }
        }
        match unmatched.is_empty() {
            true => Ok(predicates.into_values().collect()),
            false => Err(UnmatchedRules { rules: unmatched }),
        }
    }
}

/// The rules [`RulePredicate::group`] could not place, each with the
/// definand that named no declared predicate.
#[derive(Debug)]
struct UnmatchedRules<I, R> {
    rules: Vec<(I, R)>,
}

impl<I: Identifier, R: Rule> fmt::Display for UnmatchedRules<I, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.rules
            .iter()
            .enumerate()
            .try_for_each(|(idx, (name, rule))| {
                let separator = if idx == 0 { "" } else { "; " };
                write!(
                    f,
                    "{separator}rule '{}' derives '{}', which no declaration names",
                    rule.id(),
                    name,
                )
            })
    }
}

impl<I: Identifier, R> Identifiable for RulePredicate<I, R> {
    type Identifier = I;

    fn id(&self) -> &I {
        &self.name
    }
}

impl<I: Identifier, R: Rule> AggregateRules for RulePredicate<I, R> {
    type Rule = R;

    fn rules(&self) -> impl Iterator<Item = &Self::Rule> {
        self.rules.iter()
    }
}

impl<I: Identifier, R: Rule<Atom: Atom<Identifier = I>>> Predicate for RulePredicate<I, R> {
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
    /// predicate's [`columns`](Predicate::columns).
    ///
    /// Its identifier plays no part in grouping: [`RulePredicate::group`] takes
    /// the definand's name as an argument of its own, so a frontend keeping
    /// a rule's name and its definand apart need not encode one in the
    /// other. Note that [`Identifiable::id`] names the _rule_.
    fn head(&self) -> &Self::Atom;

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
}

pub trait Cond: fmt::Debug {
    type Var: TypedVar;
    type Lit: Lit;

    fn operator(&self) -> impl Into<Operator>;
    fn left(&self) -> Bind<&Self::Var, &Self::Lit>;
    fn right(&self) -> Bind<&Self::Var, &Self::Lit>;
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
}

/// Binds something to either a variable or a literal value.
#[derive(Debug)]
pub enum Bind<Var, Lit> {
    Var(Var),
    Lit(Lit),
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
                derives("path", "r0", &["edge"]),
                derives("reachable", "r1", &["path"]),
                derives("path", "r2", &["path", "edge"]),
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
        // passes the predicate's name for both, so the rules of one predicate
        // are indistinguishable by name. Grouping does not care either way.
        let predicates = RulePredicate::group(
            declarations(&["path"]),
            vec![
                derives("path", "path", &["edge"]),
                derives("path", "path", &["path", "edge"]),
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
                derives("reachable", "r0", &["path"]),
                derives("path", "r1", &["edge"]),
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
            vec![derives("path", "r0", &["edge"])],
        )
        .expect("every definand names a declared predicate");
        assert_eq!(
            grouping_of(&predicates),
            vec![("path", vec!["r0"]), ("unused", vec![])]
        );
    }

    #[test]
    fn the_definand_decides_where_a_rule_lands_not_its_head() {
        // Both rules head an atom named after neither the predicate nor
        // themselves, so only the definand can be placing them.
        let predicates = RulePredicate::group(
            declarations(&["path"]),
            vec![
                ("path".into(), rule("r0", "base_case", &["edge"])),
                (
                    "path".into(),
                    rule("r1", "transitive_closure", &["path", "edge"]),
                ),
            ],
        )
        .expect("both definands name a declared predicate");
        assert_eq!(grouping_of(&predicates), vec![("path", vec!["r0", "r1"])]);
    }

    #[test]
    fn a_rule_deriving_no_declared_predicate_is_rejected() {
        // Both offenders are reported, not just the first, and each is named
        // by its definand. `r2`'s head says `path`, which *is* declared, so
        // reporting the head would point at the wrong thing.
        let error = RulePredicate::group(
            declarations(&["path"]),
            vec![
                derives("path", "r0", &["edge"]),
                derives("pathh", "r1", &["edge"]),
                ("paths".into(), rule("r2", "path", &["path", "edge"])),
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
        let nothing = Vec::<(PredicateName, TestRule)>::new();
        let predicates =
            RulePredicate::group(declarations(&[]), nothing).expect("nothing to mismatch");
        assert!(predicates.is_empty());
    }
}
