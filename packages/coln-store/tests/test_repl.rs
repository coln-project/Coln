// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Integration tests for REPL commands.
//! Integration tests for `begin batch` … `commit` REPL semantics.

use std::path::PathBuf;

use coln_flir_rs::ir::{FlatRealm, Path};
use coln_store::{
    repl::{
        exe::run_transact,
        parse::{Command, parse_command},
    },
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

    let cmd = parse_command(
        "begin transact; gid1 = add Path.Graphs values (); gid2 = add Path.Graphs values (); \
         g0 = add Path.G0 values (gid2); g1 = add Path.G1 values (gid2); \
         v1 = add Path.G.V values (gid1); v2 = add Path.G.V values (gid1); \
         ge = add Path.G.E values (gid1 v1 v2); commit;",
    )
    .expect("parse batch");

    let Command::Batch { assignments } = cmd else {
        panic!("expected Command::Batch");
    };

    let msg = run_transact(&mut store, &assignments).expect("run batch");
    assert!(msg.contains("gid1=#"), "expected binding summary: {msg}");

    let ge = store.table_at(&Path::from("Path.G.E")).expect("Path.G.E");
    assert_eq!(ge.row_count(), 1);
}
