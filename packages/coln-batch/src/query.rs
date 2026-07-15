// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Conjunctive queries as data.
//!
//! A [`Query`] is the engine's executable form of one rule body: a list of
//! atoms over named relations, sharing variables. The shape deliberately
//! mirrors FLIR (`Prop::Atom` ↔ [`Atom`], FLIR `Term` ↔ [`Term`]) without
//! depending on it — FLIR is still stabilizing, so the adapter comes later
//! and stays mechanical.
//!
//! Example — the triangle query `Q(x,y,z) ← R_f(x,y), R_g(y,z), R_h(z,x)`
//! is three atoms over two-column relations with variables x=0, y=1, z=2
//! and head `[x, y, z]`. See [`crate::fixtures`] for ready-made instances.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};

use crate::relation::Relation;

/// A query variable, identified by its index. For the worst-case-optimal
/// executor the variable numbering doubles as the elimination order (a
/// planner that picks good orders comes later).
pub type VarId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Term {
    Var(VarId),
    Lit(u64),
}

/// One occurrence of a relation in the query body. `terms` has one entry
/// per column of the relation, in schema order.
#[derive(Clone, Debug)]
pub struct Atom {
    pub relation: String,
    pub terms: Vec<Term>,
}

#[derive(Clone, Debug)]
pub struct Query {
    /// Variable names, indexed by [`VarId`]. Length = number of variables.
    pub var_names: Vec<String>,
    pub atoms: Vec<Atom>,
    /// Projection: the variables the result contains, in output order.
    /// Must be non-empty.
    pub head: Vec<VarId>,
}

impl Query {
    pub fn num_vars(&self) -> usize {
        self.var_names.len()
    }

    /// Column names of the result relation, derived from the head.
    pub fn head_names(&self) -> Vec<String> {
        self.head
            .iter()
            .map(|&v| self.var_names[v].clone())
            .collect()
    }

    /// Structural sanity checks, independent of any data.
    pub fn validate(&self) -> Result<()> {
        if self.atoms.is_empty() {
            bail!("query has no atoms");
        }
        if self.head.is_empty() {
            bail!("query head is empty (boolean queries are not supported yet)");
        }
        let mut seen = vec![false; self.num_vars()];
        for atom in &self.atoms {
            for term in &atom.terms {
                if let Term::Var(v) = term {
                    if *v >= self.num_vars() {
                        bail!("atom over {} uses unknown variable {v}", atom.relation);
                    }
                    seen[*v] = true;
                }
            }
        }
        if let Some(v) = seen.iter().position(|s| !s) {
            bail!("variable {} ({}) appears in no atom", v, self.var_names[v]);
        }
        for &v in &self.head {
            if v >= self.num_vars() {
                bail!("head uses unknown variable {v}");
            }
        }
        Ok(())
    }
}

/// The data a query runs against: relations, addressed by name.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    map: HashMap<String, Relation>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a relation under its own name, replacing any previous one.
    pub fn insert(&mut self, rel: Relation) {
        self.map.insert(rel.name.clone(), rel);
    }

    pub fn get(&self, name: &str) -> Result<&Relation> {
        self.map
            .get(name)
            .with_context(|| format!("catalog has no relation named {name}"))
    }

    /// Validate a query against this catalog: structure, relation
    /// existence, and arity agreement.
    pub fn check(&self, query: &Query) -> Result<()> {
        query.validate()?;
        for atom in &query.atoms {
            let rel = self.get(&atom.relation)?;
            if rel.arity() != atom.terms.len() {
                bail!(
                    "atom over {} has {} terms, relation has arity {}",
                    atom.relation,
                    atom.terms.len(),
                    rel.arity()
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(name: &str) -> Relation {
        Relation::new(name, ["a", "b"], vec![vec![1], vec![2]])
    }

    #[test]
    fn validate_rejects_malformed_queries() {
        let ok = Query {
            var_names: vec!["x".into(), "y".into()],
            atoms: vec![Atom {
                relation: "R".into(),
                terms: vec![Term::Var(0), Term::Var(1)],
            }],
            head: vec![0],
        };
        assert!(ok.validate().is_ok());

        let mut no_atoms = ok.clone();
        no_atoms.atoms.clear();
        assert!(no_atoms.validate().is_err());

        let mut empty_head = ok.clone();
        empty_head.head.clear();
        assert!(empty_head.validate().is_err());

        let mut unknown_var = ok.clone();
        unknown_var.atoms[0].terms[0] = Term::Var(7);
        assert!(unknown_var.validate().is_err());

        let mut unused_var = ok.clone();
        unused_var.atoms[0].terms[1] = Term::Var(0);
        assert!(unused_var.validate().is_err(), "y appears in no atom");
    }

    #[test]
    fn catalog_checks_existence_and_arity() {
        let mut cat = Catalog::new();
        cat.insert(rel("R"));
        let q = Query {
            var_names: vec!["x".into()],
            atoms: vec![Atom {
                relation: "R".into(),
                terms: vec![Term::Var(0), Term::Lit(2)],
            }],
            head: vec![0],
        };
        assert!(cat.check(&q).is_ok());

        let mut wrong_name = q.clone();
        wrong_name.atoms[0].relation = "S".into();
        assert!(cat.check(&wrong_name).is_err());

        let mut wrong_arity = q.clone();
        wrong_arity.atoms[0].terms.pop();
        assert!(cat.check(&wrong_arity).is_err());
    }
}
