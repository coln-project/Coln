//! Integration tests for REPL commands.

use std::path::PathBuf;

use geomerge::{
    ir::{FlatTheory, Path},
    repl::{
        exe::run_transact,
        parse::{Command, parse_command},
    },
    store::Store,
};

static PATHS_IR: &str = "paths.json";

fn fixture_theory(name: &str) -> FlatTheory {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);
    let json = std::fs::read_to_string(p).expect("read theory json");
    serde_json::from_str(&json).expect("parse FlatTheory")
}

#[test]
fn transact_block_matches_manual_transact_for_paths_fixture() {
    let theory = fixture_theory(PATHS_IR);
    let mut store = Store::try_from_theory(theory).expect("valid theory");

    let cmd = parse_command(
        "begin transact; gid1 = add Graphs values (); gid2 = add Graphs values (); \
         g0 = add G0 values (gid2); g1 = add G1 values (gid2); \
         v1 = add G.V values (gid1); v2 = add G.V values (gid1); \
         ge = add G.E values (gid1 v1 v2); commit;",
    )
    .expect("parse transact");

    let Command::Transact { assignments } = cmd else {
        panic!("expected Command::Transact");
    };

    let msg = run_transact(&mut store, &assignments).expect("run transact");
    assert!(msg.contains("gid1=#"), "expected binding summary: {msg}");

    let ge = store.table_at(&Path::from("G.E")).expect("G.E");
    assert_eq!(ge.row_count(), 1);
}
