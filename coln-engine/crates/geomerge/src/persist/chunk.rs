use sha2::{Digest, Sha256};

use crate::commit::CommitHash;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChunkType {
    Store = 0,
    Commit = 1,
}

impl From<ChunkType> for u8 {
    fn from(ct: ChunkType) -> u8 {
        match ct {
            ChunkType::Store => 0,
            ChunkType::Commit => 1,
        }
    }
}

/// Compute the content hash for a chunk.
///
/// hash = sha256(chunk_type:u8 || data_len:u64_le || data)
///
/// This is the same recipe automerge uses in storage/chunk.rs, adapted for
/// geomerge chunk types. For a Commit chunk the returned value IS the commit hash.
pub(crate) fn hash(chunk_type: ChunkType, data: &[u8]) -> CommitHash {
    let mut hasher = Sha256::new();
    hasher.update([u8::from(chunk_type)]);
    hasher.update(&(data.len() as u64).to_le_bytes());
    hasher.update(data);
    CommitHash(hasher.finalize().into())
}
