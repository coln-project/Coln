// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration tests for REPL commands.
//! Integration tests for `begin batch` … `commit` REPL semantics.

use std::path::PathBuf;

use coln_flir_rs::ir::{FlatRealm, Path};
use coln_store::{
    repl::{exe::run_transact, parse::BatchAssignment},
    store::Store,
};

static PATHS_IR: &str = "Path.json";

fn fixture_theory(name: &str) -> FlatRealm {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);
    let json = std::fs::read_to_string(p).expect("read theory json");
    serde_json::from_str(&json).expect("parse FlatRealm")
}

#[test]
fn batch_block_matches_apply_batch_for_paths_fixture() {
    let theory = fixture_theory(PATHS_IR);
    let mut store = Store::try_from_theory(theory).expect("valid theory");

    let assignments = vec![
        BatchAssignment {
            name: "gid1".to_string(),
            table: "Path.Graphs".to_string(),
            row: vec![],
        },
        BatchAssignment {
            name: "gid2".to_string(),
            table: "Path.Graphs".to_string(),
            row: vec![],
        },
        BatchAssignment {
            name: "g0".to_string(),
            table: "Path.G0".to_string(),
            row: vec!["gid2".to_string()],
        },
        BatchAssignment {
            name: "g1".to_string(),
            table: "Path.G1".to_string(),
            row: vec!["gid2".to_string()],
        },
        BatchAssignment {
            name: "v1".to_string(),
            table: "Path.G.V".to_string(),
            row: vec!["gid1".to_string()],
        },
        BatchAssignment {
            name: "v2".to_string(),
            table: "Path.G.V".to_string(),
            row: vec!["gid1".to_string()],
        },
        BatchAssignment {
            name: "ge".to_string(),
            table: "Path.G.E".to_string(),
            row: vec!["gid1".to_string(), "v1".to_string(), "v2".to_string()],
        },
    ];

    let msg = run_transact(&mut store, &assignments).expect("run batch");
    assert!(msg.contains("gid1=#"), "expected binding summary: {msg}");

    let ge = store.table_at(&Path::from("Path.G.E")).expect("Path.G.E");
    assert_eq!(ge.row_count(), 1);
}
