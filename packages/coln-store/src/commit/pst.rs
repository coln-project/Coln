use std::io::Write;

use crate::commit::Commit;
use crate::commit::chunk::{Chunk, ChunkType};
use crate::commit::error::CodecError;
use crate::commit::leb128 as commit_leb128;
use crate::store::Store;
use crate::store::error::StoreIntError;
use crate::table::TableOid;

/// The store magic bytes
const MAGIC: &[u8; 4] = b"GMst";
const FORMAT_VERSION: u32 = 2;

// ── Store-level encode/decode ───────────────────────────────────────────────

/// Store file layout, scalar integers use LEB128:
///
/// `[MAGIC:4][version][next_oid][chunk_count]`
/// `[chunk_count × [chunk_header || payload]]` (see [`Header::write`])
pub fn encode_store(store: &Store) -> Result<Vec<u8>, CodecError> {
    let commits = store.commits().iter_topological().collect::<Vec<_>>();
    let mut buf: Vec<u8> = Vec::new();
    buf.write_all(MAGIC)?;
    commit_leb128::write_u32(&mut buf, FORMAT_VERSION);
    commit_leb128::write_u64(&mut buf, store.next_oid);

    commit_leb128::write_len(&mut buf, commits.len());

    for commit in commits {
        write_commit_chunk(&mut buf, commit);
    }

    Ok(buf)
}

/// Decode a store from bytes produced by [`encode_store`].
pub fn decode_store(data: &[u8]) -> Result<Store, Box<StoreIntError>> {
    let encoded = read_store_envelope(data)?;
    decode_store_chunks(encoded.next_oid, encoded.chunks)
}

struct EncodedStore {
    next_oid: TableOid,
    chunks: Vec<Chunk>,
}

fn read_store_envelope(data: &[u8]) -> Result<EncodedStore, CodecError> {
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

    let next_oid = commit_leb128::read_u64(data, &mut pos, "next_oid")?;

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

    Ok(EncodedStore { next_oid, chunks })
}

fn write_commit_chunk(buf: &mut Vec<u8>, commit: &Commit<'_>) {
    Chunk::from(commit).write(buf)
}

