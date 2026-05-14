use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use crate::commit::Commit;
use crate::commit::chunk::ChunkType;
use crate::commit::error::PersistError;
use crate::commit::graph::CommitGraph;
use crate::commit::utils::*;
use crate::store::Store;

const MAGIC: &[u8; 4] = b"GMst";
const FORMAT_VERSION: u32 = 2;

// ── Store-level encode/decode ───────────────────────────────────────────────

/// Store file layout (little-endian):
///
/// `[MAGIC:4][version:u32][chunk_count:u32]`
/// `[chunk_count × ([chunk_type:u8][payload_len:u64][payload])]`
pub fn encode_store(store: &Store) -> Result<Vec<u8>, PersistError> {
    let commits = store.commits().iter_topological().collect::<Vec<_>>();
    let mut buf: Vec<u8> = Vec::new();
    buf.write_all(MAGIC)?;
    buf.write_all(&FORMAT_VERSION.to_le_bytes())?;

    let chunk_count: u32 = commits
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("too many commits in store".into()))?;
    buf.write_all(&chunk_count.to_le_bytes())?;

    for commit in commits {
        write_commit_chunk(&mut buf, commit)?;
    }

    Ok(buf)
}

/// Decode a store from bytes produced by [`encode_store`].
pub fn decode_store(data: &[u8]) -> Result<Store, PersistError> {
    if data.len() < MAGIC.len() {
        return Err(PersistError::DataFormatError(
            "truncated: missing magic".into(),
        ));
    }
    if data[..MAGIC.len()] != *MAGIC {
        return Err(PersistError::DataFormatError("bad magic".into()));
    }

    let mut pos = MAGIC.len();

    let version = read_u32(data, &mut pos, "format version")?;
    if version != FORMAT_VERSION {
        return Err(PersistError::DataFormatError(format!(
            "unsupported format version: {version}"
        )));
    }

    let chunk_count = read_u32(data, &mut pos, "chunk count")? as usize;
    let mut chunks = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        chunks.push(read_commit_chunk(data, &mut pos)?);
    }
    if pos != data.len() {
        return Err(PersistError::DataFormatError(format!(
            "trailing bytes after store chunks: {} bytes",
            data.len() - pos
        )));
    }

    decode_store_chunks(chunks)
}

fn write_commit_chunk(buf: &mut Vec<u8>, commit: &Commit<'_>) -> Result<(), PersistError> {
    buf.write_all(&[u8::from(commit.chunk_type())])?;
    write_len_prefixed_bytes(buf, commit.payload(), "commit payload too large")
}

fn read_commit_chunk(data: &[u8], pos: &mut usize) -> Result<(ChunkType, Vec<u8>), PersistError> {
    let chunk_type = match read_u8(data, pos, "chunk type")? {
        0 => ChunkType::Commit,
        1 => ChunkType::Root,
        tag => {
            return Err(PersistError::DataFormatError(format!(
                "unknown chunk type {tag}"
            )));
        }
    };
    let payload = read_len_prefixed_bytes(data, pos, "commit payload")?.to_vec();
    Ok((chunk_type, payload))
}

