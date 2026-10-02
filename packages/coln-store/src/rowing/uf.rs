// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::public::PublicRowId;
use ena::unify::{InPlaceUnificationTable, UnifyKey};

pub(super) type UnionFind = InPlaceUnificationTable<NodeId>;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(super) struct NodeId(u32);

impl UnifyKey for NodeId {
    type Value = PublicRowId;

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
