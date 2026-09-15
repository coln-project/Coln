// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod gabow;

use crate::{
    host::{expr::Literal, operator::Operator},
    scalarial::ScalarType,
};
use std::{collections::HashMap, fmt};

trait Identifier: Clone + fmt::Debug + fmt::Display + PartialEq + Eq + std::hash::Hash {}

trait Identifiable<Identifier> {
    fn id(&self) -> &Identifier;
}

// TODO: On top of these traits, implement:
// - Protect against unknown predicate references, that is, predicates neither
//   part of the IDB nor EDB
// - Conjunctive query translation

trait LogicalProgram {
    type Identifier: Identifier;
    type Predicate: Predicate<Identifier = Self::Identifier>;

    /// All predicates of the program, each exactly once and in no particular
    /// order.
    fn predicates(&self) -> impl Iterator<Item = &Self::Predicate>;

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
        // An atom naming something that is not a predicate (a base table from
        // the EDB) resolves to no index and hence contributes no edge.
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

/// A rule contains atoms and conditions, sometimes united under the umbrella
/// term _proposition_.
trait Rule: Identifiable<Self::Identifier> {
    type Identifier: Identifier;
    type Atom: Atom<Identifier = Self::Identifier>;
    type Cond: Cond;

    fn atoms(&self) -> impl Iterator<Item = &Self::Atom>;
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

#[cfg(test)]
mod tests {
    use super::*;

    impl Identifier for String {}

    /// A predicate named `name`, defined by one rule per entry in `rules`,
    /// each rule listing the names its atoms reference. A name that is not a
    /// predicate of the program stands for a base table from the EDB.
    fn pred(name: &str, rules: &[&[&str]]) -> Pred {
        Pred {
            name: name.to_owned(),
            rules: rules
                .iter()
                .enumerate()
                .map(|(idx, atoms)| Rl {
                    name: format!("{name}#{idx}"),
                    atoms: atoms
                        .iter()
                        .map(|atom| Atm {
                            name: (*atom).to_owned(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn program(predicates: Vec<Pred>) -> Program {
        Program { predicates }
    }

    /// The names of the predicates per clique, in execution order.
    fn cliques_of(program: &Program) -> Vec<Vec<String>> {
        program
            .cliques()
            .into_iter()
            .map(|clique| clique.members().map(|member| member.id().clone()).collect())
            .collect()
    }

    /// The names of the non-recursive and the recursive rules of every clique,
    /// in execution order.
    fn rule_split_of(program: &Program) -> Vec<(Vec<String>, Vec<String>)> {
        program
            .cliques()
            .into_iter()
            .map(|clique| {
                let names = |rules: &mut dyn Iterator<Item = &Rl>| {
                    rules.map(|rule| rule.id().clone()).collect()
                };
                (
                    names(&mut clique.non_rec_rules()),
                    names(&mut clique.rec_rules()),
                )
            })
            .collect()
    }

    #[test]
    fn members_outlive_the_clique_they_came_from() {
        let program = program(vec![pred("a", &[&["b"]]), pred("b", &[&["edge"]])]);
        let cliques = program.cliques();
        // The members borrow from the program, so they may be collected out of
        // the cliques and outlive iterating them.
        let members: Vec<&Pred> = cliques.iter().flat_map(|clique| clique.members()).collect();
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

    struct Program {
        predicates: Vec<Pred>,
    }

    impl LogicalProgram for Program {
        type Identifier = String;
        type Predicate = Pred;

        fn predicates(&self) -> impl Iterator<Item = &Self::Predicate> {
            self.predicates.iter()
        }
    }

    struct Pred {
        name: String,
        rules: Vec<Rl>,
    }

    impl Identifiable<String> for Pred {
        fn id(&self) -> &String {
            &self.name
        }
    }

    impl Predicate for Pred {}

    impl AggregateRules for Pred {
        type Identifier = String;
        type Rule = Rl;

        fn rules(&self) -> impl Iterator<Item = &Self::Rule> {
            self.rules.iter()
        }
    }

    struct Rl {
        name: String,
        atoms: Vec<Atm>,
    }

    impl Identifiable<String> for Rl {
        fn id(&self) -> &String {
            &self.name
        }
    }

    impl Rule for Rl {
        type Identifier = String;
        type Atom = Atm;
        type Cond = Condition;

        fn atoms(&self) -> impl Iterator<Item = &Self::Atom> {
            self.atoms.iter()
        }

        fn conditions(&self) -> impl Iterator<Item = &Self::Cond> {
            std::iter::empty()
        }
    }

    struct Atm {
        name: String,
    }

    impl Identifiable<String> for Atm {
        fn id(&self) -> &String {
            &self.name
        }
    }

    impl Atom for Atm {
        type Identifier = String;
        type Var = Var;
        type Lit = Value;

        fn bindings(&self) -> impl Iterator<Item = Bind<&Self::Var, &Self::Lit>> {
            std::iter::empty()
        }

        fn vars(&self) -> impl Iterator<Item = &Self::Var> {
            std::iter::empty()
        }
    }

    struct Var {
        name: String,
    }

    impl Identifiable<String> for Var {
        fn id(&self) -> &String {
            &self.name
        }
    }

    impl TypedVar for Var {
        type Identifier = String;

        fn ty(&self) -> ScalarType {
            ScalarType::Uint
        }
    }

    struct Value(Literal);

    impl From<Value> for Literal {
        fn from(value: Value) -> Self {
            value.0
        }
    }

    impl Lit for Value {}

    /// Conditions play no part in the reference graph, so the test programs
    /// carry none and this type exists only to satisfy [`Rule::Cond`].
    struct Condition {
        operator: Operator,
        left: Bind<Var, Value>,
        right: Bind<Var, Value>,
    }

    impl Cond for Condition {
        type Identifier = String;
        type Var = Var;
        type Lit = Value;

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
