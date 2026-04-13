use std::io::{self, Write};

use geolog_lang::ir::{ColType, PrimType};
use hexane::v1::Column;
use serde::{Deserialize, Serialize};

use crate::table::{CellValue, RowId, Table};

#[derive(Debug)]
pub enum PersisError {
    HeaderError(postcard::Error),
    IOError(io::Error),
    SchemaError(String),
    DataFormatError(String),
    Other(String),
}

impl From<postcard::Error> for PersisError {
    fn from(value: postcard::Error) -> Self {
        Self::HeaderError(value)
    }
}

impl From<io::Error> for PersisError {
    fn from(value: io::Error) -> Self {
        Self::IOError(value)
    }
}

const MAGIC: &[u8; 4] = b"GMtb";

#[derive(Debug, Serialize, Deserialize)]
pub struct TblHeader {
    pub format_version: u32,
    pub next_rowid: u64,
    pub path: String,
}

fn encode_header(h: &TblHeader) -> Result<Vec<u8>, PersisError> {
    Ok(postcard::to_allocvec(h)?)
}

/// One [`Column::save`] blob per logical column: row ids first, then schema order.
///
/// hexane decodes **one** column from its slice. Concatenating
/// blobs with no length framing is invalid: the RLE decoder would treat the next column’s
/// bytes as part of the first column. On load, read `column_count` length-prefixed chunks
fn encode_columns(table: &Table) -> Result<Vec<Vec<u8>>, PersisError> {
    let mut cols = Vec::new();
    let rowid_col = Column::<RowId>::from_values(table.row_ids.clone());
    cols.push(rowid_col.save());

    for (i, col) in table.cols.iter().enumerate() {
        match &table.schema().columns[i] {
            ColType::EntityType { .. } => {
                let ids: Vec<u64> = col
                    .iter()
                    .map(|cell| match cell {
                        CellValue::Id(id) => Ok(*id),
                        _ => Err(PersisError::SchemaError(format!(
                            "column {i}: expected entity id, got {:?}",
                            cell
                        ))),
                    })
                    .collect::<Result<_, PersisError>>()?;
                let id_col = Column::<RowId>::from_values(ids);
                cols.push(id_col.save());
            }
            ColType::PrimType { prim } => match prim {
                PrimType::PrimInt => {
                    let ints: Vec<i64> = col
                        .iter()
                        .map(|cell| match cell {
                            CellValue::Int(v) => Ok(*v),
                            _ => Err(PersisError::SchemaError(format!(
                                "column {i}: expected int, got {:?}",
                                cell
                            ))),
                        })
                        .collect::<Result<_, PersisError>>()?;
                    let int_col = Column::<i64>::from_values(ints);
                    cols.push(int_col.save());
                }
                PrimType::PrimString => {
                    let strings: Vec<String> = col
                        .iter()
                        .map(|cell| match cell {
                            CellValue::Str(s) => Ok(s.clone()),
                            _ => Err(PersisError::SchemaError(format!(
                                "column {i}: expected string, got {:?}",
                                cell
                            ))),
                        })
                        .collect::<Result<_, PersisError>>()?;
                    let str_col = Column::<String>::from_values(strings);
                    cols.push(str_col.save());
                }
            },
            ColType::Tuple { .. } => unimplemented!("tuple column type not here yet"),
        }
    }

    Ok(cols)
}

fn encode_table_bytes(header: &[u8], column_blobs: &[Vec<u8>]) -> Result<Vec<u8>, PersisError> {
    let mut buf = Vec::new();
    buf.write_all(MAGIC)?;

    let h: u32 = header
        .len()
        .try_into()
        .map_err(|_| PersisError::Other("Header too big".into()))?;
    buf.write_all(&h.to_le_bytes())?;
    buf.write_all(header)?;

    let n: u32 = column_blobs
        .len()
        .try_into()
        .map_err(|_| PersisError::Other("too many columns".into()))?;
    buf.write_all(&n.to_le_bytes())?;

    for blob in column_blobs {
        let len: u64 = blob
            .len()
            .try_into()
            .map_err(|_| PersisError::Other("column blob too large".into()))?;
        buf.write_all(&len.to_le_bytes())?;
        buf.write_all(blob)?;
    }

    Ok(buf)
}

