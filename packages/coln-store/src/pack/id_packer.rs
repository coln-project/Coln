// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::commit::hash_dict::HashMapper;
use crate::op::Op;
use crate::pack::{PackedOp, PackedRowId, PackedValue};
use crate::rollback::Rollback;
use crate::table::{WireRowId, WireValue};

/// A packer doing dictionary encoding while supporting rollbacks.
#[derive(Debug)]
pub(crate) struct IdPacker {
    dict: HashMapper,
    snapshot_len: Option<usize>,
}

#[must_use]
pub(crate) struct IdPackerSnapshot;

impl IdPacker {
    pub(crate) fn new() -> Self {
        Self {
            dict: HashMapper::new(),
            snapshot_len: None,
        }
    }

    /// Packs `id`, interning its commit hash if it is new.
    pub(crate) fn pack_row_id(&mut self, id: WireRowId) -> PackedRowId {
        PackedRowId {
            commit_idx: self.dict.insert(id.commit),
            counter: id.counter,
        }
    }

    /// Packs `id` without interning its commit hash.
    ///
    /// Returns `None` when the commit hash has not already been interned.
    pub(crate) fn lookup_row_id(&self, id: &WireRowId) -> Option<PackedRowId> {
        Some(PackedRowId {
            commit_idx: self.dict.index(id.commit)?,
            counter: id.counter,
        })
    }

    pub(crate) fn unpack_row_id(&self, id: PackedRowId) -> WireRowId {
        WireRowId {
            commit: self
                .dict
                .hash_at(id.commit_idx)
                .expect("packed row id commit hash was interned on insert"),
            counter: id.counter,
        }
    }

    pub(crate) fn pack_value(&mut self, value: WireValue) -> PackedValue {
        match value {
            WireValue::Id(id) => PackedValue::Id(self.pack_row_id(id)),
            WireValue::Int(value) => PackedValue::Int(value),
            WireValue::Str(value) => PackedValue::Str(value),
        }
    }

    /// Packs a cell without modifying the dictionary.
    ///
    /// Returns `None` when an ID cell's commit hash has not been interned.
    pub(crate) fn try_pack_value(&self, value: &WireValue) -> Option<PackedValue> {
        Some(match value {
            WireValue::Id(id) => PackedValue::Id(self.lookup_row_id(id)?),
            WireValue::Int(value) => PackedValue::Int(*value),
            WireValue::Str(value) => PackedValue::Str(value.clone()),
        })
    }

    pub(crate) fn unpack_value(&self, value: PackedValue) -> WireValue {
        value.map_owned(|id| self.unpack_row_id(id))
    }

    pub(crate) fn pack_op(&mut self, op: Op) -> PackedOp {
        match op {
            Op::Add { row_id, values, .. } => {
                let row_id = self.pack_row_id(row_id);
                let values = values
                    .into_iter()
                    .map(|value| self.pack_value(value))
                    .collect();
                PackedOp::Add { row_id, values }
            }
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.dict.hashes().len()
    }

    // TODO we should have a method to remove a hash from dictionary?
    // Perhaps we could do mark and sweep, as we are doing a full rebuild of a
    // table because we need to scan the table anyway.
}

impl Rollback for IdPacker {
    type Snapshot = IdPackerSnapshot;

    fn snapshot(&mut self) -> Self::Snapshot {
        assert!(
            self.snapshot_len.is_none(),
            "nested ID packer snapshots are not supported"
        );
        self.snapshot_len = Some(self.len());
        IdPackerSnapshot
    }

    fn commit(&mut self, _snapshot: Self::Snapshot) {
        self.snapshot_len
            .take()
            .expect("ID packer has no active snapshot");
    }

    fn rollback_to(&mut self, _snapshot: Self::Snapshot) {
        let snapshot_len = self
            .snapshot_len
            .take()
            .expect("ID packer has no active snapshot");
        self.dict.truncate(snapshot_len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::row_id_from;

    #[test]
    fn rollback_removes_hashes_added_after_snapshot() {
        let mut packer = IdPacker::new();
        assert_eq!(packer.pack_row_id(row_id_from(1, 0)).commit_idx, 0);
        let snapshot = packer.snapshot();

        assert_eq!(packer.pack_row_id(row_id_from(2, 0)).commit_idx, 1);
        assert_eq!(packer.pack_row_id(row_id_from(1, 1)).commit_idx, 0);
        packer.rollback_to(snapshot);

        assert_eq!(
            packer
                .lookup_row_id(&row_id_from(1, 0))
                .map(|id| id.commit_idx),
            Some(0)
        );
        assert_eq!(packer.lookup_row_id(&row_id_from(2, 0)), None);
        assert_eq!(packer.pack_row_id(row_id_from(3, 0)).commit_idx, 1);
    }

    #[test]
    fn commit_snapshot_keeps_added_hashes() {
        let mut packer = IdPacker::new();
        let snapshot = packer.snapshot();
        assert_eq!(packer.pack_row_id(row_id_from(1, 0)).commit_idx, 0);

        packer.commit(snapshot);

        assert_eq!(
            packer
                .lookup_row_id(&row_id_from(1, 0))
                .map(|id| id.commit_idx),
            Some(0)
        );
    }
}
