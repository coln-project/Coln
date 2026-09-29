// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Test machinery shared by the test suites: hand-built inputs, random
//! inputs of every shape, and a definition of what queries and programs
//! compute that does not depend on the engine.
//!
//! The definition works on decoded values and uses no engine code. It tries
//! every combination of stored rows, multiplies weights as `i128`, and
//! finds recursion by a transitive closure instead of Tarjan's algorithm.
//! It is slow on purpose and short enough to check by eye.

#![allow(dead_code)] // every test crate uses a different part of it

use std::collections::{BTreeMap, BTreeSet};

use coln_batch::fixpoint::Exec;
use coln_batch::query::{Atom, Catalog, Query, Term};
use coln_batch::relation::Relation;
use coln_batch::rng::SplitMix64;
use coln_batch::rule::{Program, Rule};
use coln_batch::types::{Column, Dictionary, ScalarType, Schema, Value, Weight};
use coln_batch::{binary_join, generic_join, reference};

// ---------------------------------------------------------------------------
// Hand-built inputs

pub const EXECUTORS: [(&str, Exec); 3] = [
    ("oracle", reference::execute as Exec),
    ("binary join", binary_join::execute as Exec),
    ("generic join", generic_join::execute as Exec),
];

pub fn atom(relation: &str, terms: Vec<Term>) -> Atom {
    Atom {
        relation: relation.into(),
        terms,
    }
}

/// `head(vars) ← body(vars), …` over the uint variables `0..`, weight 1.
pub fn rule(head: (&str, &[usize]), body: &[(&str, &[usize])]) -> Rule {
    let vars = |vars: &[usize]| vars.iter().map(|&v| Term::Var(v)).collect();
    let num_vars = body
        .iter()
        .flat_map(|(_, vars)| vars.iter())
        .max()
        .map_or(0, |&v| v + 1);
    Rule {
        var_names: (0..num_vars).map(|v| format!("v{v}")).collect(),
        head: atom(head.0, vars(head.1)),
        body: body.iter().map(|&(r, v)| atom(r, vars(v))).collect(),
        weight: 1,
    }
}

/// `rule` with another weight.
pub fn weighing(weight: Weight, rule: Rule) -> Rule {
    Rule { weight, ..rule }
}

/// A relation of uint columns `c0, c1, …` holding `rows` as given: in that
/// order, with those weights.
pub fn stored(name: &str, arity: usize, rows: &[(&[u64], Weight)]) -> Relation {
    let cols = (0..arity)
        .map(|c| rows.iter().map(|(row, _)| row[c]).collect())
        .collect();
    let schema = Schema::uint((0..arity).map(|c| format!("c{c}")));
    Relation::with_weights(name, schema, cols, rows.iter().map(|&(_, w)| w).collect())
}

/// The rows of a relation of uint columns with their weights, in order.
pub fn entries(rel: &Relation) -> Vec<(Vec<u64>, Weight)> {
    (0..rel.len())
        .map(|i| (rel.row(i), rel.weight(i)))
        .collect()
}

// ---------------------------------------------------------------------------
// Z-sets of decoded rows

/// Decoded rows as stored, with their weights: a row may appear several
/// times, and a weight may be 0 or negative.
pub type Rows = Vec<(Vec<Value>, i128)>;

/// A Z-set of decoded rows: every row once, no weight 0.
pub type ZSet = BTreeMap<Vec<Value>, i128>;

/// The rows of a relation as stored, decoded.
pub fn stored_rows(rel: &Relation, dict: &Dictionary) -> Rows {
    (0..rel.len())
        .map(|i| (rel.row_values(i, dict).unwrap(), i128::from(rel.weight(i))))
        .collect()
}

/// Add up the weights of equal rows and drop the rows that cancel out.
fn zset(rows: impl IntoIterator<Item = (Vec<Value>, i128)>) -> ZSet {
    let mut z = ZSet::new();
    for (row, weight) in rows {
        *z.entry(row).or_insert(0) += weight;
    }
    z.retain(|_, weight| *weight != 0);
    z
}

/// A relation's content as a Z-set, whatever form it is stored in.
pub fn zset_of(rel: &Relation, dict: &Dictionary) -> ZSet {
    zset(stored_rows(rel, dict))
}

/// The rows of positive weight, each with weight 1.
pub fn support(z: &ZSet) -> ZSet {
    z.iter()
        .filter(|(_, w)| **w > 0)
        .map(|(row, _)| (row.clone(), 1))
        .collect()
}

