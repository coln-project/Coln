// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Laws, a Datalog extension for implications a program expects to hold and
//! reports the violations of. See [`Law`].

use super::{Atom, Bind, Conjunction, Identifiable, Language, Predicate, Prop, Rule, TypedVar};

/// A law `antecedent => consequent`. Similar to a [`Rule`]'s body, the
/// antecedent and the consequent are each a [`Conjunction`] of atoms and
/// conditions.
///
/// The expectation of a law is that the implication holds. Where it does
/// _not_, its violations are reported: the assignments of the antecedent's
/// variables for which the consequent does not hold, that is,
/// `!(antecedent => consequent)`. [`lower`](Self::lower) derives them by
/// ordinary [rules](Rule). Due to the negation, this extension requires
/// Datalog with negation, and coln-query implements stratified negation.
///
/// The consequent is negated as a whole, `!(c₁, ..., cₙ)`, which no single rule
/// body can express due to DeMorgan's laws. Hence, the consequent becomes a
/// helper predicate of its own, named by [`Language::derive_pred_id`] with the
/// tag `consequent`, and the law's rule negates that one atom. For a law `m`,
/// in FLIR's naming:
///
/// ```text
/// m#consequent(x) :- t(_, _, x), x == 1.
/// m(x)            :- t(_, _, x), !m#consequent(x).
/// ```
///
/// The helper's columns are the antecedent's variables its body grounds, which
/// are what the negated atom matches on. A variable only the consequent grounds
/// is projected away, which makes it existential.
///
/// The helper's body is the given consequent alone if it stands as a rule on
/// its own, that is, it is [safe](Conjunction::is_safe). Otherwise, as for
/// `t(x) => x == 1`, the antecedent joins the consequent in the helper's
/// body. That changes nothing, as the law's rule requires the antecedent
/// anyway, but costs computing it twice, which is why it is not done if not
/// required.
///
/// No helper is needed, and the consequent is _inlined_ negated into the law's
/// rule instead, if:
///
/// 1. It is a single proposition, which can be negated on its own: negating
///    several of them takes parentheses no Datalog rule body has.
/// 2. The antecedent grounds every variable the consequent uses. A variable only
///    the consequent uses is existential, which a single negated atom or
///    condition cannot express: `!u(x, y)` leaves `y` ungrounded, and a negated
///    `!u(x, y)` turns into `u(x, y)`, which asks for _some_ `y` rather than
///    _every_ `y`. An empty antecedent grounds nothing and hence only admits a
///    proposition using no variable.
///
/// For the example above, the inlined version is:
///
/// ```text
/// m(x) :- t(_, _, x), x != 1.
/// ```
#[derive(Debug, Clone)]
pub struct Law<L: Language> {
    /// The law's predicate, which its violations are reported under.
    name: L::PredId,
    /// The rule deriving the law's violations.
    rule_id: L::RuleId,
    /// The variables a violation lists, in this order. Only the ones the
    /// antecedent grounds make it into a violation: a variable only the
    /// consequent grounds is existential and has no value a violation could
    /// carry.
    vars: Vec<L::Var>,
    /// Can be empty, as it then demands the consequent to be true at all times.
    antecedent: Conjunction<L>,
    /// Has to be non-empty. An empty consequent holds trivially, so a law
    /// with one has no violations to derive.
    consequent: Conjunction<L>,
    /// What the frontend attaches to both rules the law lowers to.
    meta: L::RuleMeta,
}

