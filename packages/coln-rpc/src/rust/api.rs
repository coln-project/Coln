use coln_flir_rs::ir;
use serde::{Deserialize, Serialize};
use specta::Type;
// use coln_store::id_packer::IdPacker;
use coln_store::{table::{WireValue}, txn::TxnWireRowId};

#[derive(Type, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WhereClause {
    PrimaryKey { values: Vec<WireValue> },
    ExceptRowId { values: Vec<WireValue> },
    Generic { values_at: Vec<(u32, WireValue)> },
}