fn plus(a: &ZSet, b: &ZSet) -> ZSet {
    zset(a.iter().chain(b).map(|(row, w)| (row.clone(), *w)))
}

fn as_rows(z: &ZSet) -> Rows {
    z.iter().map(|(row, w)| (row.clone(), *w)).collect()
}

// ---------------------------------------------------------------------------
// The definition of queries and rules

/// Every binding of `num_vars` variables that one stored row per atom
/// produces, with the product of those rows' weights. A literal matches an
/// equal value; a variable matches the same value wherever it occurs.
fn bindings(
    atoms: &[Atom],
    num_vars: usize,
    stored: &BTreeMap<String, Rows>,
) -> Vec<(Vec<Value>, i128)> {
    fn extend(
        atoms: &[Atom],
        stored: &BTreeMap<String, Rows>,
        binding: &mut Vec<Option<Value>>,
        weight: i128,
        out: &mut Vec<(Vec<Value>, i128)>,
    ) {
        let Some((atom, rest)) = atoms.split_first() else {
            let values = binding
                .iter()
                .map(|v| v.clone().expect("every variable occurs in an atom"))
                .collect();
            out.push((values, weight));
            return;
        };
        for (row, w) in &stored[&atom.relation] {
            let before = binding.clone();
            let fits = atom.terms.iter().zip(row).all(|(term, value)| match term {
                Term::Lit(literal) => literal == value,
                Term::Var(v) => {
                    if binding[*v].is_none() {
                        binding[*v] = Some(value.clone());
                    }
                    binding[*v].as_ref() == Some(value)
                }
            });
            if fits {
                extend(rest, stored, binding, weight * w, out);
            }
            *binding = before;
        }
    }
    let mut out = Vec::new();
    extend(atoms, stored, &mut vec![None; num_vars], 1, &mut out);
    out
}

/// What a query computes, by definition: every binding of its body adds
/// its weight to the head row it projects to.
pub fn query_definition(query: &Query, stored: &BTreeMap<String, Rows>) -> ZSet {
    zset(
        bindings(&query.atoms, query.num_vars(), stored)
            .into_iter()
            .map(|(binding, w)| (query.head.iter().map(|&v| binding[v].clone()).collect(), w)),
    )
}

/// What a rule derives, by definition: every binding of its body adds its
/// weight, times the rule's, to the head row it fills in.
fn rule_definition(rule: &Rule, stored: &BTreeMap<String, Rows>) -> ZSet {
    let head = |binding: &[Value]| -> Vec<Value> {
        rule.head
            .terms
            .iter()
            .map(|term| match term {
                Term::Var(v) => binding[*v].clone(),
                Term::Lit(literal) => literal.clone(),
            })
            .collect()
    };
    zset(
        bindings(&rule.body, rule.var_names.len(), stored)
            .into_iter()
            .map(|(binding, w)| (head(&binding), w * i128::from(rule.weight))),
    )
}

// ---------------------------------------------------------------------------
// The definition of programs

/// What the engine must make of a program.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The value of every derived relation.
    Derived(BTreeMap<String, ZSet>),
    /// A refusal, with a phrase its message contains.
    Refused(&'static str),
}

/// The derived relations that read themselves, directly or through other
/// derived relations: the recursive ones.
pub fn recursive_relations(program: &Program) -> BTreeSet<String> {
    let reaches = reaches(program);
    reaches
        .iter()
        .filter(|(name, read)| read.contains(*name))
        .map(|(name, _)| name.clone())
        .collect()
}

/// For every derived relation, the derived relations it reads directly or
/// through others, by repeated extension until nothing changes.
fn reaches(program: &Program) -> BTreeMap<String, BTreeSet<String>> {
    let derived: BTreeSet<&str> = program
        .rules
        .iter()
        .map(|r| r.head.relation.as_str())
        .collect();
    let mut reaches: BTreeMap<String, BTreeSet<String>> = derived
        .iter()
        .map(|&head| {
            let read = program
                .rules
                .iter()
                .filter(|r| r.head.relation == head)
                .flat_map(|r| &r.body)
                .map(|atom| atom.relation.clone())
                .filter(|name| derived.contains(name.as_str()))
                .collect();
            (head.to_owned(), read)
        })
        .collect();
    loop {
        let mut changed = false;
        for name in derived.iter() {
            let via: BTreeSet<String> = reaches[*name]
                .iter()
                .flat_map(|other| reaches[other].iter().cloned())
                .collect();
            let entry = reaches.get_mut(*name).expect("every derived relation");
            for other in via {
                changed |= entry.insert(other);
            }
        }
        if !changed {
            return reaches;
        }
    }
}