impl<L: Language> Law<L> {
    /// Create a new [`Law`]. Regarding the fields meaning refer to the
    /// respective field doc.
    pub fn new(
        name: L::PredId,
        rule_id: L::RuleId,
        vars: Vec<L::Var>,
        antecedent: Conjunction<L>,
        consequent: Conjunction<L>,
        meta: L::RuleMeta,
    ) -> Self {
        Self {
            name,
            rule_id,
            vars,
            antecedent,
            consequent,
            meta,
        }
    }
    /// The law's predicate, deriving its violations, and, unless the
    /// consequent is inlined, the helper predicate deriving its consequent, in
    /// this order. Each is defined by a single rule.
    pub fn lower(self) -> (Predicate<L>, Option<Predicate<L>>) {
        let Law {
            name,
            rule_id,
            vars,
            mut antecedent,
            consequent,
            meta,
        } = self;
        debug_assert!(
            !consequent.is_empty(),
            "law '{name}' has an empty consequent, which holds trivially"
        );

        let grounded = antecedent.grounded_vars();
        let law_vars: Vec<L::Var> = vars
            .into_iter()
            .filter(|var| grounded.contains_key(var.id()))
            .collect();
        let consequent = match <[_; 1]>::try_from(consequent.0) {
            Ok([lone])
                if lone
                    .as_ref()
                    .vars()
                    .all(|var| grounded.contains_key(var.id())) =>
            {
                antecedent.push(!lone);
                None
            }
            Ok(lone) => Some(lone.into_iter().collect()),
            Err(props) => Some(Conjunction(props)),
        };

        let helper = consequent.map(|consequent| {
            let helper_body = match consequent.is_safe() {
                true => consequent,
                false => antecedent.clone().into_iter().chain(consequent).collect(),
            };
            // Of the law's columns, the helper exposes the ones its body grounds.
            let helper_vars: Vec<L::Var> = {
                let helper = helper_body.grounded_vars();
                law_vars
                    .iter()
                    .filter(|var| helper.contains_key(var.id()))
                    .cloned()
                    .collect()
            };

            let helper_name = L::derive_pred_id(&name, "consequent");
            let helper_fields = || helper_vars.iter().cloned().map(Bind::Var);
            antecedent.push(Prop::Atom(Atom::new_negated(
                helper_name.clone(),
                helper_fields().enumerate(),
            )));
            Predicate {
                name: helper_name.clone(),
                columns: helper_vars.iter().map(TypedVar::column).collect(),
                rules: vec![Rule {
                    id: L::derive_rule_id(&rule_id, "consequent"),
                    head: Atom::new_head(helper_name, helper_fields().collect()),
                    body: helper_body,
                    meta: meta.clone(),
                }],
            }
        });
        let law = Predicate {
            name: name.clone(),
            columns: law_vars.iter().map(TypedVar::column).collect(),
            rules: vec![Rule {
                id: rule_id,
                head: Atom::new_head(name, law_vars.into_iter().map(Bind::Var).collect()),
                body: antecedent,
                meta,
            }],
        };
        (law, helper)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LogicalProgram, test_utils::*};
    use super::*;

    /// The law `m` over `vars`, `antecedent => consequent`, each side a
    /// [`conjunction`] spec.
    fn law(vars: &[&str], antecedent: &[&str], consequent: &[&str]) -> Law<TestLanguage> {
        Law {
            name: "m".into(),
            rule_id: "m".into(),
            vars: vars.iter().map(|name| var(name)).collect(),
            antecedent: conjunction(antecedent),
            consequent: conjunction(consequent),
            meta: (),
        }
    }

    /// The law's predicate and its helper predicate, if any, printed in that
    /// order.
    fn lowered(law: Law<TestLanguage>) -> (String, Option<String>) {
        let (law, helper) = law.lower();
        (law.to_string(), helper.map(|helper| helper.to_string()))
    }

    #[test]
    fn a_safe_consequent_is_the_helper_s_body_alone() {
        assert_eq!(
            lowered(law(&["x"], &["t(x)"], &["u(x)", "x == 1"])),
            (
                ".decl m(x: uint)\nm(x) :- t(x), !m#consequent(x).".into(),
                Some(".decl m#consequent(x: uint)\nm#consequent(x) :- u(x), x == 1.".into()),
            )
        );
    }

