// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub mod author;
pub mod chunk;
pub mod error;
pub mod graph;
pub mod hash;
pub(crate) mod hash_dict;
pub(crate) mod leb128;
pub mod pst;
pub(crate) mod utils;
pub mod wire;

use std::borrow::Cow;

use crate::{
    commit::{
        author::Author,
        chunk::{Chunk, ChunkType, Header},
        error::CodecError,
        hash::CommitHash,
        hash_dict::HashMapper,
        wire::{CommitData, root::RootCommitData},
    },
    ir::{Path, Schema},
    txn::ops::{Op, PendingOp, RowRef, TxnCellValue},
};

/// A commit: canonical payload bytes, content hash, and parsed metadata.
///
/// Same broad shape as Automerge’s `Change`: payload bytes plus decoded
/// metadata. Automerge keeps ops behind column metadata and a subslice for
/// lazy iteration; we decode ops eagerly into `PendingOp` while the encoding
/// is still row-wise.
///
/// [`Commit::bytes`] holds the payload only. [`Commit::header`] retains the
/// parsed or derived chunk header, and [`Chunk`] owns framed byte encoding.
/// The hash is
/// `blake3(chunk_type:u8 || data_len:u64_le || payload)`, computed over the
/// payload, so verifying a loaded commit is re-running
/// [`crate::commit::chunk::hash`] on [`Commit::payload`] and comparing to
/// [`Commit::hash`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit<'a> {
    /// Canonical payload bytes (the chunk body, without the header).
    bytes: Cow<'a, [u8]>,
    pub(crate) header: Header,
    pub author: Author,
    pub other_hashes: Vec<CommitHash>,
    /// parents of this commit
    pub deps: Vec<CommitHash>,
    /// Identifier of the commit author. Currently a placeholder of all zeros.
    pub timestamp: i64,
    pub message: Option<String>,
    /// Commit hashes referenced by op ids, dictionary order on the wire.
    /// Does not the hash of this transaction, which would be stored in header
    pub(crate) pending: Vec<PendingOp>, // NOTE consider dropping this once applied to save memory
}

impl Commit<'static> {
    /// Creating the commit data structure from the deserialized root data
    pub(crate) fn from_root_data(root: &RootCommitData) -> Result<Self, CodecError> {
        let bytes = wire::serialize_root(root)?;
        Ok(Self::from_root_bytes(bytes))
    }

    pub(crate) fn from_commit_data<'s, F>(
        mut data: CommitData,
        schema_for: F,
    ) -> Result<Self, CodecError>
    where
        F: Fn(&Path) -> Option<&'s Schema>,
    {
        let mut hash_mapper = HashMapper::new();
        collect_op_hashes(&data.pending, &mut hash_mapper);
        data.other_hashes = hash_mapper.hashes().to_vec();
        let bytes = wire::serialize(&data, &hash_mapper, schema_for)?;
        Ok(Self::from_commit_bytes(bytes, data))
    }

    fn from_root_bytes(bytes: Vec<u8>) -> Self {
        let header = Header::new(ChunkType::Root, &bytes);
        Self::from_root_payload(header, bytes)
    }

    fn from_root_payload(header: Header, bytes: Vec<u8>) -> Self {
        Commit {
            bytes: Cow::Owned(bytes),
            header,
            deps: vec![],
            author: Author::foo(),
            timestamp: 0,
            message: None,
            other_hashes: vec![],
            pending: vec![],
        }
    }

    fn from_commit_bytes(bytes: Vec<u8>, data: CommitData) -> Self {
        let header = Header::new(ChunkType::Commit, &bytes);
        Self::from_commit_payload(header, bytes, data)
    }

    fn from_commit_payload(header: Header, bytes: Vec<u8>, data: CommitData) -> Self {
        Commit {
            bytes: Cow::Owned(bytes),
            header,
            deps: data.deps,
            author: data.author,
            timestamp: data.timestamp,
            message: data.message,
            other_hashes: data.other_hashes,
            pending: data.pending,
        }
    }

    pub(crate) fn from_chunk<'s, F>(chunk: Chunk, schema_for: F) -> Result<Self, CodecError>
    where
        F: Fn(&Path) -> Option<&'s Schema>,
    {
        let (header, bytes) = chunk.into_parts();
        Self::decode_payload_with_header(header, bytes, schema_for)
    }

    fn decode_payload_with_header<'s, F>(
        header: Header,
        bytes: Vec<u8>,
        schema_for: F,
    ) -> Result<Self, CodecError>
    where
        F: Fn(&Path) -> Option<&'s Schema>,
    {
        match header.chunk_type {
            ChunkType::Root => {
                // check we can serialize the bytes into a root payload
                let _root = wire::deserialize_root(&bytes)?;
                Ok(Self::from_root_payload(header, bytes))
            }
            ChunkType::Commit => {
                let data = wire::deserialize(&bytes, schema_for)?;
                Ok(Self::from_commit_payload(header, bytes, data))
            }
        }
    }
}

impl<'a> Commit<'a> {
    pub fn hash(&self) -> CommitHash {
        self.header.hash
    }