/// What a program computes, by definition (see `coln_batch::rule`). A
/// relation that reaches itself belongs to a group of relations that
/// reach each other; the group is a set, computed by applying its rules
/// until nothing changes, and it must neither subtract nor read rows of
/// negative weight. Every other derived relation is its initial facts plus
/// what its rules derive, reduced to a set if declared distinct. `stored`
/// holds the input, initial facts of derived relations included.
pub fn program_definition(program: &Program, stored: &BTreeMap<String, Rows>) -> Outcome {
    if program.rules.iter().any(|r| r.weight == 0) {
        return Outcome::Refused("weight 0");
    }
    let reaches = reaches(program);
    if program
        .distinct
        .iter()
        .any(|name| !reaches.contains_key(name))
    {
        return Outcome::Refused("declared distinct");
    }
    let same_group =
        |a: &str, b: &str| a == b || (reaches[a].contains(b) && reaches[b].contains(a));
    let initial = |name: &str| zset(stored.get(name).cloned().unwrap_or_default());
    let rules_of = |name: &str| {
        program
            .rules
            .iter()
            .filter(|r| r.head.relation == name)
            .collect::<Vec<_>>()
    };

    let mut values: BTreeMap<String, ZSet> = BTreeMap::new();
    let mut pending: Vec<String> = reaches.keys().cloned().collect();
    while !pending.is_empty() {
        // A group is ready once everything it reads outside itself is known.
        let ready = pending
            .iter()
            .find(|name| {
                reaches[*name]
                    .iter()
                    .all(|other| same_group(name, other) || values.contains_key(other))
            })
            .expect("the groups read each other without cycles")
            .clone();
        let group: Vec<String> = pending
            .iter()
            .filter(|name| same_group(&ready, name))
            .cloned()
            .collect();
        pending.retain(|name| !group.contains(name));

        let mut input = stored.clone();
        for (name, value) in &values {
            input.insert(name.clone(), as_rows(value));
        }

        if !reaches[&ready].contains(&ready) {
            let mut value = initial(&ready);
            for rule in rules_of(&ready) {
                value = plus(&value, &rule_definition(rule, &input));
            }
            if program.distinct.contains(&ready) {
                value = support(&value);
            }
            values.insert(ready, value);
            continue;
        }

        // A recursion computes sets: it may neither subtract nor read a row
        // of negative weight, from outside or among its initial facts.
        let group_rules: Vec<&Rule> = group.iter().flat_map(|name| rules_of(name)).collect();
        let negative = |z: ZSet| z.values().any(|w| *w < 0);
        let reads_negative = group_rules
            .iter()
            .flat_map(|r| &r.body)
            .filter(|atom| !group.contains(&atom.relation))
            .any(|atom| negative(zset(input[&atom.relation].clone())));
        if group_rules.iter().any(|r| r.weight < 0)
            || reads_negative
            || group.iter().any(|name| negative(initial(name)))
        {
            return Outcome::Refused("recursion computes sets");
        }

        let mut current: BTreeMap<String, ZSet> = group
            .iter()
            .map(|name| (name.clone(), support(&initial(name))))
            .collect();
        loop {
            let mut state = input.clone();
            for (name, value) in &current {
                state.insert(name.clone(), as_rows(value));
            }
            let next: BTreeMap<String, ZSet> = group
                .iter()
                .map(|name| {
                    let mut value = initial(name);
                    for rule in rules_of(name) {
                        value = plus(&value, &rule_definition(rule, &state));
                    }
                    (name.clone(), support(&value))
                })
                .collect();
            if next == current {
                break;
            }
            current = next;
        }
        values.extend(current);
    }
    Outcome::Derived(values)
}

// ---------------------------------------------------------------------------
// Random inputs

const TYPES: [ScalarType; 5] = [
    ScalarType::Uint,
    ScalarType::Iint,
    ScalarType::String,
    ScalarType::Bool,
    ScalarType::Char,
];

pub fn pick<T: Copy>(rng: &mut SplitMix64, items: &[T]) -> T {
    items[rng.below(items.len() as u64) as usize]
}

