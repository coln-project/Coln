// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Saving/loading relations as Arrow IPC files.
//!
//! Arrow is the *test-data* substrate: the real storage layer will be
//! Hexane-backed indexes. The engine only depends on the
//! [`crate::table::SortedTable`] trait, so swapping the substrate later
//! does not touch the join code.

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use arrow::ipc::reader::FileReader;
use arrow::ipc::writer::FileWriter;

use crate::relation::Relation;

/// Write a relation to `path` as a single-batch Arrow IPC file.
pub fn save_relation(rel: &Relation, path: &Path) -> Result<()> {
    let batch = rel.to_record_batch()?;
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let schema = batch.schema();
    let mut writer = FileWriter::try_new(file, schema.as_ref())?;
    writer.write(&batch)?;
    writer.finish()?;
    Ok(())
}

/// Read an Arrow IPC file back into a relation, concatenating all record
/// batches in the file.
pub fn load_relation(name: &str, path: &Path) -> Result<Relation> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = FileReader::try_new(file, None)?;
    let mut result: Option<Relation> = None;
    for batch in reader {
        let part = Relation::from_record_batch(name, &batch?)?;
        match &mut result {
            None => result = Some(part),
            Some(acc) => {
                for (dst, src) in acc.cols.iter_mut().zip(part.cols) {
                    dst.extend(src);
                }
            }
        }
    }
    result.with_context(|| format!("{}: empty IPC file", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate;
    use std::path::PathBuf;

    fn tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("coln-batch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn roundtrip_small() {
        let dir = tmp_dir();
        let [f, g, h] = generate::triangle(1_000, 10_000, 100, 42);
        for rel in [&f, &g, &h] {
            let path = dir.join(format!("{}.arrow", rel.name));
            save_relation(rel, &path).unwrap();
            let back = load_relation(&rel.name, &path).unwrap();
            assert_eq!(rel, &back);
        }
    }

    /// Scale test: generate ~1M rows per relation, save, reload.
    /// Run with: cargo test -p coln-batch -- --include-ignored
    #[test]
    #[ignore = "large; run explicitly"]
    fn roundtrip_one_million_rows() {
        let dir = tmp_dir();
        let [f, _g, _h] = generate::triangle(1_000_000, 1_000_000, 10_000, 42);
        assert!(f.len() > 950_000, "got {}", f.len());
        let path = dir.join("R_f_1m.arrow");
        save_relation(&f, &path).unwrap();
        let back = load_relation("R_f", &path).unwrap();
        assert_eq!(f, back);
    }
}
