// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod analysis;
mod display;
mod graph_utils;
#[cfg(test)]
mod test_utils;
mod translation;
mod visualize;

pub use translation::Prepared;
// Unused until a caller outside this module renders an analysis.
#[allow(unused_imports)]
pub use visualize::Columns;

use crate::{
    frontend::{
        analysis::{Analysis, ExecutionOrder, Rejected, Verified, analyze},
        display::{ProgramDisplay, separated},
        translation::Translator,
    },
    host::{expr::Literal, operator::Operator},
    relational::schema::Column,
    scalarial::ScalarType,
};
use std::{collections::HashMap, fmt};

pub trait Identifier: Clone + fmt::Debug + fmt::Display + PartialEq + Eq + std::hash::Hash {}

pub trait Identifiable {
    type Identifier: Identifier;

    fn id(&self) -> &Self::Identifier;
}

/// The identifier of something [`Identifiable`]. Named because the path is
/// unreadable wherever the identifiable type is itself an associated type.
pub type IdOf<T> = <T as Identifiable>::Identifier;

/// What a frontend plugs into this layer's Datalog: the types of its names,
/// its variables, and its literals. [`Predicate`], [`Rule`], [`Atom`], and
/// [`Cond`] are generic over it, so that every frontend shares one
/// representation, which a rewrite can build and change, while the leaves stay
/// the frontend's own.
///
/// [`Clone`] and [`fmt::Debug`] so that the types generic over it can derive
/// both. An implementor is a marker type deriving them.
pub trait Language: Clone + fmt::Debug {
    /// The identifier naming a predicate, spelled as the rules referencing it
    /// spell it: an [`Atom`]'s identifier _is_ a predicate reference, which
    /// makes the rule's atoms the origin of this space rather than the
    /// predicates themselves.
    type PredId: Identifier;
    /// The identifier naming a rule itself. A frontend keeping rule names apart
    /// from predicate names instantiates the two differently; plain Datalog may
    /// pass the same type for both.
    type RuleId: Identifier;
    type Var: TypedVar + Clone;
    type Lit: Lit + Clone;
    /// What a frontend attaches to each of its rules, such as where in its
    /// source a rule stems from. `()` for nothing.
    type RuleMeta: fmt::Debug + Clone;
}

pub trait LogicalProgram {
    type Lang: Language;

    /// All predicates of the program, each exactly once and in no particular
    /// order. Together they form the IDB.
    fn predicates(&self) -> impl Iterator<Item = &Predicate<Self::Lang>>;

    /// If the program exposes `predicate`'s relation as an output. One that is
    /// not is either dead code or computed for the predicates reading it,
    /// but reports nowhere, like a helper predicate a frontend may introduce.
    ///
    /// Asked of the program rather than of the predicate, because which
    /// relations a program exposes is a property of the program as a whole.
    /// By default, every IDB predicate is an output: the EDB is the program's
    /// input.
    fn is_output(&self, predicate: &Predicate<Self::Lang>) -> bool {
        predicate.is_idb_predicate()
    }

    /// Builds the program's graphs, checking nothing, so that even an invalid
    /// program's can be inspected. See [`Analysis`].
    fn analyze(&self) -> Analysis<'_, Self::Lang>
    where
        Self: Sized,
    {
        analyze(self)
    }

    /// Builds the program's graphs and checks the program, which is what
    /// [`prepare`](Self::prepare) requires. Shorthand for
    /// `self.analyze().verify()`.
    fn verify(&self) -> Result<Analysis<'_, Self::Lang, Verified>, Rejected<'_, Self::Lang>>
    where
        Self: Sized,
    {
        self.analyze().verify()
    }

