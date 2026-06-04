use hexane::v1::{Column, DeltaColumn};
use std::io::Write;

use crate::commit::author::Author;
use crate::commit::leb128 as commit_leb128;
use crate::commit::wire::prim::{PrimValue, ValueMeta, ValueType};
use crate::{
    commit::{
        error::CodecError,
        hash::{CommitHash, HASH_SIZE},
        hash_dict::{HashMapper, read_hash_dict, write_hash_dict},
        utils::read_slice,
    },
    ir::{ColType, Path, PrimType, Schema},
    table::RowId,
    txn::ops::{OP_KIND_ADD, PendingOp, RowRef, TempRowId, TxnCellValue},
};

// TODO change this to i32 when we support it as a column type
/// This encodes the hash index of all hashes referring to the current commit
const LOCAL_COMMIT_HASH_INDEX: i64 = -1;

/// Data structure to be hashed, in the order as the fields are defined here.
/// See also payload canocial payload encoding format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommitData {
    pub(crate) author: Author,
    pub(crate) deps: Vec<CommitHash>,
    /// Commit hashes referenced by op ids, dictionary order on the wire.
    pub(crate) other_hashes: Vec<CommitHash>,
    /// Identifier of the commit author. Currently a placeholder of all zeros.
    pub(crate) timestamp: i64,
    pub(crate) message: Option<String>,
    pub(crate) pending: Vec<PendingOp>,
    /// Uninterpreted trailing bytes, reserved for forward compatibility
    pub(crate) extra_bytes: Vec<u8>,
}

impl CommitData {
    pub(crate) fn new(
        deps: Vec<CommitHash>,
        author: Author,
        timestamp: i64,
        message: Option<String>,
        pending: Vec<PendingOp>,
    ) -> Self {
        Self {
            deps,
            author,
            timestamp,
            message,
            other_hashes: vec![],
            pending,
            extra_bytes: vec![],
        }
    }
}

// ── Canonical payload encoding ───────────────────────────────────────────
//
// A persisted or transmitted commit is framed as `header || payload`:
//
//   [MAGIC][checksum:4][chunk_type:1][data_len:leb]   (written by Header::write)
//   [payload]                                         (produced here)
//
// The hash and checksum are computed over the payload alone, so the framing is
// never part of the hash preimage. `serialize` and `deserialize` deal only with
// the payload; the header is written and parsed by the chunk layer.
//
// The payload is laid out like so (scalar integers use LEB128):
//
//   [author_len]
//   [author: author_len bytes]                   (author id)
//   [deps_count]
//   [CommitHash × deps_count]            (32 bytes each)
//   [other_hash_count]
//   [CommitHash × other_hash_count]      (32 bytes each)
//   [timestamp]
//   [message_len]                        (0 when None)
//   [message: utf-8 bytes]
//   [commit body]                        (operations, see encode_commit_body)
//   [extra_bytes]                        (trailing reserved bytes, see CommitData::extra_bytes)
//
pub(crate) fn serialize<'s, F>(
    data: &CommitData,
    // TODO hashmapper order
    hash_mapper: &HashMapper,
    schema_for: F,
) -> Result<Vec<u8>, CodecError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
    let mut buf: Vec<u8> = Vec::new();

    commit_leb128::write_len(&mut buf, data.author.as_bytes().len());
    buf.write_all(data.author.as_bytes()).unwrap();
    commit_leb128::write_len(&mut buf, data.deps.len());
    for dep in &data.deps {
        buf.write_all(dep.as_bytes()).unwrap();
    }
    write_hash_dict(&mut buf, hash_mapper).expect("hash dictionary serializes");

    commit_leb128::write_i64(&mut buf, data.timestamp);

    let msg = data.message.as_deref().unwrap_or("");
    commit_leb128::write_len(&mut buf, msg.len());
    buf.write_all(msg.as_bytes()).unwrap();

    let body = encode_commit_body(&data.pending, schema_for, hash_mapper)?;
    buf.write_all(&body).unwrap();

    buf.write_all(&data.extra_bytes).unwrap();

    Ok(buf)
}

