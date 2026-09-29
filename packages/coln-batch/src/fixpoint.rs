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
//! The input can be any [`Tables`] source: an in-memory [`Catalog`] or a
//! storage layer serving its own sorted tables ([`semi_naive_over`]). The
//! derived relations live in a catalog of their own, layered on top of the
//! input, so the input is not copied. The one exception: facts the input
//! already holds for a derived relation seed that relation's totals.
//!
//! Known limitation, deliberate for now: an in-memory catalog sorts a copy
//! per request, so long fixpoints re-sort the growing totals every round.
//! Persistent, incrementally maintained indexes behind the `SortedTable`
//! trait would remove that.

use std::collections::BTreeMap;

use anyhow::Result;

use crate::query::{Catalog, Layered, Query, Tables};
use crate::relation::Relation;
use crate::rule::{CompiledProgram, Program, delta_name};
use crate::types::Schema;

/// A query executor, e.g. `generic_join::execute` or
/// `binary_join::execute`.
pub type Exec = fn(&Query, &dyn Tables) -> Result<Relation>;

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

    // Initial totals: facts the input already holds for a derived relation
    // count as already derived; otherwise start empty.
    let mut totals: BTreeMap<String, Relation> = BTreeMap::new();
    for (name, schema) in &compiled.idb_schemas {
        let rel = if edb.schema(name).is_ok() {
            let identity: Vec<usize> = (0..schema.arity()).collect();
            let initial = edb.sorted(name, &identity)?;
            Relation::from_table(name.clone(), schema.clone(), &*initial)?.sorted_dedup()
        } else {
            Relation::empty(name.clone(), schema.clone())
        };
        totals.insert(name.clone(), rel);
    }
    for rel in totals.values() {
        local.insert(rel.clone());
    }

    let mut stats = FixpointStats {
        rounds: 0,
        new_facts_per_round: Vec::new(),
    };

    // Round 1 is always a full (naive) evaluation: it fires the
    // non-recursive rules and folds in any initial IDB facts.
    let staging = derive_full(&compiled, &Layered::new(&local, edb), exec)?;
    let mut deltas = merge_round(&compiled, &mut totals, staging, &mut stats);

    // TODO(perf): every round re-runs the executors, which ask for sorted
    // tables over the growing totals again. Persistent indexes behind
    // `SortedTable` would remove this rebuild.
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

        let work = Layered::new(&local, edb);
        let staging = if semi {
            derive_from_deltas(&compiled, &work, exec)?
        } else {
            derive_full(&compiled, &work, exec)?
        };
        deltas = merge_round(&compiled, &mut totals, staging, &mut stats);
    }

    let mut catalog = Catalog::new();
    *catalog.dictionary_mut() = local.dictionary().clone();
    for rel in totals.into_values() {
        catalog.insert(rel);
    }
    Ok(FixpointResult { catalog, stats })
}

/// Evaluate every rule against the current totals.
fn derive_full(
    compiled: &CompiledProgram,
    work: &dyn Tables,
    exec: Exec,
) -> Result<BTreeMap<String, Relation>> {
    let mut staging = empty_staging(compiled);
    for rule in &compiled.rules {
        let result = exec(&rule.query, work)?;
        accumulate(
            &mut staging,
            rule.materialize_head(&result, schema(compiled, rule)),
        );
    }
    Ok(staging)
}

/// Evaluate, per rule and per IDB body atom, the delta-rewritten body.
fn derive_from_deltas(
    compiled: &CompiledProgram,
    work: &dyn Tables,
    exec: Exec,
) -> Result<BTreeMap<String, Relation>> {
    let mut staging = empty_staging(compiled);
    for rule in &compiled.rules {
        for &position in &rule.idb_positions {
            let query = rule.query_with_delta(position);
            let result = exec(&query, work)?;
            accumulate(
                &mut staging,
                rule.materialize_head(&result, schema(compiled, rule)),
            );
        }
    }
    Ok(staging)
}

fn schema<'a>(compiled: &'a CompiledProgram, rule: &crate::rule::LoweredRule) -> &'a Schema {
    &compiled.idb_schemas[&rule.head_relation]
}

fn empty_staging(compiled: &CompiledProgram) -> BTreeMap<String, Relation> {
    compiled
        .idb_schemas
        .iter()
        .map(|(name, schema)| (name.clone(), Relation::empty(name.clone(), schema.clone())))
        .collect()
}

fn accumulate(staging: &mut BTreeMap<String, Relation>, derived: Relation) {
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
