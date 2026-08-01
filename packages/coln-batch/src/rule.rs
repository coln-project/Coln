// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Datalog rules and programs.
//!
//! A [`Rule`] is `head ← body`: if the body (a conjunctive query, same
//! shape as [`crate::query::Query`]) matches, the head row must exist.
//! This mirrors FLIR's `Rule { antecedents, consequents }` for the
//! single-consequent, `Chased` case — the batch engine computes the least
//! fixpoint (in Coln terms: the initial model) of a set of such rules.
//!
//! Relations that appear in some head are **IDB** (derived); all others
//! are **EDB** (stored input). An IDB relation may also have initial
//! facts in the catalog; they are treated as already-derived rows.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::query::{Atom, Catalog, Query, Term};
use crate::relation::Relation;

#[derive(Clone, Debug)]
pub struct Rule {
    /// Variable names; indices are the rule's variable ids. The numbering
    /// doubles as the generic join's elimination order for the body.
    pub var_names: Vec<String>,
    pub head: Atom,
    pub body: Vec<Atom>,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub rules: Vec<Rule>,
}

/// Reserved name prefix for the per-round delta relations of semi-naive
/// evaluation. User relations must not use it.
pub(crate) const DELTA_PREFIX: &str = "__delta_";

pub(crate) fn delta_name(relation: &str) -> String {
    format!("{DELTA_PREFIX}{relation}")
}

/// One column of a rule head: either the k-th projected body variable or
/// a literal value.
#[derive(Clone, Copy, Debug)]
pub(crate) enum HeadCol {
    Var(usize),
    Lit(u64),
}

/// A rule lowered to an executable form.
pub(crate) struct LoweredRule {
    /// The body as a query; its head projects the rule head's variables
    /// in order of occurrence.
    pub query: Query,
    pub head_relation: String,
    pub head_cols: Vec<HeadCol>,
    /// Body positions whose relation is an IDB relation.
    pub idb_positions: Vec<usize>,
}

impl LoweredRule {
    /// The body query with the IDB atom at `position` redirected to its
    /// delta relation (semi-naive rewriting).
    pub fn query_with_delta(&self, position: usize) -> Query {
        let mut q = self.query.clone();
        q.atoms[position].relation = delta_name(&q.atoms[position].relation);
        q
    }

    /// Turn a body-query result into rows of the head relation.
    pub fn materialize_head(&self, result: &Relation, col_names: &[String]) -> Relation {
        let cols = self
            .head_cols
            .iter()
            .map(|hc| match hc {
                HeadCol::Var(k) => result.cols[*k].clone(),
                HeadCol::Lit(x) => vec![*x; result.len()],
            })
            .collect();
        Relation::new(self.head_relation.clone(), col_names.to_vec(), cols).sorted_dedup()
    }
}

/// A validated, lowered program, ready for fixpoint evaluation.
pub(crate) struct CompiledProgram {
    pub rules: Vec<LoweredRule>,
    /// IDB relation name → column names (from initial facts if present,
    /// else from the first defining rule head).
    pub idb_schemas: BTreeMap<String, Vec<String>>,
}

