use geolog_lang::ir::LawEntry;
use serde::{Deserialize, Serialize};
use serde_json;
use std::io::Write;

use crate::persist::error::PersisError;
use crate::persist::ptbl::{
    TableEntry, decode_table_raw, encode_table_raw, write_len_prefixed_data,
};
use crate::persist::utils::*;
use crate::store::Store;
use crate::table::{Table, TableOid};

const MAGIC: &[u8; 4] = b"GMst";
const FORMAT_VERSION: u32 = 0;

// ── Store header ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct StoreHeader {
    format_version: u32,
    next_oid: TableOid,
    tables: Vec<TableEntry>,
    laws: Vec<LawEntry>,
}

// ── Store-level encode/decode ───────────────────────────────────────────────

/// Store file layout (little-endian):
///
/// `[MAGIC:4][version:u32][header_len:u32][StoreHeader postcard]`
/// `[table_count × ([table_payload_len:u64][table_payload])]`
///
/// Table payloads are raw column data.
/// Schemas, paths, next_rowid, and oid mapping live in the `StoreHeader`.
pub fn encode_store(store: &Store) -> Result<Vec<u8>, PersisError> {
    let mut table_entries = Vec::new();
    let mut table_payloads = Vec::new();

    for (&oid, table) in store.tables() {
        table_entries.push(TableEntry {
            path: table.path().to_string(),
            oid,
            next_rowid: table.next_rowid,
            schema: table.schema().clone(),
        });
        table_payloads.push(encode_table_raw(table)?);
    }

    let header = StoreHeader {
        format_version: FORMAT_VERSION,
        next_oid: store.next_oid(),
        tables: table_entries,
        laws: store.law_entries().to_vec(),
    };
    let header_bytes = serde_json::to_vec(&header)?;

    let mut buf = Vec::new();
    buf.write_all(MAGIC)?;
    buf.write_all(&FORMAT_VERSION.to_le_bytes())?;

    let h: u32 = header_bytes
        .len()
        .try_into()
        .map_err(|_| PersisError::Other("store header too big".into()))?;
    buf.write_all(&h.to_le_bytes())?;
    buf.write_all(&header_bytes)?;

    for payload in &table_payloads {
        write_len_prefixed_data(&mut buf, payload)?;
    }

    Ok(buf)
}

/// Decode a store from bytes produced by [`encode_store`].
pub fn decode_store(data: &[u8]) -> Result<Store, PersisError> {
    if data.len() < MAGIC.len() {
        return Err(PersisError::DataFormatError(
            "truncated: missing magic".into(),
        ));
    }
    if data[..MAGIC.len()] != *MAGIC {
        return Err(PersisError::DataFormatError("bad magic".into()));
    }

    let mut pos = MAGIC.len();

    let version = read_u32(data, &mut pos, "format version")?;
    if version != FORMAT_VERSION {
        return Err(PersisError::DataFormatError(format!(
            "unsupported format version: {version}"
        )));
    }

    let header_len = read_u32(data, &mut pos, "store header length")? as usize;
    let header_slice = read_slice(data, &mut pos, header_len, "store header")?;
    let header: StoreHeader = serde_json::from_slice(header_slice)?;

    let mut tables = Vec::with_capacity(header.tables.len());
    for entry in &header.tables {
        let payload_len = read_u64(data, &mut pos, "table payload length")? as usize;
        let payload = read_slice(data, &mut pos, payload_len, "table payload")?;
        let (row_ids, cols) = decode_table_raw(payload, &entry.schema)?;

        let table = Table::new_from_persist(entry, row_ids, cols);
        tables.push((entry.oid, table));
    }

    Store::from_persisted(header.next_oid, tables, header.laws)
        .map_err(|e| PersisError::Other(format!("law compile error: {e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Path, Schema};
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

    #[test]
    fn store_round_trip_empty() {
        let store = Store::new();
        let bytes = encode_store(&store).unwrap();
        let restored = decode_store(&bytes).unwrap();

        assert_eq!(restored.table_count(), 0);
    }

    #[test]
    fn store_round_trip_with_tables() {
        let mut store = Store::new();
        store.add_table(Path::from("t1"), int_schema());
        store.add_table(Path::from("t2"), mixed_schema());

        let t1_oid = store.resolve_table(&Path::from("t1")).unwrap();
        let op = store
            .table_mut(t1_oid)
            .unwrap()
            .add(vec![CellValue::Int(99)]);
        store.apply_batch(vec![op]).expect("apply batch successful");

        let bytes = encode_store(&store).unwrap();
        let restored = decode_store(&bytes).unwrap();

        assert_eq!(restored.table_count(), 2);

        let rt1_oid = restored.resolve_table(&Path::from("t1")).unwrap();
        let rt1 = restored.table(rt1_oid).unwrap();
        assert_eq!(rt1.row_count(), 1);
        assert_eq!(rt1.cell_at(0, 0), Some(&CellValue::Int(99)));

        let rt2_oid = restored.resolve_table(&Path::from("t2")).unwrap();
        let rt2 = restored.table(rt2_oid).unwrap();
        assert_eq!(rt2.row_count(), 0);
    }

    #[test]
    fn store_decode_rejects_bad_magic() {
        assert!(matches!(
            decode_store(b"XXXX________"),
            Err(PersisError::DataFormatError(_))
        ));
    }

    #[test]
    fn store_decode_rejects_truncated_input() {
        assert!(decode_store(b"GM").is_err());
        assert!(decode_store(b"GMst").is_err());
    }
}
