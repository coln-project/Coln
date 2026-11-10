// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::{HashMap, HashSet};

use coln_flir_rs::hash::CommitHash;
use coln_flir_rs::public::PublicScalarValue;
use tracing::info;

use crate::commit::chunk::Chunk;
use crate::commit::error::CodecError;
use crate::commit::frame::Frame;
use crate::commit::graph::CommitGraph;
use crate::commit::{Commit, DataCommit};
use crate::op::PublicOp;
use crate::store::Store;
use crate::store::error::{CommitApplyError, StoreError};
use crate::table::{TableMeta, TableOid, ValidationError};

mod batch;

pub(crate) use batch::PreparedCommit;
pub use batch::{CommitBatch, PrepareState, Prepared, Unprepared};

/// For consumption by subduction
pub struct CommitChunk {
    pub hash: CommitHash,
    pub parents: Vec<CommitHash>,
    pub bytes: Vec<u8>,
}

impl<T> From<&Commit<'_, T>> for CommitChunk {
    fn from(commit: &Commit<'_, T>) -> Self {
        Self {
            hash: commit.hash(),
            parents: commit.deps.clone(),
            bytes: Frame::from(commit).encoded(),
        }
    }
}

impl Store {
    pub fn heads(&self) -> Vec<CommitHash> {
        self.commits.heads().cloned().collect()
    }

    /// Look up a data commit. Access the root through [`CommitGraph::root_commit`].
    pub fn commit_by_hash(&self, hash: &CommitHash) -> Option<&DataCommit> {
        self.commits.get_data(hash)
    }