    /// Translates the program, which only a [`Verified`] analysis permits, so
    /// the [`Prepared`] code it returns is known to stem from a valid program.
    /// Consumes the `analysis`, so its graphs only live until the program is
    /// translated.
    ///
    /// Infallible, as verification rejects everything translation could not
    /// handle.
    fn prepare<'a>(&'a self, analysis: Analysis<'a, Self::Lang, Verified>) -> Prepared {
        let outputs = self
            .predicates()
            .filter(|predicate| self.is_output(predicate))
            .map(Identifiable::id)
            .collect();
        Translator::new(outputs).run(analysis.execution_order())
    }

    /// The program in Datalog notation. See [`display`] for the notation.
    fn display(&self) -> ProgramDisplay<'_, Self>
    where
        Self: Sized,
    {
        ProgramDisplay(self)
    }
}

#[derive(Debug)]
pub struct DatalogProgram<L: Language> {
    predicates: Vec<Predicate<L>>,
}

impl<L: Language> DatalogProgram<L> {
    pub fn new(predicates: Vec<Predicate<L>>) -> Self {
        Self { predicates }
    }
}

impl<L: Language> LogicalProgram for DatalogProgram<L> {
    type Lang = L;

    fn predicates(&self) -> impl Iterator<Item = &Predicate<L>> {
        self.predicates.iter()
    }
}

/// A predicate is either defined by one or multiple rules (part of the IDB),
/// or it is given externally (part of the EDB).
///
/// A predicate is named in the space its rules' atoms reference it by, which is
/// what lets a rule reference a predicate at all: [`is_self_recursive`] hands
/// this predicate's own [`id`](Identifiable::id) to
/// [`references`](Self::references), and a
/// [`PredicateComponent`](analysis::PredicateComponent) hands it its members'.
/// Note that this is [`Language::PredId`] and not [`Language::RuleId`]: a
/// rule's own name is a space of its own. Its name and its rules' definands
/// hence share one space by construction.
///
/// [`is_self_recursive`]: Self::is_self_recursive
#[derive(Debug)]
pub struct Predicate<L: Language> {
    name: L::PredId,
    columns: Vec<Column>,
    rules: Vec<Rule<L>>,
}

impl<L: Language> Predicate<L> {
    /// Slots `rules` into the predicates `declarations` declares, each rule
    /// into the one its [`definand`](Rule::definand) names.
    ///
    /// The definand is read off the rule's head, never off the rule's own name.
    /// FLIR keeps the two apart, for provenance; plain Datalog does not and
    /// names the rule after its head. Either way, grouping does not care.
    ///
    /// The declarations define which predicates exist, in which order, and with
    /// which columns; a rule only says which one it contributes to. That name
    /// has to be the one _body_ atoms use to reference the predicate.
    ///
    /// The predicates come back in declaration order with their rules in
    /// arrival order, so one input always yields the same program. A declared
    /// predicate with no rules comes back empty rather than being omitted and
    /// turns into an EDB predicate.
    ///
    /// A predicate name declared more than once comes back once per declaration
    /// but all its rules are slotted into the first, rather than the duplicate
    /// declarations silently merging. [Verification](Analysis::verify) rejects
    /// such a program later in the pipeline.
    ///
    /// Fails if any rule's definand matches no declaration. All offenders are
    /// collected, not just the first.
    pub fn group(
        declarations: impl IntoIterator<Item = (L::PredId, Vec<Column>)>,
        rules: impl IntoIterator<Item = Rule<L>>,
    ) -> Result<Vec<Self>, UnmatchedRules<L>> {
        let mut predicates: Vec<Self> = declarations
            .into_iter()
            .map(|(name, columns)| Self {
                name,
                columns,
                rules: Vec::new(),
            })
            .collect();
        // Keyed by name for slotting rules in. Only a name's first declaration
        // is a slot, which is also the one verification keeps.
        let mut slots: HashMap<L::PredId, usize> = HashMap::new();
        for (idx, predicate) in predicates.iter().enumerate() {
            slots.entry(predicate.name.clone()).or_insert(idx);
        }
        let mut unmatched: Vec<Rule<L>> = Vec::new();
        for rule in rules {
            match slots.get(rule.definand()) {
                Some(&idx) => predicates[idx].rules.push(rule),
                None => unmatched.push(rule),
            }
        }
        match unmatched.is_empty() {
            true => Ok(predicates),
            false => Err(UnmatchedRules { rules: unmatched }),
        }
    }

