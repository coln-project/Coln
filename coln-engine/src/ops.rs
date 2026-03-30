use crate::{ir, table::CellValue};

pub enum Op {
    Add {
        table: ir::Path, // using path so it's stable across replicas
        values: Vec<CellValue>,
    },
}
