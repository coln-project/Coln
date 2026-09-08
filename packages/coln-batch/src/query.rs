// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Conjunctive queries as data.
//!
//! A [`Query`] is the engine's executable form of one rule body: a list of
//! atoms over named relations, sharing variables. The shape deliberately
//! mirrors FLIR (`Prop::Atom` ↔ [`Atom`], FLIR `Term` ↔ [`Term`]) without
//! depending on it, so the adapter from a plan stays mechanical.
//!
//! Example — the triangle query `Q(x,y,z) ← R_f(x,y), R_g(y,z), R_h(z,x)`
//! is three atoms over two-column relations with variables x=0, y=1, z=2
//! and head `[x, y, z]`. See [`crate::fixtures`] for ready-made instances.
//!
//! Queries are typed: a variable takes the type of the columns it stands
//! in, all of which must agree, and a literal must match its column.
//! [`Catalog::check`] establishes this and yields the result schema.

use std::collections::HashMap;

use anyhow::{Context, Result, bail};

use crate::relation::Relation;
use crate::types::{Column, Dictionary, Key, ScalarType, Schema, Value};

/// A query variable, identified by its index. For the worst-case-optimal
/// executor the variable numbering doubles as the elimination order
pub type VarId = usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Term {
    Var(VarId),
    Lit(Value),
}

impl Term {
    /// A literal term from any value type.
    pub fn lit(value: impl Into<Value>) -> Self {
        Term::Lit(value.into())
    }
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

/// The outcome of type-checking a query: one type per variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Typing {
    pub var_types: Vec<ScalarType>,
}

impl Typing {
    /// The schema of the query's result: the head variables with their
    /// names and types.
    pub fn head_schema(&self, query: &Query) -> Schema {
        query
            .head
            .iter()
            .map(|&v| Column::new(query.var_names[v].clone(), self.var_types[v]))
            .collect()
    }
}

/// Infer the type of every variable from the columns it stands in and
/// check literals against their columns. `schema_of` resolves a relation
/// name; a column of unknown type (`None`) constrains nothing.
pub(crate) fn infer_var_types(
    num_vars: usize,
    atoms: &[Atom],
    schema_of: impl Fn(&str) -> Result<Vec<Option<ScalarType>>>,
) -> Result<Vec<Option<ScalarType>>> {
    let mut var_types: Vec<Option<ScalarType>> = vec![None; num_vars];
    let mut bound_at: Vec<Option<(String, usize)>> = vec![None; num_vars];
    for atom in atoms {
        let types = schema_of(&atom.relation)?;
        if types.len() != atom.terms.len() {
            bail!(
                "atom over {} has {} terms, relation has arity {}",
                atom.relation,
                atom.terms.len(),
                types.len()
            );
        }
        for (c, (term, col_type)) in atom.terms.iter().zip(&types).enumerate() {
            let Some(col_type) = *col_type else {
                continue;
            };
            match term {
                Term::Lit(value) => {
                    if value.scalar_type() != col_type {
                        bail!(
                            "literal {value} has type {}, but column {c} of {} has type {col_type}",
                            value.scalar_type(),
                            atom.relation
                        );
                    }
                }
                Term::Var(v) => match var_types[*v] {
                    None => {
                        var_types[*v] = Some(col_type);
                        bound_at[*v] = Some((atom.relation.clone(), c));
                    }
                    Some(known) if known != col_type => {
                        let (rel, col) = bound_at[*v].clone().expect("recorded with the type");
                        bail!(
                            "variable {v} has type {known} from column {col} of {rel}, \
                             but column {c} of {} has type {col_type}",
                            atom.relation
                        );
                    }
                    Some(_) => {}
                },
            }
        }
    }
    Ok(var_types)
}

/// A term with its literal encoded to a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyTerm {
    Var(VarId),
    Lit(Key),
}

#[derive(Clone, Debug)]
pub(crate) struct KeyAtom {
    pub relation: String,
    pub terms: Vec<KeyTerm>,
}

/// A checked query with its literals encoded, ready to execute.
pub(crate) struct Prepared {
    /// Schema of the result.
    pub schema: Schema,
    /// The body with encoded literals, or `None` if a string literal is
    /// unknown to the dictionary: no stored row can match it, so the
    /// result is empty without looking at any data.
    pub atoms: Option<Vec<KeyAtom>>,
}

