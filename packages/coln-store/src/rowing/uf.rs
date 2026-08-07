// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use ena::unify::{InPlaceUnificationTable, UnifyKey, UnifyValue};

use crate::table::RowId;

pub(super) type UnionFind = InPlaceUnificationTable<NodeId>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct NodeId(u32);

impl UnifyKey for NodeId {
    type Value = RowId;

    fn index(&self) -> u32 {
        self.0
    }

    fn from_index(u: u32) -> Self {
        Self(u)
    }

    fn tag() -> &'static str {
        "rowing"
    }
}

impl UnifyValue for RowId {
    type Error = ena::unify::NoError;

    fn unify_values(value1: &Self, value2: &Self) -> Result<Self, Self::Error> {
        Ok((*value1).min(*value2))
    }
}