/// A random value of `ty`: mostly from a domain small enough that equal
/// values meet often, now and then the type's extremes, the empty string
/// or a non-ASCII one.
fn value(rng: &mut SplitMix64, ty: ScalarType) -> Value {
    let rare = rng.below(8);
    match ty {
        ScalarType::Uint => match rare {
            0 => Value::Uint(u64::MAX),
            _ => Value::Uint(rng.below(4)),
        },
        ScalarType::Iint => match rare {
            0 => Value::Iint(i64::MIN),
            1 => Value::Iint(i64::MAX),
            _ => Value::Iint(rng.below(4) as i64 - 2),
        },
        ScalarType::Bool => Value::Bool(rng.below(2) == 1),
        ScalarType::Char => match rare {
            0 => Value::Char(char::MAX),
            1 => Value::Char('\0'),
            _ => Value::Char(pick(rng, &['a', 'b', 'ß'])),
        },
        ScalarType::String => match rare {
            0 => Value::from(""),
            1 => Value::from("日本語 \u{1F600}"),
            _ => Value::from(pick(rng, &["ant", "bee", "cat"])),
        },
    }
}

/// A value for a literal: like [`value`], but a string is sometimes one
/// no relation holds.
fn literal(rng: &mut SplitMix64, ty: ScalarType) -> Value {
    if ty == ScalarType::String && rng.below(10) == 0 {
        Value::from("nowhere")
    } else {
        value(rng, ty)
    }
}

/// A random weight. Signed weights include negative ones and, for rows
/// stored as they come, 0.
fn weight(rng: &mut SplitMix64, signed: bool) -> Weight {
    if signed {
        pick(rng, &[1, 1, 2, 3, 5, -1, -2, 0])
    } else {
        pick(rng, &[1, 1, 2, 3, 5])
    }
}

/// A schema of `arity` columns of random types. With `uint_first` the
/// first column is `uint`, so a rule head can always take a variable.
pub fn schema(rng: &mut SplitMix64, arity: usize, uint_first: bool) -> Schema {
    Schema::new((0..arity).map(|c| {
        let ty = if c == 0 && uint_first {
            ScalarType::Uint
        } else {
            pick(rng, &TYPES)
        };
        Column::new(format!("c{c}"), ty)
    }))
}

/// Up to `max_rows` random rows of `schema` with weights.
pub fn rows(
    rng: &mut SplitMix64,
    schema: &Schema,
    max_rows: u64,
    signed: bool,
) -> Vec<(Vec<Value>, Weight)> {
    (0..rng.below(max_rows + 1))
        .map(|_| {
            let row = schema
                .types()
                .into_iter()
                .map(|ty| value(rng, ty))
                .collect();
            (row, weight(rng, signed))
        })
        .collect()
}

/// How a relation is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    /// In normal form: sorted, every row once, no weight 0.
    Consolidated,
    /// As it comes: unsorted, and some rows split into several copies whose
    /// weights add up to the row's, weight 0 included.
    Copies,
    /// Every row of weight `w` as `|w|` copies of weight 1 or -1, unsorted.
    Units,
}

/// Store `rows` in `cat` under `name`, in the given form. Zero weights only
/// survive in [`Storage::Copies`].
pub fn store(
    rng: &mut SplitMix64,
    cat: &mut Catalog,
    name: &str,
    schema: &Schema,
    rows: Vec<(Vec<Value>, Weight)>,
    storage: Storage,
) {
    let rel = match storage {
        Storage::Consolidated => {
            Relation::from_weighted_rows(name, schema.clone(), rows, cat.dictionary_mut())
                .unwrap()
                .consolidate()
        }
        Storage::Copies => {
            let mut copies = Vec::new();
            for (row, w) in rows {
                if rng.below(3) == 0 {
                    let part = pick(rng, &[-2, -1, 1, 3]);
                    copies.push((row.clone(), w - part));
                    copies.push((row, part));
                } else {
                    copies.push((row, w));
                }
            }
            shuffle(rng, &mut copies);
            Relation::from_weighted_rows(name, schema.clone(), copies, cat.dictionary_mut())
                .unwrap()
        }
        Storage::Units => {
            let mut units = Vec::new();
            for (row, w) in rows {
                for _ in 0..w.unsigned_abs() {
                    units.push((row.clone(), w.signum()));
                }
            }
            shuffle(rng, &mut units);
            Relation::from_weighted_rows(name, schema.clone(), units, cat.dictionary_mut()).unwrap()
        }
    };
    cat.insert(rel);
}

