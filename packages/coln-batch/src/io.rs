// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Saving/loading relations as Arrow IPC files.
//!
//! Arrow is the *test-data* substrate: the real storage layer will be
//! Hexane-backed indexes. The engine only depends on the
//! [`crate::table::SortedTable`] trait, so swapping the substrate later
//! does not touch the join code.
//!
//! Files hold decoded values (see [`Relation::to_record_batch`] for the
//! Arrow types), so a relation saved from one catalog loads into any
//! other; strings are re-interned into the loading catalog's dictionary.

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use arrow::ipc::reader::FileReader;
use arrow::ipc::writer::FileWriter;

use crate::relation::Relation;
use crate::types::Dictionary;

/// Write a relation to `path` as a single-batch Arrow IPC file.
pub fn save_relation(rel: &Relation, dict: &Dictionary, path: &Path) -> Result<()> {
    let batch = rel.to_record_batch(dict)?;
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let schema = batch.schema();
    let mut writer = FileWriter::try_new(file, schema.as_ref())?;
    writer.write(&batch)?;
    writer.finish()?;
    Ok(())
}

/// Read an Arrow IPC file back into a relation, concatenating all record
/// batches in the file.
pub fn load_relation(name: &str, path: &Path, dict: &mut Dictionary) -> Result<Relation> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = FileReader::try_new(file, None)?;
    let mut result: Option<Relation> = None;
    for batch in reader {
        let part = Relation::from_record_batch(name, &batch?, dict)?;
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
        let dict = Dictionary::new();
        let [f, g, h] = generate::triangle(1_000, 10_000, 100, 42);
        for rel in [&f, &g, &h] {
            let path = dir.join(format!("{}.arrow", rel.name));
            save_relation(rel, &dict, &path).unwrap();
            let back = load_relation(&rel.name, &path, &mut Dictionary::new()).unwrap();
            assert_eq!(rel, &back);
        }
    }

    #[test]
    fn roundtrip_typed() {
        let dir = tmp_dir();
        let mut dict = Dictionary::new();
        let rel = generate::labeled_edges(20, 60, &["road", "rail"], 5, &mut dict);
        let path = dir.join("labeled.arrow");
        save_relation(&rel, &dict, &path).unwrap();

        // Loading into a catalog whose dictionary already has other
        // strings must reproduce the same values.
        let mut other = Dictionary::new();
        other.intern("unrelated");
        let back = load_relation(&rel.name, &path, &mut other).unwrap();
        assert_eq!(back.schema, rel.schema);
        assert_eq!(back.len(), rel.len());
        for i in 0..rel.len() {
            assert_eq!(
                back.row_values(i, &other).unwrap(),
                rel.row_values(i, &dict).unwrap()
            );
        }
    }

    /// Scale test: generate ~10M rows per relation, save, reload.
    /// Run with: cargo test -p coln-batch --release -- --include-ignored
    #[test]
    #[ignore = "large; run explicitly (use --release)"]
    fn roundtrip_ten_million_rows() {
        let dir = tmp_dir();
        let dict = Dictionary::new();
        let [f, _g, _h] = generate::triangle(10_000_000, 10_000_000, 100_000, 42);
        assert!(f.len() > 9_500_000, "got {}", f.len());
        let path = dir.join("R_f_10m.arrow");
        save_relation(&f, &dict, &path).unwrap();
        let back = load_relation("R_f", &path, &mut Dictionary::new()).unwrap();
        assert_eq!(f, back);
    }
}
