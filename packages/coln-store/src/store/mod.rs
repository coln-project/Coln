// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub mod auto;
pub mod commit;
pub mod error;
pub mod frag;

use std::collections::{HashMap, HashSet};

use coln_flir_rs::engine::packed::{PackedRowId, StoreTuple};
use coln_flir_rs::engine::schema::ColnDef;
use coln_flir_rs::engine::tx::TxRowId;
use coln_flir_rs::hash::CommitHash;
use coln_flir_rs::public::PublicRowId;
use coln_flir_rs::query::WhereClause;
use tracing::info;

use crate::commit::Commit;
use crate::commit::graph::CommitGraph;
use crate::commit::wire::RootCommitData;
use crate::ir::{self, FlatRealm};
pub use crate::pack::id_packer::IdLookup;
use crate::pack::{IdPacker, IdPackerSnapshot};
use crate::rollback::Rollback;
use crate::rowing::{self, RowingSnapshot};
use crate::store::auto::AutoStore;
use crate::store::error::StoreError;
use crate::table::{Table, TableHandle, TableOid, TableSnapshot, ValidationError};
use crate::txn::rw::StoreRead;
use crate::txn::{ReadOnly, ReadWrite, Transaction};
use crate::{commit::error::CodecError, txn::id::Promote};

#[derive(Debug)]
pub struct Store {
    path_to_oid: HashMap<ir::Path, TableOid>,
    /// Oids are dense and tables are never dropped, so the next oid is `tables.len()`.
    tables: HashMap<TableOid, Table>,
    id_packer: IdPacker,
    /// Source rule entries retained for persistence. Compiled form lives in `rules`.
    ir: FlatRealm,
    commits: CommitGraph,
    rowing: rowing::Rowing,
    pending_commits: Vec<Commit<'static>>,
}

pub(crate) struct StoreSnapshot {
    tables: Vec<(TableOid, TableSnapshot)>,
    id_packer: IdPackerSnapshot,
    rowing: RowingSnapshot,
}

impl Rollback for Store {
    type Snapshot = StoreSnapshot;

    fn snapshot(&mut self) -> Self::Snapshot {
        let tables = self
            .tables
            .iter_mut()
            .map(|(&oid, table)| (oid, table.snapshot()))
            .collect();
        let id_packer = self.id_packer.snapshot();
        let rowing = self.rowing.snapshot();
        StoreSnapshot {
            tables,
            id_packer,
            rowing,
        }
    }

    fn commit(&mut self, snapshot: Self::Snapshot) {
        let StoreSnapshot {
            tables,
            id_packer,
            rowing,
        } = snapshot;
        for (oid, snapshot) in tables {
            self.tables
                .get_mut(&oid)
                .expect("snapshotted table should still exist")
                .commit(snapshot);
        }
        self.id_packer.commit(id_packer);
        self.rowing.commit(rowing);
    }

    fn rollback_to(&mut self, snapshot: Self::Snapshot) {
        let StoreSnapshot {
            tables,
            id_packer,
            rowing,
        } = snapshot;
        for (oid, snapshot) in tables {
            self.tables
                .get_mut(&oid)
                .expect("snapshotted table should still exist")
                .rollback_to(snapshot);
        }
        self.id_packer.rollback_to(id_packer);
        self.rowing.rollback_to(rowing);
    }
}

impl Store {
    // Constructors and basic accessors

    /// # Panics
    ///
    /// If we fail to start Coln Query, which is probably a fatal problem
    pub fn new() -> Self {
        let ir = FlatRealm {
            tables: Vec::new(),
            definitions: Vec::new(),
            rules: Vec::new(),
        };
        let empty_colndef = ColnDef {
            theory: String::new(),
            realm: String::new(),
        };
        let empty_root = RootCommitData::new(ir.clone(), empty_colndef);

        let commits =
            Self::graph_with_root_commit(empty_root).expect("empty root commit should build");
        Self {
            path_to_oid: HashMap::new(),
            tables: HashMap::new(),
            id_packer: IdPacker::new(),
            ir,
            commits,
            rowing: rowing::Rowing::new(),
            pending_commits: Vec::new(),
        }
    }

    pub fn tables(&self) -> impl Iterator<Item = (&TableOid, TableHandle<'_>)> {
        self.tables
            .iter()
            .map(|(oid, table)| (oid, TableHandle::new(table, &self.id_packer, &self.rowing)))
    }

    pub fn commits(&self) -> &CommitGraph {
        &self.commits
    }

    /// Add commit to the commit graph. This is a low level API, typically you
    /// want to use `apply_commits`
    pub(crate) fn record_in_commit_graph(&mut self, commit: Commit<'static>) {
        self.commits.add_commit(commit);
    }

