use geolog_lang::ir::{ColType, PrimType, Schema};
use hexane::v1::Column;
use serde::{Deserialize, Serialize};
use std::io::Write;

use crate::commit::error::PersistError;
use crate::commit::hash::CommitHash;
use crate::commit::hash_dict::{HashMapper, decode_row_id_column, encode_rowid_column};
use crate::commit::utils::*;
use crate::table::{CellValue, RowId, Table, TableOid};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TableEntry {
    pub(crate) path: String,
    pub(crate) oid: TableOid,
    pub(crate) schema: Schema,
}

pub(crate) fn write_len_prefixed_data(buf: &mut Vec<u8>, data: &[u8]) -> Result<(), PersistError> {
    let len: u64 = data
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("blob too large".into()))?;
    buf.write_all(&len.to_le_bytes())?;
    buf.write_all(data)?;
    Ok(())
}

// ── Table-level raw encode/decode ────────────────────

/// Encodes a single table's column data. Layout:
///
/// `[column_count:u32][repeat column_count × ([len:u64][column_bytes])]`
///
/// Column order: row-id column, then each schema column. No magic or version.
pub(crate) fn encode_table_raw(
    table: &Table,
    hash_mapper: &HashMapper,
) -> Result<Vec<u8>, PersistError> {
    let column_blobs = encode_columns(table, hash_mapper)?;
    let mut buf = Vec::new();

    let n: u32 = column_blobs
        .len()
        .try_into()
        .map_err(|_| PersistError::Other("too many columns".into()))?;
    buf.write_all(&n.to_le_bytes())?;

    for blob in &column_blobs {
        write_len_prefixed_data(&mut buf, blob)?;
    }

    Ok(buf)
}

/// Decodes a raw table payload (produced by [`encode_table_raw`]).
/// Returns (row_ids, data_columns).
pub(crate) fn decode_table_raw(
    data: &[u8],
    schema: &Schema,
    hashes: &[CommitHash],
) -> Result<(Vec<RowId>, Vec<Vec<CellValue>>), PersistError> {
    let mut pos = 0usize;
    let n = read_u32(data, &mut pos, "column count")? as usize;

    let expected = schema.columns.len() + 1;
    if n != expected {
        return Err(PersistError::DataFormatError(format!(
            "column count mismatch: file has {n}, schema expects {expected}"
        )));
    }

    let rowid_len = read_u64(data, &mut pos, "row-id column length")? as usize;
    let rowid_blob = read_slice(data, &mut pos, rowid_len, "row-id column")?;
    let row_ids = decode_row_id_column(rowid_blob, hashes)?;

    let mut cols: Vec<Vec<CellValue>> = Vec::with_capacity(schema.columns.len());
    for (i, col_type) in schema.columns.iter().enumerate() {
        let len = read_u64(data, &mut pos, "column blob length")? as usize;
        let blob = read_slice(data, &mut pos, len, "column blob")?;
        match col_type {
            ColType::EntityType { .. } => {
                cols.push(
                    decode_row_id_column(blob, hashes)?
                        .into_iter()
                        .map(CellValue::Id)
                        .collect(),
                );
            }
            ColType::PrimType { prim } => match prim {
                PrimType::PrimInt => {
                    cols.push(
                        Column::<i64>::load(blob)?
                            .iter()
                            .map(CellValue::Int)
                            .collect(),
                    );
                }
                PrimType::PrimString => {
                    cols.push(
                        Column::<String>::load(blob)?
                            .iter()
                            .map(|s| CellValue::Str(s.to_owned()))
                            .collect(),
                    );
                }
            },
            ColType::Tuple { .. } => {
                return Err(PersistError::SchemaError(format!(
                    "column {i}: tuple columns not supported yet"
                )));
            }
        }
    }

    if pos != data.len() {
        return Err(PersistError::DataFormatError(format!(
            "trailing bytes after columns: {} bytes",
            data.len() - pos
        )));
    }

    Ok((row_ids, cols))
}

// ── Column encoding ──────────────────────────────────

