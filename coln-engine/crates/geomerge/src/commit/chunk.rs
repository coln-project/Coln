use crate::commit::CommitHash;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChunkType {
    Commit = 0,
    Root = 1,
}

impl From<ChunkType> for u8 {
    fn from(ct: ChunkType) -> u8 {
        match ct {
            ChunkType::Commit => 0,
            ChunkType::Root => 1,
        }
    }
}

/// Compute the content hash for a chunk.
///
/// hash = blake3(chunk_type:u8 || data_len:u64_le || data)
///
/// This mirrors the preimage automerge builds in storage/chunk.rs, adapted for
/// geomerge chunk types, but hashes it with BLAKE3 rather than SHA-256.
pub(crate) fn hash(chunk_type: ChunkType, data: &[u8]) -> CommitHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[u8::from(chunk_type)]);
    hasher.update(&(data.len() as u64).to_le_bytes());
    hasher.update(data);
    CommitHash(hasher.finalize().into())
}
