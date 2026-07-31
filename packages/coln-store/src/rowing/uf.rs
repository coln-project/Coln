use ena::{
    snapshot_vec::Snapshot,
    unify::{InPlaceUnificationTable, UnifyKey, UnifyValue},
};

use crate::{
    commit::hash_dict::HashMapper,
    table::{PackedRowId, RowId},
};

pub(super) type UnionFind = InPlaceUnificationTable<NodeId>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct NodeId(u32);

impl UnifyKey for NodeId {
    type Value = RowId;

    fn index(&self) -> u32 {
        todo!()
    }

    fn from_index(u: u32) -> Self {
        todo!()
    }

    fn tag() -> &'static str {
        todo!()
    }
}

impl UnifyValue for RowId {
    type Error = ena::unify::NoError;

    fn unify_values(value1: &Self, value2: &Self) -> Result<Self, Self::Error> {
        todo!()
    }
}
