// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fixpoint evaluation of Datalog programs — recursive rules run until
//! nothing new can be derived (the least fixpoint; in Coln terms, the
//! initial model).
//!
//! A program is evaluated stratum by stratum, in the order
//! [`Program::compile`] puts them (see [`crate::rule`] for the semantics):
//!
//! - A **non-recursive** stratum holds one relation. Its rules run once
//!   over complete inputs, and the relation is its initial facts plus the
//!   rows every rule derives, with their weights (a rule of weight -1
//!   subtracts). If the relation is declared distinct, it keeps every row
//!   of positive weight once.
//! - A **recursive** stratum is a fixpoint over sets. Every round ends with
//!   a Z-set `distinct`, so a row derived twice is still one fact and the
//!   iteration stops on cycles. For the same reason its inputs, the
//!   relations its rules read from outside it and its initial facts, must
//!   not hold a row of negative weight: a fixpoint over sets cannot take
//!   rows away. What counts is a row's net weight, the sum over its stored
//!   copies; a row whose copies cancel out is absent.
//!
//! Two strategies for the recursive strata, over the same machinery:
//!
//! - [`semi_naive`] — the real evaluator. Round 1 evaluates every rule of
//!   the stratum once; every later round evaluates, per rule and per body
//!   atom of the same stratum, a rewritten body in which that atom reads
//!   only the **delta** (the rows that were new in the previous round).
//!   Facts derived again are not new, so work per round shrinks with the
//!   delta.
//! - [`naive`] — the test oracle. Re-evaluates every rule of the stratum
//!   against the full totals every round. Correct by inspection, wasteful
//!   by design.
//!
//! Rule bodies are executed by one of the crate's query executors (the
//! [`Exec`] parameter), so recursion composes with both the hash-join
//! chain and the worst-case-optimal generic join — and the differential
//! tests run all combinations.
//!
//! The input can be any [`Tables`] source: an in-memory [`Catalog`] or a
//! storage layer serving its own sorted tables ([`semi_naive_over`]). The
//! derived relations live in a catalog of their own, layered on top of the
//! input, so the input is not copied. The one exception: facts the input
//! already holds for a derived relation seed that relation.
//!
//! Known limitation, deliberate for now: an in-memory catalog sorts a copy
//! per request, so long fixpoints re-sort the growing totals every round.
//! Persistent, incrementally maintained indexes behind the `SortedTable`
//! trait would remove that.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use crate::query::{Catalog, Layered, Query, Tables};
use crate::relation::Relation;
use crate::rule::{CompiledProgram, LoweredRule, Program, Stratum, delta_name};
use crate::types::{Schema, add_weights};

/// A query executor, e.g. `generic_join::execute` or
/// `binary_join::execute`.
pub type Exec = fn(&Query, &dyn Tables) -> Result<Relation>;

#[derive(Clone, Debug, Default)]
pub struct FixpointStats {
    /// Number of evaluation rounds over all strata: one per non-recursive
    /// stratum, and for every recursive stratum its rounds including the
    /// final one that derives nothing new.
    pub rounds: usize,
    /// Rows added per round, summed over the relations of the stratum: all
    /// rows of a non-recursive relation, the new facts of a recursive
    /// round.
    pub new_facts_per_round: Vec<usize>,
}

impl FixpointStats {
    pub fn total_new_facts(&self) -> usize {
        self.new_facts_per_round.iter().sum()
    }
}

pub struct FixpointResult {
    /// The final IDB relations with the dictionary that decodes them (it
    /// also holds the program's string literals). [`semi_naive`] and
    /// [`naive`] add the input relations, ready for follow-up queries;
    /// [`semi_naive_over`] leaves the input where it is.
    pub catalog: Catalog,
    pub stats: FixpointStats,
}

/// Evaluate `program` over `edb` with semi-naive iteration.
pub fn semi_naive(program: &Program, edb: &Catalog, exec: Exec) -> Result<FixpointResult> {
    with_input(edb, evaluate(program, edb, exec, true)?)
}

/// Evaluate `program` over `edb` by naive re-evaluation (test oracle).
pub fn naive(program: &Program, edb: &Catalog, exec: Exec) -> Result<FixpointResult> {
    with_input(edb, evaluate(program, edb, exec, false)?)
}

/// Evaluate `program` with semi-naive iteration over input tables from any
/// [`Tables`] source, such as a storage layer's sorted tables. The input is
/// read in place (only facts it holds for a derived relation are copied, as
/// that relation's start), and the result holds the derived relations
/// only.
pub fn semi_naive_over(program: &Program, edb: &dyn Tables, exec: Exec) -> Result<FixpointResult> {
    evaluate(program, edb, exec, true)
}

/// Put the input relations under the derived ones, as the catalog-based
/// entry points promise.
fn with_input(edb: &Catalog, derived: FixpointResult) -> Result<FixpointResult> {
    let mut catalog = edb.clone();
    *catalog.dictionary_mut() = derived.catalog.dictionary().clone();
    for name in derived.catalog.names() {
        catalog.insert(derived.catalog.get(name)?.clone());
    }
    Ok(FixpointResult {
        catalog,
        stats: derived.stats,
    })
}