    /// Canonical payload bytes: the slice that [`hash`](crate::commit::chunk::hash) is run on.
    pub fn payload(&self) -> &[u8] {
        self.bytes.as_ref()
    }

    /// Consumes the commit and returns its payload as bytes
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes.into_owned()
    }

    fn chunk_type(&self) -> ChunkType {
        self.header.chunk_type
    }

    pub fn is_root(&self) -> bool {
        self.chunk_type() == ChunkType::Root
    }

    pub(crate) fn root_payload(&self) -> Result<RootCommitData, CodecError> {
        if self.chunk_type() != ChunkType::Root {
            return Err(CodecError::ChunkMismatch {
                expected: ChunkType::Root,
                got: self.chunk_type(),
            });
        }

        wire::deserialize_root(self.payload())
    }

    pub(crate) fn resolved_ops(&self) -> Vec<Op> {
        let hash = self.hash();
        self.pending
            .iter()
            .map(|pending| pending.resolve(hash))
            .collect()
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
    use std::sync::LazyLock;

    use super::*;
    use crate::commit::chunk::{Chunk, hash};
    use crate::commit::hash::HASH_SIZE;
    use crate::commit::wire::root::{RootCommitData, RootTableEntry};
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path};
    use crate::table::RowId;
    use crate::txn::ops::{RowRef, TempRowId};

    fn zero_hash() -> CommitHash {
        CommitHash([0u8; HASH_SIZE])
    }

    fn int_schema() -> &'static Schema {
        static SCHEMA: LazyLock<Schema> = LazyLock::new(|| Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        });
        &SCHEMA
    }

    fn entity_pair_schema() -> &'static Schema {
        static SCHEMA: LazyLock<Schema> = LazyLock::new(|| Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![
                ColumnEntry {
                    path: Path::from("c0"),
                    col_type: ColType::RowId {
                        path: Path::from("T.E"),
                    },
                },
                ColumnEntry {
                    path: Path::from("c1"),
                    col_type: ColType::RowId {
                        path: Path::from("T.E"),
                    },
                },
            ],
            primary_key: None,
        });
        &SCHEMA
    }

    fn mixed_schema() -> &'static Schema {
        static SCHEMA: LazyLock<Schema> = LazyLock::new(|| Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![
                ColumnEntry {
                    path: Path::from("c0"),
                    col_type: ColType::RowId {
                        path: Path::from("T.E"),
                    },
                },
                ColumnEntry {
                    path: Path::from("c1"),
                    col_type: ColType::RowId {
                        path: Path::from("T.E"),
                    },
                },
                ColumnEntry {
                    path: Path::from("c2"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinStr,
                    },
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

    fn owned_int_schema() -> Schema {
        Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: Some(vec![Path::from("c0")]),
        }
    }

    fn data(
        deps: Vec<CommitHash>,
        author: Author,
        timestamp: i64,
        message: Option<&str>,
        pending: Vec<PendingOp>,
    ) -> CommitData {
        CommitData::new(deps, author, timestamp, message.map(str::to_owned), pending)
    }

    #[test]
    fn decode_root_preserves_payload_and_hash() {
        let root = RootCommitData {
            tables: vec![RootTableEntry {
                path: "T".to_owned(),
                oid: 0,
                schema: owned_int_schema(),
            }],
            laws: vec![],
        };
        let original = Commit::from_root_data(&root).expect("build root");

        let bytes = Chunk::from(&original).encoded();
        let chunk = Chunk::decode(&bytes).expect("decode root chunk");
        let decoded = Commit::from_chunk(chunk, |_| None).expect("decode root");

        assert_eq!(decoded.chunk_type(), ChunkType::Root);
        assert_eq!(decoded.hash(), original.hash());
        assert_eq!(decoded.payload(), original.payload());
        assert!(decoded.deps.is_empty());
        assert!(decoded.pending.is_empty());
    }

    #[test]
    fn decode_data_preserves_payload_metadata_and_ops() {
        let dep = zero_hash();
        let deps = vec![dep];
        let rid = RowId {
            commit: dep,
            counter: 7,
        };
        let pending = vec![
            PendingOp::Add {
                row_id: TempRowId(0),
                table: Path::from("T"),
                values: vec![1i64.into()],
            },
            PendingOp::Add {
                row_id: TempRowId(1),
                table: Path::from("U.V"),
                values: vec![
                    TxnCellValue::Id(RowRef::Existing(rid)),
                    TxnCellValue::Id(RowRef::Pending(TempRowId(0))),
                    TxnCellValue::Str("x".into()),
                ],
            },
        ];
        let original = Commit::from_commit_data(
            data(deps.clone(), Author::foo(), 42, Some("hi"), pending.clone()),
            payload_schema_for,
        )
        .expect("build commit");

        let bytes = Chunk::from(&original).encoded();
        let chunk = Chunk::decode(&bytes).expect("decode commit chunk");
        let decoded = Commit::from_chunk(chunk, payload_schema_for).expect("decode commit");

        assert_eq!(decoded.chunk_type(), ChunkType::Commit);
        assert_eq!(decoded.hash(), original.hash());
        assert_eq!(decoded.payload(), original.payload());
        assert_eq!(decoded.deps, deps);
        assert_eq!(decoded.timestamp, 42);
        assert_eq!(decoded.message.as_deref(), Some("hi"));
        assert_eq!(decoded.other_hashes, vec![dep]);
        assert_eq!(decoded.pending, pending);
    }

    #[test]
    fn build_produces_stable_hash() {
        let deps = vec![zero_hash()];
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::from_commit_data(
            data(deps.clone(), Author::foo(), 0, None, pending.clone()),
            |_| None,
        )
        .expect("build a");
        let b = Commit::from_commit_data(data(deps, Author::foo(), 0, None, pending), |_| None)
            .expect("build b");
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn different_timestamps_produce_different_hashes() {
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::from_commit_data(
            data(vec![], Author::foo(), 1, None, pending.clone()),
            |_| None,
        )
        .expect("build a");
        let b = Commit::from_commit_data(data(vec![], Author::foo(), 2, None, pending), |_| None)
            .expect("build b");
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn different_messages_produce_different_hashes() {
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, Some("hello"), pending.clone()),
            |_| None,
        )
        .expect("build a");
        let b = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, Some("world"), pending),
            |_| None,
        )
        .expect("build b");
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn different_authors_produce_different_hashes() {
        let pending: Vec<PendingOp> = vec![];
        let a = Commit::from_commit_data(
            data(
                vec![],
                Author::from(vec![0u8; 32]),
                0,
                None,
                pending.clone(),
            ),
            |_| None,
        )
        .expect("build a");
        let b = Commit::from_commit_data(
            data(vec![], Author::from(vec![1u8; 32]), 0, None, pending),
            |_| None,
        )
        .expect("build b");
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn different_ops_produce_different_hashes() {
        let op = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("T"),
            values: vec![42.into()],
        };
        let a = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, None, vec![op]),
            int_schema_for,
        )
        .expect("build a");

        let op2 = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("T"),
            values: vec![99.into()],
        };
        let b = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, None, vec![op2]),
            int_schema_for,
        )
        .expect("build b");

        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn hash_is_function_of_bytes() {
        let pending: Vec<PendingOp> = vec![];
        let commit =
            Commit::from_commit_data(data(vec![], Author::foo(), 0, None, pending), |_| None)
                .expect("build commit");
        let expected = hash(ChunkType::Commit, commit.payload());
        assert_eq!(commit.hash(), expected);
    }

    #[test]
    fn root_commit_wraps_and_decodes_root_payload() {
        let root = RootCommitData {
            tables: vec![RootTableEntry {
                path: "T".to_owned(),
                oid: 0,
                schema: owned_int_schema(),
            }],
            laws: vec![],
        };

        let commit = Commit::from_root_data(&root).expect("build root");

        assert_eq!(commit.chunk_type(), ChunkType::Root);
        assert!(commit.deps.is_empty());
        assert!(commit.pending.is_empty());
        assert_eq!(commit.hash(), hash(ChunkType::Root, commit.payload()));

        let decoded = commit.root_payload().expect("decode root payload");
        assert_eq!(decoded.tables.len(), 1);
        assert_eq!(decoded.tables[0].path, "T");
        assert_eq!(decoded.tables[0].oid, 0);
        assert_eq!(decoded.tables[0].schema.columns, owned_int_schema().columns);
        assert_eq!(
            decoded.tables[0].schema.primary_key,
            Some(vec![Path::from("c0")])
        );
        assert!(decoded.laws.is_empty());
    }

    #[test]
    fn root_payload_rejects_data_commit() {
        let commit =
            Commit::from_commit_data(data(vec![], Author::foo(), 0, None, vec![]), |_| None)
                .expect("build commit");

        assert!(matches!(
            commit.root_payload(),
            Err(CodecError::ChunkMismatch {
                expected: ChunkType::Root,
                got: ChunkType::Commit,
            })
        ));
    }

    #[test]
    fn build_records_metadata_and_pending_ops() {
        let dep = zero_hash();
        let deps = vec![dep];
        let author = Author::foo();
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
        let commit = Commit::from_commit_data(
            data(deps.clone(), author, 42, Some("hi"), pending.clone()),
            payload_schema_for,
        )
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
        let author = Author::foo();
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
        let commit = Commit::from_commit_data(
            data(deps, author, 42, Some("hi"), pending),
            payload_schema_for,
        )
        .expect("build commit");

        let got =
            wire::data::deserialize(commit.payload(), payload_schema_for).expect("decode commit");
        assert_eq!(got.deps, commit.deps);
        assert_eq!(got.author, commit.author);
        assert_eq!(got.timestamp, commit.timestamp);
        assert_eq!(got.message, commit.message);
        assert_eq!(got.other_hashes, commit.other_hashes);
        assert_eq!(got.pending, commit.pending);
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
        let commit = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, None, vec![op0, op1]),
            entity_pair_schema_for,
        )
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
        let no_row_refs = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, None, vec![op_int]),
            int_schema_for,
        )
        .expect("build");
        assert!(
            no_row_refs.other_hashes.is_empty(),
            "no Existing row refs → empty hash dictionary"
        );
    }
}
