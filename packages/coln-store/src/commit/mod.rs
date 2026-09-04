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

use coln_flir_rs::ir::FlatRealm;

use crate::{
    commit::{
        author::Author,
        chunk::{Chunk, ChunkType, Header},
        error::CodecError,
        hash::CommitHash,
        hash_dict::HashMapper,
        wire::CommitData,
    },
    ir::Path,
    op::Op,
    table::{TableMeta, TableOid},
    txn::{PendingOp, TxnWireRowId, TxnWireValue},
};

/// A commit: canonical payload bytes, content hash, and parsed metadata.
///
/// Same broad shape as Automerge’s `Change`: payload bytes plus decoded
/// metadata. Like Automerge, ops stay encoded in the payload and are decoded
/// on demand (see [`Commit::resolved_ops`]); only the small metadata fields
/// are kept decoded. This keeps the commit graph from retaining a second,
/// decoded copy of every op.
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
    /// Commit hashes referenced by op ids, dictionary order on the wire.
    /// Does not store the hash of this transaction, which would be stored in header
    pub other_hashes: Vec<CommitHash>,
    /// parents of this commit
    pub deps: Vec<CommitHash>,
    /// Identifier of the commit author. Currently a placeholder of all zeros.
    pub timestamp: i64,
    pub message: Option<String>,
}