    /// All rules that contribute to the definition of this predicate.
    ///
    /// Whether a rule is recursive is deliberately not asked here: it is a
    /// property of the [`PredicateComponent`](analysis::PredicateComponent) a
    /// predicate ends up in, not of the predicate itself. See
    /// [`PredicateComponent::rec_rules`](analysis::PredicateComponent::rec_rules).
    pub fn rules(&self) -> impl Iterator<Item = &Rule<L>> {
        self.rules.iter()
    }

    /// If any rule references the `identifier`, which names a predicate and is
    /// hence not the space a rule's own [`id`](Identifiable::id) lives in.
    pub fn references(&self, identifier: &L::PredId) -> bool {
        self.rules().any(|rule| rule.references(identifier))
    }

    /// The columns this predicate's relation exposes, in order. Every rule's
    /// head fills exactly these, positionally.
    ///
    /// Declared rather than derived: a predicate defined by several rules
    /// becomes a union of their projections, and union branches have to agree
    /// on column names, while variable names are rule-local. Deriving names
    /// from one rule's head would silently impose that rule's naming on all the
    /// others.
    pub fn columns(&self) -> impl Iterator<Item = &Column> {
        self.columns.iter()
    }

    /// The number of [`columns`](Self::columns), which bounds the positions
    /// an atom referencing this predicate may bind.
    pub fn arity(&self) -> usize {
        self.columns.len()
    }

    /// A predicate is a predicate of the EDB (base table, externally given)
    /// if it does contain only a single rule with an empty body: no atoms, and
    /// no conditions either. That rule declares the relation rather than
    /// deriving it, so its head binds nothing.
    pub fn is_edb_predicate(&self) -> bool {
        let mut iter = self.rules();
        let first_is_edb = iter.next().is_some_and(|rule| rule.body().next().is_none());
        let is_only = iter.next().is_none();
        is_only && first_is_edb
    }

    /// A predicate is a predicate of the IDB (derived view, defined by rules)
    /// if it does contain some rules.
    pub fn is_idb_predicate(&self) -> bool {
        !self.is_edb_predicate()
    }

    /// If the predicate is self-recursive, that is, referencing itself in some
    /// of its rules. Unlike mutual recursion, this is visible from the
    /// predicate alone and needs no
    /// [`PredicateComponent`](analysis::PredicateComponent) computation.
    pub fn is_self_recursive(&self) -> bool {
        self.references(self.id())
    }
}

/// The rules [`Predicate::group`] could not place.
#[derive(Debug)]
pub struct UnmatchedRules<L: Language> {
    rules: Vec<Rule<L>>,
}

impl<L: Language> fmt::Display for UnmatchedRules<L> {
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

impl<L: Language> Identifiable for Predicate<L> {
    type Identifier = L::PredId;

    fn id(&self) -> &Self::Identifier {
        &self.name
    }
}

/// A proposition is either an atom or a condition, the two things a
/// [`Rule`]'s body is a conjunction of.
#[derive(Debug, Clone, Copy)]
pub enum Proposition<A, C> {
    Atom(A),
    Cond(C),
}

impl<A, C> Proposition<A, C> {
    pub fn as_ref(&self) -> Proposition<&A, &C> {
        use Proposition::*;
        match self {
            Atom(atom) => Atom(atom),
            Cond(cond) => Cond(cond),
        }
    }

    /// The atom, if this proposition is one.
    pub fn atom(self) -> Option<A> {
        match self {
            Proposition::Atom(atom) => Some(atom),
            Proposition::Cond(_) => None,
        }
    }

