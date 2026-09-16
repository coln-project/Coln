// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod gabow;

use crate::{
    host::{expr::Literal, operator::Operator},
    scalarial::ScalarType,
};
use std::{
    collections::{HashMap, HashSet},
    fmt,
};

trait Identifier: Clone + fmt::Debug + fmt::Display + PartialEq + Eq + std::hash::Hash {}

trait Identifiable<Identifier> {
    fn id(&self) -> &Identifier;
}

// TODO: On top of these traits, implement:
// - Conjunctive query translation

trait LogicalProgram {
    type Identifier: Identifier;
    type Predicate: Predicate<Identifier = Self::Identifier>;

    /// All predicates of the program, each exactly once and in no particular
    /// order. Together they form the IDB.
    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate>;

    /// If `identifier` names a base relation of the EDB, that is, something the
    /// program reads but does not define. The counterpart of
    /// [`Self::predicates`], which enumerates what the program _does_ define.
    ///
    /// A name may be both: a predicate shadows a base relation of the same
    /// name, see [`Self::cliques`].
    fn is_base_relation(&self, identifier: &Self::Identifier) -> bool;

    /// Every atom naming neither a predicate of this program nor a base
    /// relation of the EDB. Such a reference cannot be evaluated: nothing ever
    /// produces the relation it asks for.
    ///
    /// An empty result is the precondition of [`Self::cliques`] and of any
    /// translation built on it. An atom repeating a dangling name is reported
    /// once per occurrence, so every offending site can be pointed at.
    fn dangling_references(&self) -> Vec<DanglingReference<'_, Self::Predicate>> {
        let defined: HashSet<&Self::Identifier> =
            self.predicates().map(|predicate| predicate.id()).collect();
        // Re-borrowed so that the `move` closures below copy a reference
        // instead of fighting over the set itself.
        let defined = &defined;
        self.predicates()
            .flat_map(move |predicate| {
                predicate.rules().flat_map(move |rule| {
                    rule.atoms()
                        .map(|atom| atom.id())
                        .filter(move |identifier| {
                            !defined.contains(identifier) && !self.is_base_relation(identifier)
                        })
                        .map(move |identifier| DanglingReference {
                            predicate,
                            rule,
                            identifier,
                        })
                })
            })
            .collect()
    }

    /// Groups the predicates into [`Clique`]s, the strongly connected
    /// components of the reference graph, and orders those cliques such that
    /// each clique comes after everything it references. Evaluating the program
    /// in this order means every predicate a clique reads has been computed by
    /// the time the clique runs.
    ///
    /// A predicate that is not mutually recursive with any other ends up in a
    /// clique of its own, whether it is self-recursive or not.
    ///
    /// Materialised rather than lazy, for two reasons: the cliques are computed
    /// once instead of once per call, and they outlive the borrows handed out
    /// by [`Clique::members`], so callers can collect predicates out of them.
    ///
    /// Assumes the program has no [`Self::dangling_references`]: a name that is
    /// neither a predicate nor a base relation is silently taken for the
    /// latter. A predicate _shadowing_ a base relation of the same name, on the
    /// other hand, is intended and resolves to the predicate.
    fn cliques(&self) -> Vec<PredicateClique<'_, Self::Predicate>> {
        // A predicate's position in `predicates` is its node in the reference
        // graph. That correspondence never leaves this function, so no part of
        // the trait has to promise anything about predicate order.
        let predicates: Vec<&Self::Predicate> = self.predicates().collect();
        let indices: HashMap<&Self::Identifier, usize> = predicates
            .iter()
            .enumerate()
            .map(|(idx, predicate)| (predicate.id(), idx))
            .collect();
        // An atom naming a base relation of the EDB resolves to no index and
        // hence contributes no edge. Consulting the IDB alone is what lets a
        // predicate shadow a base relation of the same name.
        // Atoms referencing the same predicate more than once yield duplicate
        // edges, which the search walks over without any further ado.
        let adjacency: Vec<Vec<usize>> = predicates
            .iter()
            .map(|predicate| {
                predicate
                    .rules()
                    .flat_map(|rule| rule.atoms())
                    .filter_map(|atom| indices.get(atom.id()).copied())
                    .collect()
            })
            .collect();
        let sccs_rev_topo_order = gabow::sccs(&adjacency);
        sccs_rev_topo_order
            .into_iter()
            .map(|members| members.into_iter().map(|idx| predicates[idx]).collect())
            .collect::<Vec<PredicateClique<_>>>()
    }
}