    /// Path, oid, and schema for a registered table.
    pub(crate) fn table_meta(&self, oid: TableOid) -> Option<TableMeta<'_>> {
        self.table(oid).map(|table| TableMeta {
            path: table.path(),
            oid,
            schema: table.schema(),
        })
    }

    fn ancestors_of(&self, heads: &[CommitHash]) -> HashSet<CommitHash> {
        let mut seen = HashSet::new();
        let mut stack = heads.to_vec();
        while let Some(hash) = stack.pop() {
            if !seen.insert(hash) {
                continue;
            }
            if let Some(parents) = self.commits.parents_of(&hash) {
                stack.extend(parents.iter().copied());
            }
        }
        seen
    }

    /// Root and data chunks not covered by the supplied heads, in dependency order.
    pub fn chunks_after(&self, have_heads: &[CommitHash]) -> Vec<Chunk> {
        let seen = self.ancestors_of(have_heads);
        let root = self.commits.root_commit();
        let mut chunks = Vec::new();

        if !seen.contains(&root.hash()) {
            chunks.push(Chunk::Root(root.clone()));
        }

        chunks.extend(
            self.commits
                .iter_topological()
                .filter(|commit| !seen.contains(&commit.hash()))
                .cloned()
                .map(Chunk::Data),
        );
        chunks
    }

    /// Framed root and data commits missing from the supplied heads' ancestry.
    pub fn commit_chunks_after(&self, have_heads: &[CommitHash]) -> Vec<CommitChunk> {
        let seen = self.ancestors_of(have_heads);
        let root = self.commits.root_commit();
        let mut chunks = Vec::new();
        if !seen.contains(&root.hash()) {
            chunks.push(CommitChunk::from(root));
        }
        chunks.extend(
            self.commits
                .iter_topological()
                .filter(|commit| !seen.contains(&commit.hash()))
                .map(CommitChunk::from),
        );
        chunks
    }

    /// Get commits in `other` that are not in `self`
    pub fn commits_added(&self, other: &Self) -> Vec<DataCommit> {
        // a depth first search from the heads of others backwards until hashes
        // are in self
        let mut stack = other.heads();
        let mut seen = HashSet::new();
        let mut added = Vec::new();

        while let Some(hash) = stack.pop() {
            if !seen.insert(hash) || self.commits.contains(&hash) {
                continue;
            }

            added.push(hash);
            if let Some(commit) = other.commit_by_hash(&hash) {
                stack.extend(commit.deps.iter());
            }
        }

        added.reverse();
        added
            .into_iter()
            .filter_map(|hash| other.commit_by_hash(&hash).cloned())
            .collect()
    }

    /// To prepare the bytes for application. Returns a [`CommitBatch`] which can be
    /// used to iterate over commits and apply/reject them.
    /// This is a low level API, not intended to be used directly by the user.
    pub fn prepare_commit_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<CommitBatch<'_, Unprepared>, StoreError> {
        let mut new_commits = Vec::new();
        for bytes in chunk_bytes {
            let frame = Frame::decode(&bytes)?;
            match Chunk::from_frame(frame, |path| {
                self.resolve_table(path)
                    .and_then(|oid| self.table_meta(oid))
            })? {
                Chunk::Data(commit) => new_commits.push(commit),
                Chunk::Root(root) => {
                    if !self.commits.contains(&root.hash()) {
                        return Err(CommitApplyError::RootCommit(root.hash()).into());
                    }
                }
            }
        }
        self.check_untrusted_commits(new_commits.iter())?;
        self.prepare_commits(new_commits, true)
    }

    pub(crate) fn prepare_commit(
        &mut self,
        commit: DataCommit,
    ) -> Result<CommitBatch<'_, Unprepared>, StoreError> {
        self.prepare_commits(std::iter::once(commit), false)
    }

    /// Here we assume that the commits have been checked
    pub fn prepare_commits(
        &mut self,
        new_commits: impl IntoIterator<Item = DataCommit>,
        drain_sync: bool,
    ) -> Result<CommitBatch<'_, Unprepared>, StoreError> {
        Ok(CommitBatch::new(self, new_commits, drain_sync))
    }

    /// Checks decoded commits to make sure they are sane
    fn check_untrusted_commits<'a>(
        &self,
        commits: impl Iterator<Item = &'a DataCommit>,
    ) -> Result<(), StoreError> {
        let mut seen_hashes = HashSet::new();
        for commit in commits {
            let hash = commit.hash();
            if self.commits.contains(&hash) {
                continue;
            }

            if commit.deps.is_empty() {
                return Err(CommitApplyError::DanglingCommit(hash).into());
            }

            if seen_hashes.contains(&hash) {
                return Err(CommitApplyError::DuplicateCommit(hash).into());
            }

            seen_hashes.insert(hash);
        }

        Ok(())
    }

    /// Applying the data, assuming that it has passed the format checker, i.e.
    /// the data conforms the the schema type definitions.
    /// But it might not follow all the rule definitions, it might also violate
    /// primary key constraints after hashconsing
    pub(super) fn apply_commit_ops(&mut self, ops: Vec<PublicOp>) -> Result<(), StoreError> {
        let op_count = ops.len();
        let affected = self.stage_commit_ops(ops);
        self.apply_staged_ops(&affected)?;

        info!(op_count, "applied batch");
        Ok(())
    }

    // Stage all the commit ops into the table's pending state.
    fn stage_commit_ops(&mut self, ops: Vec<PublicOp>) -> Vec<TableOid> {
        let mut affected = HashSet::new();
        for op in ops {
            let oid = op.table();
            let op = self.id_packer.pack_row(op);
            self.tables
                .get_mut(&oid)
                .expect("validated batch")
                .stage_update(op);
            affected.insert(oid);
        }
        affected.into_iter().collect()
    }

    pub(super) fn apply_staged_ops(&mut self, tables: &[TableOid]) -> Result<(), StoreError> {
        for oid in tables {
            self.tables
                .get_mut(oid)
                .expect("staged table exists")
                .apply_staged_ops(&mut self.rowing)?;
        }
        self.rowing.apply_unions(&self.id_packer);
        Ok(())
    }

    // We do as much check as possible without making changes to the tables
    // including checks like:
    //  - data following schema format
    //  - no duplication of primary keys before hashconsing
    fn precheck_commit(&self, cmt: DataCommit) -> Result<PrecheckedCommit, StoreError> {
        // TODO perhaps use late resolution, i.e. not resolving any ids, and when
        // we resolve, immediately make them packed.
        let ops = cmt.resolved_ops(|path| {
            self.resolve_table(path)
                .and_then(|oid| self.table_meta(oid))
        })?;
        self.validate_commit_ops(&ops)?;
        Ok(PrecheckedCommit { ops, original: cmt })
    }

    // TODO also need to validate that ids in op is referring to an existing id
    fn validate_commit_ops(&self, ops: &[PublicOp]) -> Result<(), StoreError> {
        let mut pending_pk: HashMap<TableOid, Vec<Vec<PublicScalarValue>>> = HashMap::new();

        for op in ops {
            let (table, values) = match op {
                PublicOp::Add { table, values } => (table, values),
                PublicOp::Delete { .. } => todo!("validate delete operations"),
            };
            let t = self
                .table(*table)
                .ok_or(ValidationError::UnknownTableOid { oid: *table })?;
            t.inner().validate_insert(&values[1..], &self.id_packer)?;

            // Check primary key conflicts within ops batch
            if let Some(key) = t.inner().primary_key_values(&values[1..]) {
                let keys = pending_pk.entry(*table).or_default();
                if keys.iter().any(|k| k == &key) {
                    return Err(ValidationError::DuplicatePrimaryKey.into());
                }
                keys.push(key);
            }
        }
        Ok(())
    }

    pub fn has_pending(&self) -> bool {
        !self.pending_commits.is_empty()
    }
}

