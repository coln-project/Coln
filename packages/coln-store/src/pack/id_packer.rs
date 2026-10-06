// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::engine::packed::{PackedRowId, StoreScalarValue, WithRowId};
use coln_flir_rs::public::{PublicRowId, PublicScalarValue};

use crate::commit::hash_dict::HashMapper;
use crate::op::{PackedOp, PublicOp};
use crate::rollback::Rollback;
use crate::table::TableOp;

/// A packer doing dictionary encoding while supporting rollbacks.
#[derive(Debug)]
pub(crate) struct IdPacker {
    dict: HashMapper,
    snapshot_len: Option<usize>,
}

/// A readonly trait that packs and unpacks row_id.
/// Does not change what is in the current ids.
pub trait IdLookup {
    fn packed(&self, id: &PublicRowId) -> Option<PackedRowId>;

    fn unpacked(&self, id: &PackedRowId) -> Option<PublicRowId>;
}

#[must_use]
pub(crate) struct IdPackerSnapshot;

impl IdLookup for IdPacker {
    /// Packs `id` without interning its commit hash.
    ///
    /// Returns `None` when the commit hash has not already been interned.
    fn packed(&self, id: &PublicRowId) -> Option<PackedRowId> {
        Some(PackedRowId {
            commit_idx: self.dict.index(id.commit)?,
            counter: id.counter,
        })
    }

    fn unpacked(&self, id: &PackedRowId) -> Option<PublicRowId> {
        Some(PublicRowId {
            commit: self.dict.hash_at(id.commit_idx)?,
            counter: id.counter,
        })
    }
}

impl IdPacker {
    pub(crate) fn new() -> Self {
        Self {
            dict: HashMapper::new(),
            snapshot_len: None,
        }
    }

    /// Packs `id`, interning its commit hash if it is new.
    pub(crate) fn pack_row_id(&mut self, id: PublicRowId) -> PackedRowId {
        PackedRowId {
            commit_idx: self.dict.insert(id.commit),
            counter: id.counter,
        }
    }

    pub(crate) fn unpack_row_id(&self, id: PackedRowId) -> PublicRowId {
        self.unpacked(&id)
            .expect("packed row id commit hash was interned on insert")
    }

    pub(crate) fn pack_value(&mut self, value: PublicScalarValue) -> StoreScalarValue {
        value.map(|id| self.pack_row_id(id))
    }

    /// Packs a cell without modifying the dictionary.
    ///
    /// Returns `None` when an ID cell's commit hash has not been interned.
    pub(crate) fn try_pack_value(&self, value: &PublicScalarValue) -> Option<StoreScalarValue> {
        value.clone().try_map(|id| self.packed(&id))
    }

    pub(crate) fn unpack_value(&self, value: StoreScalarValue) -> PublicScalarValue {
        value.map(|id| self.unpack_row_id(id))
    }

    pub(crate) fn table_op(&mut self, op: PublicOp) -> TableOp {
        match op {
            PublicOp::Add { values, .. } => {
                let row_id = self.pack_row_id(values.row_id());
                let values = values
                    .into_iter()
                    .skip(1)
                    .map(|value| self.pack_value(value))
                    .collect();
                TableOp::Add { row_id, values }
            }
        }
    }

    pub(crate) fn pack_op(&mut self, op: PublicOp) -> PackedOp {
        match op {
            PublicOp::Add { table, values } => PackedOp::Add {
                table,
                values: values
                    .into_iter()
                    .map(|value| self.pack_value(value))
                    .collect(),
            },
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
    fn pack_op_preserves_table_and_values_and_interns_ids() {
        let mut packer = IdPacker::new();
        let existing = packer.pack_row_id(row_id_from(1, 0));
        let values = vec![
            PublicScalarValue::RowId(row_id_from(2, 7)),
            PublicScalarValue::U64(42),
            PublicScalarValue::RowId(row_id_from(1, 9)),
            PublicScalarValue::RowId(row_id_from(2, 8)),
        ];

        let PackedOp::Add {
            table,
            values: packed,
        } = packer.pack_op(PublicOp::Add {
            table: 3,
            values: values.clone().into(),
        });

        assert_eq!(table, 3);
        assert_eq!(
            packed.row_id(),
            PackedRowId {
                commit_idx: 1,
                counter: 7
            }
        );
        assert_eq!(
            packed[2],
            StoreScalarValue::RowId(PackedRowId {
                commit_idx: existing.commit_idx,
                counter: 9,
            })
        );
        assert_eq!(packer.len(), 2);
        let unpacked: Vec<_> = packed
            .into_iter()
            .map(|value| packer.unpack_value(value))
            .collect();
        assert_eq!(unpacked, values);
    }

    #[test]
    fn rollback_removes_hashes_added_after_snapshot() {
        let mut packer = IdPacker::new();
        assert_eq!(packer.pack_row_id(row_id_from(1, 0)).commit_idx, 0);
        let snapshot = packer.snapshot();

        assert_eq!(packer.pack_row_id(row_id_from(2, 0)).commit_idx, 1);
        assert_eq!(packer.pack_row_id(row_id_from(1, 1)).commit_idx, 0);
        packer.rollback_to(snapshot);

        assert_eq!(
            packer.packed(&row_id_from(1, 0)).map(|id| id.commit_idx),
            Some(0)
        );
        assert_eq!(packer.packed(&row_id_from(2, 0)), None);
        assert_eq!(packer.pack_row_id(row_id_from(3, 0)).commit_idx, 1);
    }

    #[test]
    fn commit_snapshot_keeps_added_hashes() {
        let mut packer = IdPacker::new();
        let snapshot = packer.snapshot();
        assert_eq!(packer.pack_row_id(row_id_from(1, 0)).commit_idx, 0);

        packer.commit(snapshot);

        assert_eq!(
            packer.packed(&row_id_from(1, 0)).map(|id| id.commit_idx),
            Some(0)
        );
    }

    /// Distinct commit hashes are interned once, and id cells unpack to the
    /// public ids that were packed.
    #[test]
    fn pack_interns_each_commit_hash_once_and_round_trips() {
        let mut packer = IdPacker::new();
        let rows = [
            (row_id_from(1, 0), row_id_from(3, 7), row_id_from(4, 8)),
            (row_id_from(2, 1), row_id_from(3, 9), row_id_from(1, 0)),
            (row_id_from(1, 2), row_id_from(2, 1), row_id_from(3, 7)),
        ];
        let packed: Vec<_> = rows
            .iter()
            .cloned()
            .map(|(rid, src, dst)| {
                (
                    packer.pack_row_id(rid),
                    packer.pack_value(PublicScalarValue::RowId(src)),
                    packer.pack_value(PublicScalarValue::RowId(dst)),
                )
            })
            .collect();

        for ((rid, src, dst), (packed_rid, packed_src, packed_dst)) in rows.into_iter().zip(packed)
        {
            assert_eq!(packer.unpack_row_id(packed_rid), rid);
            assert_eq!(
                packer.unpack_value(packed_src),
                PublicScalarValue::RowId(src)
            );
            assert_eq!(
                packer.unpack_value(packed_dst),
                PublicScalarValue::RowId(dst)
            );
        }
        assert_eq!(packer.len(), 4);
    }
}
