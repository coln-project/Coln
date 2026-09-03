// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared test utils (feature gated behind `"test-utils"`) to have them
//! accessible for not only this crate's integration tests and unit tests but
//! also available for other crate's integration and unit tests.

use crate::ir::FlatRealm;
use std::path::PathBuf;

/// Reads and parses the coln-compiler's JSON FLIR output with `name` stored in
/// `coln-flir-rs/tests/data` into a [`FlatRealm`].
pub fn load_theory_from_json(name: &str) -> FlatRealm {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);

    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&json)
        .unwrap_or_else(|err| panic!("deserialize FlatRealm from {}: {err}", path.display()))
}
