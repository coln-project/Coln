use std::collections::HashMap;
use std::io::Write;

use crate::commit::error::CodecError;
use crate::commit::hash::{CommitHash, HASH_SIZE};
use crate::commit::leb128 as commit_leb128;
use crate::commit::utils::read_slice;

/// A hash mapper builds up all the commit hashes seen in a particular commit
/// so they can be stored in the commit for dictionary encoding
#[derive(Debug, Default)]
pub(crate) struct HashMapper {
    hashes: Vec<CommitHash>,
    indexes: HashMap<CommitHash, u32>,
}

impl HashMapper {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn insert(&mut self, hash: CommitHash) -> u32 {
        if let Some(index) = self.indexes.get(&hash) {
            return *index;
        }
        let index = self.hashes.len() as u32;
        self.hashes.push(hash);
        self.indexes.insert(hash, index);
        index
    }

    pub(crate) fn index(&self, hash: CommitHash) -> Option<u32> {
        self.indexes.get(&hash).copied()
    }

    pub(crate) fn hashes(&self) -> &[CommitHash] {
        &self.hashes
    }
}

// Write all the hashes stored in the hash_mapper. The order of these hashes are
// determined by the pending_op order, which is deterministic for a commit.
// Therefore the wire format of hash_mapper is also deterministic
pub(crate) fn write_hash_dict(
    buf: &mut Vec<u8>,
    hash_mapper: &HashMapper,
) -> Result<(), CodecError> {
    let hash_count: u32 = hash_mapper
        .hashes()
        .len()
        .try_into()
        .map_err(|_| CodecError::Other("too many hashes than u32::MAX".into()))?;
    commit_leb128::write_u32(buf, hash_count);
    for hash in hash_mapper.hashes() {
        buf.write_all(hash.as_bytes())?;
    }
    Ok(())
}

pub(crate) fn read_hash_dict(data: &[u8], pos: &mut usize) -> Result<Vec<CommitHash>, CodecError> {
    let hash_count = commit_leb128::read_u32(data, pos, "hash dictionary count")? as usize;
    let mut hashes = Vec::with_capacity(hash_count);
    for _ in 0..hash_count {
        let bytes = read_slice(data, pos, HASH_SIZE, "hash dictionary entry")?;
        let mut hash = [0; HASH_SIZE];
        hash.copy_from_slice(bytes);
        hashes.push(CommitHash(hash));
    }
    Ok(hashes)
}
