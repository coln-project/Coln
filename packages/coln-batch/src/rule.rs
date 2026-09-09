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
//!
//! Derived relations are typed like stored ones. Their schemas come from
//! the catalog when initial facts exist (an empty relation with a schema
//! is a plain declaration), otherwise they are inferred from the rules:
//! a head variable has the type of the body columns it stands in, a head
//! literal its own type. Inference iterates until every derived column is
//! typed; a column no rule pins down is an error, resolved by declaring
//! the relation in the catalog.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::query::{Atom, Catalog, Query, Term, infer_var_types};
use crate::relation::Relation;
use crate::types::{Column, Key, ScalarType, Schema};

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
/// a literal, encoded to its key.
#[derive(Clone, Copy, Debug)]
pub(crate) enum HeadCol {
    Var(usize),
    Lit(Key),
}

/// A rule lowered to an executable form.
#[derive(Debug)]
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
    pub fn materialize_head(&self, result: &Relation, schema: &Schema) -> Relation {
        let cols = self
            .head_cols
            .iter()
            .map(|hc| match hc {
                HeadCol::Var(k) => result.cols[*k].clone(),
                HeadCol::Lit(x) => vec![*x; result.len()],
            })
            .collect();
        Relation::with_schema(self.head_relation.clone(), schema.clone(), cols).sorted_dedup()
    }
}

/// A validated, lowered program, ready for fixpoint evaluation.
#[derive(Debug)]
pub(crate) struct CompiledProgram {
    pub rules: Vec<LoweredRule>,
    /// IDB relation name → schema (from initial facts if present, else
    /// column names from the first defining rule head and inferred types).
    pub idb_schemas: BTreeMap<String, Schema>,
}

/// A derived relation's schema while its types are being inferred.
struct PartialSchema {
    names: Vec<String>,
    types: Vec<Option<ScalarType>>,
}