/// Encode the normal commit body after the shared commit metadata.
///
/// Layout:
/// `[op_count]`
/// `[op_kind_column: len-prefixed hexane Column<u32>]`
/// `[table_sequence_column: len-prefixed hexane Column<u32>]`
/// `[group_count]`
/// followed by one len-prefixed [`encode_op_group`] blob per first-seen table.
fn encode_commit_body<'s, F>(
    pending: &[PendingOp],
    schema_for: F,
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, CodecError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
    // TODO grouping order should be determinstic
    let mut groups: Vec<(Path, Vec<PendingOp>)> = Vec::new();
    /*  table_sequence[i] is the group index of the ith operation. It effectively
        stores the original order the operations are in.
        operation. For example, the following two set of operations will have
        two different table_sequence.
        add table1 xx
        add table2 yy 12
        add table1 zz
        table_sequence = [0, 1, 0]

        add table1 xx
        add table1 zz
        add table2 yy 12
        table_sequence = [0, 0, 1]

    */

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
    // TODO check necessaity
    let op_count: u32 = pending
        .len()
        .try_into()
        .map_err(|_| CodecError::Other("too many ops in commit body".into()))?;
    commit_leb128::write_u32(&mut buf, op_count);

    let op_kind_col = Column::<u32>::from_values(op_kinds).save();
    commit_leb128::write_len_prefixed_bytes(&mut buf, &op_kind_col);

    let table_sequence_col = Column::<u32>::from_values(table_sequence).save();
    commit_leb128::write_len_prefixed_bytes(&mut buf, &table_sequence_col);

    let group_count: u32 = groups
        .len()
        .try_into()
        .map_err(|_| CodecError::Other("too many op groups in commit body".into()))?;
    commit_leb128::write_u32(&mut buf, group_count);

    // NOTE each group is encoded with its row and columns first, and then the column
    // body. Automerge does the header + concatenated body approach
    for (table, ops) in &groups {
        let schema = schema_for(table).ok_or_else(|| {
            CodecError::SchemaError(format!("missing schema for table {table:?}"))
        })?;
        let group_blob = encode_op_group(table, schema, ops, hash_mapper)?;
        commit_leb128::write_len_prefixed_bytes(&mut buf, &group_blob);
    }

    Ok(buf)
}

