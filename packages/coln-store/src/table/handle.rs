// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::engine::packed::{PackedRowId, StoreTuple};

use crate::ir;
use crate::ir::Schema;
use crate::op::PublicOp;
use crate::pack::IdPacker;
use crate::rowing::Rowing;

use crate::table::index::IndexMeta;
use crate::table::{Table, TableOid, TableOp, ValidationError};

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

    // This function will canonicalise the row_id on read, but will not change it
    // See `row_by_handle` which will actually canonicalise the handle.
    // We need both because the TS FFI does not deal with handles.
    pub fn row_by_id(&self, row_id: &PackedRowId) -> Option<StoreTuple> {
        let canonical_id = self.canonicaliser.canonical_id(&row_id, self.id_packer);

        self.inner
            .row_by_idx(self.inner.packed_rowid_idx(canonical_id)?)
    }

    pub fn scan(self) -> impl Iterator<Item = StoreTuple> + 'a {
        self.inner.scan()
    }

    pub fn index_meta(self) -> IndexMeta<'a> {
        self.inner.index_meta()
    }

    pub fn index_seek(
        self,
        key: &StoreTuple,
    ) -> Result<impl Iterator<Item = PackedRowId>, ValidationError> {
        Ok(self.inner.index_seek(key)?)
    }

    pub fn unique_columns(self) -> Option<usize> {
        self.inner.unique_columns()
    }

    // For internal convenience

    // For other packages that want to use PackedRowId directly
    // Not for user consumption
    #[doc(hidden)]
    pub fn inner(self) -> &'a Table {
        self.inner
    }

    #[cfg(feature = "native")]
    pub(crate) fn dump(self) -> String {
        self.inner.dump(self.id_packer)
    }
}

pub struct TableMut<'a> {
    inner: &'a mut Table,
    id_packer: &'a mut IdPacker,
    rowing: &'a mut Rowing,
}

impl<'a> TableMut<'a> {
    #[cfg(test)]
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

    pub fn stage(&mut self, op: PublicOp) {
        debug_assert_eq!(op.table(), self.inner.oid());
        let op = self.id_packer.table_op(op);
        self.inner.stage_update(op);
    }

    pub fn stage_delete(&mut self, row_id: PackedRowId) {
        self.inner.stage_update(TableOp::Delete { row_id });
    }

    pub fn apply_staged(&mut self) -> Result<(), ValidationError> {
        self.inner.apply_staged_ops(self.rowing)
    }

    pub fn insert_row(
        &mut self,
        values: StoreTuple,
        row_id: PackedRowId,
    ) -> Result<(), ValidationError> {
        self.inner.insert_row(values, row_id, self.rowing)
    }

    pub fn rebuild(&mut self) {
        self.inner.rebuild(self.rowing, self.id_packer);
    }
}

#[cfg(test)]
mod test {
    use coln_flir_rs::engine::packed::StoreScalarValue;
    use coln_flir_rs::{hash::CommitHash, ir::Path};
    use rstest::rstest;

    use super::*;
    use crate::{store::Store, test_utils::commit_int_store};

    #[rstest]
    fn row_by_id_finds_committed_row(commit_int_store: (Store, CommitHash)) {
        let (store, _commit) = commit_int_store;
        let path = Path::from("T");
        // The commit hash is the first one interned, so it packs as index 0.
        let row_id = PackedRowId {
            commit_idx: 0,
            counter: 0,
        };
        let table = store.table_at(&path).expect("table exists");

        assert_eq!(
            table.row_by_id(&row_id),
            Some(StoreTuple::from(vec![
                StoreScalarValue::RowId(row_id),
                StoreScalarValue::I32(42),
            ]))
        );
        assert_eq!(
            table.row_by_id(&PackedRowId {
                commit_idx: 0,
                counter: 1,
            }),
            None
        );
    }
}
