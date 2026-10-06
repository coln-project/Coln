// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Datalog notation for anything implementing this layer's traits.
//!
//! Each trait gets an adapter borrowing the implementor, handed out by the
//! trait's `display` method. A blanket `impl Display` is not an option: the
//! traits are generic over foreign types, and one adapter generic over all of
//! them would have overlapping impls.
//!
//! The notation, with `{}` and `{:#}` respectively:
//!
//! ```text
//! .decl path(x: uint, y: uint)
//! .output path
//! path(x, y) :- edge(x, y).
//! path(x, z) :- path(x, y), edge(y, z), !blocked(z), z != 3.
//!
//! .decl path(x: uint, y: uint)
//! .output path
//! // r0
//! path(x, y) :-
//!     edge(x, y).
//! …
//! ```
//!
//! The alternate form puts each body proposition on its own line and names
//! each rule in a comment, as Datalog has no syntax for a rule's own name.
//!
//! An atom's [`bindings`](Atom::bindings) are sparse, so a position it skips
//! prints as `_`. Only positions up to the last bound one print, though: an
//! atom does not know its relation's arity, so trailing unbound positions are
//! lost. A [`head`](Rule::head) is dense and hence unaffected.

use super::{Atom, Bind, Cond, Identifiable, Lit, LogicalProgram, Predicate, Rule, TypedVar};
use crate::host::operator::Operator;
use std::fmt::{self, Display};

/// An [`Atom`] as `name(term, …)`, prefixed with `!` if negated.
pub struct AtomDisplay<'a, A>(pub(super) &'a A);

/// A [`Cond`] as `left operator right`.
pub struct CondDisplay<'a, C>(pub(super) &'a C);

/// A [`Rule`] as `head :- proposition, ….`, or as `head.` with an empty body.
pub struct RuleDisplay<'a, R>(pub(super) &'a R);

/// A [`Predicate`] as its `.decl`, followed by its rules one per line.
///
/// An EDB predicate prints as its `.decl` alone: its single rule only marks
/// it as given externally and says nothing a reader would recognize as Datalog.
pub struct PredicateDisplay<'a, P>(pub(super) &'a P);

/// A [`LogicalProgram`] as its predicates, separated by blank lines. Each
/// predicate the program [outputs](LogicalProgram::is_output) carries an
/// `.output` directive below its `.decl`, which only the program can tell:
/// a [`PredicateDisplay`] on its own prints none.
pub struct ProgramDisplay<'a, L>(pub(super) &'a L);

impl<A: Atom> Display for AtomDisplay<'_, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let atom = self.0;
        if atom.is_negative() {
            f.write_str("!")?;
        }
        write!(f, "{}(", atom.id())?;
        separated(f, dense(atom.bindings()), ", ")?;
        f.write_str(")")
    }
}

impl<C: Cond> Display for CondDisplay<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cond = self.0;
        let operator: Operator = cond.operator().into();
        write!(
            f,
            "{} {operator} {}",
            Term(Some(cond.left())),
            Term(Some(cond.right()))
        )
    }
}

impl<R: Rule> Display for RuleDisplay<'_, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rule = self.0;
        if f.alternate() {
            writeln!(f, "// {}", rule.id())?;
        }
        AtomDisplay(rule.head()).fmt(f)?;
        let mut body = rule
            .atoms()
            .map(Proposition::Atom)
            .chain(rule.conditions().map(Proposition::Cond))
            .peekable();
        if body.peek().is_some() {
            let (neck, separator) = match f.alternate() {
                true => (" :-\n    ", ",\n    "),
                false => (" :- ", ", "),
            };
            f.write_str(neck)?;
            separated(f, body, separator)?;
        }
        f.write_str(".")
    }
}

impl<P: Predicate> Display for PredicateDisplay<'_, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        declaration(f, self.0)?;
        rules(f, self.0)
    }
}

impl<L: LogicalProgram> Display for ProgramDisplay<'_, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let program = self.0;
        let predicates = program.predicates().map(|predicate| {
            fmt::from_fn(move |f| {
                declaration(f, predicate)?;
                if program.is_output(predicate) {
                    write!(f, "\n.output {}", predicate.id())?;
                }
                rules(f, predicate)
            })
        });
        separated(f, predicates, "\n\n")
    }
}

/// A predicate's `.decl` line.
fn declaration<P: Predicate>(f: &mut fmt::Formatter<'_>, predicate: &P) -> fmt::Result {
    write!(f, ".decl {}(", predicate.id())?;
    separated(f, predicate.columns(), ", ")?;
    f.write_str(")")
}

/// A predicate's rules, each on a line of its own. An EDB predicate has none
/// worth printing, see [`PredicateDisplay`].
fn rules<P: Predicate>(f: &mut fmt::Formatter<'_>, predicate: &P) -> fmt::Result {
    if predicate.is_edb_predicate() {
        return Ok(());
    }
    predicate.rules().try_for_each(|rule| {
        f.write_str("\n")?;
        RuleDisplay(rule).fmt(f)
    })
}

/// A rule's body is made of propositions: atoms and conditions alike.
enum Proposition<'a, A, C> {
    Atom(&'a A),
    Cond(&'a C),
}

impl<A: Atom, C: Cond> Display for Proposition<'_, A, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Proposition::Atom(atom) => AtomDisplay(*atom).fmt(f),
            Proposition::Cond(cond) => CondDisplay(*cond).fmt(f),
        }
    }
}