trait AggregateRules {
    type Identifier: Identifier;
    type Rule: Rule<Identifier = Self::Identifier>;

    /// All rules that contribute to the definition of this [`Predicate`], or,
    /// for a [`Clique`], of all of its members.
    ///
    /// Whether a rule is recursive is deliberately not asked here: it is a
    /// property of the [`Clique`] a predicate ends up in, not of the predicate
    /// itself. See [`Clique::rec_rules`].
    fn rules(&self) -> impl Iterator<Item = &Self::Rule>;

    /// If any rule references the `identifier`.
    fn references(&self, identifier: &Self::Identifier) -> bool {
        self.rules().any(|rule| rule.references(identifier))
    }
}

/// A clique contains one or multiple [`Predicate`]s. In the latter case,
/// the predicates reference each other (mutual recursion) and therefore form
/// a clique.
trait Clique: AggregateRules {
    type Predicate: Predicate<Identifier = Self::Identifier>;

    fn members(&self) -> impl Iterator<Item = &Self::Predicate>;

    /// The rules referencing a member of this clique. Those are what make the
    /// clique recursive, whether a rule references the very predicate it
    /// defines (self recursion) or another member (mutual recursion).
    fn rec_rules(&self) -> impl Iterator<Item = &Self::Rule> {
        self.rules().filter(|rule| self.references_member(rule))
    }

    /// The rules referencing no member of this clique. They only read
    /// predicates of earlier cliques and base tables from the EDB, all of
    /// which are fully computed by the time this clique runs.
    fn non_rec_rules(&self) -> impl Iterator<Item = &Self::Rule> {
        self.rules().filter(|rule| !self.references_member(rule))
    }

    /// If the clique has to be evaluated recursively at all. True for every
    /// clique of more than one member, and for a single member that is
    /// self-recursive.
    fn is_recursive(&self) -> bool {
        self.rec_rules().next().is_some()
    }

    fn references_member(&self, rule: &Self::Rule) -> bool {
        self.members().any(|member| rule.references(member.id()))
    }
}

/// The [`Clique`] a [`LogicalProgram`] is cut into by [`LogicalProgram::cliques`],
/// borrowing its members from the program.
struct PredicateClique<'a, P> {
    members: Vec<&'a P>,
}

impl<'a, P> FromIterator<&'a P> for PredicateClique<'a, P> {
    fn from_iter<I: IntoIterator<Item = &'a P>>(members: I) -> Self {
        Self {
            members: members.into_iter().collect(),
        }
    }
}

impl<P: Predicate> AggregateRules for PredicateClique<'_, P> {
    type Identifier = P::Identifier;
    type Rule = P::Rule;

    fn rules(&self) -> impl Iterator<Item = &Self::Rule> {
        self.members
            .iter()
            .copied()
            .flat_map(|member| member.rules())
    }
}

impl<P: Predicate> Clique for PredicateClique<'_, P> {
    type Predicate = P;

    fn members(&self) -> impl Iterator<Item = &Self::Predicate> {
        self.members.iter().copied()
    }
}

/// A predicate may be defined through one or multiple rules.
trait Predicate: Identifiable<Self::Identifier> + AggregateRules {
    /// If the predicate is self-recursive, that is, referencing itself in some
    /// of its rules. Unlike mutual recursion, this is visible from the
    /// predicate alone and needs no [`Clique`] computation.
    fn is_self_recursive(&self) -> bool {
        self.references(self.id())
    }
}