impl Program {
    /// Validate against the EDB catalog, infer the derived schemas, and
    /// lower every rule. String literals are interned into the catalog's
    /// dictionary, which is why the catalog is borrowed mutably.
    pub(crate) fn compile(&self, edb: &mut Catalog) -> Result<CompiledProgram> {
        if self.rules.is_empty() {
            bail!("program has no rules");
        }

        // Collect the derived relations: arity from the heads, column
        // names and known types from initial facts or the first
        // defining rule.
        let mut partial: BTreeMap<String, PartialSchema> = BTreeMap::new();
        for rule in &self.rules {
            let name = &rule.head.relation;
            if name.starts_with(DELTA_PREFIX) {
                bail!("relation name {name} uses the reserved prefix {DELTA_PREFIX}");
            }
            let arity = rule.head.terms.len();
            if let Some(existing) = partial.get(name) {
                if existing.names.len() != arity {
                    bail!("relation {name} is defined with inconsistent arities");
                }
                continue;
            }
            let schema = if let Ok(initial) = edb.get(name) {
                if initial.arity() != arity {
                    bail!(
                        "initial facts for {name} have arity {}, head has {arity}",
                        initial.arity()
                    );
                }
                PartialSchema {
                    names: initial.schema.names(),
                    types: initial.schema.types().into_iter().map(Some).collect(),
                }
            } else {
                PartialSchema {
                    names: rule
                        .head
                        .terms
                        .iter()
                        .enumerate()
                        .map(|(i, t)| match t {
                            Term::Var(v) => rule.var_names[*v].clone(),
                            Term::Lit(_) => format!("c{i}"),
                        })
                        .collect(),
                    types: vec![None; arity],
                }
            };
            partial.insert(name.clone(), schema);
        }

        // Infer derived column types until nothing changes. Each pass
        // types every rule body against what is known so far and pushes
        // the head's types into its relation.
        loop {
            let mut changed = false;
            for rule in &self.rules {
                let var_types = infer_var_types(rule.var_names.len(), &rule.body, |name| {
                    if let Some(p) = partial.get(name) {
                        Ok(p.types.clone())
                    } else {
                        Ok(edb
                            .get(name)?
                            .schema
                            .types()
                            .into_iter()
                            .map(Some)
                            .collect())
                    }
                })?;
                let head = partial
                    .get_mut(&rule.head.relation)
                    .expect("collected above");
                for (c, term) in rule.head.terms.iter().enumerate() {
                    let ty = match term {
                        Term::Var(v) => var_types[*v],
                        Term::Lit(value) => Some(value.scalar_type()),
                    };
                    let Some(ty) = ty else { continue };
                    match head.types[c] {
                        None => {
                            head.types[c] = Some(ty);
                            changed = true;
                        }
                        Some(known) if known != ty => bail!(
                            "column {} of {} is {known} but a rule derives it as {ty}",
                            head.names[c],
                            rule.head.relation
                        ),
                        Some(_) => {}
                    }
                }
            }
            if !changed {
                break;
            }
        }

        let mut idb_schemas: BTreeMap<String, Schema> = BTreeMap::new();
        for (name, p) in &partial {
            let columns: Vec<Column> = p
                .names
                .iter()
                .zip(&p.types)
                .map(|(col, ty)| match ty {
                    Some(ty) => Ok(Column::new(col.clone(), *ty)),
                    None => bail!(
                        "cannot infer the type of column {col} of {name}: no rule derives it \
                         from typed data; declare the relation in the catalog (an empty \
                         relation with a schema suffices)"
                    ),
                })
                .collect::<Result<_>>()?;
            idb_schemas.insert(name.clone(), Schema::new(columns));
        }

        // Lower and validate each rule against the now complete schemas.
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
                    Term::Lit(value) => {
                        head_cols.push(HeadCol::Lit(value.to_key(edb.dictionary_mut())));
                    }
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

            // Types of every body atom, derived and stored alike, are known
            // now: check the body strictly.
            infer_var_types(query.num_vars(), &query.atoms, |name| {
                let schema = match idb_schemas.get(name) {
                    Some(schema) => schema,
                    None => &edb.get(name)?.schema,
                };
                Ok(schema.types().into_iter().map(Some).collect())
            })?;

            // Body literals are interned so the executors find their keys.
            for atom in &query.atoms {
                for term in &atom.terms {
                    if let Term::Lit(value) = term {
                        value.to_key(edb.dictionary_mut());
                    }
                }
            }

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
        let compiled = ancestor_rules().compile(&mut edb()).unwrap();
        assert_eq!(compiled.rules.len(), 2);
        assert_eq!(compiled.rules[0].idb_positions, Vec::<usize>::new());
        assert_eq!(compiled.rules[1].idb_positions, vec![1]);
        let schema = &compiled.idb_schemas["ancestor"];
        assert_eq!(schema.names(), vec!["x", "y"]);
        assert_eq!(schema.types(), vec![ScalarType::Uint, ScalarType::Uint]);
    }

    #[test]
    fn rejects_bad_programs() {
        // Unknown EDB relation.
        let mut p = ancestor_rules();
        p.rules[0].body[0].relation = "nope".into();
        assert!(p.compile(&mut edb()).is_err());

        // Head variable not bound in the body.
        let mut p = ancestor_rules();
        p.rules[0].head.terms[1] = Term::Var(1);
        p.rules[0].body[0].terms[1] = Term::lit(7u64);
        assert!(p.compile(&mut edb()).is_err());

        // Inconsistent IDB arity.
        let mut p = ancestor_rules();
        p.rules[1].head.terms.push(Term::Var(1));
        assert!(p.compile(&mut edb()).is_err());

        // Reserved prefix.
        let mut p = ancestor_rules();
        p.rules[0].head.relation = "__delta_ancestor".into();
        assert!(p.compile(&mut edb()).is_err());
    }

    fn typed_edb() -> Catalog {
        let mut cat = Catalog::new();
        cat.insert_rows(
            "edge",
            Schema::new([
                Column::new("src", ScalarType::Uint),
                Column::new("dst", ScalarType::Uint),
                Column::new("label", ScalarType::String),
            ]),
            vec![vec![0u64.into(), 1u64.into(), "road".into()]],
        )
        .unwrap();
        cat
    }