    pub fn resolve_table(&self, path: &ir::Path) -> Option<TableOid> {
        self.path_to_oid.get(path).copied()
    }

    pub fn table(&self, oid: TableOid) -> Option<TableHandle<'_>> {
        self.tables
            .get(&oid)
            .map(|table| TableHandle::new(table, &self.id_packer, &self.rowing))
    }

    pub fn table_at(&self, path: &ir::Path) -> Option<TableHandle<'_>> {
        self.resolve_table(path).and_then(|oid| self.table(oid))
    }

    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    pub fn rule_entries(&self) -> &[ir::RuleEntry] {
        &self.ir.rules
    }

    pub fn json_ir(&self) -> Result<String, StoreError> {
        let root = self.commits.root_commit()?.root_payload()?;
        Ok(serde_json::to_string(&root.ir).map_err(CodecError::from)?)
    }

    pub fn coln_def(&self) -> Result<ColnDef, StoreError> {
        let root = self.commits.root_commit()?.root_payload()?;
        Ok(root.coln_def)
    }

    // Used by txn to finalise live ids
    pub(crate) fn canonical_row_id(&self, row_id: &PublicRowId) -> Option<PublicRowId> {
        let packed = self.id_packer.packed(row_id)?;
        let canonical = self.rowing.canonical_id(&packed, &self.id_packer);
        Some(self.id_packer.unpack_row_id(canonical))
    }

    /// Read-only access to the id dictionary, for converting between public
    /// and packed row ids.
    pub fn id_lookup(&self) -> &impl IdLookup {
        &self.id_packer
    }
}

impl Store {
    // Helper methods for StoreRead

    pub(crate) fn scan_table_iter(
        &self,
        table_path: &ir::Path,
    ) -> Option<impl Iterator<Item = StoreTuple> + '_> {
        self.table_at(table_path).map(|table| table.scan())
    }

    // This function will canonicalise the row_id on read, but will not change it
    // See `row_by_liveid` which will actually canonicalise the handle.
    // We need both because the TS FFI does not deal with handles.
    pub(crate) fn row_by_id_inner(
        &self,
        table: &ir::Path,
        row_id: &PackedRowId,
    ) -> Option<StoreTuple> {
        self.table_at(table)?.row_by_id(row_id)
    }

    // TODO ignoring efficiency. I'll leave that to the query engine to produce
    // the optimal query plan! Or we can optimise later.
    pub(crate) fn all_proj_inner(
        &self,
        query: &WhereClause,
        select: &[u32],
    ) -> Result<Vec<StoreTuple>, StoreError> {
        let select: HashSet<u32> = select.iter().copied().collect();
        let t = self
            .table_at(&query.table_name)
            .ok_or(ValidationError::UnknownTable {
                path: query.table_name.clone(),
            })?;
        let v = self
            .all_row_id_inner(query)?
            .into_iter()
            .map(|r| t.row_by_id(&r).expect("index_seek return valid rowid"))
            .map(|vs| {
                vs.into_iter()
                    .enumerate()
                    .filter_map(|(i, v)| select.contains(&(i as u32)).then_some(v))
                    .collect::<StoreTuple>()
            })
            .collect::<Vec<StoreTuple>>();
        Ok(v)
    }

    pub(crate) fn all_row_id_inner(
        &self,
        query: &WhereClause,
    ) -> Result<Vec<PackedRowId>, StoreError> {
        let WhereClause {
            table_name,
            values,
            row_id,
        } = query;
        let t = self
            .table_at(table_name)
            .ok_or(ValidationError::UnknownTable {
                path: table_name.clone(),
            })?;
        // TODO `WhereClause` should carry packed values and row ids, with the
        // packing done at a higher layer. Packing here means every lookup pays
        // for the dictionary lookups, and the store API leaks public types.
        //
        // An ID whose commit hash was never interned cannot match any stored
        // row, so an unpackable key or row id yields no results.
        let Some(key) = values
            .iter()
            .map(|v| self.id_packer.try_pack_value(v))
            .collect::<Option<StoreTuple>>()
        else {
            return Ok(Vec::new());
        };
        let row_id = match row_id {
            Some(id) => match self.id_packer.packed(id) {
                Some(packed) => Some(packed),
                None => return Ok(Vec::new()),
            },
            None => None,
        };
        let row_ids = t
            .index_seek(&key)?
            .filter(|r| row_id.as_ref().is_none_or(|id| id == r))
            .collect();
        Ok(row_ids)
    }
}