pub fn shuffle<T>(rng: &mut SplitMix64, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        items.swap(i, rng.below(i as u64 + 1) as usize);
    }
}

/// The decoded content of every relation in `cat`, as stored.
pub fn stored_catalog(cat: &Catalog) -> BTreeMap<String, Rows> {
    cat.names()
        .into_iter()
        .map(|name| {
            let rows = stored_rows(cat.get(name).unwrap(), cat.dictionary());
            (name.to_owned(), rows)
        })
        .collect()
}

/// A random catalog of 1..=4 relations of arity 0..=3 with up to 8 weighted
/// rows each, every relation stored in a random form. Returns the
/// relations' names and schemas too.
pub fn catalog(rng: &mut SplitMix64, signed: bool) -> (Catalog, Vec<(String, Schema)>) {
    let mut cat = Catalog::new();
    let mut rels = Vec::new();
    for r in 0..1 + rng.below(4) {
        let arity = rng.below(4) as usize;
        let schema = schema(rng, arity, false);
        let rows = rows(rng, &schema, 8, signed);
        let storage = pick(
            rng,
            &[Storage::Consolidated, Storage::Copies, Storage::Units],
        );
        let name = format!("R{r}");
        store(rng, &mut cat, &name, &schema, rows, storage);
        rels.push((name, schema));
    }
    (cat, rels)
}

/// Terms for an atom over `schema`: a variable of the column's type from
/// `pool` where there is one (one time in four a literal instead).
fn terms(rng: &mut SplitMix64, schema: &Schema, pool: &[ScalarType]) -> Vec<Term> {
    schema
        .types()
        .into_iter()
        .map(|ty| {
            let candidates: Vec<usize> = (0..pool.len()).filter(|&v| pool[v] == ty).collect();
            if candidates.is_empty() || rng.below(4) == 0 {
                Term::Lit(literal(rng, ty))
            } else {
                Term::Var(pick(rng, &candidates))
            }
        })
        .collect()
}

/// Renumber the variables of `atoms` densely in order of first occurrence,
/// as queries require. Returns the type of every new variable.
fn renumber(atoms: &mut [Atom], pool: &[ScalarType]) -> Vec<ScalarType> {
    let mut remap: Vec<Option<usize>> = vec![None; pool.len()];
    let mut types = Vec::new();
    for atom in atoms {
        for term in &mut atom.terms {
            if let Term::Var(v) = term {
                let id = *remap[*v].get_or_insert_with(|| {
                    types.push(pool[*v]);
                    types.len() - 1
                });
                *term = Term::Var(id);
            }
        }
    }
    types
}

/// A random query over `rels`: 1..=4 atoms, nullary ones included, a pool
/// of up to 4 typed variables, and a head of 1..=3 variables in any order,
/// repeats allowed. `None` if every atom came out nullary.
pub fn query(rng: &mut SplitMix64, rels: &[(String, Schema)]) -> Option<Query> {
    let columns: Vec<ScalarType> = rels.iter().flat_map(|(_, s)| s.types()).collect();
    if columns.is_empty() {
        return None;
    }
    let pool: Vec<ScalarType> = (0..1 + rng.below(4)).map(|_| pick(rng, &columns)).collect();
    let mut atoms: Vec<Atom> = (0..1 + rng.below(4))
        .map(|_| {
            let (name, schema) = &rels[rng.below(rels.len() as u64) as usize];
            Atom {
                relation: name.clone(),
                terms: terms(rng, schema, &pool),
            }
        })
        .collect();
    let mut types = renumber(&mut atoms, &pool);
    if types.is_empty() {
        // Only literals: turn the first column there is into a variable.
        let (atom, schema) = atoms.iter_mut().find_map(|atom| {
            let schema = &rels.iter().find(|(name, _)| *name == atom.relation)?.1;
            (schema.arity() > 0).then_some((atom, schema))
        })?;
        atom.terms[0] = Term::Var(0);
        types.push(schema.column_type(0));
    }
    let head = (0..1 + rng.below(3))
        .map(|_| rng.below(types.len() as u64) as usize)
        .collect();
    Some(Query {
        var_names: (0..types.len()).map(|v| format!("v{v}")).collect(),
        atoms,
        head,
    })
}

