use std::{borrow::Cow, io::Write};

use crate::{
    commit::{
        Commit, Header,
        chunk::{ChunkType, hash},
        error::PersistError,
        hash::{CommitHash, HASH_SIZE},
        hash_dict::{HashMapper, read_hash_dict, write_hash_dict},
        utils::{read_slice, read_u32, read_u64},
    },
    ir::{ColType, Path, PrimType, Schema},
    table::RowId,
    txn::ops::{OP_KIND_ADD, PendingOp, RowRef, TempRowId, TxnCellValue},
};
use hexane::v1::{Column, DeltaColumn};

const LOCAL_COMMIT_HASH_INDEX: u32 = u32::MAX;

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
//   [commit body]                         (see encode_commit_body)
//
pub(crate) fn serialise<'s, F>(
    deps: &[CommitHash],
    nonce: [u8; 16],
    timestamp: i64,
    message: Option<&str>,
    pending: &[PendingOp],
    hash_mapper: &HashMapper,
    schema_for: F,
) -> Result<Vec<u8>, PersistError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
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

    let body = encode_commit_body(pending, schema_for, hash_mapper)?;
    buf.write_all(&body).unwrap();

    Ok(buf)
}

/// Parse canonical payload bytes (the slice passed to [`crate::persist::chunk::hash`],
/// not including the chunk type or outer length prefix).
///
/// Sets `header.hash` from [`crate::persist::chunk::hash`] applied to `ChunkType::Commit` and `data`.
/// When loading from storage, compare that to the hash in the outer chunk header.
pub fn deserialise<'s, F>(data: &[u8], schema_for: F) -> Result<Commit<'_>, PersistError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
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
                .map_err(|_| PersistError::DataFormatError("commit message: invalid utf-8".into()))?
                .to_owned(),
        )
    };

    let other_hashes = read_hash_dict(data, &mut pos)?;

    let pending = decode_commit_body(&data[pos..], &other_hashes, schema_for)?;

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

/// Encode the normal commit body after the shared commit metadata.
///
/// Layout:
/// `[op_count:u32]`
/// `[op_kind_column: len-prefixed hexane Column<u32>]`
/// `[table_sequence_column: len-prefixed hexane Column<u32>]`
/// `[group_count:u32]`
/// followed by one len-prefixed [`encode_op_group`] blob per first-seen table.
fn encode_commit_body<'s, F>(
    pending: &[PendingOp],
    schema_for: F,
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, PersistError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
    let mut groups: Vec<(Path, Vec<PendingOp>)> = Vec::new();
    // table_sequence[i] is the group index of the ith operation. This is used
    // in decoding to determine from which op group to retrieve the next operation
    let mut table_sequence = Vec::with_capacity(pending.len());
    let mut op_kinds = Vec::with_capacity(pending.len());

    for op in pending {
        match op {
            PendingOp::Add { table, .. } => {
                let group_index = if let Some(index) = groups
                    .iter()
                    .position(|(group_table, _)| group_table == table)
                {
                    index
                } else {
                    groups.push((table.clone(), Vec::new()));
                    groups.len() - 1
                };
                groups[group_index].1.push(op.clone());
                table_sequence.push(group_index as u32);
                op_kinds.push(OP_KIND_ADD);
            }
        }
    }

    let mut buf = Vec::new();
    let op_count: u32 = pending
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("too many ops in commit body".into()))?;
    buf.write_all(&op_count.to_le_bytes())?;

    let op_kind_col = Column::<u32>::from_values(op_kinds).save();
    write_len_prefixed_blob(&mut buf, &op_kind_col)?;

    let table_sequence_col = Column::<u32>::from_values(table_sequence).save();
    write_len_prefixed_blob(&mut buf, &table_sequence_col)?;

    let group_count: u32 = groups
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("too many op groups in commit body".into()))?;
    buf.write_all(&group_count.to_le_bytes())?;

    for (table, ops) in &groups {
        let schema = schema_for(table).ok_or_else(|| {
            PersistError::SchemaError(format!("missing schema for table {table:?}"))
        })?;
        let group_blob = encode_op_group(table, schema, ops, hash_mapper)?;
        write_len_prefixed_blob(&mut buf, &group_blob)?;
    }

    Ok(buf)
}

