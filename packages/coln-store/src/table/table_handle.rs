// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::id_packer::IdPacker;
use crate::ir;
use crate::ir::Schema;
use crate::rowing::Rowing;
use crate::table::index::{IndexId, IndexMeta};
use crate::table::{RowView, Table, TableOid, ValidationError, WireRowId, WireValue};
use crate::txn::TxnLiveRowId;

/// A [`Table`] together with the store-wide hash dictionary and canonicaliser,
/// for read-only access. This is what [`Store`](crate::store::Store) accessors hand out, so
/// callers can read rows without threading the dictionary themselves.
#[derive(Debug, Clone, Copy)]
pub struct TableHandle<'a> {
    inner: &'a Table,
    id_packer: &'a IdPacker,
    canonicaliser: &'a Rowing,
}

impl<'a> TableHandle<'a> {
    pub(crate) fn new(
        inner: &'a Table,
        id_packer: &'a IdPacker,
        canonicaliser: &'a Rowing,
    ) -> Self {
        Self {
            inner,
            id_packer,
            canonicaliser,
        }
    }

    pub fn path(self) -> &'a ir::Path {
        self.inner.path()
    }

    pub fn oid(self) -> TableOid {
        self.inner.oid()
    }

    pub fn schema(self) -> &'a Schema {
        self.inner.schema()
    }

    pub fn row_count(self) -> usize {
        self.inner.row_count()
    }

    pub fn col_count(self) -> usize {
        self.inner.cols.len() + 1
    }

    pub fn row_by_handle(&self, row_handle: TxnLiveRowId) -> Option<RowView> {
        let row_id = row_handle.row_id().ok()?;
        let packed_row_id = self.id_packer.lookup_row_id(row_id)?;
        let con_rowid = self
            .canonicaliser
            .canonical_id(&packed_row_id, self.id_packer);
        let unpacked_con = self.id_packer.unpack_row_id(con_rowid);
        // replace the rowid in the row_handle so it stays canonical
        if packed_row_id != con_rowid {
            row_handle.canonicalise_to(unpacked_con).ok()?
        }
        self.row_by_id(unpacked_con)
    }

    // This function will canonicalise the row_id on read, but will not change it
    // See `row_by_handle` which will actually canonicalise the handle.
    // We need both because the TS FFI does not deal with handles.
    pub fn row_by_id(&self, row_id: WireRowId) -> Option<RowView> {
        let packed_rowid = self.id_packer.lookup_row_id(row_id)?;
        let row_id = self
            .canonicaliser
            .canonical_id(&packed_rowid, self.id_packer);

        self.inner
            .row_at(self.inner.packed_rowid_idx(row_id)?, self.id_packer)
    }

    pub fn scan(self) -> impl Iterator<Item = RowView> + 'a {
        self.inner.scan(self.id_packer)
    }

    pub fn indexes_meta(self) -> Vec<IndexMeta<'a>> {
        self.inner.indexes_meta()
    }

    pub fn index_seek(
        self,
        index: IndexId,
        key: &[WireValue],
    ) -> Result<impl Iterator<Item = WireRowId>, ValidationError> {
        self.inner.index_seek(index, key, self.id_packer)
    }

    pub fn primary_index(&self) -> Option<IndexId> {
        self.inner.primary_index()
    }
}

impl<'a> TableHandle<'a> {
    // For internal convenience

    // For other packages that want to use PackedRowId directly
    // Not for user consumption
    #[doc(hidden)]
    pub fn inner(self) -> &'a Table {
        self.inner
    }

    pub(crate) fn row_id_at(self, row_idx: usize) -> Option<WireRowId> {
        self.inner.row_id_at(row_idx, self.id_packer)
    }

    #[cfg(feature = "native")]
    pub(crate) fn dump(self) -> String {
        self.inner.dump(self.id_packer)
    }

    pub(crate) fn cell_at(self, row_idx: usize, col_idx: usize) -> Option<WireValue> {
        self.inner
            .cell_at(row_idx, col_idx)
            .map(|value| self.id_packer.unpack_cell(value))
    }
}

#[cfg(test)]
mod test {
    use coln_flir_rs::ir::Path;
    use rstest::rstest;

    use crate::{
        commit::hash::CommitHash,
        store::Store,
        table::{RowView, WireRowId},
        test_utils::commit_int_store,
    };

    #[rstest]
    fn row_by_id_finds_committed_row(commit_int_store: (Store, CommitHash)) {
        let (store, commit) = commit_int_store;
        let path = Path::from("T");
        let row_id = WireRowId { commit, counter: 0 };
        let table = store.table_at(&path).expect("table exists");

        assert_eq!(
            table.row_by_id(row_id),
            Some(RowView {
                row_id,
                values: vec![42i32.into()],
            })
        );
        assert_eq!(table.row_by_id(WireRowId { commit, counter: 1 }), None);
    }
}