/// A random program with its input.
pub struct ProgramCase {
    pub edb: Catalog,
    pub program: Program,
    /// The derived relations.
    pub derived: Vec<String>,
}

/// A random program: 1..=2 stored relations, 1..=3 derived ones, 1..=5
/// rules whose bodies mix both, rule weights from {1, 2, -1}, and derived
/// relations declared distinct now and then. Stored rows carry
/// positive weights, or with `signed` any weights; some are stored as
/// copies, and some derived relations start from initial facts. Every
/// derived relation is declared in the catalog, as a plan would.
pub fn program_case(rng: &mut SplitMix64, signed: bool) -> ProgramCase {
    let mut edb = Catalog::new();
    let mut stored: Vec<(String, Schema)> = Vec::new();
    for r in 0..1 + rng.below(2) {
        let arity = 1 + rng.below(2) as usize;
        let schema = schema(rng, arity, true);
        let rows = rows(rng, &schema, 6, signed);
        let storage = pick(
            rng,
            &[Storage::Consolidated, Storage::Copies, Storage::Units],
        );
        let name = format!("E{r}");
        store(rng, &mut edb, &name, &schema, rows, storage);
        stored.push((name, schema));
    }
    let derived: Vec<(String, Schema)> = (0..1 + rng.below(3))
        .map(|d| {
            let arity = 1 + rng.below(2) as usize;
            (format!("D{d}"), schema(rng, arity, true))
        })
        .collect();
    for (name, schema) in &derived {
        let initial = if rng.below(4) == 0 {
            let signed = signed && rng.below(2) == 0;
            rows(rng, schema, 3, signed)
        } else {
            Vec::new()
        };
        store(rng, &mut edb, name, schema, initial, Storage::Consolidated);
    }

    let all: Vec<&(String, Schema)> = stored.iter().chain(&derived).collect();
    let n_rules = derived.len().max(1 + rng.below(5) as usize);
    let mut rules = Vec::new();
    for r in 0..n_rules {
        let head = r % derived.len();
        let (head_name, head_schema) = &derived[head];
        // A body atom reads a stored relation, a derived one defined before
        // the head (which layers strata), or any derived one (which may make
        // a recursion), in the ratio 3 : 2 : 1.
        let reads = |rng: &mut SplitMix64| -> &(String, Schema) {
            match rng.below(6) {
                0..3 => &stored[rng.below(stored.len() as u64) as usize],
                3..5 if head > 0 => &derived[rng.below(head as u64) as usize],
                _ => &derived[rng.below(derived.len() as u64) as usize],
            }
        };
        let pool: Vec<ScalarType> = (0..1 + rng.below(3))
            .map(|_| {
                let (_, s) = all[rng.below(all.len() as u64) as usize];
                s.column_type(rng.below(s.arity() as u64) as usize)
            })
            .collect();
        let mut body: Vec<Atom> = (0..1 + rng.below(3))
            .map(|_| {
                let (name, s) = reads(rng);
                Atom {
                    relation: name.clone(),
                    terms: terms(rng, s, &pool),
                }
            })
            .collect();
        let mut types = renumber(&mut body, &pool);
        // Every relation starts with a uint column; make sure the body binds
        // a uint variable for the head's first column.
        if !types.contains(&ScalarType::Uint) {
            body[0].terms[0] = Term::Var(types.len());
            types.push(ScalarType::Uint);
        }
        let head_terms = head_schema
            .types()
            .into_iter()
            .enumerate()
            .map(|(c, ty)| {
                let candidates: Vec<usize> = (0..types.len()).filter(|&v| types[v] == ty).collect();
                if candidates.is_empty() || (c > 0 && rng.below(5) == 0) {
                    Term::Lit(value(rng, ty))
                } else {
                    Term::Var(pick(rng, &candidates))
                }
            })
            .collect();
        rules.push(Rule {
            var_names: (0..types.len()).map(|v| format!("v{v}")).collect(),
            head: Atom {
                relation: head_name.clone(),
                terms: head_terms,
            },
            body,
            weight: pick(rng, &[1, 1, 1, 2, -1]),
        });
    }
    let mut program = Program::new(rules);
    for (name, _) in &derived {
        if rng.below(4) == 0 {
            program.distinct.insert(name.clone());
        }
    }
    ProgramCase {
        edb,
        program,
        derived: derived.into_iter().map(|(name, _)| name).collect(),
    }
}