fn decode_commit_body<'s, F>(
    data: &[u8],
    hashes: &[CommitHash],
    schema_for: F,
) -> Result<Vec<PendingOp>, PersistError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
    let mut pos = 0usize;
    let op_count = read_u32(data, &mut pos, "op count")? as usize;

    let op_kind_blob = read_len_prefixed_blob(data, &mut pos, "op kind column")?;
    let op_kinds: Vec<u32> = Column::<u32>::load(op_kind_blob)?.iter().collect();
    if op_kinds.len() != op_count {
        return Err(PersistError::DataFormatError(format!(
            "op kind column length mismatch: expected {op_count}, got {}",
            op_kinds.len()
        )));
    }

    let table_sequence_blob = read_len_prefixed_blob(data, &mut pos, "table sequence column")?;
    let table_sequence: Vec<u32> = Column::<u32>::load(table_sequence_blob)?.iter().collect();
    if table_sequence.len() != op_count {
        return Err(PersistError::DataFormatError(format!(
            "table sequence column length mismatch: expected {op_count}, got {}",
            table_sequence.len()
        )));
    }

    let group_count = read_u32(data, &mut pos, "op group count")? as usize;
    let mut groups = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        let group_blob = read_len_prefixed_blob(data, &mut pos, "op group")?;
        groups.push(decode_op_group(group_blob, hashes, &schema_for)?);
    }

    if pos != data.len() {
        return Err(PersistError::DataFormatError(format!(
            "trailing bytes after commit body: {} bytes",
            data.len() - pos
        )));
    }

    let mut group_offsets = vec![0usize; groups.len()];
    let mut pending = Vec::with_capacity(op_count);
    for op_idx in 0..op_count {
        let op_kind = op_kinds[op_idx];
        if op_kind != OP_KIND_ADD {
            return Err(PersistError::DataFormatError(format!(
                "unknown op kind {op_kind}"
            )));
        }

        let group_idx = table_sequence[op_idx] as usize;
        let group = groups.get(group_idx).ok_or_else(|| {
            PersistError::DataFormatError(format!(
                "table sequence index {group_idx} out of bounds (group count {})",
                groups.len()
            ))
        })?;
        let row_idx = group_offsets[group_idx];
        let values = group.rows.get(row_idx).cloned().ok_or_else(|| {
            PersistError::DataFormatError(format!(
                "op group {group_idx} exhausted at row {row_idx}"
            ))
        })?;
        group_offsets[group_idx] += 1;

        pending.push(PendingOp::Add {
            row_id: TempRowId(op_idx as u32),
            table: group.table.clone(),
            values,
        });
    }

    for (group_idx, (offset, group)) in group_offsets.iter().zip(&groups).enumerate() {
        if *offset != group.rows.len() {
            return Err(PersistError::DataFormatError(format!(
                "op group {group_idx} has {} unused rows",
                group.rows.len() - *offset
            )));
        }
    }

    Ok(pending)
}

struct DecodedOpGroup {
    table: Path,
    rows: Vec<Vec<TxnCellValue>>,
}