/// A [`Predicate`] assembled from the rules defining it.
///
/// Unlike [`PredicateClique`], this _owns_ its rules: a program holding both a
/// flat rule list and predicates borrowing from it would be self-referential.
/// Hand the flat list to [`Self::group`] and keep the result instead.
struct RulePredicate<I, R> {
    name: I,
    rules: Vec<R>,
}

impl<I: Identifier, R: Rule<Identifier = I>> RulePredicate<I, R> {
    /// Groups `rules` into the predicates they define, each paired with an
    /// optional name for the predicate it contributes to.
    ///
    /// `None` joins the predicate the rule's [`Rule::head`] names, which is
    /// what plain Datalog wants: the definand is the head. `Some(name)` is
    /// for frontends that keep the two apart: FLIR names a rule and its
    /// definand distinctly.
    ///
    /// Either way the name must be the one _body_ atoms use to reference the
    /// predicate: [`LogicalProgram::cliques`] pairs an [`Atom`] with a
    /// predicate by matching identifiers: Hence, providing a name no body atom
    /// mentions yields a predicate nothing depends on, and the atoms meant to
    /// reach it become [`LogicalProgram::dangling_references`].
    ///
    /// Predicates come out in order of first definition and their rules in the
    /// order they arrived, so one input always yields the same program. No
    /// predicate is empty, and a rule ends up in exactly one of them.
    fn group(rules: impl IntoIterator<Item = (Option<I>, R)>) -> Vec<Self> {
        let mut predicates: Vec<Self> = Vec::new();
        let mut indices: HashMap<I, usize> = HashMap::new();
        for (name, rule) in rules {
            let name = name.unwrap_or_else(|| rule.head().id().clone());
            let idx = match indices.get(&name) {
                Some(&idx) => idx,
                None => {
                    indices.insert(name.clone(), predicates.len());
                    predicates.push(Self {
                        name,
                        rules: Vec::new(),
                    });
                    predicates.len() - 1
                }
            };
            predicates[idx].rules.push(rule);
        }
        predicates
    }
}

impl<I: Identifier, R> Identifiable<I> for RulePredicate<I, R> {
    fn id(&self) -> &I {
        &self.name
    }
}

impl<I: Identifier, R: Rule<Identifier = I>> AggregateRules for RulePredicate<I, R> {
    type Identifier = I;
    type Rule = R;

    fn rules(&self) -> impl Iterator<Item = &Self::Rule> {
        self.rules.iter()
    }
}

impl<I: Identifier, R: Rule<Identifier = I>> Predicate for RulePredicate<I, R> {}

/// A rule contains atoms and conditions, sometimes united under the umbrella
/// term _proposition_.
trait Rule: Identifiable<Self::Identifier> {
    type Identifier: Identifier;
    type Atom: Atom<Identifier = Self::Identifier>;
    type Cond: Cond;

    /// The atom this rule derives, which is its _definand_ if not specified
    /// otherwise in [`RulePredicate::group`].
    ///
    /// Note that [`Identifiable::id`] names the _rule_, which some frontends
    /// may want to keep distinct even between rules of one predicate.
    fn head(&self) -> &Self::Atom;

    /// The atoms of the rule's _body_. The head is _not_ among them, as
    /// otherwise, every rule would be falsely classified as self-recursive.
    fn atoms(&self) -> impl Iterator<Item = &Self::Atom>;

    /// The conditions of the rule's body. Conditions constrain a variable's
    /// domain.
    fn conditions(&self) -> impl Iterator<Item = &Self::Cond>;

    fn references(&self, identifier: &Self::Identifier) -> bool {
        self.atoms().find(|atom| atom.id() == identifier).is_some()
    }
}

trait Cond {
    type Identifier: Identifier;
    type Var: TypedVar<Identifier = Self::Identifier>;
    type Lit: Lit;

