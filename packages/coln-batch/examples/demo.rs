// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! End-to-end demo: generate e-matching workloads, round-trip them through
//! Arrow IPC files, and answer both fixture queries with the generic join
//! (the worst-case-optimal executor — the one the example queries are
//! meant to run on; correctness is covered by the differential tests).
//!
//! Run with:
//!
//! ```sh
//! cargo run -p coln-batch --example demo --release
//! ```

use std::path::Path;
use std::time::Instant;

use anyhow::Result;
use coln_batch::query::{Catalog, Query};
use coln_batch::relation::Relation;
use coln_batch::{fixtures, generate, generic_join, io};

fn main() -> Result<()> {
    let dir = std::env::temp_dir().join("coln-batch-demo");
    std::fs::create_dir_all(&dir)?;
    println!("Arrow files in {}", dir.display());

    demo(
        &dir,
        "f(a, g(a))  [acyclic e-matching pattern]",
        generate::f_g_pattern(200_000, 500_000, 10_000, 42).to_vec(),
        fixtures::fg_query(),
    )?;
    demo(
        &dir,
        "triangle    [cyclic join]",
        generate::triangle(200_000, 500_000, 10_000, 42).to_vec(),
        fixtures::triangle_query(),
    )?;
    Ok(())
}

fn demo(dir: &Path, title: &str, relations: Vec<Relation>, query: Query) -> Result<()> {
    println!("\n=== {title} ===");

    // Persist and reload through Arrow IPC — the test-data substrate.
    let mut catalog = Catalog::new();
    for rel in &relations {
        let path = dir.join(format!("{}.arrow", rel.name));
        io::save_relation(rel, &path)?;
        let loaded = io::load_relation(&rel.name, &path)?;
        println!("  loaded {:<4} {:>8} rows", loaded.name, loaded.len());
        catalog.insert(loaded);
    }

    let started = Instant::now();
    let result = generic_join::execute(&query, &catalog)?;
    let elapsed = started.elapsed();

    println!(
        "  query: {} atoms, {} variables — generic join found {} rows in {elapsed:.2?}",
        query.atoms.len(),
        query.num_vars(),
        result.len()
    );
    for i in 0..result.len().min(3) {
        println!("    sample row: {:?}", result.row(i));
    }
    Ok(())
}
