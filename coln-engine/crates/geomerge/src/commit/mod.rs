pub mod graph;

use std::{borrow::Cow, fmt, io::Write};

use crate::{
    persist::chunk::{ChunkType, hash},
    persist::hash_dict::{HashMapper, write_hash_dict},
    table::{CellValue, RowId},
    txn::ops::{PendingOp, RowRef, TxnCellValue},
};

/// The number of bytes in a commit hash.
pub(crate) const HASH_SIZE: usize = 32;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct CommitHash(pub [u8; HASH_SIZE]);

impl CommitHash {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for CommitHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for CommitHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

pub(crate) struct Header {
    pub(crate) chunk_type: ChunkType,
    pub(crate) hash: CommitHash,
}

/// A commit chunk, holding the canonical serialized bytes and the precomputed
/// hash derived from them.
///
/// The hash is always `sha256(chunk_type:u8 || data_len:u64_le || bytes)`, so
/// verifying a loaded commit is just re-running [`chunk_hash_bytes`] over the
/// stored bytes and comparing.
pub struct Commit<'a> {
    /// The canonical payload bytes (everything after the chunk header).
    bytes: Cow<'a, [u8]>,
    pub(crate) header: Header,
    deps: Vec<CommitHash>,
    other_hashes: Vec<CommitHash>,
}

impl Commit<'static> {
    /// Serialize `pending` ops into canonical bytes, hash them, and return a
    /// fully-formed `Commit`.  This is the single place where serialization and
    /// hashing happen — the hash is always a function of these exact bytes.
    pub(crate) fn build(
        deps: &[CommitHash],
        // to make each txn unique
        nonce: [u8; 16],
        timestamp: i64,
        message: Option<&str>,
        pending: &[PendingOp],
    ) -> Self {
        let mut hash_mapper = HashMapper::new();
        collect_pending_hashes(pending, &mut hash_mapper);
        let other_hashes = hash_mapper.hashes().to_vec();
        let data = Self::encode(deps, nonce, timestamp, message, pending, &hash_mapper);
        let commit_hash = hash(ChunkType::Commit, &data);
        let header = Header {
            chunk_type: ChunkType::Commit,
            hash: commit_hash,
        };
        Commit {
            bytes: Cow::Owned(data),
            header,
            deps: deps.to_vec(),
            other_hashes,
        }
    }

    // ── Canonical payload encoding ───────────────────────────────────────────
    //
    // Layout (all integers little-endian):
    //
    //   [deps_count: u32]
    //   [CommitHash × deps_count]            (32 bytes each)
    //   [nonce: 16 bytes]                    (random, ensures uniqueness across clients)
    //   [timestamp: i64]
    //   [message_len: u32]                   (0 when None)
    //   [message: utf-8 bytes]
    //   [other_hash_count: u32]
    //   [CommitHash × other_hash_count]      (32 bytes each)
    //   [ops_count: u32]
    //   for each Add op (counter is implicit from position):
    //     [table_path_len: u32][table_path: utf-8]
    //     [values_count: u32]
    //     for each TxnCellValue:
    //       [tag: u8] + value bytes (see encode_txn_cell_value)
    //
    // CellValue tags:
    //   0 → Int(i64)      : 8 bytes le
    //   1 → Str(String)   : u32 len + utf-8 bytes
    //   2 → Id(RowId)     : u32 hash index + u32 counter le
    //   3 → Id(TempRowId) : u32 counter le

    // TODO switch to column encoding
    fn encode(
        deps: &[CommitHash],
        nonce: [u8; 16],
        timestamp: i64,
        message: Option<&str>,
        pending: &[PendingOp],
        hash_mapper: &HashMapper,
    ) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();

        buf.write_all(&(deps.len() as u32).to_le_bytes()).unwrap();
        for dep in deps {
            buf.write_all(dep.as_bytes()).unwrap();
        }

        buf.write_all(&nonce).unwrap();

        buf.write_all(&timestamp.to_le_bytes()).unwrap();

        let msg = message.unwrap_or("");
        buf.write_all(&(msg.len() as u32).to_le_bytes()).unwrap();
        buf.write_all(msg.as_bytes()).unwrap();

        write_hash_dict(&mut buf, hash_mapper).expect("hash dictionary serializes");

        buf.write_all(&(pending.len() as u32).to_le_bytes())
            .unwrap();
        for op in pending {
            match op {
                PendingOp::Add {
                    row_id: _,
                    table,
                    values,
                } => {
                    // Counter is implicit from the op's position in the list.
                    let path = table.to_string();
                    buf.write_all(&(path.len() as u32).to_le_bytes()).unwrap();
                    buf.write_all(path.as_bytes()).unwrap();

                    buf.write_all(&(values.len() as u32).to_le_bytes()).unwrap();
                    for value in values {
                        encode_txn_cell_value(&mut buf, value, hash_mapper);
                    }
                }
            }
        }

        buf
    }
}