    fn operator(&self) -> impl Into<Operator>;
    fn left(&self) -> Bind<&Self::Var, &Self::Lit>;
    fn right(&self) -> Bind<&Self::Var, &Self::Lit>;
}

trait Atom: Identifiable<Self::Identifier> {
    type Identifier: Identifier;
    type Var: TypedVar<Identifier = Self::Identifier>;
    type Lit: Lit;

    /// What's in the parenthesis, in order of appearance.
    fn bindings(&self) -> impl Iterator<Item = Bind<&Self::Var, &Self::Lit>>;

    /// All variables brought into scope by this [`Atom`].
    fn vars(&self) -> impl Iterator<Item = &Self::Var>;
}

/// Binds something to either a variable or a literal value.
enum Bind<Var, Lit> {
    Var(Var),
    Lit(Lit),
}

trait TypedVar: Identifiable<Self::Identifier> {
    type Identifier: Identifier;

    fn name(&self) -> &Self::Identifier {
        self.id()
    }

    fn ty(&self) -> ScalarType;
}

trait Lit: Into<Literal> {}

/// An atom that resolves to nothing, as reported by
/// [`LogicalProgram::dangling_references`]. Borrows the offending site from the
/// program so that a diagnostic can point at it.
struct DanglingReference<'a, P: Predicate> {
    /// The predicate whose definition contains the offending atom.
    predicate: &'a P,
    /// The rule within that predicate.
    rule: &'a P::Rule,
    /// The name that resolves to neither the IDB nor the EDB.
    identifier: &'a P::Identifier,
}