fn decode_op_group<'s, F>(
    data: &[u8],
    hashes: &[CommitHash],
    schema_for: &F,
) -> Result<DecodedOpGroup, PersistError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
    let mut pos = 0usize;

    let path_len = read_u32(data, &mut pos, "op group table path length")? as usize;
    let path_bytes = read_slice(data, &mut pos, path_len, "op group table path")?;
    let table =
        Path::from(std::str::from_utf8(path_bytes).map_err(|_| {
            PersistError::DataFormatError("op group table path: invalid utf-8".into())
        })?);

    let row_count = read_u32(data, &mut pos, "op group row count")? as usize;
    let column_count = read_u32(data, &mut pos, "op group column count")? as usize;
    let schema = schema_for(&table)
        .ok_or_else(|| PersistError::SchemaError(format!("missing schema for table {table:?}")))?;
    if column_count != schema.columns.len() {
        return Err(PersistError::DataFormatError(format!(
            "op group column count mismatch: encoded {column_count}, schema expects {}",
            schema.columns.len()
        )));
    }

    let mut columns = Vec::with_capacity(column_count);
    for (column_index, col_type) in schema.columns.iter().enumerate() {
        let column_blob = read_len_prefixed_blob(data, &mut pos, "op group column")?;
        let column = decode_txn_value_column(column_blob, col_type, hashes)?;
        if column.len() != row_count {
            return Err(PersistError::DataFormatError(format!(
                "op group column {column_index} length mismatch: expected {row_count}, got {}",
                column.len()
            )));
        }
        columns.push(column);
    }

    if pos != data.len() {
        return Err(PersistError::DataFormatError(format!(
            "trailing bytes after op group: {} bytes",
            data.len() - pos
        )));
    }

    let mut rows = Vec::with_capacity(row_count);
    for row_index in 0..row_count {
        rows.push(
            columns
                .iter()
                .map(|column| column[row_index].clone())
                .collect(),
        );
    }

    Ok(DecodedOpGroup { table, rows })
}

