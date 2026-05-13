// TODO add doc on what this module handles

use hexane::v1::{Column, DeltaColumn};
use std::collections::HashMap;
use std::io::Write;

use crate::commit::error::PersistError;
use crate::commit::hash::{CommitHash, HASH_SIZE};
use crate::commit::utils::{read_slice, read_u32, read_u64};
use crate::table::RowId;

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

pub(crate) fn write_hash_dict(
    buf: &mut Vec<u8>,
    hash_mapper: &HashMapper,
) -> Result<(), PersistError> {
    let hash_count: u32 = hash_mapper
        .hashes()
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("too many hashes".into()))?;
    buf.write_all(&hash_count.to_le_bytes())?;
    for hash in hash_mapper.hashes() {
        buf.write_all(hash.as_bytes())?;
    }
    Ok(())
}

pub(crate) fn read_hash_dict(
    data: &[u8],
    pos: &mut usize,
) -> Result<Vec<CommitHash>, PersistError> {
    let hash_count = read_u32(data, pos, "hash dictionary count")? as usize;
    let mut hashes = Vec::with_capacity(hash_count);
    for _ in 0..hash_count {
        let bytes = read_slice(data, pos, HASH_SIZE, "hash dictionary entry")?;
        let mut hash = [0; HASH_SIZE];
        hash.copy_from_slice(bytes);
        hashes.push(CommitHash(hash));
    }
    Ok(hashes)
}

/// The rowid column needs to be converted from (commit_hash, cnt) -> (index, cnt)
/// And then encoded. This function handles this
pub(crate) fn encode_rowid_column(
    row_ids: &[RowId],
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, PersistError> {
    let mut hash_indices = Vec::with_capacity(row_ids.len());
    let mut counters = Vec::with_capacity(row_ids.len());
    for row_id in row_ids {
        let hash_index = hash_mapper.index(row_id.commit).ok_or_else(|| {
            PersistError::SchemaError(format!(
                "missing commit hash in dictionary: {}",
                row_id.commit
            ))
        })?;
        hash_indices.push(hash_index);
        counters.push(row_id.counter);
    }

    let hash_index_col = Column::<u32>::from_values(hash_indices).save();
    let counter_col = DeltaColumn::<u32>::from_values(counters).save();

    let mut buf = Vec::new();
    write_len_prefixed_data(&mut buf, &hash_index_col)?;
    write_len_prefixed_data(&mut buf, &counter_col)?;
    Ok(buf)
}

pub(crate) fn decode_row_id_column(
    data: &[u8],
    hashes: &[CommitHash],
) -> Result<Vec<RowId>, PersistError> {
    let mut pos = 0usize;
    let hash_index_len = read_u64(data, &mut pos, "row-id hash-index column length")? as usize;
    let hash_index_blob = read_slice(data, &mut pos, hash_index_len, "row-id hash-index column")?;
    let counter_len = read_u64(data, &mut pos, "row-id counter column length")? as usize;
    let counter_blob = read_slice(data, &mut pos, counter_len, "row-id counter column")?;
    if pos != data.len() {
        return Err(PersistError::DataFormatError(format!(
            "trailing bytes after row-id column: {} bytes",
            data.len() - pos
        )));
    }

    let hash_indices: Vec<u32> = Column::<u32>::load(hash_index_blob)?.iter().collect();
    let counters: Vec<u32> = DeltaColumn::<u32>::load(counter_blob)?.iter().collect();
    if hash_indices.len() != counters.len() {
        return Err(PersistError::DataFormatError(format!(
            "row-id subcolumn length mismatch: hash indexes {}, counters {}",
            hash_indices.len(),
            counters.len()
        )));
    }

    hash_indices
        .into_iter()
        .zip(counters)
        .map(|(hash_index, counter)| {
            let commit = hashes.get(hash_index as usize).copied().ok_or_else(|| {
                PersistError::DataFormatError(format!(
                    "row-id hash index {hash_index} out of bounds"
                ))
            })?;
            Ok(RowId { commit, counter })
        })
        .collect()
}

fn write_len_prefixed_data(buf: &mut Vec<u8>, data: &[u8]) -> Result<(), PersistError> {
    let len: u64 = data
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("blob too large".into()))?;
    buf.write_all(&len.to_le_bytes())?;
    buf.write_all(data)?;
    Ok(())
}
