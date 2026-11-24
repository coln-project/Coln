// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! One realm on batch, on incremental and, where it is installed, on
//! Soufflé, fed the same commits and compared after each one.
//!
//! A realm goes the full way: its FLIR through the FLIR frontend into a plan,
//! which the batch backend restricts to the derived views ([`ViewProgram`])
//! and the incremental backend runs as a whole. Soufflé runs the frontend's
//! Datalog (see [`Souffle`]). Every view must hold the same rows on all of
//! them. The realm tests, the fuzz tests and `coln-check` all go through
//! [`Twin`].

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use anyhow::{Result, bail, ensure};
use coln_flir_rs::ir::{EntityVariant, FlatRealm};
use dbsp::typed_batch::IndexedZSetReader;

use super::BatchRuntime;
use super::souffle::Souffle;
use super::views::ViewProgram;
use crate::api::deltas::StoreDelta;
use crate::api::query::FlirProgram;
use crate::pipeline::Pipeline;
use crate::relational::Runtime;
use crate::relational::expr::{SinkId, SourceId};
use crate::relational::incremental::DbspRuntime;
use crate::relational::relation::TupleValue;
use crate::scalarial::ScalarTypedValue;

/// How many rows of each kind a difference shows.
const SHOWN_ROWS: usize = 5;

/// The backends over one realm, see the [module](self).
pub(crate) struct Twin {
    name: String,
    batch: BatchRuntime,
    incremental: DbspRuntime,
    views: Vec<SinkId>,
    /// Every view's state on the incremental backend, summed up from the
    /// changes it reports per commit.
    incremental_views: HashMap<SinkId, BTreeMap<TupleValue, i64>>,
    souffle: Option<Souffle>,
}

impl Twin {
    /// Set up `realm`, called `name` in messages. `with_souffle` asks for
    /// Soufflé as well, which takes part only if it is installed.
    pub(crate) fn new(realm: &FlatRealm, name: &str, with_souffle: bool) -> Result<Self> {
        let batch = Pipeline::batch()
            .runtime(&mut ViewProgram::new(FlirProgram::from_flat_realm(realm)?))?;
        let incremental =
            Pipeline::incremental().runtime(&mut FlirProgram::from_flat_realm(realm)?)?;

        // Batch offers exactly the views: no rule made it into its program.
        let mut views: Vec<SinkId> = batch.list_outputs().cloned().collect();
        views.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let mut expected: Vec<SinkId> = realm
            .tables
            .iter()
            .filter(|entry| matches!(entry.table.entity_variant, EntityVariant::View { .. }))
            .map(|entry| SinkId::from(entry.path.to_string()))
            .collect();
        expected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        ensure!(views == expected, "the views of {name}");
        let souffle = match with_souffle {
            true => Souffle::new(&FlirProgram::from_flat_realm(realm)?)?,
            false => None,
        };

        Ok(Self {
            name: name.to_string(),
            batch,
            incremental,
            incremental_views: views
                .iter()
                .map(|view| (view.clone(), BTreeMap::new()))
                .collect(),
            views,
            souffle,
        })
    }

    /// Whether Soufflé takes part.
    pub(crate) fn has_souffle(&self) -> bool {
        self.souffle.is_some()
    }

    /// The realm's views.
    pub(crate) fn views(&self) -> impl Iterator<Item = &str> {
        self.views.iter().map(SinkId::as_str)
    }

