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
//! What a derived relation holds follows Z-set semantics: its initial facts
//! plus the rows every rule derives, each rule's rows weighted as the
//! executors weigh a query result (see [`crate::generic_join`]). Two rules
//! deriving the same row give it weight 2, and so do two ways of deriving
//! it in one rule. A rule of weight -1 takes its rows away instead, which
//! makes a Z-set difference, and a relation declared in
//! [`Program::distinct`] keeps every row of positive weight once.
//! **Recursion is the exception.** A relation that depends on itself,
//! directly or through others, is a set: every row it holds has weight 1,
//! which is what makes the fixpoint finite on cycles. Its rules cannot
//! take rows away.
//!
//! The derived relations fall into **strata**, the groups of relations
//! that depend on each other. [`Program::compile`] orders them so that
//! every stratum comes after the strata it reads; a stratum is recursive
//! if its relations depend on themselves.
//!
//! Derived relations are typed like stored ones. Their schemas come from
//! the catalog when initial facts exist (an empty relation with a schema
//! is a plain declaration), otherwise they are inferred from the rules:
//! a head variable has the type of the body columns it stands in, a head
//! literal its own type. Inference iterates until every derived column is
//! typed; a column no rule pins down is an error, resolved by declaring
//! the relation in the catalog.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use crate::query::{Atom, Query, Tables, Term, infer_var_types};
use crate::relation::Relation;
use crate::types::{Column, Dictionary, Key, ScalarType, Schema, Weight, mul_weights};

#[derive(Clone, Debug)]
pub struct Rule {
    /// Variable names; indices are the rule's variable ids. The numbering
    /// doubles as the generic join's elimination order for the body.
    pub var_names: Vec<String>,
    pub head: Atom,
    pub body: Vec<Atom>,
    /// What the rule's rows count for in the head relation: the weight of
    /// every row the body derives is multiplied by it. 1 adds the rows, -1
    /// takes them away. Never 0, and positive in a recursive relation.
    pub weight: Weight,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub rules: Vec<Rule>,
    /// Derived relations that are sets: once their rules are added up,
    /// every row of positive weight is kept once and every other row
    /// dropped (the Z-set `distinct`). Recursive relations are sets anyway.
    pub distinct: BTreeSet<String>,
}

impl Program {
    /// A program of `rules`, with no relation declared distinct.
    pub fn new(rules: Vec<Rule>) -> Self {
        Self {
            rules,
            distinct: BTreeSet::new(),
        }
    }
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
    /// The rule's weight, see [`Rule::weight`].
    pub weight: Weight,
    /// Body positions whose relation belongs to the rule's own stratum. In
    /// a recursive stratum these are the atoms that read the previous
    /// round's delta; in a non-recursive one there are none.
    pub recursive_positions: Vec<usize>,
}

impl LoweredRule {
    /// The body query with the IDB atom at `position` redirected to its
    /// delta relation (semi-naive rewriting).
    pub fn query_with_delta(&self, position: usize) -> Query {
        let mut q = self.query.clone();
        q.atoms[position].relation = delta_name(&q.atoms[position].relation);
        q
    }

    /// Turn a body-query result into rows of the head relation, their
    /// weights multiplied by the rule's. The rows come out in normal form.
    pub fn materialize_head(&self, result: &Relation, schema: &Schema) -> Relation {
        let cols = self
            .head_cols
            .iter()
            .map(|hc| match hc {
                HeadCol::Var(k) => result.cols[*k].clone(),
                HeadCol::Lit(x) => vec![*x; result.len()],
            })
            .collect();
        let weights = result
            .weights
            .iter()
            .map(|&w| mul_weights(w, self.weight))
            .collect();
        Relation::with_weights(self.head_relation.clone(), schema.clone(), cols, weights)
            .consolidate()
    }
}

/// A group of derived relations that depend on each other, evaluated
/// together.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Stratum {
    /// The relations, sorted by name.
    pub relations: Vec<String>,
    /// Whether the relations depend on themselves. A recursive stratum is
    /// computed as a fixpoint over sets; a non-recursive one holds a
    /// single relation, evaluated once with its weights.
    pub recursive: bool,
    /// Positions in [`CompiledProgram::rules`] of the rules defining the
    /// relations.
    pub rules: Vec<usize>,
}

