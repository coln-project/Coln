// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Operations shared by storage and query engines.

use crate::{
    engine::packed::{StoreTuple, WithRowId},
    ir,
};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Op<Oid: Clone, T: Clone + WithRowId> {
    Add { table: Oid, values: T },
    // Delete {
    //     row_id: RowId,
    //     table: TableOid,
    // }, // TODO Delete + Update
}

/// Operations addressed by table path for exchange between store and query engines.
pub type TabPathOp = Op<ir::Path, StoreTuple>;

impl<O: Clone, T: Clone + WithRowId> Op<O, T> {
    pub fn id(&self) -> T::RowId {
        match self {
            Op::Add { values, .. } => values.row_id(),
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

// TODO @Leo should probably move TableDelta here as well to convert between TabPathOp and that
type TableDelta = ();

impl FromIterator<TabPathOp> for TableDelta {
    fn from_iter<T: IntoIterator<Item = TabPathOp>>(_ops: T) -> Self {
        todo!()
        // let zrows: Vec<ZRow> = ops
        //     .into_iter()
        //     .map(|op| match op {
        //         TabPathOp::Add { table, values } => {
        //             ZRow::new(1, tuple).unwrap()
        //         }
        //     })
        //     .collect();

        // TableDelta::new(self.path().to_string(), zrows)
    }
}