impl Program {
    /// Validate against the EDB catalog and lower every rule.
    pub(crate) fn compile(&self, edb: &Catalog) -> Result<CompiledProgram> {
        if self.rules.is_empty() {
            bail!("program has no rules");
        }

        // Collect IDB schemas: arity from the heads, column names from
        // initial facts or the first defining rule.
        let mut idb_schemas: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for rule in &self.rules {
            let name = &rule.head.relation;
            if name.starts_with(DELTA_PREFIX) {
                bail!("relation name {name} uses the reserved prefix {DELTA_PREFIX}");
            }
            let col_names: Vec<String> = if let Ok(initial) = edb.get(name) {
                if initial.arity() != rule.head.terms.len() {
                    bail!(
                        "initial facts for {name} have arity {}, head has {}",
                        initial.arity(),
                        rule.head.terms.len()
                    );
                }
                initial.col_names.clone()
            } else {
                rule.head
                    .terms
                    .iter()
                    .enumerate()
                    .map(|(i, t)| match t {
                        Term::Var(v) => rule.var_names[*v].clone(),
                        Term::Lit(_) => format!("c{i}"),
                    })
                    .collect()
            };
            if let Some(existing) = idb_schemas.get(name) {
                if existing.len() != rule.head.terms.len() {
                    bail!("relation {name} is defined with inconsistent arities");
                }
            } else {
                idb_schemas.insert(name.clone(), col_names);
            }
        }

        // Lower and validate each rule.
        let mut rules = Vec::with_capacity(self.rules.len());
        for rule in &self.rules {
            let mut head_vars = Vec::new();
            let mut head_cols = Vec::new();
            for term in &rule.head.terms {
                match term {
                    Term::Var(v) => {
                        head_cols.push(HeadCol::Var(head_vars.len()));
                        head_vars.push(*v);
                    }
                    Term::Lit(x) => head_cols.push(HeadCol::Lit(*x)),
                }
            }
            if head_vars.is_empty() {
                bail!(
                    "rule for {} has no variables in its head (not supported)",
                    rule.head.relation
                );
            }
            let query = Query {
                var_names: rule.var_names.clone(),
                atoms: rule.body.clone(),
                head: head_vars,
            };
            query.validate()?; // body non-empty, head vars appear in body, …

            let mut idb_positions = Vec::new();
            for (i, atom) in rule.body.iter().enumerate() {
                if atom.relation.starts_with(DELTA_PREFIX) {
                    bail!(
                        "relation name {} uses the reserved prefix {DELTA_PREFIX}",
                        atom.relation
                    );
                }
                if idb_schemas.contains_key(&atom.relation) {
                    idb_positions.push(i);
                    // IDB arity check against the head-derived schema.
                    if idb_schemas[&atom.relation].len() != atom.terms.len() {
                        bail!(
                            "atom over {} has {} terms, relation has arity {}",
                            atom.relation,
                            atom.terms.len(),
                            idb_schemas[&atom.relation].len()
                        );
                    }
                } else {
                    // EDB: must exist in the catalog with matching arity.
                    let rel = edb.get(&atom.relation)?;
                    if rel.arity() != atom.terms.len() {
                        bail!(
                            "atom over {} has {} terms, relation has arity {}",
                            atom.relation,
                            atom.terms.len(),
                            rel.arity()
                        );
                    }
                }
            }
            rules.push(LoweredRule {
                query,
                head_relation: rule.head.relation.clone(),
                head_cols,
                idb_positions,
            });
        }

        Ok(CompiledProgram { rules, idb_schemas })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edb() -> Catalog {
        let mut cat = Catalog::new();
        cat.insert(Relation::new(
            "parent",
            ["parent", "child"],
            vec![vec![0], vec![1]],
        ));
        cat
    }

    fn ancestor_rules() -> Program {
        Program {
            rules: vec![
                Rule {
                    var_names: vec!["x".into(), "y".into()],
                    head: Atom {
                        relation: "ancestor".into(),
                        terms: vec![Term::Var(0), Term::Var(1)],
                    },
                    body: vec![Atom {
                        relation: "parent".into(),
                        terms: vec![Term::Var(0), Term::Var(1)],
                    }],
                },
                Rule {
                    var_names: vec!["x".into(), "y".into(), "z".into()],
                    head: Atom {
                        relation: "ancestor".into(),
                        terms: vec![Term::Var(0), Term::Var(2)],
                    },
                    body: vec![
                        Atom {
                            relation: "parent".into(),
                            terms: vec![Term::Var(0), Term::Var(1)],
                        },
                        Atom {
                            relation: "ancestor".into(),
                            terms: vec![Term::Var(1), Term::Var(2)],
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn compiles_ancestor() {
        let compiled = ancestor_rules().compile(&edb()).unwrap();
        assert_eq!(compiled.rules.len(), 2);
        assert_eq!(compiled.rules[0].idb_positions, Vec::<usize>::new());
        assert_eq!(compiled.rules[1].idb_positions, vec![1]);
        assert_eq!(compiled.idb_schemas["ancestor"], vec!["x", "y"]);
    }

    #[test]
    fn rejects_bad_programs() {
        // Unknown EDB relation.
        let mut p = ancestor_rules();
        p.rules[0].body[0].relation = "nope".into();
        assert!(p.compile(&edb()).is_err());

        // Head variable not bound in the body.
        let mut p = ancestor_rules();
        p.rules[0].head.terms[1] = Term::Var(1);
        p.rules[0].body[0].terms[1] = Term::Lit(7);
        assert!(p.compile(&edb()).is_err());

        // Inconsistent IDB arity.
        let mut p = ancestor_rules();
        p.rules[1].head.terms.push(Term::Var(1));
        assert!(p.compile(&edb()).is_err());

        // Reserved prefix.
        let mut p = ancestor_rules();
        p.rules[0].head.relation = "__delta_ancestor".into();
        assert!(p.compile(&edb()).is_err());
    }
}