fn decode_store_chunks(chunks: Vec<(ChunkType, Vec<u8>)>) -> Result<Store, PersistError> {
    let roots = chunks
        .iter()
        .filter(|(chunk_type, _)| *chunk_type == ChunkType::Root)
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return Err(PersistError::DataFormatError(
            "commit graph has no root commit".into(),
        ));
    }
    if roots.len() > 1 {
        return Err(PersistError::DataFormatError(
            "commit graph has multiple root commits".into(),
        ));
    }

    let root_commit = Commit::decode(ChunkType::Root, roots[0].1.clone(), |_| None)?;
    let root_payload = root_commit.root_payload()?;
    let mut store = Store::from_root_commit_data(root_payload)
        .map_err(|err| PersistError::Other(format!("law compile error: {err:?}")))?;

    let mut graph = CommitGraph::new();
    let root_hash = root_commit.hash();
    graph.add_commit(root_commit);

    let mut pending = BTreeMap::new();
    let mut known_hashes = BTreeSet::from([root_hash]);
    for (chunk_type, payload) in chunks {
        if chunk_type == ChunkType::Root {
            continue;
        }

        let commit = Commit::decode(chunk_type, payload, |path| {
            store.table_at(path).map(|table| table.schema())
        })?;
        let hash = commit.hash();
        if commit.deps.is_empty() {
            return Err(PersistError::DataFormatError(format!(
                "data commit {hash} has no dependencies"
            )));
        }
        if let Some(existing) = pending.get(&hash) {
            let existing: &Commit<'static> = existing;
            if existing.chunk_type() != commit.chunk_type()
                || existing.payload() != commit.payload()
            {
                return Err(PersistError::DataFormatError(format!(
                    "duplicate commit hash with conflicting payload: {hash}"
                )));
            }
            continue;
        }
        known_hashes.insert(hash);
        pending.insert(hash, commit);
    }

    for commit in pending.values() {
        for dep in &commit.deps {
            if !known_hashes.contains(dep) {
                return Err(PersistError::DataFormatError(format!(
                    "commit {} depends on missing commit {dep}",
                    commit.hash()
                )));
            }
        }
    }

    while !pending.is_empty() {
        let ready_hashes = pending
            .iter()
            .filter_map(|(hash, commit)| {
                commit
                    .deps
                    .iter()
                    .all(|dep| graph.contains(dep))
                    .then_some(*hash)
            })
            .collect::<Vec<_>>();

        if ready_hashes.is_empty() {
            return Err(PersistError::DataFormatError(
                "commit graph has cyclic or disconnected dependencies".into(),
            ));
        }

        for hash in ready_hashes {
            let commit = pending.remove(&hash).expect("ready commit exists");
            store
                .apply_batch(commit.resolved_ops())
                .map_err(|err| PersistError::Other(format!("commit replay error: {err}")))?;
            graph.add_commit(commit);
        }
    }

    store.replace_commit_graph(graph);
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::chunk::ChunkType;
    use crate::commit::hash::{CommitHash, HASH_SIZE};
    use crate::commit::wire::CommitData;
    use crate::ir::{FlatTheory, Path, Schema, TableEntry};
    use crate::table::CellValue;
    use geolog_lang::ir::{ColType, PrimType};

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

    fn store_bytes_from_chunks(chunks: &[(u8, Vec<u8>)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.write_all(MAGIC).unwrap();
        bytes.write_all(&FORMAT_VERSION.to_le_bytes()).unwrap();
        bytes
            .write_all(&(chunks.len() as u32).to_le_bytes())
            .unwrap();
        for (chunk_type, payload) in chunks {
            bytes.write_all(&[*chunk_type]).unwrap();
            write_len_prefixed_bytes(&mut bytes, payload, "test payload").unwrap();
        }
        bytes
    }

    fn read_encoded_chunks(data: &[u8]) -> Vec<(u8, Vec<u8>)> {
        assert_eq!(&data[..MAGIC.len()], MAGIC);
        let mut pos = MAGIC.len();
        assert_eq!(
            read_u32(data, &mut pos, "format version").expect("version"),
            FORMAT_VERSION
        );
        let chunk_count = read_u32(data, &mut pos, "chunk count").expect("chunk count") as usize;
        let mut chunks = Vec::with_capacity(chunk_count);
        for _ in 0..chunk_count {
            let chunk_type = read_u8(data, &mut pos, "chunk type").expect("chunk type");
            let payload = read_len_prefixed_bytes(data, &mut pos, "payload")
                .expect("payload")
                .to_vec();
            chunks.push((chunk_type, payload));
        }
        assert_eq!(pos, data.len());
        chunks
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
        bytes.write_all(&999_u32.to_le_bytes()).unwrap();

        assert!(matches!(
            decode_store(&bytes),
            Err(PersistError::DataFormatError(_))
        ));
    }

    #[test]
    fn store_decode_rejects_unknown_chunk_type() {
        let bytes = store_bytes_from_chunks(&[(99, Vec::new())]);

        assert!(matches!(
            decode_store(&bytes),
            Err(PersistError::DataFormatError(_))
        ));
    }

    #[test]
    fn store_decode_rejects_truncated_chunk_record() {
        let mut bytes = Vec::new();
        bytes.write_all(MAGIC).unwrap();
        bytes.write_all(&FORMAT_VERSION.to_le_bytes()).unwrap();
        bytes.write_all(&1_u32.to_le_bytes()).unwrap();
        bytes.write_all(&[u8::from(ChunkType::Root)]).unwrap();

        assert!(matches!(
            decode_store(&bytes),
            Err(PersistError::DataFormatError(_))
        ));
    }

    #[test]
    fn store_decode_rejects_trailing_bytes_after_chunks() {
        let mut bytes = encode_store(&Store::new()).unwrap();
        bytes.push(0);

        assert!(matches!(
            decode_store(&bytes),
            Err(PersistError::DataFormatError(_))
        ));
    }

    #[test]
    fn store_decode_rejects_multiple_roots() {
        let bytes = encode_store(&Store::new()).unwrap();
        let chunks = read_encoded_chunks(&bytes);
        let duplicated = vec![chunks[0].clone(), chunks[0].clone()];
        let bytes = store_bytes_from_chunks(&duplicated);

        assert!(matches!(
            decode_store(&bytes),
            Err(PersistError::DataFormatError(_))
        ));
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
            CommitData::new(vec![missing], [7; 16], 42, None, vec![]),
            |_| None,
        )
        .expect("commit with missing dependency");
        let bytes = store_bytes_from_chunks(&[
            root_chunk,
            (u8::from(ChunkType::Commit), commit.payload().to_vec()),
        ]);

        assert!(matches!(
            decode_store(&bytes),
            Err(PersistError::DataFormatError(_))
        ));
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
        bytes.write_all(&FORMAT_VERSION.to_le_bytes()).unwrap();
        bytes.write_all(&0_u32.to_le_bytes()).unwrap();

        assert!(matches!(
            decode_store(&bytes),
            Err(PersistError::DataFormatError(_))
        ));
    }

    #[test]
    fn store_decode_rejects_bad_magic() {
        assert!(matches!(
            decode_store(b"XXXX________"),
            Err(PersistError::DataFormatError(_))
        ));
    }

    #[test]
    fn store_decode_rejects_truncated_input() {
        assert!(decode_store(b"GM").is_err());
        assert!(decode_store(b"GMst").is_err());
    }
}