/// What fills one position of an atom, or `None` for a position it skips.
struct Term<'a, V, L>(Option<Bind<&'a V, &'a L>>);

impl<V: TypedVar, L: Lit> Display for Term<'_, V, L> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(Bind::Var(var)) => var.name().fmt(f),
            Some(Bind::Lit(lit)) => lit.to_literal().fmt(f),
            None => f.write_str("_"),
        }
    }
}

/// The sparse `bindings` laid out by position, up to the last bound one, with
/// every position in between that binds nothing as an empty [`Term`].
fn dense<'a, V: 'a, L: 'a>(
    bindings: impl Iterator<Item = (usize, Bind<&'a V, &'a L>)>,
) -> impl Iterator<Item = Term<'a, V, L>> {
    let mut bindings: Vec<_> = bindings.collect();
    bindings.sort_by_key(|(position, _)| *position);
    let arity = bindings.last().map_or(0, |(position, _)| position + 1);
    let mut bindings = bindings.into_iter().peekable();
    (0..arity).map(move |position| {
        Term(
            bindings
                .next_if(|(at, _)| *at == position)
                .map(|(_, bind)| bind),
        )
    })
}

/// Writes `items` with `separator` in between. Each item is formatted with
/// `f` itself, so that flags like `{:#}` carry over to it.
pub(super) fn separated<T: Display>(
    f: &mut fmt::Formatter<'_>,
    items: impl IntoIterator<Item = T>,
    separator: &str,
) -> fmt::Result {
    items.into_iter().enumerate().try_for_each(|(idx, item)| {
        if idx > 0 {
            f.write_str(separator)?;
        }
        item.fmt(f)
    })
}

#[cfg(test)]
mod tests {
    use super::super::{RulePredicate, test_utils::*};
    use super::*;

    #[test]
    fn an_atom_prints_its_terms_by_position() {
        assert_eq!(atom("edge(x, 3)").display().to_string(), "edge(x, 3)");
        assert_eq!(atom("!edge(x, y)").display().to_string(), "!edge(x, y)");
        assert_eq!(atom("unit").display().to_string(), "unit()");
    }

    #[test]
    fn an_atom_prints_a_skipped_position_as_a_wildcard() {
        // The trailing `_` is lost: an atom does not know its arity.
        assert_eq!(atom("edge(_, y, _)").display().to_string(), "edge(_, y)");
    }

    #[test]
    fn a_string_literal_prints_quoted() {
        // Bare, the literal would read as the variable `ann`.
        assert_eq!(
            atom(r#"person(ann, "ann")"#).display().to_string(),
            r#"person(ann, "ann")"#
        );
    }

    #[test]
    fn a_rule_prints_its_body_on_one_line() {
        let rule = rule(
            "r0",
            "path(x, z)",
            &["path(x, y)", "edge(y, z)", "!blocked(z)", "z != 3"],
        );
        assert_eq!(
            rule.display().to_string(),
            "path(x, z) :- path(x, y), edge(y, z), !blocked(z), z != 3."
        );
    }

    #[test]
    fn a_rule_prints_its_body_one_proposition_per_line_when_alternate() {
        let rule = rule("r0", "path(x, z)", &["path(x, y)", "edge(y, z)"]);
        assert_eq!(
            format!("{:#}", rule.display()),
            "// r0\npath(x, z) :-\n    path(x, y),\n    edge(y, z)."
        );
    }

    #[test]
    fn a_rule_without_a_body_prints_as_a_fact() {
        assert_eq!(rule("r0", "zero(0)", &[]).display().to_string(), "zero(0).");
    }

    #[test]
    fn a_predicate_prints_its_declaration_and_rules() {
        let predicates = RulePredicate::group(
            [declare("path", &["x", "y"])],
            [
                rule("r0", "path(x, y)", &["edge(x, y)"]),
                rule("r1", "path(x, z)", &["path(x, y)", "edge(y, z)"]),
            ],
        )
        .expect("every definand names a declared predicate");
        assert_eq!(
            predicates[0].display().to_string(),
            ".decl path(x: uint, y: uint)\n\
             path(x, y) :- edge(x, y).\n\
             path(x, z) :- path(x, y), edge(y, z)."
        );
    }

    #[test]
    fn an_edb_predicate_prints_its_declaration_alone() {
        let predicates = RulePredicate::group(
            [declare("edge", &["from", "to"])],
            [rule("r0", "edge(from, to)", &[])],
        )
        .expect("every definand names a declared predicate");
        assert_eq!(
            predicates[0].display().to_string(),
            ".decl edge(from: uint, to: uint)"
        );
    }

    #[test]
    fn a_program_separates_its_predicates_by_blank_lines() {
        let program = program(vec![
            pred("path", &[&["edge"]]),
            pred("reachable", &[&["path"]]),
        ]);
        assert_eq!(
            format!("{:#}", program.display()),
            ".decl path(x: uint)\n\
             .output path\n\
             // path#0\n\
             path() :-\n    edge().\n\
             \n\
             .decl reachable(x: uint)\n\
             .output reachable\n\
             // reachable#0\n\
             reachable() :-\n    path()."
        );
    }
}