fn decode_store_chunks(
    next_oid: TableOid,
    chunks: Vec<Chunk>,
) -> Result<Store, Box<StoreIntError>> {
    let roots = chunks
        .iter()
        .filter(|chunk| chunk.chunk_type() == ChunkType::Root)
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return Err(CodecError::DataFormatError("commit graph has no root commit".into()).into());
    }
    if roots.len() > 1 {
        return Err(
            CodecError::DataFormatError("commit graph has multiple root commits".into()).into(),
        );
    }

    let root_commit = Commit::from_chunk((*roots[0]).clone(), |_| None)?;
    let root_payload = root_commit.root_payload()?;
    let mut store = Store::from_root_commit_data(next_oid, root_payload)?;
    store.record_in_commit_graph(root_commit);

    let mut commits = Vec::new();
    for chunk in chunks {
        if chunk.chunk_type() == ChunkType::Root {
            continue;
        }

        let commit = Commit::from_chunk(chunk, |path| store.schema_for(path))?;
        commits.push(commit);
    }

    store.apply_commits(commits)?;

    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::author::Author;
    use crate::commit::chunk::{ChunkType, Header};
    use crate::commit::hash::{CommitHash, HASH_SIZE};
    use crate::commit::wire::CommitData;
    use crate::ir::{FlatTheory, Path, Schema, TableEntry};
    use crate::table::CellValue;
    use coln_lang_rs::ir::{ColType, PrimType};

    fn int_schema() -> Schema {
        Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        }
    }

    fn int_theory() -> FlatTheory {
        FlatTheory {
            tables: vec![TableEntry {
                path: Path::from("T"),
                table: int_schema(),
            }],
            laws: vec![],
        }
    }

    fn store_envelope(framed_chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.write_all(MAGIC).unwrap();
        commit_leb128::write_u32(&mut bytes, FORMAT_VERSION);
        commit_leb128::write_u64(&mut bytes, 0);
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
        let encoded = read_store_envelope(data).expect("encoded store");
        encoded
            .chunks
            .into_iter()
            .map(|chunk| {
                let chunk_type = chunk.chunk_type();
                let (_, payload) = chunk.into_parts();
                (u8::from(chunk_type), payload)
            })
            .collect()
    }

    fn is_data_format_error(result: Result<Store, Box<StoreIntError>>) -> bool {
        matches!(
            result,
            Err(err)
                if matches!(
                    err.as_ref(),
                    StoreIntError::Encode(CodecError::DataFormatError(_))
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

    fn is_missing_dep_error(result: Result<Store, Box<StoreIntError>>) -> bool {
        matches!(
            result,
            Err(err)
                if matches!(
                    err.as_ref(),
                    StoreIntError::Commit(crate::store::error::CommitApplyError::MissingDep)
                )
        )
    }

    #[test]
    fn encode_store_writes_root_commit_chunk() {
        let store = Store::new();
        let bytes = encode_store(&store).unwrap();
        let chunks = read_encoded_chunks(&bytes);
        let root = store.commits().root_commit().expect("root commit");

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, u8::from(ChunkType::Root));
        assert_eq!(chunks[0].1, root.payload());
    }

    #[test]
    fn encode_store_writes_topological_commit_chunks() {
        let mut store = Store::try_from_theory(int_theory()).expect("store");
        let table = Path::from("T");
        let mut txn = store.transaction();
        txn.add(&table, vec![99_i64.into()]).expect("add row");
        txn.commit().expect("commit");

        let expected = store
            .commits()
            .iter_topological()
            .map(|commit| (u8::from(commit.chunk_type()), commit.payload().to_vec()))
            .collect::<Vec<_>>();
        let bytes = encode_store(&store).unwrap();
        let chunks = read_encoded_chunks(&bytes);

        assert_eq!(chunks, expected);
    }

    #[test]
    fn store_decode_rejects_unsupported_version() {
        let mut bytes = Vec::new();
        bytes.write_all(MAGIC).unwrap();
        commit_leb128::write_u32(&mut bytes, 999);

        assert!(is_data_format_error(decode_store(&bytes)));
    }

    #[test]
    fn store_decode_rejects_unknown_chunk_type() {
        let mut chunk = Vec::new();
        chunk.write_all(MAGIC).unwrap();
        chunk.extend_from_slice(&[0u8; 4]); // checksum placeholder
        chunk.push(99); // unknown chunk type
        commit_leb128::write_len(&mut chunk, 0);
        let bytes = store_envelope(&[chunk]);

        assert!(is_data_format_error(decode_store(&bytes)));
    }

    #[test]
    fn store_decode_rejects_truncated_chunk_record() {
        let mut bytes = Vec::new();
        bytes.write_all(MAGIC).unwrap();
        commit_leb128::write_u32(&mut bytes, FORMAT_VERSION);
        commit_leb128::write_u64(&mut bytes, 0);
        commit_leb128::write_len(&mut bytes, 1);
        bytes.write_all(&[0x47]).unwrap(); // partial chunk: not even a full magic

        assert!(is_data_format_error(decode_store(&bytes)));
    }

    #[test]
    fn store_decode_rejects_trailing_bytes_after_chunks() {
        let mut bytes = encode_store(&Store::new()).unwrap();
        bytes.push(0);

        assert!(is_data_format_error(decode_store(&bytes)));
    }

    #[test]
    fn store_decode_rejects_flipped_checksum_byte() {
        let bytes = encode_store(&Store::new()).expect("encode store");
        assert!(decode_store(&bytes).is_ok(), "baseline store should decode");

        let mut corrupted = bytes.clone();
        // Skip the 4-byte chunk magic to land on the stored checksum.
        let checksum_byte = first_chunk_offset(&corrupted) + MAGIC.len();
        corrupted[checksum_byte] ^= 0xFF;

        let err = decode_store(&corrupted).expect_err("checksum mismatch");
        assert!(matches!(
            err.as_ref(),
            StoreIntError::Encode(CodecError::ChecksumMismatch)
        ));
    }

    #[test]
    fn store_decode_rejects_flipped_chunk_magic_byte() {
        let bytes = encode_store(&Store::new()).expect("encode store");
        assert!(decode_store(&bytes).is_ok(), "baseline store should decode");

        let mut corrupted = bytes.clone();
        let magic_byte = first_chunk_offset(&corrupted);
        corrupted[magic_byte] ^= 0xFF;

        assert!(is_data_format_error(decode_store(&corrupted)));
    }

    #[test]
    fn store_decode_rejects_multiple_roots() {
        let bytes = encode_store(&Store::new()).unwrap();
        let root_payload = read_encoded_chunks(&bytes)
            .into_iter()
            .next()
            .expect("root chunk")
            .1;
        let framed = frame_chunk(ChunkType::Root, &root_payload);
        let bytes = store_envelope(&[framed.clone(), framed]);

        assert!(is_data_format_error(decode_store(&bytes)));
    }

    #[test]
    fn store_decode_rejects_missing_dependency() {
        let root_bytes = encode_store(&Store::new()).unwrap();
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

        assert!(is_missing_dep_error(decode_store(&bytes)));
    }

    #[test]
    fn store_round_trip_empty() {
        let store = Store::new();
        let root = store.commits().root_commit().expect("root").hash();

        let bytes = encode_store(&store).unwrap();
        let restored = decode_store(&bytes).unwrap();

        assert_eq!(restored.table_count(), 0);
        assert_eq!(restored.commits().root_commit().expect("root").hash(), root);
        assert_eq!(
            restored.commits().heads().copied().collect::<Vec<_>>(),
            vec![root]
        );
    }

    #[test]
    fn store_round_trip_replays_commits_and_preserves_graph() {
        let mut store = Store::try_from_theory(int_theory()).expect("store");
        let root = store.commits().root_commit().expect("root").hash();
        let table = Path::from("T");
        let mut txn = store.transaction();
        txn.add(&table, vec![99_i64.into()]).expect("add row");
        let commit = txn.commit().expect("commit");

        let bytes = encode_store(&store).unwrap();
        let restored = decode_store(&bytes).unwrap();

        let restored_table = restored.table_at(&table).expect("table");
        assert_eq!(restored_table.row_count(), 1);
        assert_eq!(restored_table.cell_at(0, 0), Some(&CellValue::Int(99)));
        assert_eq!(restored_table.row_id_at(0).expect("row id").commit, commit);
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
        commit_leb128::write_u64(&mut bytes, 0);
        commit_leb128::write_len(&mut bytes, 0);

        assert!(is_data_format_error(decode_store(&bytes)));
    }

    #[test]
    fn store_decode_rejects_bad_magic() {
        assert!(is_data_format_error(decode_store(b"XXXX________")));
    }

    #[test]
    fn store_decode_rejects_truncated_input() {
        assert!(decode_store(b"GM").is_err());
        assert!(decode_store(b"GMst").is_err());
    }
}