/// encode the row_ref column, which might be a pending id or a already resolved id
fn encode_txn_row_ref_column(
    values: &[TxnCellValue],
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, CodecError> {
    let mut hash_indices: Vec<i64> = Vec::with_capacity(values.len());
    let mut counters = Vec::with_capacity(values.len());

    for value in values {
        let TxnCellValue::Id(row_ref) = value else {
            return Err(CodecError::SchemaError(format!(
                "expected row reference, got {value:?}"
            )));
        };

        match row_ref {
            RowRef::Existing(RowId { commit, counter }) => {
                let hash_index = hash_mapper.index(*commit).ok_or_else(|| {
                    CodecError::SchemaError(format!("missing commit hash in dictionary: {commit}"))
                })? as i64;
                hash_indices.push(hash_index);
                counters.push(*counter);
            }
            RowRef::Pending(temp_id) => {
                hash_indices.push(LOCAL_COMMIT_HASH_INDEX);
                counters.push(temp_id.0);
            }
        }
    }

    let hash_index_col = Column::<i64>::from_values(hash_indices).save();
    let counter_col = DeltaColumn::<u32>::from_values(counters).save();

    let mut buf = Vec::new();
    commit_leb128::write_len_prefixed_bytes(&mut buf, &hash_index_col);
    commit_leb128::write_len_prefixed_bytes(&mut buf, &counter_col);
    Ok(buf)
}

fn encode_txn_prim_value_column(
    values: &[TxnCellValue],
    prim: &PrimType,
) -> Result<Vec<u8>, CodecError> {
    match prim {
        PrimType::PrimInt => {
            let mut meta: Vec<ValueMeta> = Vec::with_capacity(values.len());
            let mut value_bytes = Vec::new();
            for value in values {
                let TxnCellValue::Int(i) = value else {
                    return Err(CodecError::SchemaError(format!(
                        "expected int, got {value:?}"
                    )));
                };
                meta.push((&PrimValue::Int(*i)).into());
                // canonical bytes; length here MUST equal ValueMeta::length()
                leb128::write::signed(&mut value_bytes, *i)
                    .map_err(|e| CodecError::DataFormatError(e.to_string()))?;
            }
            let meta_blob = Column::<ValueMeta>::from_values(meta).save();
            let mut buf = Vec::new();
            commit_leb128::write_len_prefixed_bytes(&mut buf, &meta_blob);
            commit_leb128::write_len_prefixed_bytes(&mut buf, &value_bytes);
            Ok(buf)
        }
        PrimType::PrimString => {
            let mut meta: Vec<ValueMeta> = Vec::with_capacity(values.len());
            let mut value_bytes = Vec::new();
            for value in values {
                let TxnCellValue::Str(s) = value else {
                    return Err(CodecError::SchemaError(format!(
                        "expected string, got {value:?}"
                    )));
                };
                meta.push((&PrimValue::Str(s)).into());
                // raw utf-8 bytes; length here MUST equal ValueMeta::length()
                value_bytes.extend_from_slice(s.as_bytes());
            }
            let meta_blob = Column::<ValueMeta>::from_values(meta).save();
            let mut buf = Vec::new();
            commit_leb128::write_len_prefixed_bytes(&mut buf, &meta_blob);
            commit_leb128::write_len_prefixed_bytes(&mut buf, &value_bytes);
            Ok(buf)
        }
    }
}

/*
  NOTE encoding is hardcoded per `ColType` and must stay canonical/deterministic
 (commit bytes are hashed for identity). Revisit a per-column encoding tag only
 if a real workload shows an alternative encoding beats the tag overhead.
*/

/// Columnar encode for one schema column of transaction cell values.
fn encode_txn_value_column(
    values: &[TxnCellValue],
    col_type: &ColType,
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, CodecError> {
    match col_type {
        ColType::EntityType { .. } => encode_txn_row_ref_column(values, hash_mapper),
        ColType::PrimType { prim } => encode_txn_prim_value_column(values, prim),
        ColType::Tuple { .. } => Err(CodecError::SchemaError(
            "tuple columns are not supported yet".into(),
        )),
    }
}

/// Encode a same-table group of add operations as schema-shaped column blobs.
///
/// Layout:
/// `[table_path_len][table_path: utf-8][row_count][column_count]`
/// followed by one len-prefixed column blob per schema column.
fn encode_op_group(
    table: &Path,
    schema: &Schema,
    ops: &[PendingOp],
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, CodecError> {
    let mut rows = Vec::with_capacity(ops.len());
    for op in ops {
        let PendingOp::Add {
            table: op_table,
            values,
            ..
        } = op;
        if op_table != table {
            return Err(CodecError::SchemaError(format!(
                "op group table mismatch: expected {table:?}, got {op_table:?}"
            )));
        }
        if values.len() != schema.columns.len() {
            return Err(CodecError::SchemaError(format!(
                "op group column count mismatch: expected {}, got {}",
                schema.columns.len(),
                values.len()
            )));
        }
        rows.push(values);
    }

    let mut buf = Vec::new();
    let path = table.to_string();
    commit_leb128::write_len(&mut buf, path.len());
    buf.write_all(path.as_bytes())?;

    commit_leb128::write_len(&mut buf, rows.len());

    commit_leb128::write_len(&mut buf, schema.columns.len());

    for (column_index, col_type) in schema.columns.iter().enumerate() {
        let values = rows
            .iter()
            .map(|row| row[column_index].clone())
            .collect::<Vec<_>>();
        let blob = encode_txn_value_column(&values, col_type, hash_mapper)?;
        commit_leb128::write_len_prefixed_bytes(&mut buf, &blob);
    }

    Ok(buf)
}

/// Parse canonical payload bytes (the slice passed to [`crate::persist::chunk::hash`],
/// not including the chunk type or outer length prefix).
pub(crate) fn deserialize<'s, F>(data: &[u8], schema_for: F) -> Result<CommitData, CodecError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
    let mut pos = 0usize;

    let author_len = commit_leb128::read_len(data, &mut pos, "author_len")?;
    let author_bytes = read_slice(data, &mut pos, author_len, "author")?;
    let author = author_bytes.into();

    let deps_count = commit_leb128::read_len(data, &mut pos, "deps count")?;
    let mut deps = Vec::with_capacity(deps_count);
    for _ in 0..deps_count {
        let bytes = read_slice(data, &mut pos, HASH_SIZE, "dep hash")?;
        let mut h = [0u8; HASH_SIZE];
        h.copy_from_slice(bytes);
        deps.push(CommitHash(h));
    }

    let other_hashes = read_hash_dict(data, &mut pos)?;

    let timestamp = commit_leb128::read_i64(data, &mut pos, "timestamp")?;

    let msg_len = commit_leb128::read_len(data, &mut pos, "message length")?;
    let msg_bytes = read_slice(data, &mut pos, msg_len, "message")?;
    let message = if msg_len == 0 {
        None
    } else {
        Some(
            std::str::from_utf8(msg_bytes)
                .map_err(|_| CodecError::DataFormatError("commit message: invalid utf-8".into()))?
                .to_owned(),
        )
    };

    let pending = decode_commit_body(data, &mut pos, &other_hashes, schema_for)?;
    let extra_bytes = data[pos..].to_vec();

    Ok(CommitData {
        author,
        deps,
        other_hashes,
        timestamp,
        message,
        pending,
        extra_bytes,
    })
}

/// Decode the commit body, advancing `pos` past the bytes it consumes. Anything
/// left after `pos` belongs to [`CommitData::extra_bytes`].
fn decode_commit_body<'s, F>(
    data: &[u8],
    pos: &mut usize,
    hashes: &[CommitHash],
    schema_for: F,
) -> Result<Vec<PendingOp>, CodecError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
    let op_count = commit_leb128::read_len(data, pos, "op count")?;

    let op_kind_blob = commit_leb128::read_len_prefixed_bytes(data, pos, "op kind column")?;
    let op_kinds: Vec<u32> = Column::<u32>::load(op_kind_blob)?.iter().collect();
    if op_kinds.len() != op_count {
        return Err(CodecError::DataFormatError(format!(
            "op kind column length mismatch: expected {op_count}, got {}",
            op_kinds.len()
        )));
    }

    let table_sequence_blob =
        commit_leb128::read_len_prefixed_bytes(data, pos, "table sequence column")?;
    let table_sequence: Vec<u32> = Column::<u32>::load(table_sequence_blob)?.iter().collect();
    if table_sequence.len() != op_count {
        return Err(CodecError::DataFormatError(format!(
            "table sequence column length mismatch: expected {op_count}, got {}",
            table_sequence.len()
        )));
    }

    let group_count = commit_leb128::read_len(data, pos, "op group count")?;
    let mut groups = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        let group_blob = commit_leb128::read_len_prefixed_bytes(data, pos, "op group")?;
        groups.push(decode_op_group(group_blob, hashes, &schema_for)?);
    }

    // The body ends at `pos`. Anything beyond it is `extra_bytes`.

    // reconstruct the pending ops
    let mut group_offsets = vec![0usize; groups.len()];
    let mut pending = Vec::with_capacity(op_count);
    for op_idx in 0..op_count {
        let op_kind = op_kinds[op_idx];
        if op_kind != OP_KIND_ADD {
            return Err(CodecError::DataFormatError(format!(
                "unknown op kind {op_kind}"
            )));
        }

        let group_idx = table_sequence[op_idx] as usize;
        let group = groups.get(group_idx).ok_or_else(|| {
            CodecError::DataFormatError(format!(
                "table sequence index {group_idx} out of bounds (group count {})",
                groups.len()
            ))
        })?;
        let row_idx = group_offsets[group_idx];
        let values = group.rows.get(row_idx).cloned().ok_or_else(|| {
            CodecError::DataFormatError(format!("op group {group_idx} exhausted at row {row_idx}"))
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
            return Err(CodecError::DataFormatError(format!(
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
) -> Result<DecodedOpGroup, CodecError>
where
    F: Fn(&Path) -> Option<&'s Schema>,
{
    let mut pos = 0usize;

    let path_len = commit_leb128::read_len(data, &mut pos, "op group table path length")?;
    let path_bytes = read_slice(data, &mut pos, path_len, "op group table path")?;
    let table =
        Path::from(std::str::from_utf8(path_bytes).map_err(|_| {
            CodecError::DataFormatError("op group table path: invalid utf-8".into())
        })?);

    let row_count = commit_leb128::read_len(data, &mut pos, "op group row count")?;
    let column_count = commit_leb128::read_len(data, &mut pos, "op group column count")?;
    let schema = schema_for(&table)
        .ok_or_else(|| CodecError::SchemaError(format!("missing schema for table {table:?}")))?;
    if column_count != schema.columns.len() {
        return Err(CodecError::DataFormatError(format!(
            "op group column count mismatch: encoded {column_count}, schema expects {}",
            schema.columns.len()
        )));
    }

    let mut columns = Vec::with_capacity(column_count);
    for (column_index, col_type) in schema.columns.iter().enumerate() {
        let column_blob =
            commit_leb128::read_len_prefixed_bytes(data, &mut pos, "op group column")?;
        let column = decode_txn_value_column(column_blob, col_type, hashes)?;
        if column.len() != row_count {
            return Err(CodecError::DataFormatError(format!(
                "op group column {column_index} length mismatch: expected {row_count}, got {}",
                column.len()
            )));
        }
        columns.push(column);
    }

    if pos != data.len() {
        return Err(CodecError::DataFormatError(format!(
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

/// Decode a column produced by [`encode_txn_value_column`].
fn decode_txn_value_column(
    data: &[u8],
    col_type: &ColType,
    hashes: &[CommitHash],
) -> Result<Vec<TxnCellValue>, CodecError> {
    match col_type {
        ColType::EntityType { .. } => decode_txn_row_ref_column(data, hashes),
        ColType::PrimType { prim } => decode_txn_prim_value_column(data, prim),
        ColType::Tuple { .. } => Err(CodecError::SchemaError(
            "tuple columns are not supported yet".into(),
        )),
    }
}

fn decode_txn_row_ref_column(
    data: &[u8],
    hashes: &[CommitHash],
) -> Result<Vec<TxnCellValue>, CodecError> {
    let mut pos = 0usize;
    let hash_index_blob =
        commit_leb128::read_len_prefixed_bytes(data, &mut pos, "txn row-ref hash-index column")?;
    let counter_blob =
        commit_leb128::read_len_prefixed_bytes(data, &mut pos, "txn row-ref counter column")?;
    if pos != data.len() {
        return Err(CodecError::DataFormatError(format!(
            "trailing bytes after txn row-ref column: {} bytes",
            data.len() - pos
        )));
    }

    let hash_indices: Vec<i64> = Column::<i64>::load(hash_index_blob)?.iter().collect();
    let counters: Vec<u32> = DeltaColumn::<u32>::load(counter_blob)?.iter().collect();
    if hash_indices.len() != counters.len() {
        return Err(CodecError::DataFormatError(format!(
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
                    CodecError::DataFormatError(format!(
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

fn decode_txn_prim_value_column(
    data: &[u8],
    prim: &PrimType,
) -> Result<Vec<TxnCellValue>, CodecError> {
    let mut pos = 0usize;
    let meta_blob =
        commit_leb128::read_len_prefixed_bytes(data, &mut pos, "txn prim value meta column")?;
    let value_blob =
        commit_leb128::read_len_prefixed_bytes(data, &mut pos, "txn prim value column")?;
    if pos != data.len() {
        return Err(CodecError::DataFormatError(format!(
            "trailing bytes after txn prim value column: {} bytes",
            data.len() - pos
        )));
    }

    let meta: Vec<ValueMeta> = Column::<ValueMeta>::load(meta_blob)?.iter().collect();
    let mut values = Vec::with_capacity(meta.len());
    let mut offset = 0usize;

    for m in &meta {
        let len = m.length();
        let end = offset
            .checked_add(len)
            .filter(|e| *e <= value_blob.len())
            .ok_or_else(|| {
                CodecError::DataFormatError(format!(
                    "value column overrun: offset {offset} + len {len} > {}",
                    value_blob.len()
                ))
            })?;
        let bytes = &value_blob[offset..end];

        let ty = m.type_code();
        if !ty.is_valid_for(prim) {
            return Err(CodecError::SchemaError(format!(
                "value type {ty:?} is not valid for column type {prim:?}"
            )));
        }

        let value = match ty {
            ValueType::Leb => {
                let mut reader = bytes;
                let i = leb128::read::signed(&mut reader)
                    .map_err(|e| CodecError::DataFormatError(e.to_string()))?;
                if !reader.is_empty() {
                    return Err(CodecError::DataFormatError(
                        "trailing bytes in leb value".into(),
                    ));
                }
                TxnCellValue::Int(i)
            }
            ValueType::String => {
                let s = std::str::from_utf8(bytes).map_err(|_| {
                    CodecError::DataFormatError("value column: invalid utf-8".into())
                })?;
                TxnCellValue::Str(s.to_owned())
            }
            other => {
                return Err(CodecError::DataFormatError(format!(
                    "unsupported value type code {other:?}"
                )));
            }
        };

        values.push(value);
        offset = end;
    }
    if offset != value_blob.len() {
        return Err(CodecError::DataFormatError(format!(
            "trailing bytes in value column: {} bytes",
            value_blob.len() - offset
        )));
    }

    Ok(values)
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

        assert!(matches!(err, CodecError::SchemaError(_)));
    }

    #[test]
    fn txn_row_ref_column_rejects_unmapped_existing_hashes() {
        let value = TxnCellValue::Id(RowRef::Existing(RowId {
            commit: CommitHash([9u8; HASH_SIZE]),
            counter: 1,
        }));
        let err = encode_txn_row_ref_column(&[value], &HashMapper::new())
            .expect_err("existing hash must be in dictionary");

        assert!(matches!(err, CodecError::SchemaError(_)));
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
    fn txn_value_column_round_trips_int_boundaries() {
        let col = ColType::PrimType {
            prim: PrimType::PrimInt,
        };
        // Span every leb byte-length boundary, including the signed extremes,
        // so the declared meta length must agree with the signed-leb encoding.
        let values: Vec<TxnCellValue> = vec![
            0i64.into(),
            (-1i64).into(),
            1i64.into(),
            127i64.into(),
            128i64.into(),
            (-128i64).into(),
            i64::MIN.into(),
            i64::MAX.into(),
        ];
        let encoded =
            encode_txn_value_column(&values, &col, &HashMapper::new()).expect("encode int col");
        let decoded = decode_txn_value_column(&encoded, &col, &[]).expect("decode int col");
        assert_eq!(decoded, values);
    }

    #[test]
    fn txn_value_column_round_trips_empty_strings_and_empty_column() {
        let col = ColType::PrimType {
            prim: PrimType::PrimString,
        };

        // Zero-length values interleaved with non-empty ones: the value blob
        // is shorter than the row count, so offsets must advance by zero.
        let values: Vec<TxnCellValue> = vec!["".into(), "x".into(), "".into()];
        let encoded =
            encode_txn_value_column(&values, &col, &HashMapper::new()).expect("encode str col");
        let decoded = decode_txn_value_column(&encoded, &col, &[]).expect("decode str col");
        assert_eq!(decoded, values);

        // Zero rows: empty meta and value blobs must round-trip to an empty column.
        let empty: Vec<TxnCellValue> = vec![];
        let encoded =
            encode_txn_value_column(&empty, &col, &HashMapper::new()).expect("encode empty col");
        let decoded = decode_txn_value_column(&encoded, &col, &[]).expect("decode empty col");
        assert_eq!(decoded, empty);
    }

    #[test]
    fn txn_value_column_decode_rejects_meta_length_overrun() {
        let col = ColType::PrimType {
            prim: PrimType::PrimInt,
        };
        // The meta claims a 5-byte value, but the value blob is empty.
        let meta_blob =
            Column::<ValueMeta>::from_values(vec![ValueMeta::new(ValueType::Leb, 5)]).save();
        let value_blob: Vec<u8> = Vec::new();
        let mut data = Vec::new();
        commit_leb128::write_len_prefixed_bytes(&mut data, &meta_blob);
        commit_leb128::write_len_prefixed_bytes(&mut data, &value_blob);

        let err = decode_txn_value_column(&data, &col, &[]).expect_err("length overrun");
        assert!(matches!(err, CodecError::DataFormatError(_)));
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
        assert!(matches!(err, CodecError::SchemaError(_)));
    }

    #[test]
    fn txn_value_column_rejects_tuple_schema() {
        let col = ColType::Tuple { fields: vec![] };
        let err = encode_txn_value_column(&[], &col, &HashMapper::new()).expect_err("tuple");
        assert!(matches!(err, CodecError::SchemaError(_)));
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
        let path_len = commit_leb128::read_len(&encoded, &mut pos, "path length").unwrap();
        let path_bytes = read_slice(&encoded, &mut pos, path_len, "path").unwrap();
        assert_eq!(std::str::from_utf8(path_bytes).unwrap(), "T");
        assert_eq!(
            commit_leb128::read_len(&encoded, &mut pos, "row count").unwrap(),
            2
        );
        assert_eq!(
            commit_leb128::read_len(&encoded, &mut pos, "column count").unwrap(),
            3
        );

        for (index, col_type) in schema.columns.iter().enumerate() {
            let blob =
                commit_leb128::read_len_prefixed_bytes(&encoded, &mut pos, "column blob").unwrap();
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
        assert!(matches!(err, CodecError::SchemaError(_)));
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
        assert!(matches!(err, CodecError::SchemaError(_)));
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
        let schemas = [
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
        assert_eq!(
            commit_leb128::read_len(&encoded, &mut pos, "op count").unwrap(),
            3
        );

        let op_kind_blob =
            commit_leb128::read_len_prefixed_bytes(&encoded, &mut pos, "op kind").unwrap();
        let op_kinds: Vec<u32> = Column::<u32>::load(op_kind_blob).unwrap().iter().collect();
        assert_eq!(op_kinds, vec![OP_KIND_ADD, OP_KIND_ADD, OP_KIND_ADD]);

        let sequence_blob =
            commit_leb128::read_len_prefixed_bytes(&encoded, &mut pos, "sequence").unwrap();
        let table_sequence: Vec<u32> = Column::<u32>::load(sequence_blob).unwrap().iter().collect();
        assert_eq!(table_sequence, vec![0, 1, 0]);

        assert_eq!(
            commit_leb128::read_len(&encoded, &mut pos, "group count").unwrap(),
            2
        );

        let group_a =
            commit_leb128::read_len_prefixed_bytes(&encoded, &mut pos, "group a").unwrap();
        let mut group_pos = 0usize;
        let path_len =
            commit_leb128::read_len(group_a, &mut group_pos, "group a path length").unwrap();
        let path = read_slice(group_a, &mut group_pos, path_len, "group a path").unwrap();
        assert_eq!(std::str::from_utf8(path).unwrap(), "A");
        assert_eq!(
            commit_leb128::read_len(group_a, &mut group_pos, "group a rows").unwrap(),
            2
        );
        assert_eq!(
            commit_leb128::read_len(group_a, &mut group_pos, "group a columns").unwrap(),
            1
        );

        let group_b =
            commit_leb128::read_len_prefixed_bytes(&encoded, &mut pos, "group b").unwrap();
        let mut group_pos = 0usize;
        let path_len =
            commit_leb128::read_len(group_b, &mut group_pos, "group b path length").unwrap();
        let path = read_slice(group_b, &mut group_pos, path_len, "group b path").unwrap();
        assert_eq!(std::str::from_utf8(path).unwrap(), "B");
        assert_eq!(
            commit_leb128::read_len(group_b, &mut group_pos, "group b rows").unwrap(),
            1
        );
        assert_eq!(
            commit_leb128::read_len(group_b, &mut group_pos, "group b columns").unwrap(),
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
        let schemas = [
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
        let mut pos = 0usize;
        let decoded = decode_commit_body(&encoded, &mut pos, &[], schema_for).expect("decode body");

        assert_eq!(decoded, pending);
        assert_eq!(pos, encoded.len());
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

        let mut pos = 0usize;
        let err =
            decode_commit_body(&encoded, &mut pos, &[], |_| None).expect_err("missing schema");
        assert!(matches!(err, CodecError::SchemaError(_)));
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
        assert!(matches!(err, CodecError::SchemaError(_)));
    }

    #[test]
    fn serialize_round_trips_trailing_extra_bytes() {
        let mut data = CommitData::new(vec![], Author::foo(), 7, Some("hi".to_owned()), vec![]);
        data.extra_bytes = vec![0xde, 0xad, 0xbe, 0xef];

        let bytes = serialize(&data, &HashMapper::new(), |_| None).expect("serialize");
        let decoded = deserialize(&bytes, |_| None).expect("deserialize");

        assert_eq!(decoded, data);
        assert_eq!(decoded.extra_bytes, vec![0xde, 0xad, 0xbe, 0xef]);

        // Re-serializing reproduces identical bytes, so the commit hash is stable
        // even though the current version does not interpret the extra bytes.
        let reencoded = serialize(&decoded, &HashMapper::new(), |_| None).expect("reserialize");
        assert_eq!(reencoded, bytes);
    }

    #[test]
    fn serialize_defaults_to_empty_extra_bytes() {
        let data = CommitData::new(vec![], Author::foo(), 0, None, vec![]);
        let bytes = serialize(&data, &HashMapper::new(), |_| None).expect("serialize");
        let decoded = deserialize(&bytes, |_| None).expect("deserialize");
        assert!(decoded.extra_bytes.is_empty());
    }
}