/// A validated, lowered program, ready for fixpoint evaluation.
#[derive(Debug)]
pub(crate) struct CompiledProgram {
    pub rules: Vec<LoweredRule>,
    /// IDB relation name → schema (from initial facts if present, else
    /// column names from the first defining rule head and inferred types).
    pub idb_schemas: BTreeMap<String, Schema>,
    /// The derived relations grouped into strata, in evaluation order.
    pub strata: Vec<Stratum>,
    /// The derived relations declared distinct, see [`Program::distinct`].
    pub distinct: BTreeSet<String>,
}

/// A derived relation's schema while its types are being inferred.
struct PartialSchema {
    names: Vec<String>,
    types: Vec<Option<ScalarType>>,
}

impl Program {
    /// Validate against the input tables in `edb`, infer the derived
    /// schemas, and lower every rule. String literals are interned into
    /// `dict`, which must extend `edb`'s dictionary.
    pub(crate) fn compile(
        &self,
        edb: &dyn Tables,
        dict: &mut Dictionary,
    ) -> Result<CompiledProgram> {
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
            if rule.weight == 0 {
                bail!("a rule for {name} has weight 0, so it would derive nothing");
            }
            let arity = rule.head.terms.len();
            if let Some(existing) = partial.get(name) {
                if existing.names.len() != arity {
                    bail!("relation {name} is defined with inconsistent arities");
                }
                continue;
            }
            let schema = if let Ok(initial) = edb.schema(name) {
                if initial.arity() != arity {
                    bail!(
                        "initial facts for {name} have arity {}, head has {arity}",
                        initial.arity()
                    );
                }
                PartialSchema {
                    names: initial.names(),
                    types: initial.types().into_iter().map(Some).collect(),
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

        for name in &self.distinct {
            if !partial.contains_key(name) {
                bail!("{name} is declared distinct, but no rule derives it");
            }
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
                        Ok(edb.schema(name)?.types().into_iter().map(Some).collect())
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
                        head_cols.push(HeadCol::Lit(value.to_key(dict)));
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
                    None => edb.schema(name)?,
                };
                Ok(schema.types().into_iter().map(Some).collect())
            })?;

            // Body literals are interned so the executors find their keys.
            for atom in &query.atoms {
                for term in &atom.terms {
                    if let Term::Lit(value) = term {
                        value.to_key(dict);
                    }
                }
            }

            for atom in &rule.body {
                if atom.relation.starts_with(DELTA_PREFIX) {
                    bail!(
                        "relation name {} uses the reserved prefix {DELTA_PREFIX}",
                        atom.relation
                    );
                }
            }
            rules.push(LoweredRule {
                query,
                head_relation: rule.head.relation.clone(),
                head_cols,
                weight: rule.weight,
                recursive_positions: Vec::new(),
            });
        }

        let strata = stratify(&idb_schemas, &rules);
        for stratum in strata.iter().filter(|s| s.recursive) {
            for &i in &stratum.rules {
                let rule = &rules[i];
                if rule.weight < 0 {
                    bail!(
                        "a rule for {} has weight {}, but {} is recursive; recursion \
                         computes sets and cannot take rows away",
                        rule.head_relation,
                        rule.weight,
                        rule.head_relation
                    );
                }
            }
        }
        let stratum_of: BTreeMap<&str, usize> = strata
            .iter()
            .enumerate()
            .flat_map(|(i, s)| s.relations.iter().map(move |r| (r.as_str(), i)))
            .collect();
        for rule in &mut rules {
            let own = stratum_of[rule.head_relation.as_str()];
            rule.recursive_positions = (0..rule.query.atoms.len())
                .filter(|&i| stratum_of.get(rule.query.atoms[i].relation.as_str()) == Some(&own))
                .collect();
        }

        Ok(CompiledProgram {
            rules,
            idb_schemas,
            strata,
            distinct: self.distinct.clone(),
        })
    }
}

/// Group the derived relations into strata: the strongly connected
/// components of the graph in which every derived relation points to the
/// derived relations its rules read. Tarjan's algorithm completes a
/// component only after every component it reaches, so the strata come out
/// in evaluation order, each after the ones it reads.
fn stratify(idb: &BTreeMap<String, Schema>, rules: &[LoweredRule]) -> Vec<Stratum> {
    let mut reads: BTreeMap<&str, BTreeSet<&str>> = idb
        .keys()
        .map(|name| (name.as_str(), BTreeSet::new()))
        .collect();
    for rule in rules {
        for atom in &rule.query.atoms {
            if idb.contains_key(&atom.relation) {
                reads
                    .get_mut(rule.head_relation.as_str())
                    .expect("every head is a derived relation")
                    .insert(atom.relation.as_str());
            }
        }
    }

    let mut tarjan = Tarjan {
        reads: &reads,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        components: Vec::new(),
    };
    for &name in reads.keys() {
        if !tarjan.index.contains_key(name) {
            tarjan.visit(name);
        }
    }

    tarjan
        .components
        .into_iter()
        .map(|mut relations| {
            relations.sort_unstable();
            let recursive = relations.len() > 1 || reads[relations[0]].contains(relations[0]);
            let rules = (0..rules.len())
                .filter(|&i| relations.contains(&rules[i].head_relation.as_str()))
                .collect();
            Stratum {
                relations: relations.into_iter().map(str::to_owned).collect(),
                recursive,
                rules,
            }
        })
        .collect()
}

