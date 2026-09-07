// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{commit::error::CodecError, store::ColnDef};
use coln_flir_rs::ir::FlatRealm;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct RootCommitData {
    pub(crate) ir: FlatRealm,
    pub(crate) coln_def: ColnDef,
}

impl RootCommitData {
    pub(crate) fn new(ir: FlatRealm, coln_def: ColnDef) -> Self {
        Self { ir, coln_def }
    }
}

/// Encode root store metadata as compact JSON of a [`FlatRealm`].
pub(crate) fn serialize_root(root: &RootCommitData) -> Result<Vec<u8>, CodecError> {
    Ok(serde_json::to_vec(root)?)
}

pub(crate) fn deserialize_root(data: &[u8]) -> Result<RootCommitData, CodecError> {
    Ok(serde_json::from_slice(data)?)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::ir::{Path, Schema, TableEntry};
    use crate::test_utils::{
        int_schema, non_empty_root_commit_data, root_commit_data, string_schema,
    };

    fn table_entry(path: &str, schema: Schema) -> TableEntry {
        TableEntry {
            path: Path::from(path),
            table: schema,
        }
    }

    #[rstest]
    fn root_payload_round_trips(non_empty_root_commit_data: RootCommitData) {
        let root = non_empty_root_commit_data;
        let bytes = serialize_root(&root).expect("encode root");
        let decoded = deserialize_root(&bytes).expect("decode root");

        assert_eq!(decoded.ir.tables.len(), 1);
        assert_eq!(decoded.ir.tables[0].path, Path::from("T"));
        assert_eq!(
            decoded.ir.tables[0].table.columns,
            int_schema(vec!["c0"], Some(vec!["c0"])).columns
        );
        assert_eq!(
            decoded.ir.tables[0].table.primary_key,
            Some(vec![Path::from("c0")])
        );
        assert_eq!(decoded.ir.rules.len(), 1);
        assert_eq!(decoded.ir.rules[0].path, Path::from("T.non_negative"));
        assert_eq!(decoded.coln_def.theory, "theory T");
        assert_eq!(decoded.coln_def.realm, "realm R");
    }

    #[rstest]
    fn root_payload_preserves_entity_order(
        #[from(root_commit_data)] mut left: RootCommitData,
        #[from(root_commit_data)] mut right: RootCommitData,
        #[from(int_schema)]
        #[with(vec!["c0"], Some(vec!["c0"]))]
        int_schema: Schema,
        #[from(string_schema)]
        #[with(vec!["c0"], None)]
        string_schema: Schema,
    ) {
        let a = table_entry("A", int_schema);
        let b = table_entry("B", string_schema);
        left.ir.tables = vec![b.clone(), a.clone()];
        right.ir.tables = vec![a, b];

        assert_ne!(
            serialize_root(&left).expect("encode left"),
            serialize_root(&right).expect("encode right")
        );
    }

    #[rstest]
    fn root_payload_rejects_trailing_bytes(root_commit_data: RootCommitData) {
        let root = root_commit_data;
        let mut bytes = serialize_root(&root).expect("encode root");
        bytes.push(0);

        assert!(deserialize_root(&bytes).is_err());
    }
}
