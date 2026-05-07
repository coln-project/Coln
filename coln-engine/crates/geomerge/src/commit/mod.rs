pub mod graph;

use std::{borrow::Cow, fmt, io::Write};

use crate::{
    ir::Path,
    persist::chunk::{ChunkType, hash},
    persist::error::PersisError,
    persist::hash_dict::{HashMapper, read_hash_dict, write_hash_dict},
    persist::utils::{read_slice, read_u32},
    table::{CellValue, RowId},
    txn::ops::{PendingOp, RowRef, TempRowId, TxnCellValue},
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) chunk_type: ChunkType,
    pub(crate) hash: CommitHash,
}

/// A commit: canonical payload bytes, content hash, and parsed metadata.
///
/// Same broad shape as Automerge’s `Change`: raw payload bytes plus decoded
/// metadata. Automerge keeps ops behind column metadata and a subslice for
/// lazy iteration; we decode ops eagerly into `PendingOp` while the encoding
/// is still row-wise.
///
/// TODO switch to columnar encoding at some point, currently this adds complexity
/// as a single commit might have additions to multiple tables of different shapes.
///
/// The hash is always `sha256(chunk_type:u8 || data_len:u64_le || bytes)`, so
/// verifying a loaded commit is re-running [`crate::persist::chunk::hash`] on
/// [`Commit::payload`] and comparing to [`Commit::hash`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit<'a> {
    /// Canonical payload bytes (everything after the chunk header).
    bytes: Cow<'a, [u8]>,
    pub(crate) header: Header,
    pub deps: Vec<CommitHash>,
    pub timestamp: i64,
    pub message: Option<String>,
    /// Commit hashes referenced by op ids, dictionary order on the wire.
    /// Does not the hash of this transaction, which would be stored in header
    pub other_hashes: Vec<CommitHash>,
    pub(crate) pending: Vec<PendingOp>,
}