fn evaluate(program: &Program, edb: &dyn Tables, exec: Exec, semi: bool) -> Result<FixpointResult> {
    // The derived relations and, for semi-naive rounds, their deltas live
    // in `local`, layered on top of the input. Its dictionary starts as a
    // copy of the input's, so every input key decodes the same way;
    // compiling adds the program's literals.
    let mut local = Catalog::new();
    *local.dictionary_mut() = edb.dictionary().clone();
    let compiled = program.compile(edb, local.dictionary_mut())?;

    let mut stats = FixpointStats::default();
    for stratum in &compiled.strata {
        let eval = StratumEval {
            compiled: &compiled,
            stratum,
            edb,
            exec,
        };
        if stratum.recursive {
            eval.fixpoint(&mut local, semi, &mut stats)?;
        } else {
            eval.once(&mut local, &mut stats)?;
        }
    }

    let mut catalog = Catalog::new();
    *catalog.dictionary_mut() = local.dictionary().clone();
    for name in compiled.idb_schemas.keys() {
        catalog.insert(local.get(name)?.clone());
    }
    Ok(FixpointResult { catalog, stats })
}

/// One stratum under evaluation, with what every step needs.
struct StratumEval<'a> {
    compiled: &'a CompiledProgram,
    stratum: &'a Stratum,
    edb: &'a dyn Tables,
    exec: Exec,
}