/// State of Tarjan's strongly connected components algorithm.
struct Tarjan<'a> {
    reads: &'a BTreeMap<&'a str, BTreeSet<&'a str>>,
    /// Visiting order of every relation seen so far.
    index: BTreeMap<&'a str, usize>,
    /// The smallest index reachable from a relation through relations on
    /// the stack.
    low: BTreeMap<&'a str, usize>,
    stack: Vec<&'a str>,
    /// Finished components, dependencies first.
    components: Vec<Vec<&'a str>>,
}

impl<'a> Tarjan<'a> {
    fn visit(&mut self, v: &'a str) {
        let index = self.index.len();
        self.index.insert(v, index);
        self.low.insert(v, index);
        self.stack.push(v);
        let reads = self.reads;
        for &w in &reads[v] {
            let reached = if !self.index.contains_key(w) {
                self.visit(w);
                self.low[w]
            } else if self.stack.contains(&w) {
                self.index[w]
            } else {
                continue;
            };
            if reached < self.low[v] {
                self.low.insert(v, reached);
            }
        }
        if self.low[v] == index {
            let root = self
                .stack
                .iter()
                .rposition(|&x| x == v)
                .expect("a relation stays on the stack until its component is done");
            let component = self.stack.split_off(root);
            self.components.push(component);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::Catalog;

    impl Program {
        /// [`Program::compile`] against a catalog, interning literals into
        /// the catalog's own dictionary.
        fn compile_in(&self, catalog: &mut Catalog) -> Result<CompiledProgram> {
            let mut dict = catalog.dictionary().clone();
            let compiled = self.compile(catalog, &mut dict);
            *catalog.dictionary_mut() = dict;
            compiled
        }
    }

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
        Program::new(vec![
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
                weight: 1,
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
                weight: 1,
            },
        ])
    }

    #[test]
    fn compiles_ancestor() {
        let compiled = ancestor_rules().compile_in(&mut edb()).unwrap();
        assert_eq!(compiled.rules.len(), 2);
        assert_eq!(compiled.rules[0].recursive_positions, Vec::<usize>::new());
        assert_eq!(compiled.rules[1].recursive_positions, vec![1]);
        assert_eq!(
            compiled.strata,
            vec![Stratum {
                relations: vec!["ancestor".into()],
                recursive: true,
                rules: vec![0, 1],
            }]
        );
        let schema = &compiled.idb_schemas["ancestor"];
        assert_eq!(schema.names(), vec!["x", "y"]);
        assert_eq!(schema.types(), vec![ScalarType::Uint, ScalarType::Uint]);
    }

    /// `head(vars) ← body(vars), …` over the variables `0..`, all uint.
    fn rule(head: (&str, &[usize]), body: &[(&str, &[usize])]) -> Rule {
        let atom = |(relation, vars): (&str, &[usize])| Atom {
            relation: relation.into(),
            terms: vars.iter().map(|&v| Term::Var(v)).collect(),
        };
        let num_vars = body
            .iter()
            .flat_map(|(_, vars)| vars.iter())
            .max()
            .map_or(0, |&v| v + 1);
        Rule {
            var_names: (0..num_vars).map(|v| format!("v{v}")).collect(),
            head: atom(head),
            body: body.iter().copied().map(atom).collect(),
            weight: 1,
        }
    }