impl Commit<'static> {
    /// Creating the commit data structure from the deserialized root data
    pub(crate) fn from_root_data(root: &FlatRealm) -> Result<Self, CodecError> {
        let bytes = wire::serialize_root(root)?;
        Ok(Self::from_root_bytes(bytes))
    }

    pub(crate) fn from_commit_data<'s, F>(
        mut data: CommitData,
        table_meta_for: F,
    ) -> Result<Self, CodecError>
    where
        F: Fn(TableOid) -> Option<TableMeta<'s>>,
    {
        let mut hash_mapper = HashMapper::new();
        collect_op_hashes(&data.pending, &mut hash_mapper);
        data.other_hashes = hash_mapper.hashes().to_vec();
        let bytes = wire::serialize(&data, &hash_mapper, table_meta_for)?;
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
        }
    }

    pub(crate) fn from_chunk<'s, F>(chunk: Chunk, table_meta_for: F) -> Result<Self, CodecError>
    where
        F: Fn(&Path) -> Option<TableMeta<'s>>,
    {
        let (header, bytes) = chunk.into_parts();
        Self::decode_payload_with_header(header, bytes, table_meta_for)
    }

    fn decode_payload_with_header<'s, F>(
        header: Header,
        bytes: Vec<u8>,
        table_meta_for: F,
    ) -> Result<Self, CodecError>
    where
        F: Fn(&Path) -> Option<TableMeta<'s>>,
    {
        match header.chunk_type {
            ChunkType::Root => {
                // check we can serialize the bytes into a root payload
                let _root = wire::deserialize_root(&bytes)?;
                Ok(Self::from_root_payload(header, bytes))
            }
            ChunkType::Commit => {
                let data = wire::deserialize(&bytes, table_meta_for)?;
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

    pub(crate) fn root_payload(&self) -> Result<FlatRealm, CodecError> {
        if self.chunk_type() != ChunkType::Root {
            return Err(CodecError::ChunkMismatch {
                expected: ChunkType::Root,
                got: self.chunk_type(),
            });
        }

        wire::deserialize_root(self.payload())
    }

    /// Ops of this commit with row ids resolved against the commit hash.
    ///
    /// Ops are not retained in decoded form: this re-decodes them from the
    /// canonical payload, so the returned iterator owns its data and borrows
    /// neither the commit nor `table_meta_for`, hence the 'static.
    ///
    /// Root commits carry no ops and yield an empty iterator.
    pub(crate) fn resolved_ops<'s, F>(&self, table_meta_for: F) -> Result<Vec<Op>, CodecError>
    where
        F: Fn(&Path) -> Option<TableMeta<'s>>,
    {
        let hash = self.hash();
        let pending = if self.is_root() {
            vec![]
        } else {
            wire::deserialize(self.payload(), table_meta_for)?.pending
        };
        Ok(pending
            .into_iter()
            .map(move |pending| pending.resolve(hash))
            .collect())
    }
}

// collects all the hashes that are mentioned in the ops.
fn collect_op_hashes(pending: &[PendingOp], hash_mapper: &mut HashMapper) {
    for op in pending {
        let PendingOp::Add { values, .. } = op;
        for value in values {
            if let TxnWireValue::Id(TxnWireRowId::Existing(row_id)) = value {
                hash_mapper.insert(row_id.commit);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use coln_flir_rs::ir::Schema;

    use super::*;
    use crate::commit::chunk::{Chunk, hash};
    use crate::commit::hash::HASH_SIZE;
    use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, TableEntry};
    use crate::table::{TableMeta, TableOid, WireRowId};
    use crate::txn::{TempRowId, TxnWireRowId};

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

    fn path_t() -> &'static Path {
        static PATH: LazyLock<Path> = LazyLock::new(|| Path::from("T"));
        &PATH
    }

    fn path_uv() -> &'static Path {
        static PATH: LazyLock<Path> = LazyLock::new(|| Path::from("U.V"));
        &PATH
    }

    fn int_encode_table_meta(oid: TableOid) -> Option<TableMeta<'static>> {
        (oid == 0).then_some(TableMeta {
            path: path_t(),
            oid: 0,
            schema: int_schema(),
        })
    }

    fn payload_encode_table_meta(oid: TableOid) -> Option<TableMeta<'static>> {
        match oid {
            0 => Some(TableMeta {
                path: path_t(),
                oid: 0,
                schema: int_schema(),
            }),
            1 => Some(TableMeta {
                path: path_uv(),
                oid: 1,
                schema: mixed_schema(),
            }),
            _ => None,
        }
    }

    fn payload_decode_table_meta(path: &Path) -> Option<TableMeta<'static>> {
        if path == path_t() {
            Some(TableMeta {
                path: path_t(),
                oid: 0,
                schema: int_schema(),
            })
        } else if path == path_uv() {
            Some(TableMeta {
                path: path_uv(),
                oid: 1,
                schema: mixed_schema(),
            })
        } else {
            None
        }
    }

    fn entity_pair_encode_table_meta(oid: TableOid) -> Option<TableMeta<'static>> {
        (oid == 0).then_some(TableMeta {
            path: path_t(),
            oid: 0,
            schema: entity_pair_schema(),
        })
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

    fn int_theory() -> FlatRealm {
        FlatRealm {
            tables: vec![TableEntry {
                path: Path::from("T"),
                table: owned_int_schema(),
            }],
            rules: vec![],
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
        let original = Commit::from_root_data(&int_theory()).expect("build root");

        let bytes = Chunk::from(&original).encoded();
        let chunk = Chunk::decode(&bytes).expect("decode root chunk");
        let decoded = Commit::from_chunk(chunk, |_| None).expect("decode root");

        assert_eq!(decoded.chunk_type(), ChunkType::Root);
        assert_eq!(decoded.hash(), original.hash());
        assert_eq!(decoded.payload(), original.payload());
        assert!(decoded.deps.is_empty());
        assert_eq!(
            decoded.resolved_ops(|_| None).expect("resolve ops").len(),
            0,
            "root commits carry no ops"
        );
    }

    #[test]
    fn decode_data_preserves_payload_metadata_and_ops() {
        let dep = zero_hash();
        let deps = vec![dep];
        let rid = WireRowId {
            commit: dep,
            counter: 7,
        };
        let pending = vec![
            PendingOp::Add {
                row_id: TempRowId(0),
                table: 0,
                values: vec![1i32.into()],
            },
            PendingOp::Add {
                row_id: TempRowId(1),
                table: 1,
                values: vec![
                    TxnWireValue::Id(TxnWireRowId::Existing(rid)),
                    TxnWireValue::Id(TxnWireRowId::Pending(TempRowId(0))),
                    TxnWireValue::Str("x".into()),
                ],
            },
        ];
        let original = Commit::from_commit_data(
            data(deps.clone(), Author::foo(), 42, Some("hi"), pending.clone()),
            payload_encode_table_meta,
        )
        .expect("build commit");

        let bytes = Chunk::from(&original).encoded();
        let chunk = Chunk::decode(&bytes).expect("decode commit chunk");
        let decoded = Commit::from_chunk(chunk, payload_decode_table_meta).expect("decode commit");

        assert_eq!(decoded.chunk_type(), ChunkType::Commit);
        assert_eq!(decoded.hash(), original.hash());
        assert_eq!(decoded.payload(), original.payload());
        assert_eq!(decoded.deps, deps);
        assert_eq!(decoded.timestamp, 42);
        assert_eq!(decoded.message.as_deref(), Some("hi"));
        assert_eq!(decoded.other_hashes, vec![dep]);

        // Ops are decoded from the payload on demand and resolve against the
        // commit hash.
        let hash = decoded.hash();
        let expected: Vec<Op> = pending.iter().map(|op| op.resolve(hash)).collect();
        let got: Vec<Op> = decoded
            .resolved_ops(payload_decode_table_meta)
            .expect("resolve ops");
        assert_eq!(got, expected);
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
            table: 0,
            values: vec![42.into()],
        };
        let a = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, None, vec![op]),
            int_encode_table_meta,
        )
        .expect("build a");

        let op2 = PendingOp::Add {
            row_id: TempRowId(0),
            table: 0,
            values: vec![99.into()],
        };
        let b = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, None, vec![op2]),
            int_encode_table_meta,
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
        let root = int_theory();
        let commit = Commit::from_root_data(&root).expect("build root");

        assert_eq!(commit.chunk_type(), ChunkType::Root);
        assert!(commit.deps.is_empty());
        assert_eq!(commit.resolved_ops(|_| None).expect("resolve ops").len(), 0);
        assert_eq!(commit.hash(), hash(ChunkType::Root, commit.payload()));

        let decoded = commit.root_payload().expect("decode root payload");
        assert_eq!(decoded.tables.len(), 1);
        assert_eq!(decoded.tables[0].path, Path::from("T"));
        assert_eq!(decoded.tables[0].table.columns, owned_int_schema().columns);
        assert_eq!(
            decoded.tables[0].table.primary_key,
            Some(vec![Path::from("c0")])
        );
        assert!(decoded.rules.is_empty());
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
        let rid = WireRowId {
            commit: dep,
            counter: 7,
        };
        let op0 = PendingOp::Add {
            row_id: TempRowId(0),
            table: 0,
            values: vec![1i32.into()],
        };
        let op1 = PendingOp::Add {
            row_id: TempRowId(1),
            table: 1,
            values: vec![
                TxnWireValue::Id(TxnWireRowId::Existing(rid)),
                TxnWireValue::Id(TxnWireRowId::Pending(TempRowId(0))),
                TxnWireValue::Str("x".into()),
            ],
        };
        let pending = vec![op0, op1];
        let commit = Commit::from_commit_data(
            data(deps.clone(), author, 42, Some("hi"), pending.clone()),
            payload_encode_table_meta,
        )
        .expect("build commit");

        assert_eq!(commit.deps, deps);
        assert_eq!(commit.timestamp, 42);
        assert_eq!(commit.message.as_deref(), Some("hi"));
        assert_eq!(commit.other_hashes, vec![dep]);
        assert_eq!(
            wire::data::deserialize(commit.payload(), payload_decode_table_meta)
                .expect("decode payload")
                .pending,
            pending,
            "ops live in the payload, decodable on demand"
        );
        assert!(!commit.payload().is_empty());
    }

    #[test]
    fn payload_decode_round_trips_columnar_commit() {
        let dep = zero_hash();
        let deps = vec![dep];
        let author = Author::foo();
        let rid = WireRowId {
            commit: dep,
            counter: 7,
        };
        let op0 = PendingOp::Add {
            row_id: TempRowId(0),
            table: 0,
            values: vec![1i32.into()],
        };
        let op1 = PendingOp::Add {
            row_id: TempRowId(1),
            table: 1,
            values: vec![
                TxnWireValue::Id(TxnWireRowId::Existing(rid)),
                TxnWireValue::Id(TxnWireRowId::Pending(TempRowId(0))),
                TxnWireValue::Str("x".into()),
            ],
        };
        let pending = vec![op0, op1];
        let commit = Commit::from_commit_data(
            data(deps, author, 42, Some("hi"), pending.clone()),
            payload_encode_table_meta,
        )
        .expect("build commit");

        let got = wire::data::deserialize(commit.payload(), payload_decode_table_meta)
            .expect("decode commit");
        assert_eq!(got.deps, commit.deps);
        assert_eq!(got.author, commit.author);
        assert_eq!(got.timestamp, commit.timestamp);
        assert_eq!(got.message, commit.message);
        assert_eq!(got.other_hashes, commit.other_hashes);
        assert_eq!(got.pending, pending);
    }

    #[test]
    fn other_hashes_contain_right_hashes() {
        let ha = CommitHash([1u8; HASH_SIZE]);
        let hb = CommitHash([2u8; HASH_SIZE]);
        let rid_a = WireRowId {
            commit: ha,
            counter: 0,
        };
        let rid_b = WireRowId {
            commit: hb,
            counter: 3,
        };
        // also point to ha
        let rid_a_later = WireRowId {
            commit: ha,
            counter: 99,
        };

        let op0 = PendingOp::Add {
            row_id: TempRowId(0),
            table: 0,
            values: vec![
                TxnWireValue::Id(TxnWireRowId::Existing(rid_a)),
                TxnWireValue::Id(TxnWireRowId::Existing(rid_a)),
            ],
        };
        let op1 = PendingOp::Add {
            row_id: TempRowId(1),
            table: 0,
            values: vec![
                TxnWireValue::Id(TxnWireRowId::Existing(rid_b)),
                TxnWireValue::Id(TxnWireRowId::Existing(rid_a_later)),
            ],
        };
        let commit = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, None, vec![op0, op1]),
            entity_pair_encode_table_meta,
        )
        .expect("build commit");
        assert_eq!(
            commit.other_hashes,
            vec![ha, hb],
            "hash dict lists each referenced commit once, in first-seen order"
        );

        let op_int = PendingOp::Add {
            row_id: TempRowId(0),
            table: 0,
            values: vec![42.into()],
        };
        let no_row_refs = Commit::from_commit_data(
            data(vec![], Author::foo(), 0, None, vec![op_int]),
            int_encode_table_meta,
        )
        .expect("build");
        assert!(
            no_row_refs.other_hashes.is_empty(),
            "no Existing row refs → empty hash dictionary"
        );
    }
}