impl<P: Predicate> fmt::Display for DanglingReference<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rule '{}' of predicate '{}' references '{}', which is neither \
             defined by the program nor a base relation of the EDB",
            self.rule.id(),
            self.predicate.id(),
            self.identifier,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Identifier for String {}

    /// A predicate named `name`, defined by one rule per entry in `rules`,
    /// each rule listing the names its atoms reference. A name that is not a
    /// predicate of the program stands for a base table from the EDB.
    fn pred(name: &str, rules: &[&[&str]]) -> TestPredicate {
        TestPredicate {
            name: name.to_owned(),
            rules: rules
                .iter()
                .enumerate()
                .map(|(idx, atoms)| rule(&format!("{name}#{idx}"), name, atoms))
                .collect(),
        }
    }

    /// A rule called `name`, deriving the predicate `head` from body `atoms`.
    fn rule(name: &str, head: &str, atoms: &[&str]) -> TestRule {
        TestRule {
            name: name.to_owned(),
            head: TestAtom {
                name: head.to_owned(),
            },
            atoms: atoms
                .iter()
                .map(|atom| TestAtom {
                    name: (*atom).to_owned(),
                })
                .collect(),
        }
    }

    /// A program whose EDB holds every name the predicates reference but do not
    /// define, so that it is free of dangling references by construction.
    fn program(predicates: Vec<TestPredicate>) -> TestLogicalProgram {
        let defined: HashSet<&String> =
            predicates.iter().map(|predicate| &predicate.name).collect();
        let base_relations = predicates
            .iter()
            .flat_map(|predicate| predicate.rules.iter())
            .flat_map(|rule| rule.atoms.iter())
            .map(|atom| atom.name.clone())
            .filter(|name| !defined.contains(name))
            .collect();
        TestLogicalProgram {
            predicates,
            base_relations,
        }
    }

    /// A program whose EDB holds exactly `base_relations`, for the cases where
    /// that matters.
    fn program_over(predicates: Vec<TestPredicate>, base_relations: &[&str]) -> TestLogicalProgram {
        TestLogicalProgram {
            predicates,
            base_relations: base_relations
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        }
    }

    /// The names of the predicates per clique, in execution order.
    fn cliques_of(program: &TestLogicalProgram) -> Vec<Vec<String>> {
        program
            .cliques()
            .into_iter()
            .map(|clique| clique.members().map(|member| member.id().clone()).collect())
            .collect()
    }

    /// The names of the non-recursive and the recursive rules of every clique,
    /// in execution order.
    fn rule_split_of(program: &TestLogicalProgram) -> Vec<(Vec<String>, Vec<String>)> {
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
    fn dangling_names(program: &TestLogicalProgram) -> Vec<String> {
        program
            .dangling_references()
            .iter()
            .map(|reference| reference.identifier.clone())
            .collect()
    }

    /// The predicate names, and per predicate its rule names.
    fn grouping_of(predicates: &[TestPredicate]) -> Vec<(&str, Vec<&str>)> {
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

    #[test]
    fn rules_group_into_predicates_by_their_head() {
        // `r1` interleaves the two rules deriving `path`, so the grouping
        // cannot simply be a run-length split of the input.
        let predicates = RulePredicate::group(vec![
            (None, rule("r0", "path", &["edge"])),
            (None, rule("r1", "reachable", &["path"])),
            (None, rule("r2", "path", &["path", "edge"])),
        ]);
        assert_eq!(
            grouping_of(&predicates),
            vec![("path", vec!["r0", "r2"]), ("reachable", vec!["r1"])]
        );
    }

    #[test]
    fn grouping_orders_predicates_by_first_definition() {
        // `reachable` is defined first, so it comes first, even though `path`
        // is the one it reads.
        let predicates = RulePredicate::group(vec![
            (None, rule("r0", "reachable", &["path"])),
            (None, rule("r1", "path", &["edge"])),
        ]);
        assert_eq!(
            grouping_of(&predicates),
            vec![("reachable", vec!["r0"]), ("path", vec!["r1"])]
        );
    }

    #[test]
    fn a_given_name_overrides_the_head() {
        // `r1` is named differently from the predicate it defines,
        // so its definand comes alongside instead of out of its head.
        // `r0` takes the plain Datalog route, and the two still meet in `path`.
        // No frontend currently has this mixed behavior but we support it
        // anyways.
        let predicates = RulePredicate::group(vec![
            (None, rule("r0", "path", &["edge"])),
            (
                Some("path".to_owned()),
                rule("r1", "transitive_closure", &["path", "edge"]),
            ),
        ]);
        assert_eq!(grouping_of(&predicates), vec![("path", vec!["r0", "r1"])]);
    }

    #[test]
    fn one_name_for_every_rule_yields_one_predicate() {
        // An ad-hoc query: whatever the heads say, it all derives one output.
        let predicates = RulePredicate::group(vec![
            (Some("answer".to_owned()), rule("r0", "path", &["edge"])),
            (
                Some("answer".to_owned()),
                rule("r1", "reachable", &["path"]),
            ),
        ]);
        assert_eq!(grouping_of(&predicates), vec![("answer", vec!["r0", "r1"])]);
    }

    #[test]
    fn grouping_nothing_yields_no_predicates() {
        let nothing = Vec::<(Option<String>, TestRule)>::new();
        assert!(RulePredicate::group(nothing).is_empty());
    }

    #[test]
    fn a_grouped_program_stratifies() {
        // The path a real frontend takes: a flat list of rules, grouped into
        // predicates, read back as a program.
        let program = program(RulePredicate::group(vec![
            (None, rule("r0", "reachable", &["path"])),
            (None, rule("r1", "path", &["edge"])),
            (None, rule("r2", "path", &["path", "edge"])),
        ]));
        assert_eq!(dangling_names(&program), Vec::<String>::new());
        assert_eq!(cliques_of(&program), vec![vec!["path"], vec!["reachable"]]);
        let cliques = program.cliques();
        let [path, _] = cliques.as_slice() else {
            panic!("expected two cliques, got {}", cliques.len());
        };
        assert!(path.is_recursive());
        assert_eq!(
            path.rec_rules()
                .map(|rule| rule.id().as_str())
                .collect::<Vec<_>>(),
            vec!["r2"]
        );
    }

    #[test]
    fn a_well_formed_program_has_no_dangling_references() {
        let program = program(vec![
            pred("path", &[&["edge"], &["path", "edge"]]),
            pred("reachable", &[&["path"]]),
        ]);
        assert_eq!(dangling_names(&program), Vec::<String>::new());
    }

    #[test]
    fn a_name_in_neither_the_idb_nor_the_edb_dangles() {
        // `pathh` is a typo for `path` and `edgee` one for `edge`.
        let program = program_over(
            vec![pred("path", &[&["edge"], &["pathh", "edgee"]])],
            &["edge"],
        );
        assert_eq!(dangling_names(&program), vec!["pathh", "edgee"]);
    }

    #[test]
    fn a_dangling_reference_names_its_site() {
        let program = program_over(vec![pred("path", &[&["edgee"]])], &["edge"]);
        let references = program.dangling_references();
        let [reference] = references.as_slice() else {
            panic!(
                "expected exactly one dangling reference, got {}",
                references.len()
            );
        };
        assert_eq!(reference.predicate.id(), "path");
        assert_eq!(reference.rule.id(), "path#0");
        assert_eq!(
            reference.to_string(),
            "rule 'path#0' of predicate 'path' references 'edgee', which is \
             neither defined by the program nor a base relation of the EDB"
        );
    }

    #[test]
    fn a_predicate_shadows_a_base_relation_of_the_same_name() {
        // `edge` is both a predicate and a base relation. Listing `path` first
        // makes the outcome tell the two readings apart: resolving to the
        // predicate puts `edge` in an earlier clique, resolving to the base
        // relation would leave `path` without any dependency and hence first.
        let program = program_over(
            vec![pred("path", &[&["edge"]]), pred("edge", &[&["raw_edge"]])],
            &["edge", "raw_edge"],
        );
        assert_eq!(dangling_names(&program), Vec::<String>::new());
        assert_eq!(cliques_of(&program), vec![vec!["edge"], vec!["path"]]);
    }

    #[test]
    fn members_outlive_the_clique_they_came_from() {
        let program = program(vec![pred("a", &[&["b"]]), pred("b", &[&["edge"]])]);
        let cliques = program.cliques();
        // The members borrow from the program, so they may be collected out of
        // the cliques and outlive iterating them.
        let members: Vec<&TestPredicate> =
            cliques.iter().flat_map(|clique| clique.members()).collect();
        assert_eq!(
            members
                .iter()
                .map(|member| member.id().as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
    }

    #[test]
    fn self_recursion_is_visible_without_cliques() {
        let program = program(vec![
            pred("path", &[&["edge"], &["path", "edge"]]),
            pred("even", &[&["odd"]]),
            pred("odd", &[&["even"]]),
        ]);
        let self_recursive: Vec<bool> = program
            .predicates()
            .map(|predicate| predicate.is_self_recursive())
            .collect();
        assert_eq!(self_recursive, vec![true, false, false]);
        // Mutual recursion, in contrast, only shows up once the cliques are
        // known: neither `even` nor `odd` references itself.
        let recursive: Vec<bool> = program
            .cliques()
            .iter()
            .map(|clique| clique.is_recursive())
            .collect();
        assert_eq!(recursive, vec![true, true]);
    }

    #[test]
    fn a_clique_without_recursion_has_no_rec_rules() {
        let program = program(vec![pred("a", &[&["b"]]), pred("b", &[&["edge"]])]);
        let cliques = program.cliques();
        assert!(cliques.iter().all(|clique| !clique.is_recursive()));
    }

    #[test]
    fn edb_references_do_not_create_cliques() {
        // Neither `edge` nor `node` is a predicate of the program.
        let program = program(vec![pred("reachable", &[&["edge", "node"]])]);
        assert_eq!(cliques_of(&program), vec![vec!["reachable"]]);
    }

    #[test]
    fn dependencies_run_before_their_dependents() {
        // `a :- b.`, `b :- c.`, `c :- edge.`
        let program = program(vec![
            pred("a", &[&["b"]]),
            pred("b", &[&["c"]]),
            pred("c", &[&["edge"]]),
        ]);
        assert_eq!(cliques_of(&program), vec![vec!["c"], vec!["b"], vec!["a"]]);
    }

    #[test]
    fn self_recursion_stays_a_single_member_clique() {
        // The textbook transitive closure over a base table `edge`.
        let program = program(vec![pred("path", &[&["edge"], &["path", "edge"]])]);
        assert_eq!(cliques_of(&program), vec![vec!["path"]]);
        assert_eq!(
            rule_split_of(&program),
            vec![(vec!["path#0".to_owned()], vec!["path#1".to_owned()])]
        );
    }

    #[test]
    fn mutual_recursion_collapses_into_one_clique() {
        // `even :- zero.`, `even :- odd.`, `odd :- even.`
        let program = program(vec![
            pred("even", &[&["zero"], &["odd"]]),
            pred("odd", &[&["even"]]),
        ]);
        assert_eq!(cliques_of(&program), vec![vec!["even", "odd"]]);
        // The rule reaching out of the clique is the non-recursive one; both
        // rules crossing between the members are recursive.
        assert_eq!(
            rule_split_of(&program),
            vec![(
                vec!["even#0".to_owned()],
                vec!["even#1".to_owned(), "odd#0".to_owned()]
            )]
        );
    }

    #[test]
    fn a_clique_runs_after_the_clique_it_reads() {
        // `even`/`odd` are mutually recursive and both read `num`.
        let program = program(vec![
            pred("even", &[&["num"], &["odd"]]),
            pred("odd", &[&["num"], &["even"]]),
            pred("num", &[&["succ"]]),
        ]);
        assert_eq!(cliques_of(&program), vec![vec!["num"], vec!["even", "odd"]]);
        // For the recursive clique, reading `num` is not recursion: `num` sits
        // in an earlier clique and is fully computed by then.
        assert_eq!(
            rule_split_of(&program),
            vec![
                (vec!["num#0".to_owned()], Vec::new()),
                (
                    vec!["even#0".to_owned(), "odd#0".to_owned()],
                    vec!["even#1".to_owned(), "odd#1".to_owned()]
                ),
            ]
        );
    }

    struct TestLogicalProgram {
        predicates: Vec<TestPredicate>,
        base_relations: HashSet<String>,
    }

    impl LogicalProgram for TestLogicalProgram {
        type Identifier = String;
        type Predicate = TestPredicate;

        fn predicates(&self) -> impl Iterator<Item = &Self::Predicate> {
            self.predicates.iter()
        }

        fn is_base_relation(&self, identifier: &Self::Identifier) -> bool {
            self.base_relations.contains(identifier)
        }
    }

    /// The tests use [`RulePredicate`] itself as their [`Predicate`], so the
    /// scaffolding stops at the rule level.
    type TestPredicate = RulePredicate<String, TestRule>;

    struct TestRule {
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

    struct TestAtom {
        name: String,
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

        fn bindings(&self) -> impl Iterator<Item = Bind<&Self::Var, &Self::Lit>> {
            std::iter::empty()
        }

        fn vars(&self) -> impl Iterator<Item = &Self::Var> {
            std::iter::empty()
        }
    }

    struct TestTypedVar {
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

    struct TestLit(Literal);

    impl From<TestLit> for Literal {
        fn from(value: TestLit) -> Self {
            value.0
        }
    }

    impl Lit for TestLit {}

    /// Conditions play no part in the reference graph, so the test programs
    /// carry none and this type exists only to satisfy [`Rule::Cond`].
    struct TestCond {
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

    fn borrow<Var, Lit>(bind: &Bind<Var, Lit>) -> Bind<&Var, &Lit> {
        match bind {
            Bind::Var(var) => Bind::Var(var),
            Bind::Lit(lit) => Bind::Lit(lit),
        }
    }
}
