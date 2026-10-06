// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fixpoint evaluation of Datalog programs — recursive rules run until
//! nothing new can be derived (the least fixpoint; in Coln terms, the
//! initial model).
//!
//! A program is evaluated stratum by stratum, in dependency order (see
//! [`crate::rule`] for the semantics). A non-recursive stratum runs its
//! rules once, with their weights. A recursive stratum iterates to a
//! fixpoint over sets: every round ends with a Z-set `distinct`, so a fact
//! derived twice is still one fact and the iteration stops on cycles. Such
//! a fixpoint cannot take rows away, so nothing it reads, initial facts
//! included, may hold a row of negative net weight (the sum over the row's
//! stored copies).
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
//! Known limitation, deliberate for now: executors build their sorted
//! indexes per query, so long fixpoints re-sort the growing totals every
//! round. Persistent, incrementally maintained indexes are exactly what
//! the storage layer will provide behind the `SortedTable` trait.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use crate::query::{Catalog, Query};
use crate::relation::Relation;
use crate::rule::{CompiledProgram, LoweredRule, Program, Stratum, delta_name};
use crate::types::Schema;

/// A query executor, e.g. `generic_join::execute` or
/// `binary_join::execute`.
pub type Exec = fn(&Query, &Catalog) -> Result<Relation>;

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
    /// The input EDB plus the final IDB relations — ready for follow-up
    /// queries. Its dictionary also holds the program's string literals.
    pub catalog: Catalog,
    pub stats: FixpointStats,
}

/// Evaluate `program` over `edb` with semi-naive iteration.
pub fn semi_naive(program: &Program, edb: &Catalog, exec: Exec) -> Result<FixpointResult> {
    evaluate(program, edb, exec, true)
}

/// Evaluate `program` over `edb` by naive re-evaluation (test oracle).
pub fn naive(program: &Program, edb: &Catalog, exec: Exec) -> Result<FixpointResult> {
    evaluate(program, edb, exec, false)
}

fn evaluate(program: &Program, edb: &Catalog, exec: Exec, semi: bool) -> Result<FixpointResult> {
    // The working catalog: the EDB, the derived relations and, for
    // semi-naive rounds, their deltas. Compiling interns the program's
    // literals into its dictionary.
    let mut work = edb.clone();
    let compiled = program.compile(&mut work)?;

    let mut stats = FixpointStats::default();
    for stratum in &compiled.strata {
        let eval = StratumEval {
            compiled: &compiled,
            stratum,
            edb,
            exec,
        };
        if stratum.recursive {
            eval.fixpoint(&mut work, semi, &mut stats)?;
        } else {
            eval.once(&mut work, &mut stats)?;
        }
    }

    let mut catalog = edb.clone();
    *catalog.dictionary_mut() = work.dictionary().clone();
    for name in compiled.idb_schemas.keys() {
        catalog.insert(work.get(name)?.clone());
    }
    Ok(FixpointResult { catalog, stats })
}

/// One stratum under evaluation, with what every step needs.
struct StratumEval<'a> {
    compiled: &'a CompiledProgram,
    stratum: &'a Stratum,
    edb: &'a Catalog,
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
    /// empty if none.
    fn initial(&self, relation: &str) -> Relation {
        match self.edb.get(relation) {
            Ok(initial) => initial.clone().consolidate(),
            Err(_) => Relation::empty(relation, self.schema(relation).clone()),
        }
    }

    /// A non-recursive stratum: its one relation is its initial facts plus
    /// what its rules derive, reduced to a set if it is declared distinct.
    fn once(&self, work: &mut Catalog, stats: &mut FixpointStats) -> Result<()> {
        let [relation] = self.stratum.relations.as_slice() else {
            unreachable!("a non-recursive stratum holds one relation");
        };
        let derived = self.derive(work, false)?;
        let mut total = self.initial(relation).plus(&derived[relation]);
        if self.compiled.distinct.contains(relation) {
            total = total.distinct();
        }
        stats.rounds += 1;
        stats.new_facts_per_round.push(total.len());
        work.insert(total);
        Ok(())
    }

    /// A recursive stratum: iterate its rules to the least fixpoint over
    /// sets, semi-naively or naively.
    fn fixpoint(&self, work: &mut Catalog, semi: bool, stats: &mut FixpointStats) -> Result<()> {
        let outside: BTreeSet<&str> = self
            .rules()
            .flat_map(|rule| &rule.query.atoms)
            .map(|atom| atom.relation.as_str())
            .filter(|name| !self.stratum.relations.iter().any(|r| r == name))
            .collect();
        for name in outside {
            self.refuse_negative(&work.get(name)?.clone().consolidate())?;
        }
        // Initial facts count as already derived.
        let mut totals: BTreeMap<String, Relation> = BTreeMap::new();
        for relation in &self.stratum.relations {
            let initial = self.initial(relation);
            self.refuse_negative(&initial)?;
            totals.insert(relation.clone(), initial.distinct());
        }

        // Round 1 is always a full (naive) evaluation: it fires the rules
        // that read nothing of this stratum and folds in the initial facts.
        // TODO(perf): every round re-runs the executors, which rebuild
        // their sorted indexes over the growing totals. Persistent indexes
        // from the storage layer behind `SortedTable` remove this rebuild.
        let mut deltas: Option<BTreeMap<String, Relation>> = None;
        loop {
            // Publish the previous round's state.
            for rel in totals.values() {
                work.insert(rel.clone());
            }
            let from_deltas = semi && deltas.is_some();
            if from_deltas {
                for (name, delta) in deltas.iter().flatten() {
                    let mut rel = delta.clone();
                    rel.name = delta_name(name);
                    work.insert(rel);
                }
            }
            let staging = self.derive(work, from_deltas)?;
            let new = self.merge_round(&mut totals, staging, stats);
            if new.values().all(Relation::is_empty) {
                break;
            }
            deltas = Some(new);
        }
        for rel in totals.into_values() {
            work.insert(rel);
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
    fn derive(&self, work: &Catalog, from_deltas: bool) -> Result<BTreeMap<String, Relation>> {
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
    /// A recursion over sets only grows, so a round adds exactly its delta
    /// to the totals. The assertion makes a fault in the Z-set operations
    /// fail fast instead of making the iteration endless.
    fn merge_round(
        &self,
        totals: &mut BTreeMap<String, Relation>,
        staging: BTreeMap<String, Relation>,
        stats: &mut FixpointStats,
    ) -> BTreeMap<String, Relation> {
        let mut deltas = BTreeMap::new();
        let mut new_facts = 0;
        for name in &self.stratum.relations {
            let total = totals.get_mut(name).expect("totals cover the stratum");
            let next = total.plus(&staging[name]).distinct();
            let delta = next.minus(total);
            assert!(
                delta.weights.iter().all(|&w| w == 1) && next.len() == total.len() + delta.len(),
                "a round must add exactly its new facts to {name}"
            );
            new_facts += delta.len();
            *total = next;
            deltas.insert(name.clone(), delta);
        }
        stats.rounds += 1;
        stats.new_facts_per_round.push(new_facts);
        deltas
    }
}
