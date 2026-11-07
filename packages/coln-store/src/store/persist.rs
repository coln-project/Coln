// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::io::Write;

use crate::commit::Commit;
use crate::commit::chunk::Chunk;
use crate::commit::error::CodecError;
use crate::commit::leb128 as commit_leb128;
use crate::store::Store;
use crate::store::error::StoreError;

/// The store magic bytes
const MAGIC: &[u8; 4] = b"GMst";
const FORMAT_VERSION: u32 = 2;

impl Store {
    /// Store file layout, scalar integers use LEB128:
    ///
    /// `[MAGIC:4][version][chunk_count]`
    /// `[chunk_count × [chunk_header || payload]]` (see [`crate::commit::chunk::Header::write`])
    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        let commits = self.commits().iter_topological().collect::<Vec<_>>();
        let mut buf: Vec<u8> = Vec::new();
        buf.write_all(MAGIC)?;
        commit_leb128::write_u32(&mut buf, FORMAT_VERSION);
        commit_leb128::write_len(&mut buf, commits.len());

        for commit in commits {
            Chunk::from(commit).write(&mut buf);
        }

        Ok(buf)
    }

    /// Decode bytes produced by [`Self::encode`].
    ///
    /// Returns a store initialized from the root and checked data commits that
    /// still need to be applied. No data commits are replayed by this method.
    pub fn decode(data: &[u8]) -> Result<(Self, Vec<Commit<'static>>), StoreError> {
        let chunks = read_store_envelope(data)?;
        Self::try_from_chunks(chunks)
    }

    /// Restore a store without deriving data or checking rules.
    /// Rejects snapshots whose commits cannot all be applied.
    pub(crate) fn decode_plain(data: &[u8]) -> Result<Self, StoreError> {
        let encoded = read_store_envelope(data)?;
        let store = Self::try_from_chunks_plain(encoded)?;
        if store.has_pending() {
            return Err(CodecError::DataContentError(
                "store snapshot contains commits that could not be applied".into(),
            )
            .into());
        }
        Ok(store)
    }
}