struct PrecheckedCommit {
    ops: Vec<PublicOp>,
    original: DataCommit,
}

impl Store {
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
        store.commits = CommitGraph::new(root_commit);

        let mut commits = Vec::new();
        for chunk in chunks {
            if chunk.is_root() {
                continue;
            }

            let Chunk::Data(commit) = Chunk::from_frame(chunk, |path| {
                store
                    .resolve_table(path)
                    .and_then(|oid| store.table_meta(oid))
            })?
            else {
                unreachable!("root frames were skipped above");
            };
            commits.push(commit);
        }
        store.check_untrusted_commits(commits.iter())?;
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

    /// Applies ready commits in dependency order without running the query engine.
    /// Derived data and rule checks are skipped; schema validation still applies.
    /// Commits must already be checked; use [`Self::apply_chunk_bytes_plain`] for
    /// untrusted bytes. Commits with missing dependencies remain pending.
    pub fn apply_commits_plain(
        &mut self,
        commits: impl IntoIterator<Item = DataCommit>,
    ) -> Result<(), StoreError> {
        self.prepare_commits(commits, true)?.apply_plain()
    }
}

#[cfg(test)]
mod tests {
    use coln_flir_rs::engine::packed::{StoreScalarValue, StoreTuple};
    use coln_flir_rs::ir::Path;

    use super::*;
    use crate::IdLookup;
    use crate::txn::rw::StoreRead;

    #[test]
    fn resolved_op_validates_and_stores_schema_values_after_the_row_id() {
        use crate::op::PendingOp;
        use crate::test_utils::{int_schema, row_id_from};
        use coln_flir_rs::engine::tx::PendingRowId;

        let mut store = Store::new();
        let path = Path::from("T");
        let table = store
            .create_table(path.clone(), int_schema(vec!["value"], Some(vec![0])))
            .expect("create table");
        let row_id = row_id_from(1, 0);
        let op = PendingOp::add(PendingRowId(0), table, vec![42i32].into()).resolve(row_id.commit);
        assert_eq!(op.id(), row_id);
        assert_eq!(op.table(), table);
        store
            .validate_commit_ops(std::slice::from_ref(&op))
            .expect("valid row");

        let duplicate =
            PendingOp::add(PendingRowId(1), table, vec![42i32].into()).resolve(row_id.commit);
        assert!(matches!(
            store.validate_commit_ops(&[op.clone(), duplicate]),
            Err(StoreError::Validation(ValidationError::DuplicatePrimaryKey))
        ));

        store.apply_commit_ops(vec![op]).expect("apply row");
        let packed_id = store.id_lookup().packed(&row_id).expect("packed row id");
        assert_eq!(
            store.scan_table(&path).expect("table"),
            vec![StoreTuple::from_id_values(
                packed_id,
                vec![StoreScalarValue::I32(42)]
            )]
        );
    }
}