impl StratumEval<'_> {
    fn rules(&self) -> impl Iterator<Item = &LoweredRule> {
        self.stratum.rules.iter().map(|&i| &self.compiled.rules[i])
    }

    fn schema(&self, relation: &str) -> &Schema {
        &self.compiled.idb_schemas[relation]
    }

    /// The facts the input holds for a derived relation, in normal form;
    /// empty if it holds none.
    fn initial(&self, relation: &str) -> Result<Relation> {
        let schema = self.schema(relation).clone();
        if self.edb.schema(relation).is_err() {
            return Ok(Relation::empty(relation, schema));
        }
        let identity: Vec<usize> = (0..schema.arity()).collect();
        let table = self.edb.sorted(relation, &identity)?;
        Ok(Relation::from_table(relation, schema, &*table)?.consolidate())
    }

    /// A non-recursive stratum: its one relation is its initial facts plus
    /// what every rule derives, weights and all, reduced to a set if it is
    /// declared distinct.
    fn once(&self, local: &mut Catalog, stats: &mut FixpointStats) -> Result<()> {
        let [relation] = self.stratum.relations.as_slice() else {
            unreachable!("a non-recursive stratum holds one relation");
        };
        let mut total = self.initial(relation)?;
        let work = Layered::new(local, self.edb);
        for rule in self.rules() {
            let result = (self.exec)(&rule.query, &work)?;
            total = total.plus(&rule.materialize_head(&result, self.schema(relation)));
        }
        if self.compiled.distinct.contains(relation) {
            total = total.distinct();
        }
        stats.rounds += 1;
        stats.new_facts_per_round.push(total.len());
        local.insert(total);
        Ok(())
    }

    /// A recursive stratum: iterate its rules to the least fixpoint over
    /// sets, semi-naively or naively.
    fn fixpoint(&self, local: &mut Catalog, semi: bool, stats: &mut FixpointStats) -> Result<()> {
        self.check_inputs(&Layered::new(local, self.edb))?;

        // Initial facts count as already derived.
        let mut totals: BTreeMap<String, Relation> = BTreeMap::new();
        for relation in &self.stratum.relations {
            let initial = self.initial(relation)?;
            if let Some(&weight) = initial.weights.iter().find(|&&w| w < 0) {
                bail!(
                    "the initial facts of the recursive relation {relation} hold a row of \
                     weight {weight}; recursion computes sets and cannot take rows away"
                );
            }
            totals.insert(relation.clone(), initial.distinct());
        }
        for rel in totals.values() {
            local.insert(rel.clone());
        }

        // Round 1 is always a full (naive) evaluation: it fires the rules
        // that read nothing of this stratum and folds in the initial facts.
        let staging = self.derive_full(&Layered::new(local, self.edb))?;
        let mut deltas = self.merge_round(&mut totals, staging, stats)?;

        // TODO(perf): every round re-runs the executors, which ask for
        // sorted tables over the growing totals again. Persistent indexes
        // behind `SortedTable` would remove this rebuild.
        while deltas.values().any(|d| !d.is_empty()) {
            // Publish the previous round's state.
            for rel in totals.values() {
                local.insert(rel.clone());
            }
            if semi {
                for (name, delta) in &deltas {
                    let mut rel = delta.clone();
                    rel.name = delta_name(name);
                    local.insert(rel);
                }
            }

            let work = Layered::new(local, self.edb);
            let staging = if semi {
                self.derive_from_deltas(&work)?
            } else {
                self.derive_full(&work)?
            };
            deltas = self.merge_round(&mut totals, staging, stats)?;
        }

        for rel in totals.into_values() {
            local.insert(rel);
        }
        Ok(())
    }

    /// No relation a recursive stratum reads from outside itself may hold
    /// a row of negative net weight, like its initial facts. A table may
    /// hold a row in several copies; sorted by all columns, the copies are
    /// adjacent, and their weights add up to the row's net weight.
    fn check_inputs(&self, work: &dyn Tables) -> Result<()> {
        let inputs: BTreeSet<&str> = self
            .rules()
            .flat_map(|rule| &rule.query.atoms)
            .map(|atom| atom.relation.as_str())
            .filter(|name| !self.stratum.relations.iter().any(|r| r == name))
            .collect();
        for name in inputs {
            let arity = work.schema(name)?.arity();
            let identity: Vec<usize> = (0..arity).collect();
            // TODO(perf): an in-memory catalog sorts a copy just to read the
            // weights.
            let table = work.sorted(name, &identity)?;
            let mut start = 0;
            while start < table.len() {
                let same =
                    |r: usize| (0..arity).all(|c| table.value(r, c) == table.value(start, c));
                let end = (start + 1..table.len())
                    .find(|&r| !same(r))
                    .unwrap_or(table.len());
                let net = (start..end).map(|r| table.weight(r)).fold(0, add_weights);
                if net < 0 {
                    bail!(
                        "{name} holds a row of weight {net} and is read by the recursive \
                         relations {}; recursion computes sets and cannot take rows away",
                        self.stratum.relations.join(", ")
                    );
                }
                start = end;
            }
        }
        Ok(())
    }

    /// Evaluate every rule of the stratum against the current totals.
    fn derive_full(&self, work: &dyn Tables) -> Result<BTreeMap<String, Relation>> {
        let mut staging = self.empty_staging();
        for rule in self.rules() {
            let result = (self.exec)(&rule.query, work)?;
            accumulate(
                &mut staging,
                rule.materialize_head(&result, self.schema(&rule.head_relation)),
            );
        }
        Ok(staging)
    }

    /// Evaluate, per rule and per body atom of this stratum, the
    /// delta-rewritten body.
    fn derive_from_deltas(&self, work: &dyn Tables) -> Result<BTreeMap<String, Relation>> {
        let mut staging = self.empty_staging();
        for rule in self.rules() {
            for &position in &rule.recursive_positions {
                let query = rule.query_with_delta(position);
                let result = (self.exec)(&query, work)?;
                accumulate(
                    &mut staging,
                    rule.materialize_head(&result, self.schema(&rule.head_relation)),
                );
            }
        }
        Ok(staging)
    }

    fn empty_staging(&self) -> BTreeMap<String, Relation> {
        self.stratum
            .relations
            .iter()
            .map(|name| {
                let schema = self.schema(name).clone();
                (name.clone(), Relation::empty(name.clone(), schema))
            })
            .collect()
    }

    /// Fold one round of derivations into the totals; returns the new
    /// deltas and updates the statistics.
    ///
    /// A recursion over sets only grows. Should a round lose a fact
    /// anyway, which takes a table that changes its weights while it is
    /// read, the evaluation stops with an error rather than oscillate
    /// forever. And a round adds exactly its delta to the totals; an
    /// assertion checks that too, so that a fault in the Z-set operations
    /// fails fast instead of making the iteration endless.
    fn merge_round(
        &self,
        totals: &mut BTreeMap<String, Relation>,
        staging: BTreeMap<String, Relation>,
        stats: &mut FixpointStats,
    ) -> Result<BTreeMap<String, Relation>> {
        let mut deltas = BTreeMap::new();
        let mut new_facts = 0;
        for name in &self.stratum.relations {
            let total = totals.get_mut(name).expect("totals cover the stratum");
            // The facts known after this round are a set again, and the
            // delta is what the round added to them.
            let next = total.plus(&staging[name]).distinct();
            let delta = next.minus(total);
            if delta.weights.iter().any(|&w| w < 0) {
                bail!(
                    "the recursive relation {name} lost a fact in round {}; a recursion \
                     over sets can only grow, so an input changed while it was read",
                    stats.rounds + 1
                );
            }
            assert_eq!(
                next.len(),
                total.len() + delta.len(),
                "a round must add exactly its new facts to {name}"
            );
            new_facts += delta.len();
            *total = next;
            deltas.insert(name.clone(), delta);
        }
        stats.rounds += 1;
        stats.new_facts_per_round.push(new_facts);
        Ok(deltas)
    }
}

fn accumulate(staging: &mut BTreeMap<String, Relation>, derived: Relation) {
    let entry = staging
        .get_mut(&derived.name)
        .expect("head relation belongs to the stratum");
    *entry = entry.plus(&derived);
}
