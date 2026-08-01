// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Fixpoint evaluation of Datalog programs — recursive rules run until
//! nothing new can be derived (the least fixpoint; in Coln terms, the
//! initial model).
//!
//! Two strategies over the same machinery:
//!
//! - [`semi_naive`] — the real evaluator. Round 1 evaluates every rule
//!   once; every later round evaluates, per rule and per IDB body atom,
//!   a rewritten body in which that atom reads only the **delta** (the
//!   rows that were new in the previous round). Facts derived twice are
//!   removed by set difference, so work per round shrinks with the delta.
//! - [`naive`] — the test oracle. Re-evaluates every rule against the
//!   full totals every round. Correct by inspection, wasteful by design.
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

use std::collections::BTreeMap;

use anyhow::Result;

use crate::query::{Catalog, Query};
use crate::relation::Relation;
use crate::rule::{CompiledProgram, Program, delta_name};

/// A query executor, e.g. `generic_join::execute` or
/// `binary_join::execute`.
pub type Exec = fn(&Query, &Catalog) -> Result<Relation>;

#[derive(Clone, Debug)]
pub struct FixpointStats {
    /// Number of evaluation rounds, including the final one that derives
    /// nothing new.
    pub rounds: usize,
    /// New facts per round, summed over all IDB relations.
    pub new_facts_per_round: Vec<usize>,
}

impl FixpointStats {
    pub fn total_new_facts(&self) -> usize {
        self.new_facts_per_round.iter().sum()
    }
}

pub struct FixpointResult {
    /// The input EDB plus the final IDB relations — ready for follow-up
    /// queries.
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
    let compiled = program.compile(edb)?;

    // Initial totals: existing facts for IDB relations count as already
    // derived; otherwise start empty.
    let mut totals: BTreeMap<String, Relation> = BTreeMap::new();
    for (name, col_names) in &compiled.idb_schemas {
        let rel = match edb.get(name) {
            Ok(initial) => initial.clone().sorted_dedup(),
            Err(_) => Relation::new(
                name.clone(),
                col_names.clone(),
                vec![Vec::new(); col_names.len()],
            ),
        };
        totals.insert(name.clone(), rel);
    }

    let mut work = edb.clone();
    for rel in totals.values() {
        work.insert(rel.clone());
    }

    let mut stats = FixpointStats {
        rounds: 0,
        new_facts_per_round: Vec::new(),
    };

    // Round 1 is always a full (naive) evaluation: it fires the
    // non-recursive rules and folds in any initial IDB facts.
    let staging = derive_full(&compiled, &work, exec)?;
    let mut deltas = merge_round(&compiled, &mut totals, staging, &mut stats);

    // TODO(perf): every round re-runs the executors, which rebuild their
    // sorted indexes over the growing totals. Persistent indexes from the
    // storage layer behind `SortedTable` remove this rebuild.
    while deltas.values().any(|d| !d.is_empty()) {
        // Publish the previous round's state.
        for rel in totals.values() {
            work.insert(rel.clone());
        }
        if semi {
            for (name, delta) in &deltas {
                let mut rel = delta.clone();
                rel.name = delta_name(name);
                work.insert(rel);
            }
        }

        let staging = if semi {
            derive_from_deltas(&compiled, &work, exec)?
        } else {
            derive_full(&compiled, &work, exec)?
        };
        deltas = merge_round(&compiled, &mut totals, staging, &mut stats);
    }

    let mut catalog = edb.clone();
    for rel in totals.values() {
        catalog.insert(rel.clone());
    }
    Ok(FixpointResult { catalog, stats })
}

/// Evaluate every rule against the current totals.
fn derive_full(
    compiled: &CompiledProgram,
    work: &Catalog,
    exec: Exec,
) -> Result<BTreeMap<String, Relation>> {
    let mut staging = empty_staging(compiled);
    for rule in &compiled.rules {
        let result = exec(&rule.query, work)?;
        accumulate(
            &mut staging,
            compiled,
            rule.materialize_head(&result, cols(compiled, rule)),
        );
    }
    Ok(staging)
}

/// Evaluate, per rule and per IDB body atom, the delta-rewritten body.
fn derive_from_deltas(
    compiled: &CompiledProgram,
    work: &Catalog,
    exec: Exec,
) -> Result<BTreeMap<String, Relation>> {
    let mut staging = empty_staging(compiled);
    for rule in &compiled.rules {
        for &position in &rule.idb_positions {
            let query = rule.query_with_delta(position);
            let result = exec(&query, work)?;
            accumulate(
                &mut staging,
                compiled,
                rule.materialize_head(&result, cols(compiled, rule)),
            );
        }
    }
    Ok(staging)
}

fn cols<'a>(compiled: &'a CompiledProgram, rule: &crate::rule::LoweredRule) -> &'a [String] {
    &compiled.idb_schemas[&rule.head_relation]
}

fn empty_staging(compiled: &CompiledProgram) -> BTreeMap<String, Relation> {
    compiled
        .idb_schemas
        .iter()
        .map(|(name, col_names)| {
            (
                name.clone(),
                Relation::new(
                    name.clone(),
                    col_names.clone(),
                    vec![Vec::new(); col_names.len()],
                ),
            )
        })
        .collect()
}

fn accumulate(
    staging: &mut BTreeMap<String, Relation>,
    _compiled: &CompiledProgram,
    derived: Relation,
) {
    let entry = staging
        .get_mut(&derived.name)
        .expect("head relation is a known IDB relation");
    *entry = entry.union(&derived);
}

/// Fold one round of derivations into the totals; returns the new deltas
/// and updates the statistics.
fn merge_round(
    compiled: &CompiledProgram,
    totals: &mut BTreeMap<String, Relation>,
    staging: BTreeMap<String, Relation>,
    stats: &mut FixpointStats,
) -> BTreeMap<String, Relation> {
    let mut deltas = BTreeMap::new();
    let mut new_facts = 0;
    for name in compiled.idb_schemas.keys() {
        let total = totals.get_mut(name).expect("totals cover all IDB");
        let delta = staging[name].minus(total);
        new_facts += delta.len();
        *total = total.union(&delta);
        deltas.insert(name.clone(), delta);
    }
    stats.rounds += 1;
    stats.new_facts_per_round.push(new_facts);
    deltas
}
