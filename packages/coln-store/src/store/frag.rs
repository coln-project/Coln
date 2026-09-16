// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    commit::{Commit, chunk::Chunk, error::CodecError, hash::CommitHash},
    store::{Store, error::StoreError},
};

/// For consumption by subduction
pub struct CommitChunk {
    pub hash: CommitHash,
    pub parents: Vec<CommitHash>,
    pub bytes: Vec<u8>,
}

pub trait FragmentSync {
    fn commit_chunks_after(&self, have_heads: &[CommitHash]) -> Vec<CommitChunk>;
    fn apply_chunk_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, StoreError>;
}

impl FragmentSync for Store {
    fn commit_chunks_after(&self, have_heads: &[CommitHash]) -> Vec<CommitChunk> {
        self.commits_after(have_heads)
            .into_iter()
            .map(|commit| {
                let head = commit.hash();
                let parents = commit.deps.clone();
                let bytes = Chunk::from(commit).encoded();
                CommitChunk {
                    hash: head,
                    parents,
                    bytes,
                }
            })
            .collect()
    }

    /// Apply the bytes received by interpreting them as chunks, for syncing purposes
    /// Return chunk bytes that cannot be applied yet.
    fn apply_chunk_bytes(
        &mut self,
        chunk_bytes: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, StoreError> {
        let commits = chunk_bytes
            .into_iter()
            .map(|bytes| Chunk::decode(&bytes))
            .map(|chunk| {
                chunk.and_then(|chunk| {
                    Commit::from_chunk(chunk, |path| {
                        self.resolve_table(path)
                            .and_then(|oid| self.table_meta(oid))
                    })
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self::commits_to_chunk_bytes(self.apply_commits(commits)?))
    }
}

impl Store {
    // for interfacing with subduction
    // TODO add fragments API

    /// Build a store from commit chunks from scratch, assuming that the input
    /// `chunk_bytes` contains a valid root commit.
    pub fn try_from_commit_bytes(
        chunk_bytes: impl IntoIterator<Item = impl AsRef<[u8]>>,
    ) -> Result<(Self, Vec<Vec<u8>>), StoreError> {
        let chunks = chunk_bytes
            .into_iter()
            .map(|bytes| Chunk::decode(bytes.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        Self::try_from_chunks(chunks)
    }

    /// Create a store from the commit chunks, assuming the chunk contains a valid root
    pub(crate) fn try_from_chunks(chunks: Vec<Chunk>) -> Result<(Self, Vec<Vec<u8>>), StoreError> {
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

        let root_commit = Commit::from_chunk((*roots[0]).clone(), |_| None)?;
        let root_payload = root_commit.root_payload()?;
        let mut store = Store::try_from_ir(root_payload.ir, root_payload.coln_def)?;

        let mut commits = Vec::new();
        for chunk in chunks {
            if chunk.is_root() {
                continue;
            }

            let commit = Commit::from_chunk(chunk, |path| {
                store
                    .resolve_table(path)
                    .and_then(|oid| store.table_meta(oid))
            })?;
            commits.push(commit);
        }

        let pending_bytes = Self::commits_to_chunk_bytes(store.apply_commits(commits)?);
        Ok((store, pending_bytes))
    }

    fn commits_to_chunk_bytes(commits: Vec<Commit<'static>>) -> Vec<Vec<u8>> {
        commits
            .into_iter()
            .map(|commit| Chunk::from(commit).encoded())
            .collect()
    }
}