    /// The condition, if this proposition is one.
    pub fn cond(self) -> Option<C> {
        match self {
            Proposition::Atom(_) => None,
            Proposition::Cond(cond) => Some(cond),
        }
    }
}

/// A rule consists of a _body_ and a _head_. The body is a conjunction of
/// [propositions](Proposition), that is, [atoms](Self::atoms) and
/// [conditions](Self::conditions), in the order the frontend gave them.
/// The [head](Self::head) is a lone atom.
///
/// [`Identifiable::id`] names the _rule_, in a space of its own: the predicates
/// a rule references are named by its [`atoms`](Self::atoms), which is
/// [`Language::PredId`] and deliberately unrelated to [`Language::RuleId`].
/// Nothing but diagnostics and [IR variable names] reads a rule's own name.
///
/// [IR variable names]: super::frontend::translation
#[derive(Debug, Clone)]
pub struct Rule<L: Language> {
    pub(crate) id: L::RuleId,
    pub(crate) head: Atom<L>,
    pub(crate) body: Vec<Proposition<Atom<L>, Cond<L>>>,
    pub(crate) meta: L::RuleMeta,
}

impl<L: Language> Rule<L> {
    /// The atom this rule derives, its _definand_. Its
    /// [`bindings`](Atom::bindings) say which term fills each of the derived
    /// predicate's [`columns`](Predicate::columns), and its identifier is
    /// what [`Predicate::group`] slots the rule by. Note that
    /// [`Identifiable::id`] names the _rule_, not the predicate it derives.
    pub fn head(&self) -> &Atom<L> {
        &self.head
    }

    pub fn definand(&self) -> &L::PredId {
        self.head().id()
    }

    /// The propositions of the rule's _body_, atoms and conditions alike, in
    /// their given order.
    pub fn body(&self) -> impl Iterator<Item = Proposition<&Atom<L>, &Cond<L>>> {
        self.body.iter().map(Proposition::as_ref)
    }

    /// The atoms of the rule's _body_. The head is _not_ among them, as
    /// otherwise, every rule would be falsely classified as self-recursive.
    pub fn atoms(&self) -> impl Iterator<Item = &Atom<L>> {
        self.body().filter_map(Proposition::atom)
    }

    /// The atoms of the rule's _body_ which are _not_ negated.
    pub fn positive_atoms(&self) -> impl Iterator<Item = &Atom<L>> {
        self.atoms().filter(|atom| atom.is_positive())
    }

    /// The atoms of the rule's _body_ which are negated.
    pub fn negated_atoms(&self) -> impl Iterator<Item = &Atom<L>> {
        self.atoms().filter(|atom| atom.is_negated())
    }

    /// The conditions of the rule's body. Conditions constrain a variable's
    /// domain.
    pub fn conditions(&self) -> impl Iterator<Item = &Cond<L>> {
        self.body().filter_map(Proposition::cond)
    }

    /// If the rule references the predicate `identifier` names. Takes a
    /// predicate name, not a rule name, which is why it is
    /// [`Language::PredId`] rather than [`Language::RuleId`].
    pub fn references(&self, identifier: &L::PredId) -> bool {
        self.atoms().find(|atom| atom.id() == identifier).is_some()
    }

    /// What the frontend attached to this rule.
    pub fn meta(&self) -> &L::RuleMeta {
        &self.meta
    }
}

impl<L: Language> Identifiable for Rule<L> {
    type Identifier = L::RuleId;

    fn id(&self) -> &Self::Identifier {
        &self.id
    }
}

/// A condition `left operator right` in a [`Rule`]'s body.
#[derive(Debug, Clone)]
pub struct Cond<L: Language> {
    pub(crate) operator: Operator,
    pub(crate) left: Bind<L::Var, L::Lit>,
    pub(crate) right: Bind<L::Var, L::Lit>,
}

impl<L: Language> Cond<L> {
    pub fn operator(&self) -> Operator {
        self.operator
    }

