pub mod chunk;
pub mod error;
pub mod graph;
pub mod hash;
pub(crate) mod hash_dict;
pub(crate) mod utils;
pub mod wire;

use std::borrow::Cow;

use crate::{
    commit::chunk::{ChunkType, hash},
    commit::error::PersistError,
    commit::hash::CommitHash,
    commit::hash_dict::HashMapper,
    ir::{Path, Schema},
    txn::ops::{PendingOp, RowRef, TxnCellValue},
};

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
    pub(crate) fn build<'s, F>(
        deps: &[CommitHash],
        // TODO remove this, or replace with author id
        nonce: [u8; 16], // to make each txn unique
        timestamp: i64,
        message: Option<&str>,
        pending: &[PendingOp],
        schema_for: F,
    ) -> Result<Self, PersistError>
    where
        F: Fn(&Path) -> Option<&'s Schema>,
    {
        let mut hash_mapper = HashMapper::new();
        collect_op_hashes(pending, &mut hash_mapper);
        let other_hashes = hash_mapper.hashes().to_vec();
        let data = wire::serialise(
            deps,
            nonce,
            timestamp,
            message,
            pending,
            &hash_mapper,
            schema_for,
        )?;
        let commit_hash = hash(ChunkType::Commit, &data);
        let header = Header {
            chunk_type: ChunkType::Commit,
            hash: commit_hash,
        };
        Ok(Commit {
            bytes: Cow::Owned(data),
            header,
            deps: deps.to_vec(),
            timestamp,
            message: message.map(|s| s.to_owned()),
            other_hashes,
            pending: pending.to_vec(),
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::hash::HASH_SIZE;
    use crate::ir::{ColType, Path, PrimType};
    use crate::table::RowId;
    use crate::txn::ops::{RowRef, TempRowId};
    use std::sync::LazyLock;

    fn zero_hash() -> CommitHash {
        CommitHash([0u8; HASH_SIZE])
    }

    fn int_schema() -> &'static Schema {
        static SCHEMA: LazyLock<Schema> = LazyLock::new(|| Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        });
        &SCHEMA
    }

    fn entity_pair_schema() -> &'static Schema {
        static SCHEMA: LazyLock<Schema> = LazyLock::new(|| Schema {
            columns: vec![
                ColType::EntityType {
                    path: Path::from("T.E"),
                },
                ColType::EntityType {
                    path: Path::from("T.E"),
                },
            ],
            primary_key: None,
        });
        &SCHEMA
    }

    fn mixed_schema() -> &'static Schema {
        static SCHEMA: LazyLock<Schema> = LazyLock::new(|| Schema {
            columns: vec![
                ColType::EntityType {
                    path: Path::from("T.E"),
                },
                ColType::EntityType {
                    path: Path::from("T.E"),
                },
                ColType::PrimType {
                    prim: PrimType::PrimString,
                },
            ],
            primary_key: None,
        });
        &SCHEMA
    }

    fn int_schema_for(path: &Path) -> Option<&'static Schema> {
        (path == &Path::from("T")).then_some(int_schema())
    }

    fn payload_schema_for(path: &Path) -> Option<&'static Schema> {
        if path == &Path::from("T") {
            Some(int_schema())
        } else if path == &Path::from("U.V") {
            Some(mixed_schema())
        } else {
            None
        }
    }

    fn entity_pair_schema_for(path: &Path) -> Option<&'static Schema> {
        (path == &Path::from("T")).then_some(entity_pair_schema())
    }

    #[test]
    fn build_produces_stable_hash() {
        let deps = vec![zero_hash()];
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::build(&deps, [0u8; 16], 0, None, &pending, |_| None).expect("build a");
        let b = Commit::build(&deps, [0u8; 16], 0, None, &pending, |_| None).expect("build b");
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn different_timestamps_produce_different_hashes() {
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::build(&[], [0u8; 16], 1, None, &pending, |_| None).expect("build a");
        let b = Commit::build(&[], [0u8; 16], 2, None, &pending, |_| None).expect("build b");
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn different_messages_produce_different_hashes() {
        let pending: Vec<PendingOp> = vec![];
        let a =
            Commit::build(&[], [0u8; 16], 0, Some("hello"), &pending, |_| None).expect("build a");
        let b =
            Commit::build(&[], [0u8; 16], 0, Some("world"), &pending, |_| None).expect("build b");
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn different_nonces_produce_different_hashes() {
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::build(&[], [0u8; 16], 0, None, &pending, |_| None).expect("build a");
        let b = Commit::build(&[], [1u8; 16], 0, None, &pending, |_| None).expect("build b");
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn different_ops_produce_different_hashes() {
        let op = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("T"),
            values: vec![42.into()],
        };
        let a = Commit::build(&[], [0u8; 16], 0, None, &[op], int_schema_for).expect("build a");

        let op2 = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("T"),
            values: vec![99.into()],
        };
        let b = Commit::build(&[], [0u8; 16], 0, None, &[op2], int_schema_for).expect("build b");

        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn hash_is_function_of_bytes() {
        let pending: Vec<PendingOp> = vec![];
        let commit =
            Commit::build(&[], [0u8; 16], 0, None, &pending, |_| None).expect("build commit");
        let expected = hash(ChunkType::Commit, commit.payload());
        assert_eq!(commit.hash(), expected);
    }

    #[test]
    fn build_records_metadata_and_pending_ops() {
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
        let commit = Commit::build(&deps, nonce, 42, Some("hi"), &pending, payload_schema_for)
            .expect("build commit");

        assert_eq!(commit.deps, deps);
        assert_eq!(commit.timestamp, 42);
        assert_eq!(commit.message.as_deref(), Some("hi"));
        assert_eq!(commit.other_hashes, vec![dep]);
        assert_eq!(commit.pending, pending);
        assert!(!commit.payload().is_empty());
    }

    #[test]
    fn payload_decode_round_trips_columnar_commit() {
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
        let commit = Commit::build(&deps, nonce, 42, Some("hi"), &pending, payload_schema_for)
            .expect("build commit");

        let got = wire::deserialise(commit.payload(), payload_schema_for).expect("decode commit");
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
            values: vec![
                TxnCellValue::Id(RowRef::Existing(rid_a)),
                TxnCellValue::Id(RowRef::Existing(rid_a)),
            ],
        };
        let op1 = PendingOp::Add {
            row_id: TempRowId(1),
            table: Path::from("T"),
            values: vec![
                TxnCellValue::Id(RowRef::Existing(rid_b)),
                TxnCellValue::Id(RowRef::Existing(rid_a_later)),
            ],
        };
        let commit = Commit::build(&[], [0u8; 16], 0, None, &[op0, op1], entity_pair_schema_for)
            .expect("build commit");
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
        let no_row_refs =
            Commit::build(&[], [0u8; 16], 0, None, &[op_int], int_schema_for).expect("build");
        assert!(
            no_row_refs.other_hashes.is_empty(),
            "no Existing row refs → empty hash dictionary"
        );
    }
}
