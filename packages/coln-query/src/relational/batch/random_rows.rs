// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Random commits for any realm, read off its base tables in the FLIR.
//!
//! Every base table gets random rows: a fresh row id, a live row of the
//! referenced table for each reference, and a small number or string for
//! each data column, so that rows join often. A commit also deletes some rows
//! of earlier commits, and with them every row referencing them, so every
//! reference points to a live row, as in a store that enforces its foreign
//! keys. A row is laid out as the frontend expects it: its row id as two
//! numbers, then the columns in order, each reference as two numbers.

use std::collections::HashSet;

use anyhow::{Context, Result};
use coln_batch::rng::SplitMix64;
use coln_flir_rs::ir::{BuiltinTy, ColType, EntityVariant, FlatRealm};

use crate::api::deltas::{StoreDelta, TableDelta, ZRow};
use crate::relational::relation::TupleValue;
use crate::relational::schema::EntityRef;
use crate::scalarial::ScalarTypedValue;

/// The most rows a commit of case `seed` may insert per table: tiny at first,
/// larger for later seeds, at most `limit`. The first failing seed is
/// therefore a small case.
pub(crate) fn max_inserts(seed: u64, limit: u64) -> u64 {
    [1, 2, 3, 5, 8, 13][(seed / 5).min(5) as usize].min(limit)
}

/// A row id, as its two halves.
type Key = (u64, u64);

/// The base tables of one realm and their live rows.
pub(crate) struct RandomRows {
    rng: SplitMix64,
    tables: Vec<Table>,
    /// The commits so far, which is also the first half of every row id the
    /// next commit hands out.
    commits: u64,
}

struct Table {
    path: EntityRef,
    columns: Vec<Column>,
    rows: Vec<Live>,
}

enum Column {
    /// A reference to a row of the table at this index.
    Reference(usize),
    Int,
    Str,
}

struct Live {
    row: TupleValue,
    key: Key,
    references: Vec<Key>,
    commit: u64,
}

impl RandomRows {
    /// The base tables of `realm`, without rows, and a generator seeded with
    /// `seed`.
    pub(crate) fn new(realm: &FlatRealm, seed: u64) -> Result<Self> {
        let bases: Vec<_> = realm
            .tables
            .iter()
            .filter(|entry| matches!(entry.table.entity_variant, EntityVariant::Table))
            .collect();
        let tables = bases
            .iter()
            .map(|entry| {
                let columns = entry
                    .table
                    .columns
                    .iter()
                    .map(|column| match &column.col_type {
                        ColType::RowId { path } => bases
                            .iter()
                            .position(|base| &base.path == path)
                            .map(Column::Reference)
                            .with_context(|| format!("{} references {path}", entry.path)),
                        ColType::BuiltinTy { builtin_ty } => Ok(match builtin_ty {
                            BuiltinTy::BuiltinInt => Column::Int,
                            BuiltinTy::BuiltinStr => Column::Str,
                        }),
                    })
                    .collect::<Result<_>>()?;
                Ok(Table {
                    path: EntityRef::from(&entry.path),
                    columns,
                    rows: Vec::new(),
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            rng: SplitMix64::new(seed),
            tables,
            commits: 0,
        })
    }

    /// The next commit, inserting up to `most` rows per table, and twice that
    /// for tables with references. Also returns how many rows each table
    /// holds after it.
    pub(crate) fn commit(&mut self, most: u64) -> (StoreDelta, String) {
        let commit = self.commits;
        self.commits += 1;
        let mut deltas: Vec<Vec<ZRow>> = self.tables.iter().map(|_| Vec::new()).collect();

        // Row ids are unique across tables, as in a store.
        let mut ctr = 0;
        for (at, delta) in deltas.iter_mut().enumerate() {
            let references = self.tables[at]
                .columns
                .iter()
                .any(|column| matches!(column, Column::Reference(_)));
            let inserts = match references {
                true => self.rng.below(2 * most + 1),
                false => self.rng.below(most + 1),
            };
            for _ in 0..inserts {
                if let Some(live) = self.random_row(at, (commit, ctr), commit) {
                    ctr += 1;
                    delta.push(ZRow::new(1, live.row.clone()).expect("weight 1"));
                    self.tables[at].rows.push(live);
                }
            }
        }

        // Deleting a row deletes the rows referencing it, and theirs in turn.
        let mut gone: Vec<Key> = Vec::new();
        for table in &mut self.tables {
            for _ in 0..self.rng.below(most / 2 + 1) {
                let older: Vec<usize> = (0..table.rows.len())
                    .filter(|&at| table.rows[at].commit < commit)
                    .collect();
                if !older.is_empty() {
                    let at = older[self.rng.below(older.len() as u64) as usize];
                    gone.push(table.rows[at].key);
                }
            }
        }
        let mut deleted: HashSet<Key> = HashSet::new();
        while let Some(key) = gone.pop() {
            if !deleted.insert(key) {
                continue;
            }
            for (at, table) in self.tables.iter_mut().enumerate() {
                let (removed, kept) = std::mem::take(&mut table.rows)
                    .into_iter()
                    .partition(|live| live.key == key || live.references.contains(&key));
                table.rows = kept;
                for live in removed {
                    gone.push(live.key);
                    deltas[at].push(ZRow::new(-1, live.row).expect("weight -1"));
                }
            }
        }

        let sizes: Vec<String> = self
            .tables
            .iter()
            .map(|table| format!("{} {}", table.path, table.rows.len()))
            .collect();
        let delta = self
            .tables
            .iter()
            .zip(deltas)
            .filter(|(_, rows)| !rows.is_empty())
            .map(|(table, rows)| TableDelta::new(table.path.clone(), rows))
            .collect::<Vec<_>>();
        (StoreDelta::new(delta), sizes.join(", "))
    }

    /// A new row of table `at` with row id `key`, or `None` if it references
    /// a table without rows.
    fn random_row(&mut self, at: usize, key: Key, commit: u64) -> Option<Live> {
        let mut data = vec![ScalarTypedValue::Uint(key.0), ScalarTypedValue::Uint(key.1)];
        let mut references = Vec::new();
        for column in &self.tables[at].columns {
            match column {
                Column::Reference(target) => {
                    let rows = &self.tables[*target].rows;
                    if rows.is_empty() {
                        return None;
                    }
                    let target = rows[self.rng.below(rows.len() as u64) as usize].key;
                    data.extend([
                        ScalarTypedValue::Uint(target.0),
                        ScalarTypedValue::Uint(target.1),
                    ]);
                    references.push(target);
                }
                Column::Int => data.push(ScalarTypedValue::Iint(self.rng.below(4) as i64)),
                Column::Str => {
                    let text = ["a", "b", "c"][self.rng.below(3) as usize];
                    data.push(ScalarTypedValue::String(text.to_string()));
                }
            }
        }
        Some(Live {
            row: TupleValue::new(data),
            key,
            references,
            commit,
        })
    }
}
