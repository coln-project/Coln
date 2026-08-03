use crate::id_packer::IdPacker;
use crate::ir;
use crate::ir::Schema;
use crate::table::index::{IndexId, IndexMeta};
use crate::table::{CellValue, RowId, RowView, SeekKey, Table, TableOid, ValidationError};

/// A [`Table`] together with the store-wide hash dictionary, for read-only
/// access. This is what [`Store`](crate::store::Store) accessors hand out, so
/// callers can read rows without threading the dictionary themselves.
#[derive(Debug, Clone, Copy)]
pub struct TableRef<'a> {
    table: &'a Table,
    id_packer: &'a IdPacker,
}

impl<'a> TableRef<'a> {
    pub(crate) fn new(table: &'a Table, id_packer: &'a IdPacker) -> Self {
        Self { table, id_packer }
    }

    pub fn path(self) -> &'a ir::Path {
        self.table.path()
    }

    pub fn oid(self) -> TableOid {
        self.table.oid()
    }

    pub fn schema(self) -> &'a Schema {
        self.table.schema()
    }

    pub fn row_count(self) -> usize {
        self.table.row_count()
    }

    pub fn row_id_at(self, row_idx: usize) -> Option<RowId> {
        self.table.row_id_at(row_idx, self.id_packer)
    }

    pub fn cell_at(self, row_idx: usize, col_idx: usize) -> Option<CellValue> {
        self.table.cell_at(row_idx, col_idx, self.id_packer)
    }

    pub(crate) fn row_at(self, row_idx: usize) -> Option<RowView> {
        self.table.row_at(row_idx, self.id_packer)
    }

    pub fn row_position(self, row_id: RowId) -> Option<usize> {
        let row_id = self.id_packer.lookup_row_id(row_id)?;
        self.table.row_idx(row_id)
    }

    pub fn table_scan(self) -> impl Iterator<Item = RowView> + 'a {
        self.table.table_scan(self.id_packer)
    }

    pub fn indexes_meta(self) -> Vec<IndexMeta<'a>> {
        self.table.indexes_meta()
    }

    pub fn seek(self, key: &[SeekKey]) -> Result<impl Iterator<Item = RowId>, ValidationError> {
        self.table.seek(key, self.id_packer)
    }

    pub fn index_seek(
        self,
        index: IndexId,
        key: &[CellValue],
    ) -> Result<impl Iterator<Item = RowId>, ValidationError> {
        self.table.index_seek(index, key, self.id_packer)
    }

    pub fn lookup(self, key: &[SeekKey]) -> Result<bool, ValidationError> {
        self.table.lookup(key, self.id_packer)
    }

    pub fn index_lookup(self, index: IndexId, key: &[CellValue]) -> Result<bool, ValidationError> {
        self.table.index_lookup(index, key, self.id_packer)
    }

    pub fn dump(self) -> String {
        self.table.dump(self.id_packer)
    }

    pub fn validate_column_count(self, got: usize) -> Result<(), ValidationError> {
        self.table.validate_column_count(got)
    }

    pub fn validate_insert(self, values: &[CellValue]) -> Result<(), ValidationError> {
        self.table.validate_insert(values, self.id_packer)
    }

    pub fn primary_key_values(self, values: &[CellValue]) -> Option<Vec<CellValue>> {
        self.table.primary_key_values(values)
    }
}