/// encode the row_ref column, which might be a pending id or a already resolved id
fn encode_txn_row_ref_column(
    values: &[TxnCellValue],
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, PersistError> {
    let mut hash_indices = Vec::with_capacity(values.len());
    let mut counters = Vec::with_capacity(values.len());

    for value in values {
        let TxnCellValue::Id(row_ref) = value else {
            return Err(PersistError::SchemaError(format!(
                "expected row reference, got {value:?}"
            )));
        };

        match row_ref {
            RowRef::Existing(RowId { commit, counter }) => {
                let hash_index = hash_mapper.index(*commit).ok_or_else(|| {
                    PersistError::SchemaError(format!(
                        "missing commit hash in dictionary: {commit}"
                    ))
                })?;
                if hash_index == LOCAL_COMMIT_HASH_INDEX {
                    return Err(PersistError::Other(
                        "too many hashes for row reference column".into(),
                    ));
                }
                hash_indices.push(hash_index);
                counters.push(*counter);
            }
            RowRef::Pending(temp_id) => {
                hash_indices.push(LOCAL_COMMIT_HASH_INDEX);
                counters.push(temp_id.0);
            }
        }
    }

    let hash_index_col = Column::<u32>::from_values(hash_indices).save();
    let counter_col = DeltaColumn::<u32>::from_values(counters).save();

    let mut buf = Vec::new();
    write_len_prefixed_blob(&mut buf, &hash_index_col)?;
    write_len_prefixed_blob(&mut buf, &counter_col)?;
    Ok(buf)
}

/// Columnar encode for one schema column of transaction cell values.
fn encode_txn_value_column(
    values: &[TxnCellValue],
    col_type: &ColType,
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, PersistError> {
    match col_type {
        ColType::EntityType { .. } => encode_txn_row_ref_column(values, hash_mapper),
        ColType::PrimType { prim } => match prim {
            PrimType::PrimInt => {
                let mut ints = Vec::with_capacity(values.len());
                for value in values {
                    let TxnCellValue::Int(i) = value else {
                        return Err(PersistError::SchemaError(format!(
                            "expected int, got {value:?}"
                        )));
                    };
                    ints.push(*i);
                }
                Ok(Column::<i64>::from_values(ints).save())
            }
            PrimType::PrimString => {
                let mut strings = Vec::with_capacity(values.len());
                for value in values {
                    let TxnCellValue::Str(s) = value else {
                        return Err(PersistError::SchemaError(format!(
                            "expected string, got {value:?}"
                        )));
                    };
                    strings.push(s.clone());
                }
                Ok(Column::<String>::from_values(strings).save())
            }
        },
        ColType::Tuple { .. } => Err(PersistError::SchemaError(
            "tuple columns are not supported yet".into(),
        )),
    }
}

/// Decode a column produced by [`encode_txn_value_column`].
fn decode_txn_value_column(
    data: &[u8],
    col_type: &ColType,
    hashes: &[CommitHash],
) -> Result<Vec<TxnCellValue>, PersistError> {
    match col_type {
        ColType::EntityType { .. } => decode_txn_row_ref_column(data, hashes),
        ColType::PrimType { prim } => match prim {
            PrimType::PrimInt => Ok(Column::<i64>::load(data)?
                .iter()
                .map(TxnCellValue::Int)
                .collect()),
            PrimType::PrimString => Ok(Column::<String>::load(data)?
                .iter()
                .map(|s| TxnCellValue::Str(s.to_owned()))
                .collect()),
        },
        ColType::Tuple { .. } => Err(PersistError::SchemaError(
            "tuple columns are not supported yet".into(),
        )),
    }
}

/// Encode a same-table group of add operations as schema-shaped column blobs.
///
/// Layout:
/// `[table_path_len:u32][table_path: utf-8][row_count:u32][column_count:u32]`
/// followed by one len-prefixed column blob per schema column.
fn encode_op_group(
    table: &Path,
    schema: &Schema,
    ops: &[PendingOp],
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, PersistError> {
    let mut rows = Vec::with_capacity(ops.len());
    for op in ops {
        let PendingOp::Add {
            table: op_table,
            values,
            ..
        } = op;
        if op_table != table {
            return Err(PersistError::SchemaError(format!(
                "op group table mismatch: expected {table:?}, got {op_table:?}"
            )));
        }
        if values.len() != schema.columns.len() {
            return Err(PersistError::SchemaError(format!(
                "op group column count mismatch: expected {}, got {}",
                schema.columns.len(),
                values.len()
            )));
        }
        rows.push(values);
    }

    let mut buf = Vec::new();
    let path = table.to_string();
    let path_len: u32 = path
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("table path too long".into()))?;
    buf.write_all(&path_len.to_le_bytes())?;
    buf.write_all(path.as_bytes())?;

    let row_count: u32 = rows
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("too many rows in op group".into()))?;
    buf.write_all(&row_count.to_le_bytes())?;

    let column_count: u32 = schema
        .columns
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("too many columns in op group".into()))?;
    buf.write_all(&column_count.to_le_bytes())?;

    for (column_index, col_type) in schema.columns.iter().enumerate() {
        let values = rows
            .iter()
            .map(|row| row[column_index].clone())
            .collect::<Vec<_>>();
        let blob = encode_txn_value_column(&values, col_type, hash_mapper)?;
        write_len_prefixed_blob(&mut buf, &blob)?;
    }

    Ok(buf)
}