/// Encodes a table to bytes. Layout (little-endian):
///
/// `[MAGIC:4][header_len:u32][TblHeader postcard][column_count:u32][repeat column_count × ([len:u64][bytes])]`
///
/// Column order is: **row-id column**, then each schema column in order. Each `[bytes]` is
/// exactly what [`Column::save`] produced for that column; use [`Column::load`] on that slice
/// alone (hexane v1 does not split a concatenated payload without per-column lengths).
pub fn encode_table(table: &Table) -> Result<Vec<u8>, PersisError> {
    let header = TblHeader {
        format_version: 0,
        next_rowid: table.next_rowid,
        path: table.path().to_string(),
    };

    let header_bytes = encode_header(&header)?;
    let column_blobs = encode_columns(table)?;
    encode_table_bytes(&header_bytes, &column_blobs)
}

fn read_u32(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<u32, PersisError> {
    if data.len() < *pos + 4 {
        return Err(PersisError::DataFormatError(format!(
            "truncated while reading {ctx}"
        )));
    }
    let v = u32::from_le_bytes(data[*pos..*pos + 4].try_into().unwrap());
    *pos += 4;
    Ok(v)
}

fn read_u64(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<u64, PersisError> {
    if data.len() < *pos + 8 {
        return Err(PersisError::DataFormatError(format!(
            "truncated while reading {ctx}"
        )));
    }
    let v = u64::from_le_bytes(data[*pos..*pos + 8].try_into().unwrap());
    *pos += 8;
    Ok(v)
}

/// read the slice and advance `pos`.
fn read_slice<'a>(
    data: &'a [u8],
    pos: &mut usize,
    len: usize,
    ctx: &'static str,
) -> Result<&'a [u8], PersisError> {
    if data.len() < *pos + len {
        return Err(PersisError::DataFormatError(format!(
            "truncated while reading {ctx}"
        )));
    }
    let slice = &data[*pos..*pos + len];
    *pos += len;
    Ok(slice)
}

fn decode_header(data: &[u8]) -> Result<TblHeader, PersisError> {
    postcard::from_bytes(data).map_err(PersisError::from)
}

fn decode_columns(data: &[u8]) -> Result<Vec<Vec<u8>>, PersisError> {
    if data.len() < 4 {
        return Err(PersisError::DataFormatError(
            "truncated: missing column count".into(),
        ));
    }
    let mut pos = 0usize;
    let n = read_u32(data, &mut pos, "column count")? as usize;

    let mut cols = Vec::with_capacity(n);
    for _ in 0..n {
        let len_u64 = read_u64(data, &mut pos, "column blob length")?;
        let len = usize::try_from(len_u64)
            .map_err(|_| PersisError::Other("column blob length does not fit in usize".into()))?;
        let blob = read_slice(data, &mut pos, len, "column blob")?;
        cols.push(blob.to_vec());
    }

    if pos != data.len() {
        return Err(PersisError::DataFormatError(format!(
            "trailing bytes after columns: {} bytes",
            data.len() - pos
        )));
    }

    Ok(cols)
}