impl<'a> Commit<'a> {
    pub fn hash(&self) -> CommitHash {
        self.header.hash
    }
}

fn collect_pending_hashes(pending: &[PendingOp], hash_mapper: &mut HashMapper) {
    for op in pending {
        let PendingOp::Add { values, .. } = op;
        for value in values {
            if let TxnCellValue::Id(RowRef::Existing(row_id)) = value {
                hash_mapper.insert(row_id.commit);
            }
        }
    }
}

fn encode_txn_cell_value(buf: &mut Vec<u8>, value: &TxnCellValue, hash_mapper: &HashMapper) {
    match value {
        TxnCellValue::Id(RowRef::Existing(row_id)) => {
            encode_cell_value(buf, &CellValue::Id(*row_id), hash_mapper)
        }
        TxnCellValue::Id(RowRef::Pending(temp_id)) => {
            buf.push(3);
            buf.write_all(&temp_id.0.to_le_bytes()).unwrap();
        }
        TxnCellValue::Int(value) => encode_cell_value(buf, &CellValue::Int(*value), hash_mapper),
        TxnCellValue::Str(value) => {
            encode_cell_value(buf, &CellValue::Str(value.clone()), hash_mapper)
        }
    }
}

fn encode_cell_value(buf: &mut Vec<u8>, value: &CellValue, hash_mapper: &HashMapper) {
    match value {
        CellValue::Int(i) => {
            buf.push(0);
            buf.write_all(&i.to_le_bytes()).unwrap();
        }
        CellValue::Str(s) => {
            buf.push(1);
            buf.write_all(&(s.len() as u32).to_le_bytes()).unwrap();
            buf.write_all(s.as_bytes()).unwrap();
        }
        CellValue::Id(RowId { commit, counter }) => {
            buf.push(2);
            let hash_idx = hash_mapper
                .index(*commit)
                .expect("hash collected before encode");
            buf.write_all(&hash_idx.to_le_bytes()).unwrap();
            buf.write_all(&counter.to_le_bytes()).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Path;

    fn zero_hash() -> CommitHash {
        CommitHash([0u8; HASH_SIZE])
    }

    #[test]
    fn build_produces_stable_hash() {
        let deps = vec![zero_hash()];
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::build(&deps, [0u8; 16], 0, None, &pending);
        let b = Commit::build(&deps, [0u8; 16], 0, None, &pending);
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn different_timestamps_produce_different_hashes() {
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::build(&[], [0u8; 16], 1, None, &pending);
        let b = Commit::build(&[], [0u8; 16], 2, None, &pending);
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn different_messages_produce_different_hashes() {
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::build(&[], [0u8; 16], 0, Some("hello"), &pending);
        let b = Commit::build(&[], [0u8; 16], 0, Some("world"), &pending);
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn different_nonces_produce_different_hashes() {
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::build(&[], [0u8; 16], 0, None, &pending);
        let b = Commit::build(&[], [1u8; 16], 0, None, &pending);
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn different_ops_produce_different_hashes() {
        use crate::txn::ops::TempRowId;

        let op = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("T"),
            values: vec![42.into()],
        };
        let a = Commit::build(&[], [0u8; 16], 0, None, &[op]);

        let op2 = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("T"),
            values: vec![99.into()],
        };
        let b = Commit::build(&[], [0u8; 16], 0, None, &[op2]);

        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn hash_is_function_of_bytes() {
        let pending: Vec<PendingOp> = vec![];
        let commit = Commit::build(&[], [0u8; 16], 0, None, &pending);
        let expected = hash(ChunkType::Commit, &commit.bytes);
        assert_eq!(commit.hash(), expected);
    }
}
