// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `coln-check` as a user runs it, e.g.
//!
//! ```text
//! cargo run -p coln-query --bin coln-check -- TransitiveClosureRealm --seeds 50
//! cargo run -p coln-query --bin coln-check -- path/to/realm.json --seed 7
//! ```

use std::process::{Command, Output};

fn coln_check(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_coln-check"))
        .args(args)
        .output()
        .expect("run coln-check")
}

#[test]
fn a_test_realm_agrees_on_every_backend() {
    let output = coln_check(&["TransitiveClosureRealm", "--seeds", "3"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.starts_with("TransitiveClosureRealm: 1 views, 3 cases of 6 commits on batch"),
        "{stdout}"
    );
    assert!(stdout.contains("all views agree"), "{stdout}");
}

#[test]
fn a_realm_file_runs_one_seed() {
    let realm = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../coln-flir-rs/tests/data/GraphRealm.json"
    );
    let output = coln_check(&[realm, "--seed", "7", "--no-souffle"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.starts_with("GraphRealm: 2 views, 1 cases of 6 commits on batch and incremental\n"),
        "{stdout}"
    );
}

#[test]
fn a_wrong_call_shows_the_usage() {
    let output = coln_check(&["--seeds"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr.starts_with("--seeds needs a number"), "{stderr}");
    assert!(stderr.contains("usage: coln-check"), "{stderr}");
}