/// Decodes bytes produced by [`encode_table`]. Returns the postcard header and each column blob
/// (row-id column first, then schema order) for use with [`Column::load`].
pub fn decode_bytes(data: &[u8]) -> Result<(TblHeader, Vec<Vec<u8>>), PersisError> {
    if data.len() < MAGIC.len() {
        return Err(PersisError::Other("truncated: missing magic".into()));
    }
    if data[..MAGIC.len()] != *MAGIC {
        return Err(PersisError::Other("bad magic".into()));
    }

    let mut pos = MAGIC.len();
    let header_len_u32: u32 = read_u32(data, &mut pos, "header length")?;
    let header_len = header_len_u32 as usize;

    let header_slice = read_slice(data, &mut pos, header_len, "header")?;
    let header = decode_header(header_slice)?;

    let columns = decode_columns(&data[pos..])?;
    Ok((header, columns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Path, Schema};
    use hexane::v1::Column;

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

    #[test]
    fn round_trip_empty_table() {
        let tbl = Table::new(Path::from("empty"), empty_schema());
        let bytes = encode_table(&tbl).unwrap();
        let (header, cols) = decode_bytes(&bytes).unwrap();

        assert_eq!(header.format_version, 0);
        assert_eq!(header.next_rowid, 0);
        assert_eq!(header.path, "empty");
        // row-id column only, no data columns
        assert_eq!(cols.len(), 1);
        let rowid_col = Column::<u64>::load(&cols[0]).unwrap();
        assert_eq!(rowid_col.len(), 0);
    }

    #[test]
    fn round_trip_single_int_column() {
        let mut tbl = Table::new(Path::from("ints"), int_schema());
        tbl.append_row_validated(vec![CellValue::Int(10)]).unwrap();
        tbl.append_row_validated(vec![CellValue::Int(20)]).unwrap();
        tbl.append_row_validated(vec![CellValue::Int(30)]).unwrap();

        let bytes = encode_table(&tbl).unwrap();
        let (header, cols) = decode_bytes(&bytes).unwrap();

        assert_eq!(header.next_rowid, 3);
        assert_eq!(header.path, "ints");
        // 1 row-id column + 1 data column
        assert_eq!(cols.len(), 2);

        let rowid_col = Column::<u64>::load(&cols[0]).unwrap();
        assert_eq!(rowid_col.to_vec(), vec![0u64, 1, 2]);

        let int_col = Column::<i64>::load(&cols[1]).unwrap();
        assert_eq!(int_col.to_vec(), vec![10i64, 20, 30]);
    }

    #[test]
    fn round_trip_mixed_columns() {
        let mut tbl = Table::new(Path::from("mixed"), mixed_schema());
        tbl.append_row_validated(vec![
            CellValue::Int(42),
            CellValue::Str("hello".into()),
            CellValue::Id(0),
        ])
        .unwrap();
        tbl.append_row_validated(vec![
            CellValue::Int(-1),
            CellValue::Str("world".into()),
            CellValue::Id(1),
        ])
        .unwrap();

        let bytes = encode_table(&tbl).unwrap();
        let (header, cols) = decode_bytes(&bytes).unwrap();

        assert_eq!(header.next_rowid, 2);
        // row-id + 3 data columns
        assert_eq!(cols.len(), 4);

        let rowid_col = Column::<u64>::load(&cols[0]).unwrap();
        assert_eq!(rowid_col.to_vec(), vec![0u64, 1]);

        let int_col = Column::<i64>::load(&cols[1]).unwrap();
        assert_eq!(int_col.to_vec(), vec![42i64, -1]);

        let str_col = Column::<String>::load(&cols[2]).unwrap();
        assert_eq!(str_col.to_vec(), vec!["hello", "world"]);

        let entity_col = Column::<u64>::load(&cols[3]).unwrap();
        assert_eq!(entity_col.to_vec(), vec![0u64, 1]);
    }

    #[test]
    fn decode_rejects_bad_magic() {
        let result = decode_bytes(b"XXXX____");
        assert!(matches!(result, Err(PersisError::Other(_))));
    }

    #[test]
    fn decode_rejects_truncated_input() {
        assert!(decode_bytes(b"GM").is_err());
        assert!(decode_bytes(b"GMtb").is_err());
    }

    #[test]
    fn decode_rejects_trailing_bytes() {
        let tbl = Table::new(Path::from("t"), empty_schema());
        let mut bytes = encode_table(&tbl).unwrap();
        bytes.extend_from_slice(b"extra");
        assert!(matches!(
            decode_bytes(&bytes),
            Err(PersisError::DataFormatError(_))
        ));
    }

    #[test]
    fn header_round_trip() {
        let hdr = TblHeader {
            format_version: 1,
            next_rowid: 999,
            path: "some.path".into(),
        };
        let encoded = encode_header(&hdr).unwrap();
        let decoded = decode_header(&encoded).unwrap();
        assert_eq!(decoded.format_version, 1);
        assert_eq!(decoded.next_rowid, 999);
        assert_eq!(decoded.path, "some.path");
    }
}
