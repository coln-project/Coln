// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::ir;
use crate::ir::Schema;
#[cfg(test)]
use crate::op::Op;
use crate::pack::IdPacker;
use crate::rowing::Rowing;
#[cfg(test)]
use crate::table::PackedOp;
use crate::table::index::{IndexId, IndexMeta};
use crate::table::{
    PackedRowView, PackedValue, Table, TableOid, ValidationError, WireRowId, WireValue,
};
use crate::txn::TxnLiveRowId;

/// Public facing row value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireRowView {
    pub row_id: WireRowId,
    pub values: Vec<WireValue>,
}

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

    pub fn row_by_handle(&self, row_handle: TxnLiveRowId) -> Option<WireRowView> {
        let row_id = row_handle.row_id().ok()?;
        let packed_row_id = self.id_packer.lookup_row_id(&row_id)?;
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
    pub fn row_by_id(&self, row_id: WireRowId) -> Option<WireRowView> {
        let packed_row_id = self.id_packer.lookup_row_id(&row_id)?;
        let packed_row_id = self
            .canonicaliser
            .canonical_id(&packed_row_id, self.id_packer);

        self.inner
            .row_at(self.inner.packed_rowid_idx(packed_row_id)?)
            .map(|packed_view| self.unpack_row_view(packed_view))
    }

    pub fn scan(self) -> impl Iterator<Item = WireRowView> + 'a {
        self.inner
            .scan()
            .map(move |packed_view| self.unpack_row_view(packed_view))
    }

    fn unpack_row_view(&self, packed_view: PackedRowView) -> WireRowView {
        WireRowView {
            row_id: self.id_packer.unpack_row_id(packed_view.row_id),
            values: packed_view
                .values
                .into_iter()
                .map(|packed_value| packed_value.map_owned(|id| self.id_packer.unpack_row_id(id)))
                .collect(),
        }
    }

    pub fn indexes_meta(self) -> Vec<IndexMeta<'a>> {
        self.inner.indexes_meta()
    }

    pub fn index_seek(
        self,
        index: IndexId,
        key: &[WireValue],
    ) -> Result<impl Iterator<Item = WireRowId>, ValidationError> {
        let packed_key = key
            .iter()
            .map(|wire_val: &WireValue| match wire_val {
                WireValue::Id(wire_id) => {
                    let packed = self
                        .id_packer
                        .lookup_row_id(wire_id)
                        .ok_or(ValidationError::InvalidRowId { wire_id: *wire_id })?;
                    Ok(PackedValue::Id(packed))
                }
                WireValue::Int(i) => Ok(PackedValue::Int(*i)),
                WireValue::Str(s) => Ok(PackedValue::Str(s.clone())),
            })
            .collect::<Result<Vec<PackedValue>, _>>()?;
        Ok(self
            .inner
            .index_seek(index, &packed_key)?
            .map(|packed_id| self.id_packer.unpack_row_id(packed_id)))
    }

    pub fn primary_index(&self) -> Option<IndexId> {
        self.inner.primary_index()
    }

    // For internal convenience

    // For other packages that want to use PackedRowId directly
    // Not for user consumption
    #[doc(hidden)]
    pub fn inner(self) -> &'a Table {
        self.inner
    }

    pub(crate) fn row_id_at(self, row_idx: usize) -> Option<WireRowId> {
        let packed = self.inner.row_id_at(row_idx)?;
        Some(self.id_packer.unpack_row_id(packed))
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
pub(crate) struct TableMut<'a> {
    inner: &'a mut Table,
    id_packer: &'a mut IdPacker,
    rowing: &'a mut Rowing,
}

#[cfg(test)]
impl<'a> TableMut<'a> {
    pub(crate) fn new(
        inner: &'a mut Table,
        id_packer: &'a mut IdPacker,
        rowing: &'a mut Rowing,
    ) -> Self {
        Self {
            inner,
            id_packer,
            rowing,
        }
    }

    pub(crate) fn stage(&mut self, op: Op) {
        debug_assert_eq!(op.table(), self.inner.oid());
        let op = self.id_packer.pack_op(op);
        self.inner.stage_update(op);
    }

    pub(crate) fn stage_delete(&mut self, row_id: WireRowId) {
        let row_id = self.id_packer.pack_row_id(row_id);
        self.inner.stage_update(PackedOp::Delete { row_id });
    }

    pub(crate) fn apply_staged(&mut self) -> Result<(), ValidationError> {
        self.inner.apply_staged_ops(self.rowing)
    }

    pub(crate) fn insert_row(
        &mut self,
        values: Vec<WireValue>,
        row_id: WireRowId,
    ) -> Result<(), ValidationError> {
        let row_id = self.id_packer.pack_row_id(row_id);
        let values = values
            .into_iter()
            .map(|value| self.id_packer.pack_cell(value))
            .collect();
        self.inner.insert_row(values, row_id, self.rowing)
    }

    pub(crate) fn rebuild(&mut self) {
        self.inner.rebuild(self.rowing, self.id_packer);
    }
}

#[cfg(test)]
mod test {
    use coln_flir_rs::ir::Path;
    use rstest::rstest;

    use super::*;
    use crate::{
        commit::hash::CommitHash, store::Store, table::WireRowId, test_utils::commit_int_store,
    };

    #[rstest]
    fn row_by_id_finds_committed_row(commit_int_store: (Store, CommitHash)) {
        let (store, commit) = commit_int_store;
        let path = Path::from("T");
        let row_id = WireRowId { commit, counter: 0 };
        let table = store.table_at(&path).expect("table exists");

        assert_eq!(
            table.row_by_id(row_id),
            Some(WireRowView {
                row_id,
                values: vec![42i32.into()],
            })
        );
        assert_eq!(table.row_by_id(WireRowId { commit, counter: 1 }), None);
    }
}
