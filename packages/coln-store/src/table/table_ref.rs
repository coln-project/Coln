use crate::commit::hash_dict::HashMapper;
use crate::ir;
use crate::ir::Schema;
use crate::table::{CellValue, RowId, RowView, Table, ValidationError};

/// A [`Table`] together with the store-wide hash dictionary, for read-only
/// access. This is what [`Store`](crate::store::Store) accessors hand out, so
/// callers can read rows without threading the dictionary themselves.
#[derive(Debug, Clone, Copy)]
pub struct TableRef<'a> {
    table: &'a Table,
    dict: &'a HashMapper,
}

impl<'a> TableRef<'a> {
    pub(crate) fn new(table: &'a Table, dict: &'a HashMapper) -> Self {
        Self { table, dict }
    }

    pub fn path(self) -> &'a ir::Path {
        self.table.path()
    }

    pub fn schema(self) -> &'a Schema {
        self.table.schema()
    }

    pub fn row_count(self) -> usize {
        self.table.row_count()
    }

    pub fn row_id_at(self, row_idx: usize) -> Option<RowId> {
        self.table.row_id_at(row_idx, self.dict)
    }

    pub fn cell_at(self, row_idx: usize, col_idx: usize) -> Option<CellValue> {
        self.table.cell_at(row_idx, col_idx, self.dict)
    }

    pub(crate) fn row_at(self, row_idx: usize) -> Option<RowView> {
        self.table.row_at(row_idx, self.dict)
    }

    pub fn row_position(self, row_id: RowId) -> Option<usize> {
        self.table.row_position(row_id, self.dict)
    }

    pub fn dump(self) -> String {
        self.table.dump(self.dict)
    }

    pub fn validate_column_count(self, got: usize) -> Result<(), ValidationError> {
        self.table.validate_column_count(got)
    }

    pub fn validate_insert(self, values: &[CellValue]) -> Result<(), ValidationError> {
        self.table.validate_insert(values, self.dict)
    }

    pub fn primary_key_values(self, values: &[CellValue]) -> Option<Vec<CellValue>> {
        self.table.primary_key_values(values)
    }
}