    #[test]
    fn an_unsafe_consequent_joins_the_antecedent_in_the_helper_s_body() {
        // `x > 1, x < 5` alone would leave `x` ungrounded.
        assert_eq!(
            lowered(law(&["x"], &["t(x)"], &["x > 1", "x < 5"])),
            (
                ".decl m(x: uint)\nm(x) :- t(x), !m#consequent(x).".into(),
                Some(".decl m#consequent(x: uint)\nm#consequent(x) :- t(x), x > 1, x < 5.".into()),
            )
        );
    }

    #[test]
    fn a_variable_only_the_consequent_grounds_is_existential() {
        // `y` has no value a violation could carry, so neither predicate has
        // a column for it.
        assert_eq!(
            lowered(law(&["x", "y"], &["t(x)"], &["u(x, y)"])),
            (
                ".decl m(x: uint)\nm(x) :- t(x), !m#consequent(x).".into(),
                Some(".decl m#consequent(x: uint)\nm#consequent(x) :- u(x, y).".into()),
            )
        );
    }

    #[test]
    fn a_consequent_sharing_no_variable_holds_or_fails_as_a_whole() {
        // The helper has no column, so its negation matches on nothing.
        assert_eq!(
            lowered(law(&["x", "y"], &["t(x)"], &["u(y)"])),
            (
                ".decl m(x: uint)\nm(x) :- t(x), !m#consequent().".into(),
                Some(".decl m#consequent()\nm#consequent() :- u(y).".into()),
            )
        );
    }

    #[test]
    fn the_columns_follow_the_given_variables_not_their_occurrence() {
        assert_eq!(
            lowered(law(&["y", "x"], &["t(x, y)"], &["u(x, y, z)"])),
            (
                ".decl m(y: uint, x: uint)\nm(y, x) :- t(x, y), !m#consequent(y, x).".into(),
                Some(
                    ".decl m#consequent(y: uint, x: uint)\nm#consequent(y, x) :- u(x, y, z)."
                        .into()
                ),
            )
        );
    }

    #[test]
    fn a_lone_proposition_the_antecedent_grounds_is_inlined_negated() {
        for (consequent, negated) in [("u(x)", "!u(x)"), ("!u(x)", "u(x)"), ("x == 1", "x != 1")] {
            assert_eq!(
                lowered(law(&["x"], &["t(x)"], &[consequent])),
                (format!(".decl m(x: uint)\nm(x) :- t(x), {negated}."), None)
            );
        }
        assert_eq!(
            lowered(law(&[], &[], &["u"])),
            (".decl m()\nm() :- !u().".into(), None)
        );
    }

    #[test]
    fn a_lone_proposition_with_an_existential_variable_is_not_inlined() {
        // Inlined, `!u(x, y)` would turn into `u(x, y)`, asking for some `y`
        // rather than every `y`.
        assert_eq!(
            lowered(law(&["x"], &["t(x)"], &["!u(x, y)"])),
            (
                ".decl m(x: uint)\nm(x) :- t(x), !m#consequent(x).".into(),
                Some(".decl m#consequent(x: uint)\nm#consequent(x) :- t(x), !u(x, y).".into()),
            )
        );
        // An empty antecedent grounds nothing.
        assert_eq!(
            lowered(law(&["x"], &[], &["u(x)"])),
            (
                ".decl m()\nm() :- !m#consequent().".into(),
                Some(".decl m#consequent()\nm#consequent() :- u(x).".into()),
            )
        );
    }

    #[test]
    fn a_lowered_law_is_a_valid_program() {
        // Both shapes of the helper, the consequent alone and joined with the
        // antecedent, and no helper at all.
        for consequent in [&["u(x)", "x == 1"][..], &["x > 1", "x < 5"], &["u(x)"]] {
            let (law, helper) = law(&["x"], &["t(x)"], consequent).lower();
            let mut predicates = vec![pred("t", &[&[]]), pred("u", &[&[]]), law];
            predicates.extend(helper);
            let program = program(predicates);
            program
                .verify()
                .unwrap_or_else(|rejected| panic!("{consequent:?}: {}", rejected.error));
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "law 'm' has an empty consequent")]
    fn an_empty_consequent_is_rejected() {
        law(&["x"], &["t(x)"], &[]).lower();
    }
}