impl Commit<'static> {
    /// Serialize `pending` ops into canonical bytes, hash them, and return a
    /// fully-formed `Commit`.  This is the single place where serialization and
    /// hashing happen — the hash is always a function of these exact bytes.
    pub(crate) fn build(
        deps: &[CommitHash],
        nonce: [u8; 16], // to make each txn unique
        timestamp: i64,
        message: Option<&str>,
        pending: &[PendingOp],
    ) -> Self {
        let mut hash_mapper = HashMapper::new();
        collect_op_hashes(pending, &mut hash_mapper);
        let other_hashes = hash_mapper.hashes().to_vec();
        let data = Self::serialise(deps, nonce, timestamp, message, pending, &hash_mapper);
        let commit_hash = hash(ChunkType::Commit, &data);
        let header = Header {
            chunk_type: ChunkType::Commit,
            hash: commit_hash,
        };
        Commit {
            bytes: Cow::Owned(data),
            header,
            deps: deps.to_vec(),
            timestamp,
            message: message.map(|s| s.to_owned()),
            other_hashes,
            pending: pending.to_vec(),
        }
    }

    /// Parse canonical payload bytes (the slice passed to [`crate::persist::chunk::hash`],
    /// not including the chunk type or outer length prefix).
    ///
    /// Sets `header.hash` from [`crate::persist::chunk::hash`] applied to `ChunkType::Commit` and `data`.
    /// When loading from storage, compare that to the hash in the outer chunk header.
    pub fn deserialise(data: &[u8]) -> Result<Self, PersisError> {
        let mut pos = 0usize;

        let deps_count = read_u32(data, &mut pos, "deps count")? as usize;
        let mut deps = Vec::with_capacity(deps_count);
        for _ in 0..deps_count {
            let bytes = read_slice(data, &mut pos, HASH_SIZE, "dep hash")?;
            let mut h = [0u8; HASH_SIZE];
            h.copy_from_slice(bytes);
            deps.push(CommitHash(h));
        }

        read_slice(data, &mut pos, 16, "nonce")?;

        let ts_bytes = read_slice(data, &mut pos, 8, "timestamp")?;
        let timestamp = i64::from_le_bytes(ts_bytes.try_into().unwrap());

        let msg_len = read_u32(data, &mut pos, "message length")? as usize;
        let msg_bytes = read_slice(data, &mut pos, msg_len, "message")?;
        let message = if msg_len == 0 {
            None
        } else {
            Some(
                std::str::from_utf8(msg_bytes)
                    .map_err(|_| {
                        PersisError::DataFormatError("commit message: invalid utf-8".into())
                    })?
                    .to_owned(),
            )
        };

        let other_hashes = read_hash_dict(data, &mut pos)?;

        let ops_count = read_u32(data, &mut pos, "ops count")? as usize;
        let mut pending = Vec::with_capacity(ops_count);
        for op_idx in 0..ops_count {
            let path_len = read_u32(data, &mut pos, "table path length")? as usize;
            let path_bytes = read_slice(data, &mut pos, path_len, "table path")?;
            let table =
                Path::from(std::str::from_utf8(path_bytes).map_err(|_| {
                    PersisError::DataFormatError("table path: invalid utf-8".into())
                })?);

            let values_count = read_u32(data, &mut pos, "values count")? as usize;
            let mut values = Vec::with_capacity(values_count);
            for _ in 0..values_count {
                values.push(decode_txn_cell_value(data, &mut pos, &other_hashes)?);
            }

            pending.push(PendingOp::Add {
                row_id: TempRowId(op_idx as u32),
                table,
                values,
            });
        }

        if pos != data.len() {
            return Err(PersisError::DataFormatError(format!(
                "trailing bytes after commit payload: {} bytes",
                data.len() - pos
            )));
        }

        let owned: Vec<u8> = data.to_vec();
        let commit_hash = hash(ChunkType::Commit, &owned);
        let header = Header {
            chunk_type: ChunkType::Commit,
            hash: commit_hash,
        };

        Ok(Commit {
            bytes: Cow::Owned(owned),
            header,
            deps,
            timestamp,
            message,
            other_hashes,
            pending,
        })
    }

    // ── Canonical payload encoding ───────────────────────────────────────────
    //
    // Layout (all integers little-endian):
    //
    //   [deps_count: u32]
    //   [CommitHash × deps_count]            (32 bytes each)
    //   [nonce: 16 bytes]                    (random; part of payload only, not a Commit field)
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

    // TODO switch to column encoding, currently this is row encoded
    fn serialise(
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

    /// Canonical payload bytes (the same buffer that [`hash`](crate::persist::chunk::hash) is run on).
    pub fn payload(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

// collects all the hashes that are mentioned in the ops.
fn collect_op_hashes(pending: &[PendingOp], hash_mapper: &mut HashMapper) {
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

fn decode_txn_cell_value(
    data: &[u8],
    pos: &mut usize,
    hashes: &[CommitHash],
) -> Result<TxnCellValue, PersisError> {
    let tag = read_slice(data, pos, 1, "txn cell tag")?[0];
    match tag {
        0 => {
            let b = read_slice(data, pos, 8, "txn int")?;
            Ok(TxnCellValue::Int(i64::from_le_bytes(b.try_into().unwrap())))
        }
        1 => {
            let slen = read_u32(data, pos, "txn string length")? as usize;
            let sbytes = read_slice(data, pos, slen, "txn string")?;
            let s = std::str::from_utf8(sbytes)
                .map_err(|_| PersisError::DataFormatError("txn string: invalid utf-8".into()))?;
            Ok(TxnCellValue::Str(s.to_owned()))
        }
        2 => {
            let hash_idx = read_u32(data, pos, "txn rowid hash index")? as usize;
            let counter = read_u32(data, pos, "txn rowid counter")?;
            let commit = hashes.get(hash_idx).copied().ok_or_else(|| {
                PersisError::DataFormatError(format!(
                    "txn row id: hash index {hash_idx} out of range (len {})",
                    hashes.len()
                ))
            })?;
            Ok(TxnCellValue::Id(RowRef::Existing(RowId {
                commit,
                counter,
            })))
        }
        3 => {
            let t = read_u32(data, pos, "txn temp row id")?;
            Ok(TxnCellValue::Id(RowRef::Pending(TempRowId(t))))
        }
        other => Err(PersisError::DataFormatError(format!(
            "unknown txn cell tag {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Path;
    use crate::table::RowId;
    use crate::txn::ops::{RowRef, TempRowId};

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
        let expected = hash(ChunkType::Commit, commit.payload());
        assert_eq!(commit.hash(), expected);
    }

    #[test]
    fn payload_decode_round_trips_build() {
        let dep = zero_hash();
        let deps = vec![dep];
        let nonce = [5u8; 16];
        let rid = RowId {
            commit: dep,
            counter: 7,
        };
        let op0 = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("T"),
            values: vec![1i64.into()],
        };
        let op1 = PendingOp::Add {
            row_id: TempRowId(1),
            table: Path::from("U.V"),
            values: vec![
                TxnCellValue::Id(RowRef::Existing(rid)),
                TxnCellValue::Id(RowRef::Pending(TempRowId(0))),
                TxnCellValue::Str("x".into()),
            ],
        };
        let pending = vec![op0, op1];
        let commit = Commit::build(&deps, nonce, 42, Some("hi"), &pending);
        let got = Commit::deserialise(commit.payload()).expect("decode");
        assert_eq!(got, commit);
    }

    #[test]
    fn other_hashes_contain_right_hashes() {
        let ha = CommitHash([1u8; HASH_SIZE]);
        let hb = CommitHash([2u8; HASH_SIZE]);
        let rid_a = RowId {
            commit: ha,
            counter: 0,
        };
        let rid_b = RowId {
            commit: hb,
            counter: 3,
        };
        // also point to ha
        let rid_a_later = RowId {
            commit: ha,
            counter: 99,
        };

        let op0 = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("T"),
            values: vec![TxnCellValue::Id(RowRef::Existing(rid_a))],
        };
        let op1 = PendingOp::Add {
            row_id: TempRowId(1),
            table: Path::from("T"),
            values: vec![
                TxnCellValue::Id(RowRef::Existing(rid_b)),
                TxnCellValue::Id(RowRef::Existing(rid_a_later)),
            ],
        };
        let commit = Commit::build(&[], [0u8; 16], 0, None, &[op0, op1]);
        assert_eq!(
            commit.other_hashes,
            vec![ha, hb],
            "hash dict lists each referenced commit once, in first-seen order"
        );

        let op_int = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("T"),
            values: vec![42.into()],
        };
        let no_row_refs = Commit::build(&[], [0u8; 16], 0, None, &[op_int]);
        assert!(
            no_row_refs.other_hashes.is_empty(),
            "no Existing row refs → empty hash dictionary"
        );
    }
}