    /// reach(x, y, l) ← edge(x, y, l); reach(x, z, l) ← reach(x, y, l), edge(y, z, l)
    fn labeled_reach() -> Program {
        let (x, y, z, l) = (0, 1, 2, 3);
        Program {
            rules: vec![
                Rule {
                    var_names: vec!["x".into(), "y".into(), "l".into()],
                    head: Atom {
                        relation: "reach".into(),
                        terms: vec![Term::Var(0), Term::Var(1), Term::Var(2)],
                    },
                    body: vec![Atom {
                        relation: "edge".into(),
                        terms: vec![Term::Var(0), Term::Var(1), Term::Var(2)],
                    }],
                },
                Rule {
                    var_names: vec!["x".into(), "y".into(), "z".into(), "l".into()],
                    head: Atom {
                        relation: "reach".into(),
                        terms: vec![Term::Var(x), Term::Var(z), Term::Var(l)],
                    },
                    body: vec![
                        Atom {
                            relation: "reach".into(),
                            terms: vec![Term::Var(x), Term::Var(y), Term::Var(l)],
                        },
                        Atom {
                            relation: "edge".into(),
                            terms: vec![Term::Var(y), Term::Var(z), Term::Var(l)],
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn infers_derived_types_through_recursion() {
        let compiled = labeled_reach().compile(&mut typed_edb()).unwrap();
        let schema = &compiled.idb_schemas["reach"];
        assert_eq!(schema.names(), vec!["x", "y", "l"]);
        assert_eq!(
            schema.types(),
            vec![ScalarType::Uint, ScalarType::Uint, ScalarType::String]
        );
    }

    #[test]
    fn infers_from_a_later_rule_and_interns_literals() {
        // The first rule only mentions the derived relation; its types come
        // from the second rule. The head literal "seed" enters the
        // dictionary at compile time.
        let mut program = labeled_reach();
        program.rules.swap(0, 1);
        program.rules[1].head.terms[2] = Term::lit("seed");
        let mut edb = typed_edb();
        let compiled = program.compile(&mut edb).unwrap();
        assert_eq!(
            compiled.idb_schemas["reach"].types(),
            vec![ScalarType::Uint, ScalarType::Uint, ScalarType::String]
        );
        assert!(edb.dictionary().code("seed").is_some());
        assert!(matches!(compiled.rules[1].head_cols[2], HeadCol::Lit(_)));
    }

    #[test]
    fn rejects_type_conflicts_in_heads() {
        // Second rule derives the label column as uint.
        let mut program = labeled_reach();
        program.rules[1].head.terms[2] = Term::lit(5u64);
        let err = program.compile(&mut typed_edb()).unwrap_err().to_string();
        assert!(
            err.contains("is string but a rule derives it as uint"),
            "{err}"
        );
    }

    #[test]
    fn rejects_uninferable_columns_unless_declared() {
        // p(x) ← p(x): nothing pins down the type of p.
        let program = Program {
            rules: vec![Rule {
                var_names: vec!["x".into()],
                head: Atom {
                    relation: "p".into(),
                    terms: vec![Term::Var(0)],
                },
                body: vec![Atom {
                    relation: "p".into(),
                    terms: vec![Term::Var(0)],
                }],
            }],
        };
        let err = program
            .compile(&mut Catalog::new())
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot infer the type"), "{err}");

        // Declared through an empty relation in the catalog, it compiles.
        let mut cat = Catalog::new();
        cat.insert(Relation::empty(
            "p",
            Schema::new([Column::new("x", ScalarType::String)]),
        ));
        let compiled = program.compile(&mut cat).unwrap();
        assert_eq!(compiled.idb_schemas["p"].types(), vec![ScalarType::String]);
    }

    #[test]
    fn initial_facts_fix_the_schema() {
        let mut cat = typed_edb();
        cat.insert_rows(
            "reach",
            Schema::new([
                Column::new("from", ScalarType::Uint),
                Column::new("to", ScalarType::Uint),
                Column::new("via", ScalarType::String),
            ]),
            vec![vec![7u64.into(), 8u64.into(), "rail".into()]],
        )
        .unwrap();
        let compiled = labeled_reach().compile(&mut cat).unwrap();
        assert_eq!(
            compiled.idb_schemas["reach"].names(),
            vec!["from", "to", "via"]
        );

        // Initial facts of the wrong type are a conflict.
        let mut cat = typed_edb();
        cat.insert_rows(
            "reach",
            Schema::uint(["from", "to", "via"]),
            vec![vec![7u64.into(), 8u64.into(), 9u64.into()]],
        )
        .unwrap();
        let err = labeled_reach().compile(&mut cat).unwrap_err().to_string();
        assert!(
            err.contains("is uint but a rule derives it as string"),
            "{err}"
        );
    }
}
