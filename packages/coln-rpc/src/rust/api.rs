use coln_flir_rs::ir::{self, Path};
use serde::{Deserialize, Serialize};
use specta::Type;
// use coln_store::id_packer::IdPacker;
use coln_store::{table::{WireRowId, WireValue}};

#[derive(Type, Serialize, Deserialize)]
pub struct WhereClause {
    table_name: Path,
    row_id: Option<WireRowId>,
    values: Vec<WireValue> // A prefix of column values
}
