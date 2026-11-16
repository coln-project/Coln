// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::HashSet;

use coln_flir_rs::hash::CommitHash;

use crate::commit::chunk::Chunk;
use crate::commit::frame::Frame;
use crate::commit::{Commit, DataCommit};
use crate::store::error::{CommitApplyError, StoreError};
use crate::store::txn::ReadTxnable;
use crate::store::{ReadTxn, ReadView, Store};

mod batch;

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

impl ReadTxn {
    pub fn chunks_after(&self, have_heads: &[CommitHash]) -> Vec<Chunk> {
        self.read().chunks_after(have_heads)
    }

    pub fn commit_chunks_after(&self, have_heads: &[CommitHash]) -> Vec<CommitChunk> {
        self.read().commit_chunks_after(have_heads)
    }

    pub fn commits_added(&self, other: &(impl ReadTxnable + ?Sized)) -> Vec<DataCommit> {
        self.read().commits_added(other)
    }
}

impl ReadView<'_> {
    /// Root and data chunks not covered by the supplied heads, in dependency order.
    pub fn chunks_after(&self, have_heads: &[CommitHash]) -> Vec<Chunk> {
        let seen = self.ancestors_of(have_heads);
        let root = self.data.commits.root_commit();
        let mut chunks = Vec::new();

        if !seen.contains(&root.hash()) {
            chunks.push(Chunk::Root(root.clone()));
        }

        chunks.extend(
            self.data
                .commits
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
        let root = self.data.commits.root_commit();
        let mut chunks = Vec::new();
        if !seen.contains(&root.hash()) {
            chunks.push(CommitChunk::from(root));
        }
        chunks.extend(
            self.data
                .commits
                .iter_topological()
                .filter(|commit| !seen.contains(&commit.hash()))
                .map(CommitChunk::from),
        );
        chunks
    }

    /// Get commits in `other` that are not in `self`
    pub fn commits_added(&self, other: &(impl ReadTxnable + ?Sized)) -> Vec<DataCommit> {
        let other = other.read();
        // a depth first search from the heads of others backwards until hashes
        // are in self
        let mut stack = other.heads();
        let mut seen = HashSet::new();
        let mut added = Vec::new();

        while let Some(hash) = stack.pop() {
            if !seen.insert(hash) || self.data.commits.contains(&hash) {
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

    fn ancestors_of(&self, heads: &[CommitHash]) -> HashSet<CommitHash> {
        let mut seen = HashSet::new();
        let mut stack = heads.to_vec();
        while let Some(hash) = stack.pop() {
            if !seen.insert(hash) {
                continue;
            }
            if let Some(parents) = self.data.commits.parents_of(&hash) {
                stack.extend(parents.iter().copied());
            }
        }
        seen
    }
}

impl Store {
    /// To prepare the bytes for application. Returns a [`CommitBatch`] which can be
    /// used to iterate over commits and apply/reject them.
    /// This is a low level API, not intended to be used directly by the user.
    pub fn prepare_commit_bytes(
        &self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<CommitBatch<Unprepared>, StoreError> {
        let mut new_commits = Vec::new();
        let mut roots = Vec::new();
        for bytes in chunk_bytes {
            let frame = Frame::decode(&bytes)?;
            match Chunk::from_frame(frame, |path| {
                self.resolve_table(path)
                    .and_then(|oid| self.catalog.table_meta(oid))
            })? {
                Chunk::Data(commit) => new_commits.push(commit),
                Chunk::Root(root) => roots.push(root.hash()),
            }
        }

        let data = self.data.write_arc();
        if let Some(root) = roots.into_iter().find(|h| !data.commits.contains(h)) {
            return Err(CommitApplyError::RootCommit(root).into());
        }
        data.check_commit_deps(new_commits.iter())?;
        Ok(CommitBatch::new(
            self.catalog.clone(),
            data,
            new_commits,
            true,
        ))
    }

    /// Here we assume that the commits have been checked
    pub fn prepare_commits(
        &self,
        new_commits: impl IntoIterator<Item = DataCommit>,
        drain_sync: bool,
    ) -> Result<CommitBatch<Unprepared>, StoreError> {
        Ok(CommitBatch::new(
            self.catalog.clone(),
            self.data.write_arc(),
            new_commits,
            drain_sync,
        ))
    }
}

#[cfg(test)]
mod tests {
    use coln_flir_rs::engine::packed::{StoreScalarValue, StoreTuple};
    use coln_flir_rs::ir::Path;
    use coln_flir_rs::public::PublicRowId;

    use super::*;
    use crate::IdLookup;
    use crate::commit::author::Author;
    use crate::commit::wire::CommitData;
    use crate::table::ValidationError;

    #[test]
    fn resolved_op_validates_and_stores_schema_values_after_the_row_id() {
        use crate::op::PendingOp;
        use crate::test_utils::int_schema;
        use coln_flir_rs::engine::tx::PendingRowId;

        let mut store = Store::new();
        let path = Path::from("T");
        let table = store
            .create_table(path.clone(), int_schema(vec!["value"], Some(vec![0])))
            .expect("create table");
        let pending = PendingOp::add(PendingRowId(0), table, vec![42i32].into());
        let mut commit_data = CommitData::new(
            store.begin_read().heads(),
            Author::foo(),
            42,
            None,
            vec![pending.clone()],
        );
        let commit = DataCommit::from_commit_data(commit_data.clone(), |oid| store.table_meta(oid))
            .expect("build commit");
        let row_id = PublicRowId {
            commit: commit.hash(),
            counter: 0,
        };
        let op = pending.resolve(row_id.commit);
        assert_eq!(op.id(), row_id);
        assert_eq!(op.table(), table);
        let checked = store
            .data
            .read()
            .check_commit_ops(commit, &store.catalog)
            .expect("valid row");
        assert_eq!(checked.ops, vec![op]);

        commit_data
            .pending
            .push(PendingOp::add(PendingRowId(1), table, vec![42i32].into()));
        let duplicate = DataCommit::from_commit_data(commit_data, |oid| store.table_meta(oid))
            .expect("build duplicate-key commit");
        assert!(matches!(
            store
                .data
                .read()
                .check_commit_ops(duplicate, &store.catalog),
            Err(StoreError::Validation(ValidationError::DuplicatePrimaryKey))
        ));

        store
            .data
            .write()
            .apply_commit_ops(checked.ops)
            .expect("apply row");
        let read = store.begin_read();
        let packed_id = read.id_lookup().packed(&row_id).expect("packed row id");
        assert_eq!(
            read.scan_table(&path).expect("table"),
            vec![StoreTuple::from_id_values(
                packed_id,
                vec![StoreScalarValue::I32(42)]
            )]
        );
    }
}
