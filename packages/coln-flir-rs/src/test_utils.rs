// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared test utils (feature gated behind `"test-utils"`) to have them
//! accessible for not only this crate's integration tests and unit tests but
//! also available for other crate's integration and unit tests.

use crate::ir::FlatRealm;
use std::path::{Path, PathBuf};

/// Reads and parses the coln-compiler's JSON FLIR output with `name` stored in
/// `coln-flir-rs/tests/data` into a [`FlatRealm`].
pub fn load_theory_from_json<T: AsRef<Path>>(name: T) -> FlatRealm {
    let path = test_data_dir().join(name);

    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_str(&json)
        .unwrap_or_else(|err| panic!("deserialize FlatRealm from {}: {err}", path.display()))
}

pub fn all_theory_fixtures() -> std::io::Result<Vec<PathBuf>> {
    list_json_files(test_data_dir())
}

fn list_json_files<T: AsRef<Path>>(dir: T) -> std::io::Result<Vec<PathBuf>> {
    Ok(std::fs::read_dir(dir)?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let is_json = path.extension().and_then(|s| s.to_str()) == Some("json");
            (path.is_file() && is_json).then_some(path)
        })
        .collect())
}

fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}