fn decode_txn_row_ref_column(
    data: &[u8],
    hashes: &[CommitHash],
) -> Result<Vec<TxnCellValue>, PersistError> {
    let mut pos = 0usize;
    let hash_index_len = read_u64(data, &mut pos, "txn row-ref hash-index column length")? as usize;
    let hash_index_blob = read_slice(
        data,
        &mut pos,
        hash_index_len,
        "txn row-ref hash-index column",
    )?;
    let counter_len = read_u64(data, &mut pos, "txn row-ref counter column length")? as usize;
    let counter_blob = read_slice(data, &mut pos, counter_len, "txn row-ref counter column")?;
    if pos != data.len() {
        return Err(PersistError::DataFormatError(format!(
            "trailing bytes after txn row-ref column: {} bytes",
            data.len() - pos
        )));
    }

    let hash_indices: Vec<u32> = Column::<u32>::load(hash_index_blob)?.iter().collect();
    let counters: Vec<u32> = DeltaColumn::<u32>::load(counter_blob)?.iter().collect();
    if hash_indices.len() != counters.len() {
        return Err(PersistError::DataFormatError(format!(
            "txn row-ref subcolumn length mismatch: hash indexes {}, counters {}",
            hash_indices.len(),
            counters.len()
        )));
    }

    hash_indices
        .into_iter()
        .zip(counters)
        .map(|(hash_index, counter)| {
            if hash_index == LOCAL_COMMIT_HASH_INDEX {
                Ok(TxnCellValue::Id(RowRef::Pending(TempRowId(counter))))
            } else {
                let commit = hashes.get(hash_index as usize).copied().ok_or_else(|| {
                    PersistError::DataFormatError(format!(
                        "txn row-ref hash index {hash_index} out of bounds"
                    ))
                })?;
                Ok(TxnCellValue::Id(RowRef::Existing(RowId {
                    commit,
                    counter,
                })))
            }
        })
        .collect()
}

fn write_len_prefixed_blob(buf: &mut Vec<u8>, data: &[u8]) -> Result<(), PersistError> {
    let len: u64 = data
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("blob too large".into()))?;
    buf.write_all(&len.to_le_bytes())?;
    buf.write_all(data)?;
    Ok(())
}

