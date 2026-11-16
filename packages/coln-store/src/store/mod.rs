// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub mod auto;
mod catalog;
pub mod commit;
mod data;
pub mod error;
mod persist;
mod rebuild;
pub mod txn;

use std::collections::HashMap;
use std::sync::Arc;

use coln_flir_rs::engine::schema::ColnDef;
use parking_lot::RwLock;
use tracing::info;

use crate::commit::chunk::Chunk;
use crate::commit::error::CodecError;
use crate::commit::frame::Frame;
use crate::commit::graph::CommitGraph;
use crate::commit::wire::RootCommitData;
use crate::commit::{DataCommit, RootCommit};
use crate::ir::{self, FlatRealm};
use crate::pack::IdPacker;
pub use crate::pack::id_packer::IdLookup;
use crate::rowing::Rowing;
use crate::store::catalog::Catalog;
use crate::store::data::StoreData;
use crate::store::error::StoreError;
pub use crate::store::txn::{ReadTxn, ReadView, WriteTxn};
use crate::table::{Table, TableMeta, TableOid};

#[derive(Debug)]
pub struct Store {
    catalog: Arc<Catalog>,
    data: Arc<RwLock<StoreData>>,
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
        Self::try_from_ir(&ir, empty_colndef).expect("empty root commit should build")
    }

    // create stores from theory

    /// Builds an empty column store per `theory.tables` and keeps only `theory.rules`
    /// (schemas are stored on each [`Table`]).
    pub fn try_from_ir(ir: &FlatRealm, coln_def: ColnDef) -> Result<Self, StoreError> {
        info!(
            table_count = ir.tables.len(),
            rule_count = ir.rules.len(),
            "building store from theory"
        );

        let mut path_to_oid = HashMap::new();
        let mut table_meta = Vec::with_capacity(ir.tables.len());
        let mut tables = HashMap::new();

        for (oid, entry) in ir.tables.iter().enumerate() {
            let table = Table::new(entry.path.clone(), oid, entry.table.clone());
            path_to_oid.insert(entry.path.clone(), oid);
            table_meta.push(table.meta());
            tables.insert(oid, table);
        }

        let root_data = RootCommitData::new(ir.clone(), coln_def);
        let commits = Self::graph_with_root_commit(&root_data)?;

        let data = Arc::new(RwLock::new(StoreData {
            tables,
            id_packer: IdPacker::new(),
            commits,
            rowing: Rowing::new(),
            pending_commits: Vec::new(),
        }));

        Ok(Self {
            catalog: Arc::new(Catalog {
                path_to_oid,
                root_data,
                table_meta,
                ir: ir.clone(),
            }),
            data,
        })
    }

    fn graph_with_root_commit(root_commit: &RootCommitData) -> Result<CommitGraph, CodecError> {
        Ok(CommitGraph::new(RootCommit::from_root_data(root_commit)?))
    }

    // Creating a store from bytes

    /// Takes some bytes that _contain_ the root commits
    /// Attempt to create a [`Store`] out of it, and also the other commits left
    /// to be applied. Callers need to make a [`CommitBatch`] out of it, and apply
    pub fn try_from_commits_bytes(
        bytes: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Result<(Self, Vec<DataCommit>), StoreError> {
        let chunks = bytes
            .into_iter()
            .map(|bytes| Frame::decode(bytes.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Self::try_from_chunks(chunks)
    }

    /// Create a store from the commit chunks. Return the store and all the data commits, checked.
    pub(crate) fn try_from_chunks(
        chunks: Vec<Frame>,
    ) -> Result<(Self, Vec<DataCommit>), StoreError> {
        let roots = chunks
            .iter()
            .filter(|chunk| chunk.is_root())
            .collect::<Vec<_>>();
        if roots.is_empty() {
            return Err(
                CodecError::DataFormatError("commit graph has no root commit".into()).into(),
            );
        }
        if roots.len() > 1 {
            return Err(CodecError::DataFormatError(
                "commit graph has multiple root commits".into(),
            )
            .into());
        }

        let Chunk::Root(root_commit) = Chunk::from_frame((*roots[0]).clone(), |_| None)? else {
            unreachable!("root frames were selected above");
        };
        let root_payload = root_commit.root_payload();
        let mut store = Store::try_from_ir(&root_payload.ir, root_payload.coln_def)?;

        let commits = chunks
            .into_iter()
            .filter(|chunk| !chunk.is_root())
            .map(|chunk| {
                match Chunk::from_frame(chunk, |path| {
                    store
                        .catalog
                        .resolve_table(path)
                        .and_then(|oid| store.catalog.table_meta(oid))
                })? {
                    Chunk::Data(commit) => Ok(commit),
                    Chunk::Root(_) => unreachable!("root frames were filtered out"),
                }
            })
            .collect::<Result<Vec<_>, StoreError>>()?;

        let data = Arc::get_mut(&mut store.data)
            .expect("new store data is not shared")
            .get_mut();
        data.commits = CommitGraph::new(root_commit);
        data.check_commit_deps(commits.iter())?;
        Ok((store, commits))
    }
}

impl Store {
    // To apply bytes to store, but without deriving new data
    // mostly for testing purposes

    /// Restores a store from commit bytes without running the query engine.
    /// Derived data and rule checks are skipped. Commits with missing dependencies
    /// remain pending until their parents arrive.
    pub fn try_from_commit_bytes_plain(
        bytes: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Result<Self, StoreError> {
        let (mut store, commits) = Self::try_from_commits_bytes(bytes)?;
        store.apply_commits_plain(commits)?;
        Ok(store)
    }

    pub(crate) fn try_from_chunks_plain(chunks: Vec<Frame>) -> Result<Self, StoreError> {
        let (mut store, commits) = Self::try_from_chunks(chunks)?;
        store.apply_commits_plain(commits)?;
        Ok(store)
    }

    /// Decodes and applies commit bytes without running the query engine.
    /// Schema validation still applies. Commits with missing dependencies remain
    /// pending and are retried on the next plain apply call.
    pub fn apply_chunk_bytes_plain(
        &mut self,
        bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<(), StoreError> {
        self.prepare_commit_bytes(bytes)?.apply_plain()
    }

    pub(crate) fn apply_commits_plain(
        &mut self,
        commits: impl IntoIterator<Item = DataCommit>,
    ) -> Result<(), StoreError> {
        self.prepare_commits(commits, true)?.apply_plain()
    }
}

impl Store {
    // Catalog access, no lock needed

    pub fn resolve_table(&self, path: &ir::Path) -> Option<TableOid> {
        self.catalog.resolve_table(path)
    }

    pub fn table_count(&self) -> usize {
        self.catalog.table_count()
    }

    pub fn rule_entries(&self) -> &[ir::RuleEntry] {
        self.catalog.rule_entries()
    }

    pub fn json_ir(&self) -> String {
        self.catalog.json_ir()
    }

    pub fn ir(&self) -> &ir::FlatRealm {
        self.catalog.ir()
    }

    pub fn coln_def(&self) -> &ColnDef {
        self.catalog.coln_def()
    }

    pub fn table_meta(&self, oid: TableOid) -> Option<&TableMeta> {
        self.catalog.table_meta(oid)
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
        // Owned transactions can remain alive during a mutable borrow of Store.
        let catalog = Arc::get_mut(&mut self.catalog).ok_or(StoreError::StoreInUse)?;
        let data = Arc::get_mut(&mut self.data)
            .ok_or(StoreError::StoreInUse)?
            .get_mut();

        // Build the new root commit before mutating, so a codec error leaves
        // the store unchanged.
        let oid = data.tables.len();
        let mut ir = catalog.ir.clone();
        ir.tables.push(ir::TableEntry {
            path: path.clone(),
            table: schema.clone(),
        });
        let root_data = RootCommitData::new(ir, catalog.root_data.coln_def.clone());
        let commits = Self::graph_with_root_commit(&root_data)?;

        let table = Table::new(path.clone(), oid, schema);
        catalog.path_to_oid.insert(path, oid);
        catalog.table_meta.push(table.meta());
        catalog.ir = root_data.ir.clone();
        catalog.root_data = root_data;
        data.tables.insert(oid, table);
        data.commits = commits;
        Ok(oid)
    }

    /// Dump every table in the store for debugging, in ascending [`TableOid`] order,
    /// separated by a blank line.
    ///
    /// Takes a read lock, so calling this while the same thread holds a
    /// [`ReadTxn`] may deadlock if a writer is waiting.
    pub fn dump(&self) -> String {
        let data = self.data.read();
        let mut oids: Vec<TableOid> = data.tables.keys().copied().collect();
        oids.sort_unstable();
        oids.into_iter()
            .map(|oid| data.tables[&oid].dump(&data.id_packer))
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