    pub fn left(&self) -> Bind<&L::Var, &L::Lit> {
        self.left.as_ref()
    }

    pub fn right(&self) -> Bind<&L::Var, &L::Lit> {
        self.right.as_ref()
    }

    /// The variables this condition constrains. Unlike an [`Atom`]'s, they are
    /// not brought into scope but have to be bound by a positive atom.
    pub fn vars(&self) -> impl Iterator<Item = &L::Var> {
        [self.left(), self.right()]
            .into_iter()
            .filter_map(|bind| match bind {
                Bind::Var(var) => Some(var),
                Bind::Lit(_) => None,
            })
    }
}

/// An atom `name(term, …)` referencing the predicate `name`, possibly negated.
#[derive(Debug, Clone)]
pub struct Atom<L: Language> {
    pub(crate) name: L::PredId,
    pub(crate) negated: bool,
    /// See [`bindings`](Self::bindings).
    #[allow(clippy::type_complexity)]
    pub(crate) bindings: Vec<(usize, Bind<L::Var, L::Lit>)>,
}

impl<L: Language> Atom<L> {
    pub fn is_positive(&self) -> bool {
        !self.negated
    }

    pub fn is_negated(&self) -> bool {
        self.negated
    }

    /// What's in the parenthesis, as `(column position, term)` pairs.
    ///
    /// Sparse: a position the atom does not mention is simply absent. It binds
    /// no variable, so it constrains no join and reaches no projection, which
    /// is what keeps an atom over a wide relation cheap and spares Datalog's
    /// `_` any representation at all.
    ///
    /// Each position is below the referenced predicate's
    /// [`arity`](Predicate::arity) and occurs at most once.
    ///
    /// A rule's [`head`](Rule::head) is the exception and has to be dense,
    /// covering every one of its predicate's [`columns`](Predicate::columns)
    /// exactly once: a column no head fills has nothing to project into it.
    ///
    /// [Verification](Analysis::verify) checks all of this.
    pub fn bindings(&self) -> impl Iterator<Item = (usize, Bind<&L::Var, &L::Lit>)> {
        self.bindings
            .iter()
            .map(|(position, bind)| (*position, bind.as_ref()))
    }

    /// All variables brought into scope by this [`Atom`].
    pub fn vars(&self) -> impl Iterator<Item = &L::Var> {
        self.bindings().filter_map(|(_, bind)| match bind {
            Bind::Var(var) => Some(var),
            Bind::Lit(_) => None,
        })
    }
}

impl<L: Language> Identifiable for Atom<L> {
    type Identifier = L::PredId;

    fn id(&self) -> &Self::Identifier {
        &self.name
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
        let predicates = Predicate::group(
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
        let predicates = Predicate::group(
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
        let predicates = Predicate::group(
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
    fn a_name_declared_twice_comes_back_twice() {
        // The first declaration takes the rules and the second stays empty,
        // rather than the two silently merging, so that verification can
        // reject the program.
        let predicates = Predicate::group(
            declarations(&["edge", "path", "path"]),
            vec![
                rule("e0", "edge(x)", &[]),
                rule("r0", "path(x)", &["edge(x)"]),
            ],
        )
        .expect("every definand names a declared predicate");
        assert_eq!(
            grouping_of(&predicates),
            vec![("edge", vec!["e0"]), ("path", vec!["r0"]), ("path", vec![])]
        );
        let program = program(predicates);
        let rejected = program.verify().expect_err("`path` is declared twice");
        assert_eq!(
            rejected.error.message(),
            "predicate 'path' is declared more than once"
        );
    }

    #[test]
    fn a_declared_predicate_without_rules_stays_empty() {
        let predicates = Predicate::group(
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
        let error = Predicate::group(
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
        let nothing = Vec::<Rule<TestLanguage>>::new();
        let predicates = Predicate::group(declarations(&[]), nothing).expect("nothing to mismatch");
        assert!(predicates.is_empty());
    }
}