    #[test]
    fn strata_come_in_dependency_order() {
        let program = Program::new(vec![
            // reach: recursive over parent.
            rule(("reach", &[0, 1]), &[("parent", &[0, 1])]),
            rule(
                ("reach", &[0, 2]),
                &[("reach", &[0, 1]), ("parent", &[1, 2])],
            ),
            // pairs: reads reach, but not itself.
            rule(
                ("pairs", &[0, 1]),
                &[("reach", &[0, 1]), ("reach", &[1, 0])],
            ),
            // sym: recursive over pairs.
            rule(("sym", &[0, 1]), &[("pairs", &[0, 1])]),
            rule(("sym", &[0, 2]), &[("sym", &[0, 1]), ("sym", &[1, 2])]),
            // a and b: recursive through each other.
            rule(("a", &[0]), &[("parent", &[0, 1])]),
            rule(("a", &[0]), &[("b", &[0])]),
            rule(("b", &[0]), &[("a", &[0])]),
        ]);
        let compiled = program.compile_in(&mut edb()).unwrap();
        let strata: Vec<(Vec<&str>, bool)> = compiled
            .strata
            .iter()
            .map(|s| {
                (
                    s.relations.iter().map(String::as_str).collect(),
                    s.recursive,
                )
            })
            .collect();
        assert_eq!(
            strata,
            vec![
                (vec!["a", "b"], true),
                (vec!["reach"], true),
                (vec!["pairs"], false),
                (vec!["sym"], true),
            ]
        );
        // Only atoms of the rule's own stratum read deltas.
        assert_eq!(compiled.rules[1].recursive_positions, vec![0]);
        assert_eq!(compiled.rules[2].recursive_positions, Vec::<usize>::new());
        assert_eq!(compiled.rules[4].recursive_positions, vec![0, 1]);
        assert_eq!(compiled.rules[6].recursive_positions, vec![0]);
    }

    #[test]
    fn rejects_bad_programs() {
        // Unknown EDB relation.
        let mut p = ancestor_rules();
        p.rules[0].body[0].relation = "nope".into();
        assert!(p.compile_in(&mut edb()).is_err());

        // Head variable not bound in the body.
        let mut p = ancestor_rules();
        p.rules[0].head.terms[1] = Term::Var(1);
        p.rules[0].body[0].terms[1] = Term::lit(7u64);
        assert!(p.compile_in(&mut edb()).is_err());

        // Inconsistent IDB arity.
        let mut p = ancestor_rules();
        p.rules[1].head.terms.push(Term::Var(1));
        assert!(p.compile_in(&mut edb()).is_err());

        // Reserved prefix.
        let mut p = ancestor_rules();
        p.rules[0].head.relation = "__delta_ancestor".into();
        assert!(p.compile_in(&mut edb()).is_err());

        // A rule of weight 0.
        let mut p = ancestor_rules();
        p.rules[0].weight = 0;
        let err = p.compile_in(&mut edb()).unwrap_err().to_string();
        assert!(err.contains("weight 0"), "{err}");

        // A subtracting rule of a recursive relation.
        let mut p = ancestor_rules();
        p.rules[1].weight = -1;
        let err = p.compile_in(&mut edb()).unwrap_err().to_string();
        assert!(err.contains("ancestor is recursive"), "{err}");

        // Distinct for a relation no rule derives.
        let mut p = ancestor_rules();
        p.distinct.insert("parent".into());
        let err = p.compile_in(&mut edb()).unwrap_err().to_string();
        assert!(err.contains("parent is declared distinct"), "{err}");
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
        Program::new(vec![
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
                weight: 1,
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
                weight: 1,
            },
        ])
    }

    #[test]
    fn infers_derived_types_through_recursion() {
        let compiled = labeled_reach().compile_in(&mut typed_edb()).unwrap();
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
        let compiled = program.compile_in(&mut edb).unwrap();
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
        let err = program
            .compile_in(&mut typed_edb())
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("is string but a rule derives it as uint"),
            "{err}"
        );
    }

    #[test]
    fn rejects_uninferable_columns_unless_declared() {
        // p(x) ← p(x): nothing pins down the type of p.
        let program = Program::new(vec![Rule {
            var_names: vec!["x".into()],
            head: Atom {
                relation: "p".into(),
                terms: vec![Term::Var(0)],
            },
            body: vec![Atom {
                relation: "p".into(),
                terms: vec![Term::Var(0)],
            }],
            weight: 1,
        }]);
        let err = program
            .compile_in(&mut Catalog::new())
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot infer the type"), "{err}");

        // Declared through an empty relation in the catalog, it compiles.
        let mut cat = Catalog::new();
        cat.insert(Relation::empty(
            "p",
            Schema::new([Column::new("x", ScalarType::String)]),
        ));
        let compiled = program.compile_in(&mut cat).unwrap();
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
        let compiled = labeled_reach().compile_in(&mut cat).unwrap();
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
        let err = labeled_reach()
            .compile_in(&mut cat)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("is uint but a rule derives it as string"),
            "{err}"
        );
    }
}
