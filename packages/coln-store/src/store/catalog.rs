// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::HashMap;

use coln_flir_rs::{engine::schema::ColnDef, ir};

use crate::{
    commit::wire::RootCommitData,
    table::{TableMeta, TableOid},
};

#[derive(Debug)]
pub struct Catalog {
    pub(super) path_to_oid: HashMap<ir::Path, TableOid>,
    /// Source rule entries retained for persistence. Compiled form lives in `rules`.
    pub(super) root_data: RootCommitData,
    pub(super) table_meta: Vec<TableMeta>,
    pub(super) ir: ir::FlatRealm,
}

impl Catalog {
    pub(super) fn resolve_table(&self, path: &ir::Path) -> Option<TableOid> {
        self.path_to_oid.get(path).copied()
    }

    pub(super) fn table_count(&self) -> usize {
        self.table_meta.len()
    }

    pub(super) fn rule_entries(&self) -> &[ir::RuleEntry] {
        &self.ir.rules
    }

    pub(super) fn json_ir(&self) -> String {
        let root = &self.root_data;
        serde_json::to_string(&root.ir).expect("stored root ir contains valid data")
    }

    pub(super) fn ir(&self) -> &ir::FlatRealm {
        &self.ir
    }

    pub(super) fn coln_def(&self) -> &ColnDef {
        &self.root_data.coln_def
    }

    /// Path, oid, and schema for a registered table.
    pub(super) fn table_meta(&self, oid: TableOid) -> Option<&TableMeta> {
        self.table_meta.get(oid)
    }
}