/// The data a query runs against: relations, addressed by name, sharing
/// one string [`Dictionary`].
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    map: HashMap<String, Relation>,
    dict: Dictionary,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a relation under its own name, replacing any previous one.
    /// The relation's string keys must stem from this catalog's dictionary.
    pub fn insert(&mut self, rel: Relation) {
        self.map.insert(rel.name.clone(), rel);
    }

    /// Encode typed rows into a new relation (sorted and deduplicated) and
    /// insert it.
    pub fn insert_rows(
        &mut self,
        name: impl Into<String>,
        schema: Schema,
        rows: impl IntoIterator<Item = Vec<Value>>,
    ) -> Result<()> {
        let rel = Relation::from_rows(name, schema, rows, &mut self.dict)?.sorted_dedup();
        self.insert(rel);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<&Relation> {
        self.map
            .get(name)
            .with_context(|| format!("catalog has no relation named {name}"))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    /// Names of all relations, sorted.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.map.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// The string dictionary shared by all relations.
    pub fn dictionary(&self) -> &Dictionary {
        &self.dict
    }

    pub fn dictionary_mut(&mut self) -> &mut Dictionary {
        &mut self.dict
    }

    /// Decode a relation's rows (test and display helper).
    pub fn rows_of(&self, name: &str) -> Result<Vec<Vec<Value>>> {
        let rel = self.get(name)?;
        (0..rel.len())
            .map(|i| rel.row_values(i, &self.dict))
            .collect()
    }

    /// Validate and type-check a query against this catalog: structure,
    /// relation existence, arity agreement, and consistent types.
    pub fn check(&self, query: &Query) -> Result<Typing> {
        query.validate()?;
        let var_types = infer_var_types(query.num_vars(), &query.atoms, |name| {
            Ok(self
                .get(name)?
                .schema
                .types()
                .into_iter()
                .map(Some)
                .collect())
        })?;
        let var_types = var_types
            .into_iter()
            .map(|t| t.expect("every variable occurs in some atom (validated)"))
            .collect();
        Ok(Typing { var_types })
    }

    /// Check the query and encode its literals.
    pub(crate) fn prepare(&self, query: &Query) -> Result<Prepared> {
        let typing = self.check(query)?;
        let schema = typing.head_schema(query);
        let mut atoms = Vec::with_capacity(query.atoms.len());
        for atom in &query.atoms {
            let mut terms = Vec::with_capacity(atom.terms.len());
            for term in &atom.terms {
                terms.push(match term {
                    Term::Var(v) => KeyTerm::Var(*v),
                    Term::Lit(value) => match value.key_if_known(&self.dict) {
                        Some(key) => KeyTerm::Lit(key),
                        None => {
                            return Ok(Prepared {
                                schema,
                                atoms: None,
                            });
                        }
                    },
                });
            }
            atoms.push(KeyAtom {
                relation: atom.relation.clone(),
                terms,
            });
        }
        Ok(Prepared {
            schema,
            atoms: Some(atoms),
        })
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
                terms: vec![Term::Var(0), Term::lit(2u64)],
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

    fn typed_catalog() -> Catalog {
        let mut cat = Catalog::new();
        cat.insert_rows(
            "person",
            Schema::new([
                Column::new("id", ScalarType::Uint),
                Column::new("name", ScalarType::String),
                Column::new("age", ScalarType::Iint),
            ]),
            vec![vec![1u64.into(), "ann".into(), 30i64.into()]],
        )
        .unwrap();
        cat.insert_rows(
            "likes",
            Schema::new([
                Column::new("who", ScalarType::String),
                Column::new("what", ScalarType::String),
            ]),
            vec![vec!["ann".into(), "tea".into()]],
        )
        .unwrap();
        cat
    }

    #[test]
    fn typing_follows_the_columns() {
        let cat = typed_catalog();
        // Q(id, what) ← person(id, name, 30), likes(name, what)
        let q = Query {
            var_names: vec!["id".into(), "name".into(), "what".into()],
            atoms: vec![
                Atom {
                    relation: "person".into(),
                    terms: vec![Term::Var(0), Term::Var(1), Term::lit(30i64)],
                },
                Atom {
                    relation: "likes".into(),
                    terms: vec![Term::Var(1), Term::Var(2)],
                },
            ],
            head: vec![2, 0],
        };
        let typing = cat.check(&q).unwrap();
        assert_eq!(
            typing.var_types,
            vec![ScalarType::Uint, ScalarType::String, ScalarType::String]
        );
        let schema = typing.head_schema(&q);
        assert_eq!(schema.names(), vec!["what", "id"]);
        assert_eq!(schema.types(), vec![ScalarType::String, ScalarType::Uint]);

        let prepared = cat.prepare(&q).unwrap();
        assert!(prepared.atoms.is_some());
    }

    #[test]
    fn typing_rejects_mismatches() {
        let cat = typed_catalog();
        // A variable in a string column and a uint column.
        let mixed = Query {
            var_names: vec!["x".into()],
            atoms: vec![
                Atom {
                    relation: "person".into(),
                    terms: vec![Term::Var(0), Term::Var(0), Term::lit(30i64)],
                },
                Atom {
                    relation: "likes".into(),
                    terms: vec![Term::Var(0), Term::Var(0)],
                },
            ],
            head: vec![0],
        };
        let err = cat.check(&mixed).unwrap_err().to_string();
        assert!(err.contains("has type uint"), "{err}");

        // A literal of the wrong type.
        let bad_lit = Query {
            var_names: vec!["x".into()],
            atoms: vec![Atom {
                relation: "person".into(),
                terms: vec![Term::Var(0), Term::lit(7u64), Term::lit(30i64)],
            }],
            head: vec![0],
        };
        let err = cat.check(&bad_lit).unwrap_err().to_string();
        assert!(err.contains("literal 7 has type uint"), "{err}");
    }

    #[test]
    fn unknown_string_literal_is_unsatisfiable() {
        let cat = typed_catalog();
        let q = Query {
            var_names: vec!["what".into()],
            atoms: vec![Atom {
                relation: "likes".into(),
                terms: vec![Term::lit("nobody"), Term::Var(0)],
            }],
            head: vec![0],
        };
        let prepared = cat.prepare(&q).unwrap();
        assert!(prepared.atoms.is_none());
        assert_eq!(prepared.schema.types(), vec![ScalarType::String]);
    }
}