    /// Commit `rows` on every backend and compare every view. Returns the
    /// number of rows per view.
    pub(crate) fn commit(&mut self, rows: StoreDelta) -> Result<Vec<(String, usize)>> {
        for table in rows.into_table_deltas() {
            let source = SourceId::from(table.for_entity());
            let delta = table.into_delta();
            if let Some(souffle) = &mut self.souffle {
                souffle.feed(source.as_str(), &delta)?;
            }
            // The batch program reads only the base tables some view needs,
            // so it may not know `source`. Were a needed one missing, the
            // views would differ.
            self.batch.feed(&source, delta.clone())?;
            ensure!(
                self.incremental.feed(&source, delta)?,
                "incremental knows {source}"
            );
        }
        self.batch.commit()?;
        self.incremental.commit()?;

        let on_souffle = match &self.souffle {
            Some(souffle) => {
                let views: Vec<&str> = self.views().collect();
                Some(souffle.evaluate(&views)?)
            }
            None => None,
        };

        let mut sizes = Vec::new();
        let mut differences = String::new();
        for view in &self.views {
            // Reading an incremental output drains what changed since the
            // last read, so the sum over all commits is the view's state.
            let state = self.incremental_views.get_mut(view).expect("one per view");
            for (row, (), weight) in self
                .incremental
                .output(view)?
                .debug_view(false)
                .to_zset()
                .iter()
            {
                let entry = state.entry(row.clone()).or_insert(0);
                *entry += weight;
                if *entry == 0 {
                    state.remove(&row);
                }
            }
            // The batch backend reports a view as a set. The incremental one
            // counts how often a row of a recursive view is derived, e.g.
            // twice for a vertex on a cycle, which reaches itself directly and
            // around the cycle. So compare which rows each holds.
            if let Some((row, weight)) = state.iter().find(|(_, weight)| **weight < 0) {
                bail!(
                    "view {view} of {} holds {} {weight} times on incremental",
                    self.name,
                    show(row)
                );
            }

            let on_batch = self.batch.output(view)?;
            let batch_rows: BTreeSet<&TupleValue> = on_batch.rows().iter().collect();
            ensure!(
                batch_rows.len() == on_batch.len(),
                "view {view} repeats a row on batch"
            );
            let mut sides = vec![
                ("batch", batch_rows),
                ("incremental", state.keys().collect()),
            ];
            if let Some(on_souffle) = &on_souffle {
                sides.push(("Soufflé", on_souffle[view.as_str()].iter().collect()));
            }
            if let Some(difference) = difference(&sides) {
                write!(differences, "\n  view {view}:{difference}")?;
            }
            sizes.push((view.as_str().to_string(), on_batch.len()));
        }
        ensure!(
            differences.is_empty(),
            "the views of {} differ{differences}",
            self.name
        );
        Ok(sizes)
    }
}

/// How the rows of one view differ between `sides`, or `None` if every side
/// holds the same rows. Rows are grouped by the sides holding them, e.g.
/// "only on batch" or "missing on Soufflé".
fn difference(sides: &[(&str, BTreeSet<&TupleValue>)]) -> Option<String> {
    let all: BTreeSet<&TupleValue> = sides.iter().flat_map(|(_, rows)| rows).copied().collect();
    let mut groups: BTreeMap<Vec<bool>, Vec<&TupleValue>> = BTreeMap::new();
    for row in all {
        let held: Vec<bool> = sides.iter().map(|(_, rows)| rows.contains(row)).collect();
        if held.contains(&false) {
            groups.entry(held).or_default().push(row);
        }
    }
    if groups.is_empty() {
        return None;
    }
    let mut text = String::new();
    for (held, rows) in groups {
        let names = |on: bool| -> Vec<&str> {
            sides
                .iter()
                .zip(&held)
                .filter(|(_, held)| **held == on)
                .map(|((name, _), _)| *name)
                .collect()
        };
        let label = match (names(true).as_slice(), names(false).as_slice()) {
            ([only], _) => format!("only on {only}"),
            (_, [missing]) => format!("missing on {missing}"),
            (on, _) => format!("only on {}", on.join(" and ")),
        };
        let shown: Vec<String> = rows.iter().take(SHOWN_ROWS).map(|row| show(row)).collect();
        let more = match rows.len().saturating_sub(SHOWN_ROWS) {
            0 => String::new(),
            more => format!(" and {more} more"),
        };
        write!(
            text,
            "\n    {label} ({}): {}{more}",
            rows.len(),
            shown.join(" ")
        )
        .expect("writing to a string");
    }
    Some(text)
}

/// A row as its values in brackets, e.g. `(0 4 0 7)`.
fn show(row: &TupleValue) -> String {
    let values: Vec<String> = row
        .data
        .iter()
        .map(|value| match value {
            ScalarTypedValue::Uint(value) => value.to_string(),
            ScalarTypedValue::Iint(value) => value.to_string(),
            ScalarTypedValue::String(value) => format!("{value:?}"),
            value => format!("{value:?}"),
        })
        .collect();
    format!("({})", values.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tuple;

    #[test]
    fn a_difference_groups_rows_by_the_sides_holding_them() {
        let (a, b, c) = (
            tuple!(0_u64, 1_u64),
            tuple!(0_u64, 2_u64),
            tuple!(0_u64, 3_u64),
        );
        let sides = [
            ("batch", BTreeSet::from([&a, &b])),
            ("incremental", BTreeSet::from([&a, &b, &c])),
            ("Soufflé", BTreeSet::from([&a, &c])),
        ];
        assert_eq!(
            difference(&sides).as_deref(),
            Some(
                "\n    missing on batch (1): (0 3)\
                 \n    missing on Soufflé (1): (0 2)"
            )
        );
        assert_eq!(difference(&sides[..1]), None);
    }
}