fn read_len_prefixed_blob<'a>(
    data: &'a [u8],
    pos: &mut usize,
    label: &'static str,
) -> Result<&'a [u8], PersistError> {
    let len = read_u64(data, pos, "length-prefixed blob length")? as usize;
    read_slice(data, pos, len, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::hash::HASH_SIZE;
    use crate::ir::{ColType, Path, PrimType};
    use crate::table::RowId;
    use crate::txn::ops::{RowRef, TempRowId};

    #[test]
    fn txn_row_ref_column_round_trips_existing_and_pending_refs() {
        let ha = CommitHash([1u8; HASH_SIZE]);
        let hb = CommitHash([2u8; HASH_SIZE]);
        let mut hash_mapper = HashMapper::new();
        hash_mapper.insert(ha);
        hash_mapper.insert(hb);
        let values = vec![
            TxnCellValue::Id(RowRef::Existing(RowId {
                commit: ha,
                counter: 7,
            })),
            TxnCellValue::Id(RowRef::Pending(TempRowId(0))),
            TxnCellValue::Id(RowRef::Existing(RowId {
                commit: hb,
                counter: 11,
            })),
            TxnCellValue::Id(RowRef::Pending(TempRowId(2))),
        ];

        let encoded = encode_txn_row_ref_column(&values, &hash_mapper).expect("encode row refs");
        let decoded =
            decode_txn_row_ref_column(&encoded, hash_mapper.hashes()).expect("decode row refs");

        assert_eq!(decoded, values);
    }

    #[test]
    fn txn_row_ref_column_rejects_non_ref_values() {
        let err = encode_txn_row_ref_column(&[TxnCellValue::Int(42)], &HashMapper::new())
            .expect_err("int is not a row ref");

        assert!(matches!(err, PersistError::SchemaError(_)));
    }

    #[test]
    fn txn_row_ref_column_rejects_unmapped_existing_hashes() {
        let value = TxnCellValue::Id(RowRef::Existing(RowId {
            commit: CommitHash([9u8; HASH_SIZE]),
            counter: 1,
        }));
        let err = encode_txn_row_ref_column(&[value], &HashMapper::new())
            .expect_err("existing hash must be in dictionary");

        assert!(matches!(err, PersistError::SchemaError(_)));
    }

    #[test]
    fn txn_value_column_round_trips_int() {
        let col = ColType::PrimType {
            prim: PrimType::PrimInt,
        };
        let values = vec![1i64.into(), 2i64.into(), (-3i64).into()];
        let encoded =
            encode_txn_value_column(&values, &col, &HashMapper::new()).expect("encode int col");
        let decoded = decode_txn_value_column(&encoded, &col, &[]).expect("decode int col");
        assert_eq!(decoded, values);
    }

    #[test]
    fn txn_value_column_round_trips_string() {
        let col = ColType::PrimType {
            prim: PrimType::PrimString,
        };
        let values = vec!["a".into(), "bc".into()];
        let encoded =
            encode_txn_value_column(&values, &col, &HashMapper::new()).expect("encode str col");
        let decoded = decode_txn_value_column(&encoded, &col, &[]).expect("decode str col");
        assert_eq!(decoded, values);
    }

    #[test]
    fn txn_value_column_entity_delegates_to_row_ref_encoding() {
        let ha = CommitHash([1u8; HASH_SIZE]);
        let mut hash_mapper = HashMapper::new();
        hash_mapper.insert(ha);
        let col = ColType::EntityType {
            path: Path::from("T.E"),
        };
        let values = vec![
            TxnCellValue::Id(RowRef::Existing(RowId {
                commit: ha,
                counter: 1,
            })),
            TxnCellValue::Id(RowRef::Pending(TempRowId(0))),
        ];
        let encoded =
            encode_txn_value_column(&values, &col, &hash_mapper).expect("encode entity col");
        let decoded = decode_txn_value_column(&encoded, &col, hash_mapper.hashes())
            .expect("decode entity col");
        assert_eq!(decoded, values);
    }

    #[test]
    fn txn_value_column_rejects_type_mismatch_for_int_column() {
        let col = ColType::PrimType {
            prim: PrimType::PrimInt,
        };
        let err = encode_txn_value_column(
            &[TxnCellValue::Str("nope".into())],
            &col,
            &HashMapper::new(),
        )
        .expect_err("string in int column");
        assert!(matches!(err, PersistError::SchemaError(_)));
    }

    #[test]
    fn txn_value_column_rejects_tuple_schema() {
        let col = ColType::Tuple { fields: vec![] };
        let err = encode_txn_value_column(&[], &col, &HashMapper::new()).expect_err("tuple");
        assert!(matches!(err, PersistError::SchemaError(_)));
    }

    #[test]
    fn op_group_encodes_schema_columns() {
        let table = Path::from("T");
        let entity = Path::from("T.E");
        let schema = Schema {
            columns: vec![
                ColType::PrimType {
                    prim: PrimType::PrimInt,
                },
                ColType::PrimType {
                    prim: PrimType::PrimString,
                },
                ColType::EntityType { path: entity },
            ],
            primary_key: None,
        };
        let ha = CommitHash([1u8; HASH_SIZE]);
        let mut hash_mapper = HashMapper::new();
        hash_mapper.insert(ha);
        let ops = vec![
            PendingOp::Add {
                row_id: TempRowId(0),
                table: table.clone(),
                values: vec![
                    1i64.into(),
                    "a".into(),
                    TxnCellValue::Id(RowRef::Existing(RowId {
                        commit: ha,
                        counter: 7,
                    })),
                ],
            },
            PendingOp::Add {
                row_id: TempRowId(1),
                table: table.clone(),
                values: vec![
                    2i64.into(),
                    "b".into(),
                    TxnCellValue::Id(RowRef::Pending(TempRowId(0))),
                ],
            },
        ];

        let encoded = encode_op_group(&table, &schema, &ops, &hash_mapper).expect("encode group");
        let mut pos = 0usize;
        let path_len = read_u32(&encoded, &mut pos, "path length").unwrap() as usize;
        let path_bytes = read_slice(&encoded, &mut pos, path_len, "path").unwrap();
        assert_eq!(std::str::from_utf8(path_bytes).unwrap(), "T");
        assert_eq!(read_u32(&encoded, &mut pos, "row count").unwrap(), 2);
        assert_eq!(read_u32(&encoded, &mut pos, "column count").unwrap(), 3);

        for (index, col_type) in schema.columns.iter().enumerate() {
            let len = read_u64(&encoded, &mut pos, "column length").unwrap() as usize;
            let blob = read_slice(&encoded, &mut pos, len, "column blob").unwrap();
            let decoded =
                decode_txn_value_column(blob, col_type, hash_mapper.hashes()).expect("decode col");
            match index {
                0 => assert_eq!(decoded, vec![1i64.into(), 2i64.into()]),
                1 => assert_eq!(decoded, vec!["a".into(), "b".into()]),
                2 => assert_eq!(
                    decoded,
                    vec![
                        TxnCellValue::Id(RowRef::Existing(RowId {
                            commit: ha,
                            counter: 7,
                        })),
                        TxnCellValue::Id(RowRef::Pending(TempRowId(0))),
                    ]
                ),
                _ => unreachable!(),
            }
        }
        assert_eq!(pos, encoded.len());
    }

    #[test]
    fn op_group_rejects_table_mismatch() {
        let schema = Schema {
            columns: vec![],
            primary_key: None,
        };
        let op = PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("Other"),
            values: vec![],
        };
        let err = encode_op_group(&Path::from("T"), &schema, &[op], &HashMapper::new())
            .expect_err("table mismatch");
        assert!(matches!(err, PersistError::SchemaError(_)));
    }

    #[test]
    fn op_group_rejects_column_count_mismatch() {
        let table = Path::from("T");
        let schema = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let op = PendingOp::Add {
            row_id: TempRowId(0),
            table: table.clone(),
            values: vec![],
        };
        let err = encode_op_group(&table, &schema, &[op], &HashMapper::new())
            .expect_err("column count mismatch");
        assert!(matches!(err, PersistError::SchemaError(_)));
    }

    #[test]
    fn commit_body_encodes_table_sequence_and_first_seen_groups() {
        let table_a = Path::from("A");
        let table_b = Path::from("B");
        let schema_a = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let schema_b = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimString,
            }],
            primary_key: None,
        };
        let schemas = vec![
            (table_a.clone(), schema_a.clone()),
            (table_b.clone(), schema_b.clone()),
        ];
        let pending = vec![
            PendingOp::Add {
                row_id: TempRowId(0),
                table: table_a.clone(),
                values: vec![1i64.into()],
            },
            PendingOp::Add {
                row_id: TempRowId(1),
                table: table_b.clone(),
                values: vec!["x".into()],
            },
            PendingOp::Add {
                row_id: TempRowId(2),
                table: table_a.clone(),
                values: vec![2i64.into()],
            },
        ];

        let encoded = encode_commit_body(
            &pending,
            |path| {
                schemas
                    .iter()
                    .find(|(p, _)| p == path)
                    .map(|(_, schema)| schema)
            },
            &HashMapper::new(),
        )
        .expect("encode commit body");

        let mut pos = 0usize;
        assert_eq!(read_u32(&encoded, &mut pos, "op count").unwrap(), 3);

        let op_kind_len = read_u64(&encoded, &mut pos, "op kind length").unwrap() as usize;
        let op_kind_blob = read_slice(&encoded, &mut pos, op_kind_len, "op kind").unwrap();
        let op_kinds: Vec<u32> = Column::<u32>::load(op_kind_blob).unwrap().iter().collect();
        assert_eq!(op_kinds, vec![OP_KIND_ADD, OP_KIND_ADD, OP_KIND_ADD]);

        let sequence_len = read_u64(&encoded, &mut pos, "sequence length").unwrap() as usize;
        let sequence_blob = read_slice(&encoded, &mut pos, sequence_len, "sequence").unwrap();
        let table_sequence: Vec<u32> = Column::<u32>::load(sequence_blob).unwrap().iter().collect();
        assert_eq!(table_sequence, vec![0, 1, 0]);

        assert_eq!(read_u32(&encoded, &mut pos, "group count").unwrap(), 2);

        let group_a_len = read_u64(&encoded, &mut pos, "group a length").unwrap() as usize;
        let group_a = read_slice(&encoded, &mut pos, group_a_len, "group a").unwrap();
        let mut group_pos = 0usize;
        let path_len = read_u32(group_a, &mut group_pos, "group a path length").unwrap() as usize;
        let path = read_slice(group_a, &mut group_pos, path_len, "group a path").unwrap();
        assert_eq!(std::str::from_utf8(path).unwrap(), "A");
        assert_eq!(
            read_u32(group_a, &mut group_pos, "group a rows").unwrap(),
            2
        );
        assert_eq!(
            read_u32(group_a, &mut group_pos, "group a columns").unwrap(),
            1
        );

        let group_b_len = read_u64(&encoded, &mut pos, "group b length").unwrap() as usize;
        let group_b = read_slice(&encoded, &mut pos, group_b_len, "group b").unwrap();
        let mut group_pos = 0usize;
        let path_len = read_u32(group_b, &mut group_pos, "group b path length").unwrap() as usize;
        let path = read_slice(group_b, &mut group_pos, path_len, "group b path").unwrap();
        assert_eq!(std::str::from_utf8(path).unwrap(), "B");
        assert_eq!(
            read_u32(group_b, &mut group_pos, "group b rows").unwrap(),
            1
        );
        assert_eq!(
            read_u32(group_b, &mut group_pos, "group b columns").unwrap(),
            1
        );

        assert_eq!(pos, encoded.len());
    }

    #[test]
    fn commit_body_round_trips_interleaved_groups() {
        let table_a = Path::from("A");
        let table_b = Path::from("B");
        let schema_a = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        };
        let schema_b = Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimString,
            }],
            primary_key: None,
        };
        let schemas = vec![
            (table_a.clone(), schema_a.clone()),
            (table_b.clone(), schema_b.clone()),
        ];
        let pending = vec![
            PendingOp::Add {
                row_id: TempRowId(0),
                table: table_a.clone(),
                values: vec![1i64.into()],
            },
            PendingOp::Add {
                row_id: TempRowId(1),
                table: table_b.clone(),
                values: vec!["x".into()],
            },
            PendingOp::Add {
                row_id: TempRowId(2),
                table: table_a.clone(),
                values: vec![2i64.into()],
            },
        ];
        let schema_for = |path: &Path| {
            schemas
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, schema)| schema)
        };

        let encoded =
            encode_commit_body(&pending, schema_for, &HashMapper::new()).expect("encode body");
        let decoded = decode_commit_body(&encoded, &[], schema_for).expect("decode body");

        assert_eq!(decoded, pending);
    }

    #[test]
    fn commit_body_decode_rejects_missing_schema() {
        let table = Path::from("T");
        let schema = Schema {
            columns: vec![],
            primary_key: None,
        };
        let pending = vec![PendingOp::Add {
            row_id: TempRowId(0),
            table: table.clone(),
            values: vec![],
        }];
        let encoded = encode_commit_body(
            &pending,
            |path| (path == &table).then_some(&schema),
            &HashMapper::new(),
        )
        .expect("encode body");

        let err = decode_commit_body(&encoded, &[], |_| None).expect_err("missing schema");
        assert!(matches!(err, PersistError::SchemaError(_)));
    }

    #[test]
    fn commit_body_rejects_missing_schema() {
        let pending = vec![PendingOp::Add {
            row_id: TempRowId(0),
            table: Path::from("Missing"),
            values: vec![],
        }];

        let err =
            encode_commit_body(&pending, |_| None, &HashMapper::new()).expect_err("missing schema");
        assert!(matches!(err, PersistError::SchemaError(_)));
    }
}