pub(crate) fn collect_table_hashes(
    table: &Table,
    hash_mapper: &mut HashMapper,
) -> Result<(), PersistError> {
    for row_id in &table.row_ids {
        hash_mapper.insert(row_id.commit);
    }
    for (i, col) in table.cols.iter().enumerate() {
        if matches!(&table.schema().columns[i], ColType::EntityType { .. }) {
            for cell in col {
                match cell {
                    CellValue::Id(id) => {
                        hash_mapper.insert(id.commit);
                    }
                    _ => {
                        return Err(PersistError::SchemaError(format!(
                            "column {i}: expected entity id, got {:?}",
                            cell
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Encode table columns, using hexane's columnar encoding
fn encode_columns(table: &Table, hash_mapper: &HashMapper) -> Result<Vec<Vec<u8>>, PersistError> {
    let mut cols = Vec::new();
    cols.push(encode_rowid_column(&table.row_ids, hash_mapper)?);

    for (i, col) in table.cols.iter().enumerate() {
        match &table.schema().columns[i] {
            ColType::EntityType { .. } => {
                let ids: Vec<RowId> = col
                    .iter()
                    .map(|cell| match cell {
                        CellValue::Id(id) => Ok(*id),
                        _ => Err(PersistError::SchemaError(format!(
                            "column {i}: expected entity id, got {:?}",
                            cell
                        ))),
                    })
                    .collect::<Result<_, PersistError>>()?;
                cols.push(encode_rowid_column(&ids, hash_mapper)?);
            }
            ColType::PrimType { prim } => match prim {
                PrimType::PrimInt => {
                    let ints: Vec<i64> = col
                        .iter()
                        .map(|cell| match cell {
                            CellValue::Int(v) => Ok(*v),
                            _ => Err(PersistError::SchemaError(format!(
                                "column {i}: expected int, got {:?}",
                                cell
                            ))),
                        })
                        .collect::<Result<_, PersistError>>()?;
                    cols.push(Column::<i64>::from_values(ints).save());
                }
                PrimType::PrimString => {
                    let strings: Vec<String> = col
                        .iter()
                        .map(|cell| match cell {
                            CellValue::Str(s) => Ok(s.clone()),
                            _ => Err(PersistError::SchemaError(format!(
                                "column {i}: expected string, got {:?}",
                                cell
                            ))),
                        })
                        .collect::<Result<_, PersistError>>()?;
                    cols.push(Column::<String>::from_values(strings).save());
                }
            },
            ColType::Tuple { .. } => unimplemented!("tuple column type not here yet"),
        }
    }

    Ok(cols)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commit::hash::{CommitHash, HASH_SIZE},
        ir::{Path, Schema},
    };

    fn test_row_id(commit: u8, counter: u32) -> RowId {
        RowId {
            commit: CommitHash([commit; HASH_SIZE]),
            counter,
        }
    }

    fn int_schema() -> Schema {
        Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: None,
        }
    }

    fn mixed_schema() -> Schema {
        Schema {
            columns: vec![
                ColType::PrimType {
                    prim: PrimType::PrimInt,
                },
                ColType::PrimType {
                    prim: PrimType::PrimString,
                },
                ColType::EntityType {
                    path: Path::from("G.E"),
                },
            ],
            primary_key: None,
        }
    }

    fn empty_schema() -> Schema {
        Schema {
            columns: vec![],
            primary_key: None,
        }
    }

    fn encode_decode_table(
        table: &Table,
        schema: &Schema,
    ) -> Result<(Vec<RowId>, Vec<Vec<CellValue>>), PersistError> {
        let mut hash_mapper = HashMapper::new();
        collect_table_hashes(table, &mut hash_mapper)?;
        let bytes = encode_table_raw(table, &hash_mapper)?;
        decode_table_raw(&bytes, schema, hash_mapper.hashes())
    }

    #[test]
    fn raw_round_trip_empty_table() {
        let schema = empty_schema();
        let tbl = Table::new(Path::from("empty"), schema.clone());
        let (row_ids, cols) = encode_decode_table(&tbl, &schema).unwrap();

        assert!(row_ids.is_empty());
        assert!(cols.is_empty());
    }

    #[test]
    fn raw_round_trip_single_int_column() {
        let schema = int_schema();
        let mut tbl = Table::new(Path::from("ints"), schema.clone());
        tbl.append_row(vec![CellValue::Int(10)], test_row_id(1, 0));
        tbl.append_row(vec![CellValue::Int(20)], test_row_id(1, 1));

        let (row_ids, cols) = encode_decode_table(&tbl, &schema).unwrap();

        assert_eq!(row_ids, vec![test_row_id(1, 0), test_row_id(1, 1)]);
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0], vec![CellValue::Int(10), CellValue::Int(20)]);
    }

    #[test]
    fn raw_round_trip_mixed_columns() {
        let schema = mixed_schema();
        let mut tbl = Table::new(Path::from("mixed"), schema.clone());
        let entity_id = test_row_id(7, 99);
        tbl.append_row(
            vec![
                CellValue::Int(42),
                CellValue::Str("hello".into()),
                CellValue::Id(entity_id),
            ],
            test_row_id(1, 0),
        );

        let (row_ids, cols) = encode_decode_table(&tbl, &schema).unwrap();

        assert_eq!(row_ids, vec![test_row_id(1, 0)]);
        assert_eq!(cols[0], vec![CellValue::Int(42)]);
        assert_eq!(cols[1], vec![CellValue::Str("hello".into())]);
        assert_eq!(cols[2], vec![CellValue::Id(entity_id)]);
    }
}