// Autocommit method that opens up a txn, does a single operations
// then immediately closes the txn
impl StoreRead for Store {
    fn scan_table(&self, table: &ir::Path) -> Option<Vec<StoreTuple>> {
        let txn = self.ro_transaction();
        txn.scan_table(table)
    }

    fn row_by_id(&self, table: &ir::Path, row_id: &PackedRowId) -> Option<StoreTuple> {
        self.ro_transaction().row_by_id(table, row_id)
    }

    fn all_proj(&self, query: &WhereClause, select: &[u32]) -> Result<Vec<StoreTuple>, StoreError> {
        self.ro_transaction().all_proj(query, select)
    }

    fn all_row_id(&self, query: &WhereClause) -> Result<Vec<PackedRowId>, StoreError> {
        self.ro_transaction().all_row_id(query)
    }
}

impl Store {
    // create stores from theory and transactions on stores

    fn graph_with_root_commit(root_commit: RootCommitData) -> Result<CommitGraph, CodecError> {
        let mut graph = CommitGraph::new();
        graph.add_commit(Commit::from_root_data(&root_commit)?);
        Ok(graph)
    }

    /// Builds an empty column store per `theory.tables` and keeps only `theory.rules`
    /// (schemas are stored on each [`Table`]).
    pub fn try_from_ir(ir: &FlatRealm, coln_def: ColnDef) -> Result<Self, StoreError> {
        info!(
            table_count = ir.tables.len(),
            rule_count = ir.rules.len(),
            "building store from theory"
        );

        let mut path_to_oid = HashMap::new();
        let mut tables_map = HashMap::new();

        for (oid, entry) in ir.tables.iter().enumerate() {
            path_to_oid.insert(entry.path.clone(), oid);
            tables_map.insert(
                oid,
                Table::new(entry.path.clone(), oid, entry.table.clone()),
            );
        }

        let commits = Self::graph_with_root_commit(RootCommitData::new(ir.clone(), coln_def))?;

        Ok(Self {
            path_to_oid,
            tables: tables_map,
            id_packer: IdPacker::new(),
            ir: ir.clone(),
            commits,
            rowing: rowing::Rowing::new(),
            pending_commits: Vec::new(),
        })
    }
}

impl Store {
    // transactions

    pub fn auto(self) -> AutoStore {
        AutoStore::new(self)
    }

    pub fn ro_transaction(&self) -> Transaction<ReadOnly<'_>> {
        Transaction::<ReadOnly<'_>>::new(self)
    }

    pub fn transaction(&mut self) -> Transaction<ReadWrite<'_>> {
        Transaction::<ReadWrite<'_>>::new(self)
    }
}

impl Promote for Store {
    fn promote(
        &self,
        pending_ids: impl IntoIterator<Item = TxRowId>,
        hash: CommitHash,
    ) -> Vec<PublicRowId> {
        pending_ids
            .into_iter()
            .map(|pending| {
                let wire_id = match pending {
                    TxRowId::Pending(pending) => pending.resolve(hash),
                    TxRowId::Existing(row_id) => row_id,
                };
                self.canonical_row_id(&wire_id).unwrap_or(wire_id)
            })
            .collect()
    }
}

impl Store {
    // for debugging and testing and experiments

    #[cfg(feature = "native")]
    // used in SQL mode only
    pub(crate) fn create_table(
        &mut self,
        path: ir::Path,
        schema: ir::Schema,
    ) -> Result<TableOid, StoreError> {
        let oid = self.tables.len();
        self.path_to_oid.insert(path.clone(), oid);
        self.tables.insert(oid, Table::new(path, oid, schema));

        let mut tables: Vec<_> = self
            .tables
            .values()
            .map(|table| {
                (
                    table.oid(),
                    ir::TableEntry {
                        path: table.path().clone(),
                        table: table.schema().clone(),
                    },
                )
            })
            .collect();
        tables.sort_by_key(|(oid, _)| *oid);
        let ir = FlatRealm {
            tables: tables.into_iter().map(|(_, entry)| entry).collect(),
            definitions: Vec::new(),
            rules: self.rule_entries().to_vec(),
        };
        let root_commit = RootCommitData::new(
            ir,
            ColnDef {
                theory: String::new(),
                realm: String::new(),
            },
        );
        self.commits = Self::graph_with_root_commit(root_commit)?;
        Ok(oid)
    }

    /// Dump every table in the store for debugging, in ascending [`TableOid`] order,
    /// separated by a blank line.
    pub fn dump(&self) -> String {
        let mut oids: Vec<TableOid> = self.tables.keys().copied().collect();
        oids.sort_unstable();
        oids.into_iter()
            .map(|oid| self.tables[&oid].dump(&self.id_packer))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
