use crate::{
    ir,
    table::{CellValue, RowId},
};

pub enum Op {
    Add {
        row_id: RowId,
        table: ir::Path, // using path so it's stable across replicas
        values: Vec<CellValue>,
    },
}

impl Op {
    pub fn id(&self) -> RowId {
        match self {
            Op::Add { row_id, .. } => *row_id,
        }
    }
}
