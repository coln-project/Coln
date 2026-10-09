// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{
    engine::{
        delta::{TableDelta, ZRow},
        packed::{StoreTuple, WithRowId},
        tx::{PendingRowId, TxRowId, TxScalarValue, TxTuple},
    },
    hash::CommitHash,
    ir,
    public::PublicTuple,
};

use crate::table::TableOid;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Op<Oid: Clone, T: Clone + WithRowId> {
    Add { table: Oid, values: T },
    // Delete {
    //     row_id: RowId,
    //     table: TableOid,
    // }, // TODO Delete + Update
}

impl<O: Clone, T: Clone + WithRowId> Op<O, T> {
    pub fn id(&self) -> T::RowId {
        match self {
            Op::Add { values, .. } => values.row_id(),
        }
    }

    pub fn values(&self) -> &T {
        match self {
            Op::Add { values, .. } => values,
        }
    }

    pub fn table(&self) -> O {
        match self {
            Op::Add { table, .. } => table.clone(),
        }
    }

    pub fn map_oid<O2: Clone>(self, f: impl FnOnce(O) -> O2) -> Op<O2, T> {
        match self {
            Op::Add { table, values } => Op::Add {
                table: f(table),
                values,
            },
        }
    }
}

pub const OP_KIND_ADD: u32 = 0;

pub type PublicOp = Op<TableOid, PublicTuple>;

pub type PackedOp = Op<TableOid, StoreTuple>;

impl From<Op<ir::Path, StoreTuple>> for ZRow<StoreTuple> {
    fn from(value: Op<ir::Path, StoreTuple>) -> Self {
        match value {
            Op::Add { values, .. } => ZRow::new_unchecked(1, values),
        }
    }
}

impl From<Op<ir::Path, StoreTuple>> for TableDelta<StoreTuple> {
    fn from(op: Op<ir::Path, StoreTuple>) -> Self {
        Self::new(op.table(), std::iter::once(op.into()))
    }
}

/// An operation staged within a transaction.
// Needs newtype because of we want resolve function on it
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingOp(Op<TableOid, TxTuple>);

impl PendingOp {
    pub(crate) fn add(row_id: PendingRowId, table: TableOid, values: TxTuple) -> Self {
        let values = std::iter::once(TxScalarValue::RowId(TxRowId::Pending(row_id)))
            .chain(values)
            .collect();
        Self(Op::Add { table, values })
    }

    pub(crate) fn table(&self) -> TableOid {
        self.0.table()
    }

    /// Schema column values, excluding the tuple's leading row ID.
    pub(crate) fn column_values(&self) -> &[TxScalarValue] {
        match &self.0 {
            Op::Add { values, .. } => &values.as_slice()[1..],
        }
    }

    pub(crate) fn resolve(self, commit: CommitHash) -> PublicOp {
        match self.0 {
            Op::Add { table, values } => PublicOp::Add {
                table,
                values: values
                    .into_iter()
                    .map(|value| value.map(|id| id.resolve(commit)))
                    .collect(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coln_flir_rs::{
        hash::HASH_SIZE,
        public::{PublicRowId, PublicScalarValue},
    };

    #[test]
    fn resolve_preserves_columns_and_resolves_each_row_id_once() {
        let hash = CommitHash([1; HASH_SIZE]);
        let existing = PublicRowId {
            commit: CommitHash([2; HASH_SIZE]),
            counter: 7,
        };
        let columns = vec![
            TxScalarValue::RowId(TxRowId::Pending(PendingRowId(0))),
            TxScalarValue::RowId(TxRowId::Existing(existing.clone())),
            TxScalarValue::I32(42),
        ];
        let op = PendingOp::add(PendingRowId(1), 3, columns.clone().into());
        assert_eq!(op.column_values(), columns);

        let resolved = op.resolve(hash);
        assert_eq!(resolved.table(), 3);
        assert_eq!(resolved.id(), PendingRowId(1).resolve(hash));
        let PublicOp::Add { values, .. } = resolved;
        assert_eq!(
            &values[..],
            [
                PublicScalarValue::RowId(PendingRowId(1).resolve(hash)),
                PublicScalarValue::RowId(PendingRowId(0).resolve(hash)),
                PublicScalarValue::RowId(existing),
                PublicScalarValue::I32(42),
            ]
        );
    }

    #[test]
    fn resolve_empty_row_contains_only_its_id() {
        let hash = CommitHash([1; HASH_SIZE]);
        let op = PendingOp::add(PendingRowId(0), 3, TxTuple::empty());
        assert!(op.column_values().is_empty());

        let PublicOp::Add { values, .. } = op.resolve(hash);
        assert_eq!(
            &values[..],
            [PublicScalarValue::RowId(PendingRowId(0).resolve(hash))]
        );
    }
}