fn read_store_envelope(data: &[u8]) -> Result<Vec<Chunk>, CodecError> {
    if data.len() < MAGIC.len() {
        return Err(CodecError::DataFormatError(
            "truncated: missing magic".into(),
        ));
    }
    if data[..MAGIC.len()] != *MAGIC {
        return Err(CodecError::DataFormatError("bad magic".into()));
    }

    let mut pos = MAGIC.len();

    let version = commit_leb128::read_u32(data, &mut pos, "format version")?;
    if version != FORMAT_VERSION {
        return Err(CodecError::DataFormatError(format!(
            "unsupported format version: {version}"
        )));
    }

    let chunk_count = commit_leb128::read_len(data, &mut pos, "chunk count")?;
    let mut chunks = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        chunks.push(Chunk::read_at(data, &mut pos)?);
    }
    if pos != data.len() {
        return Err(CodecError::DataFormatError(format!(
            "trailing bytes after store chunks: {} bytes",
            data.len() - pos
        )));
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use coln_flir_rs::engine::packed::{StoreScalarValue, StoreTuple};
    use coln_flir_rs::hash::{CommitHash, HASH_SIZE};
    use rstest::{fixture, rstest};

    use super::*;
    use crate::commit::author::Author;
    use crate::commit::chunk::{ChunkType, Header};
    use crate::commit::wire::{CommitData, RootCommitData};
    use crate::ir::Path;
    use crate::pack::id_packer::IdLookup;
    use crate::test_utils::non_empty_root_commit_data;
    use crate::txn::rw::{StoreRead, StoreWrite};

    #[fixture]
    fn int_store(non_empty_root_commit_data: RootCommitData) -> Store {
        let RootCommitData { ir, coln_def } = non_empty_root_commit_data;
        Store::try_from_ir(&ir, coln_def).expect("store")
    }

    fn store_envelope(framed_chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.write_all(MAGIC).unwrap();
        commit_leb128::write_u32(&mut bytes, FORMAT_VERSION);
        commit_leb128::write_len(&mut bytes, framed_chunks.len());
        for chunk in framed_chunks {
            bytes.write_all(chunk).unwrap();
        }
        bytes
    }

    fn frame_chunk(chunk_type: ChunkType, payload: &[u8]) -> Vec<u8> {
        let header = Header::new(chunk_type, payload);
        let mut buf = Vec::new();
        header.write(&mut buf);
        buf.write_all(payload).unwrap();
        buf
    }

    fn read_encoded_chunks(data: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let chunks = read_store_envelope(data).expect("encoded store");
        chunks
            .into_iter()
            .map(|chunk| {
                let chunk_type = chunk.chunk_type();
                let (_, payload) = chunk.into_parts();
                (u8::from(chunk_type), payload)
            })
            .collect()
    }

    fn is_data_format_error(result: Result<Store, StoreError>) -> bool {
        matches!(
            result,
            Err(err)
                if matches!(
                    &err,
                    StoreError::Encode(CodecError::DataFormatError(_))
                )
        )
    }

    /// Offset of the first chunk header (the start of its magic) inside an
    /// encoded store.
    fn first_chunk_offset(bytes: &[u8]) -> usize {
        let framed = frame_chunk(ChunkType::Root, &[]);
        let chunk_magic = &framed[..MAGIC.len()];
        bytes
            .windows(chunk_magic.len())
            .position(|window| window == chunk_magic)
            .expect("encoded store contains a chunk magic")
    }

    fn is_data_content_error(result: Result<Store, StoreError>) -> bool {
        matches!(
            result,
            Err(err)
                if matches!(
                    &err,
                    StoreError::Encode(CodecError::DataContentError(_))
                )
        )
    }

    #[test]
    fn encode_store_writes_root_commit_chunk() {
        let store = Store::new();
        let bytes = store.encode().unwrap();
        let chunks = read_encoded_chunks(&bytes);
        let root = store.commits().root_commit().expect("root commit");

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, u8::from(ChunkType::Root));
        assert_eq!(chunks[0].1, root.payload());
    }

    #[rstest]
    fn encode_store_writes_topological_commit_chunks(#[from(int_store)] mut store: Store) {
        let table = Path::from("T");
        let mut txn = store.transaction();
        txn.add(&table, vec![99_i32]).expect("add row");
        txn.commit().expect("commit");

        let expected = store
            .commits()
            .iter_topological()
            .map(|commit| {
                let chunk_type = if commit.is_root() {
                    ChunkType::Root
                } else {
                    ChunkType::Commit
                };
                (u8::from(chunk_type), commit.payload().to_vec())
            })
            .collect::<Vec<_>>();
        let bytes = store.encode().unwrap();
        let chunks = read_encoded_chunks(&bytes);

        assert_eq!(chunks, expected);
    }

    #[test]
    fn store_decode_rejects_unsupported_version() {
        let mut bytes = Vec::new();
        bytes.write_all(MAGIC).unwrap();
        commit_leb128::write_u32(&mut bytes, 999);

        assert!(is_data_format_error(Store::decode_plain(&bytes)));
    }

    #[test]
    fn store_decode_rejects_unknown_chunk_type() {
        let mut chunk = Vec::new();
        chunk.write_all(MAGIC).unwrap();
        chunk.extend_from_slice(&[0u8; 4]); // checksum placeholder
        chunk.push(99); // unknown chunk type
        commit_leb128::write_len(&mut chunk, 0);
        let bytes = store_envelope(&[chunk]);

        assert!(is_data_format_error(Store::decode_plain(&bytes)));
    }

    #[test]
    fn store_decode_rejects_truncated_chunk_record() {
        let mut bytes = Vec::new();
        bytes.write_all(MAGIC).unwrap();
        commit_leb128::write_u32(&mut bytes, FORMAT_VERSION);
        commit_leb128::write_len(&mut bytes, 1);
        bytes.write_all(&[0x47]).unwrap(); // partial chunk: not even a full magic

        assert!(is_data_format_error(Store::decode_plain(&bytes)));
    }

    #[test]
    fn store_decode_rejects_trailing_bytes_after_chunks() {
        let mut bytes = Store::new().encode().unwrap();
        bytes.push(0);

        assert!(is_data_format_error(Store::decode_plain(&bytes)));
    }

    #[test]
    fn store_decode_rejects_flipped_checksum_byte() {
        let bytes = Store::new().encode().expect("encode store");
        assert!(
            Store::decode_plain(&bytes).is_ok(),
            "baseline store should decode"
        );

        let mut corrupted = bytes.clone();
        // Skip the 4-byte chunk magic to land on the stored checksum.
        let checksum_byte = first_chunk_offset(&corrupted) + MAGIC.len();
        corrupted[checksum_byte] ^= 0xFF;

        let err = Store::decode_plain(&corrupted).expect_err("checksum mismatch");
        assert!(matches!(
            err,
            StoreError::Encode(CodecError::ChecksumMismatch)
        ));
    }

    #[test]
    fn store_decode_rejects_flipped_chunk_magic_byte() {
        let bytes = Store::new().encode().expect("encode store");
        assert!(
            Store::decode_plain(&bytes).is_ok(),
            "baseline store should decode"
        );

        let mut corrupted = bytes.clone();
        let magic_byte = first_chunk_offset(&corrupted);
        corrupted[magic_byte] ^= 0xFF;

        assert!(is_data_format_error(Store::decode_plain(&corrupted)));
    }

    #[test]
    fn store_decode_rejects_multiple_roots() {
        let bytes = Store::new().encode().unwrap();
        let root_payload = read_encoded_chunks(&bytes)
            .into_iter()
            .next()
            .expect("root chunk")
            .1;
        let framed = frame_chunk(ChunkType::Root, &root_payload);
        let bytes = store_envelope(&[framed.clone(), framed]);

        assert!(is_data_format_error(Store::decode_plain(&bytes)));
    }

    #[test]
    fn store_decode_rejects_missing_dependency() {
        let root_bytes = Store::new().encode().unwrap();
        let root_chunk = read_encoded_chunks(&root_bytes)
            .into_iter()
            .next()
            .expect("root chunk");
        let missing = CommitHash([0xff; HASH_SIZE]);
        let commit = Commit::from_commit_data(
            CommitData::new(vec![missing], Author::foo(), 42, None, vec![]),
            |_| None,
        )
        .expect("commit with missing dependency");
        let bytes = store_envelope(&[
            frame_chunk(ChunkType::Root, &root_chunk.1),
            frame_chunk(ChunkType::Commit, commit.payload()),
        ]);

        assert!(is_data_content_error(Store::decode_plain(&bytes)));
    }

    #[test]
    fn store_round_trip_empty() {
        let store = Store::new();
        let root = store.commits().root_commit().expect("root").hash();

        let bytes = store.encode().unwrap();
        let restored = Store::decode_plain(&bytes).unwrap();

        assert_eq!(restored.table_count(), 0);
        assert_eq!(restored.commits().root_commit().expect("root").hash(), root);
        assert_eq!(
            restored.commits().heads().copied().collect::<Vec<_>>(),
            vec![root]
        );
    }

    #[rstest]
    fn store_round_trip_replays_commits_and_preserves_graph(#[from(int_store)] mut store: Store) {
        let root = store.commits().root_commit().expect("root").hash();
        let table = Path::from("T");
        let mut txn = store.transaction();
        let live_id = txn.add(&table, vec![99_i32]).expect("add row");
        let commit = txn.commit().expect("commit");
        let row_id = store.promote_one(live_id, commit);

        let bytes = store.encode().unwrap();
        let restored = Store::decode_plain(&bytes).unwrap();

        let packed = restored
            .id_lookup()
            .packed(&row_id)
            .expect("restored store interned the commit hash");
        assert_eq!(
            restored.row_by_id(&table, &packed),
            Some(StoreTuple::from(vec![
                StoreScalarValue::RowId(packed),
                StoreScalarValue::I32(99),
            ]))
        );
        assert_eq!(
            restored.commits().parents_of(&commit),
            Some([root].as_slice())
        );
        assert_eq!(
            restored.commits().heads().copied().collect::<Vec<_>>(),
            vec![commit]
        );
    }

    #[test]
    fn store_decode_rejects_missing_root() {
        let mut bytes = Vec::new();
        bytes.write_all(MAGIC).unwrap();
        commit_leb128::write_u32(&mut bytes, FORMAT_VERSION);
        commit_leb128::write_len(&mut bytes, 0);

        assert!(is_data_format_error(Store::decode_plain(&bytes)));
    }

    #[test]
    fn store_decode_rejects_bad_magic() {
        assert!(is_data_format_error(Store::decode_plain(b"XXXX________")));
    }

    #[test]
    fn store_decode_rejects_truncated_input() {
        assert!(Store::decode_plain(b"GM").is_err());
        assert!(Store::decode_plain(b"GMst").is_err());
    }
}
