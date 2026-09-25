use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    ffi::{WireRowId, WireValue},
    ir::Path,
};

#[derive(Debug, Type, Serialize, Deserialize)]
pub struct WhereClause {
    pub table_name: Path,
    pub row_id: Option<WireRowId>,
    pub values: Vec<WireValue>, // A prefix of column values
}
