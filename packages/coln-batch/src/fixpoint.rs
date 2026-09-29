// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fixpoint evaluation of Datalog programs — recursive rules run until
//! nothing new can be derived (the least fixpoint; in Coln terms, the
//! initial model).
//!
//! A program is evaluated stratum by stratum, in dependency order (see
//! [`crate::rule`] for the semantics).
//! A non-recursive stratum runs its rules once, with their weights. A
//! recursive stratum iterates to a fixpoint over sets: every round ends
//! with a Z-set `distinct`, so a fact derived twice is still one fact and
//! the iteration stops on cycles. Such a fixpoint cannot take rows away, so
//! nothing it reads, initial facts included, may hold a row of negative
//! net weight (the sum over the row's stored copies).
//!
//! Two strategies for the recursive strata, over the same machinery:
//!
//! - [`semi_naive`] — the real evaluator. Round 1 evaluates every rule of
//!   the stratum once; every later round evaluates, per rule and per body
//!   atom of the same stratum, a rewritten body in which that atom reads
//!   only the **delta** (the facts that were new in the previous round).
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
use crate::types::Schema;

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

    /// The facts the input holds for a derived relation; empty if none.
    fn initial(&self, relation: &str) -> Result<Relation> {
        if self.edb.schema(relation).is_err() {
            return Ok(Relation::empty(relation, self.schema(relation).clone()));
        }
        read(self.edb, relation)
    }

    /// A non-recursive stratum: its one relation is its initial facts plus
    /// what its rules derive, reduced to a set if it is declared distinct.
    fn once(&self, local: &mut Catalog, stats: &mut FixpointStats) -> Result<()> {
        let [relation] = self.stratum.relations.as_slice() else {
            unreachable!("a non-recursive stratum holds one relation");
        };
        let derived = self.derive(&Layered::new(local, self.edb), false)?;
        let mut total = self.initial(relation)?.plus(&derived[relation]);
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
        let outside: BTreeSet<&str> = self
            .rules()
            .flat_map(|rule| &rule.query.atoms)
            .map(|atom| atom.relation.as_str())
            .filter(|name| !self.stratum.relations.iter().any(|r| r == name))
            .collect();
        for name in outside {
            self.refuse_negative(&read(&Layered::new(local, self.edb), name)?)?;
        }
        // Initial facts count as already derived.
        let mut totals: BTreeMap<String, Relation> = BTreeMap::new();
        for relation in &self.stratum.relations {
            let initial = self.initial(relation)?;
            self.refuse_negative(&initial)?;
            totals.insert(relation.clone(), initial.distinct());
        }

        // Round 1 is always a full (naive) evaluation: it fires the rules
        // that read nothing of this stratum and folds in the initial facts.
        // TODO(perf): every round re-runs the executors, which ask for
        // sorted tables over the growing totals again. Persistent indexes
        // behind `SortedTable` would remove this rebuild.
        let mut deltas: Option<BTreeMap<String, Relation>> = None;
        loop {
            // Publish the previous round's state.
            for rel in totals.values() {
                local.insert(rel.clone());
            }
            let from_deltas = semi && deltas.is_some();
            if from_deltas {
                for (name, delta) in deltas.iter().flatten() {
                    let mut rel = delta.clone();
                    rel.name = delta_name(name);
                    local.insert(rel);
                }
            }
            let staging = self.derive(&Layered::new(local, self.edb), from_deltas)?;
            let new = self.merge_round(&mut totals, staging, stats)?;
            if new.values().all(Relation::is_empty) {
                break;
            }
            deltas = Some(new);
        }
        for rel in totals.into_values() {
            local.insert(rel);
        }
        Ok(())
    }

    /// A fixpoint over sets cannot take rows away: refuse an input that
    /// holds a row of negative weight.
    fn refuse_negative(&self, input: &Relation) -> Result<()> {
        if let Some(&weight) = input.weights.iter().find(|&&w| w < 0) {
            bail!(
                "{} holds a row of weight {weight} and feeds the recursion over {}; \
                 recursion computes sets and cannot take rows away",
                input.name,
                self.stratum.relations.join(", ")
            );
        }
        Ok(())
    }

    /// Evaluate the stratum's rules against `work`, every rule once, or with
    /// `from_deltas` once per body atom of this stratum, that atom reading
    /// the delta.
    fn derive(&self, work: &dyn Tables, from_deltas: bool) -> Result<BTreeMap<String, Relation>> {
        let mut staging: BTreeMap<String, Relation> = self
            .stratum
            .relations
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    Relation::empty(name.clone(), self.schema(name).clone()),
                )
            })
            .collect();
        for rule in self.rules() {
            let queries = if from_deltas {
                rule.recursive_positions
                    .iter()
                    .map(|&position| rule.query_with_delta(position))
                    .collect()
            } else {
                vec![rule.query.clone()]
            };
            for query in &queries {
                let result = (self.exec)(query, work)?;
                let derived = rule.materialize_head(&result, self.schema(&rule.head_relation));
                let entry = staging
                    .get_mut(&rule.head_relation)
                    .expect("the rule's head belongs to the stratum");
                *entry = entry.plus(&derived);
            }
        }
        Ok(staging)
    }

    /// Fold one round of derivations into the totals; returns the new
    /// deltas and updates the statistics.
    ///
    /// A recursion over sets only grows, and a round adds exactly its delta.
    /// A lost fact, which takes a table that changes while it is read,
    /// stops the evaluation with an error; the second property is asserted.
    /// Either way a broken round fails instead of making the iteration
    /// endless.
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

/// `relation` as `tables` serves it, in normal form: copies of a row add
/// up to its net weight.
fn read(tables: &dyn Tables, relation: &str) -> Result<Relation> {
    let schema = tables.schema(relation)?.clone();
    let identity: Vec<usize> = (0..schema.arity()).collect();
    let table = tables.sorted(relation, &identity)?;
    Ok(Relation::from_table(relation, schema, &*table)?.consolidate())
}
