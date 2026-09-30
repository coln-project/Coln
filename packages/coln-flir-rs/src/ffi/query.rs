use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{PublicTuple, ffi::PublicRowId, ir::Path};

#[derive(Debug, Type, Serialize, Deserialize)]
pub struct WhereClause {
    pub table_name: Path,
    pub row_id: Option<PublicRowId>,
    pub values: PublicTuple, // A prefix of column values
}
