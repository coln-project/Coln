// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration tests for REPL commands.
//! Integration tests for `begin batch` … `commit` REPL semantics.

use coln_flir_rs::ir::{FlatRealm, Path};
use coln_store::{
    repl::{exe::run_transact, parse::coln::BatchAssignment},
    store::{ColnDef, Store},
};
use rstest::{fixture, rstest};

static GRAPH_IR: &str = include_str!("../../coln-flir-rs/tests/data/Graph.json");

#[fixture]
fn theory() -> FlatRealm {
    serde_json::from_str(GRAPH_IR).expect("parse Graph FlatRealm")
}

#[fixture]
fn coln_def() -> ColnDef {
    ColnDef {
        theory: String::new(),
        realm: "Graph".to_owned(),
    }
}

#[rstest]
fn batch_block_matches_apply_batch_for_graph_fixture(theory: FlatRealm, coln_def: ColnDef) {
    let mut store = Store::try_from_ir(theory, coln_def).expect("valid theory");

    let assignments = vec![
        BatchAssignment {
            name: "v1".to_string(),
            table: "Graph.V".to_string(),
            row: vec![],
        },
        BatchAssignment {
            name: "v2".to_string(),
            table: "Graph.V".to_string(),
            row: vec![],
        },
        BatchAssignment {
            name: "ge".to_string(),
            table: "Graph.E".to_string(),
            row: vec!["v1".to_string(), "v2".to_string()],
        },
    ];

    let msg = run_transact(&mut store, &assignments).expect("run batch");
    assert!(msg.contains("v1=#"), "expected binding summary: {msg}");

    let ge = store.table_at(&Path::from("Graph.E")).expect("Graph.E");
    assert_eq!(ge.row_count(), 1);
}
