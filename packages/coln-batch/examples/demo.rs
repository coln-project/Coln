// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! End-to-end demo: generate workloads with a known number of planted
//! matches, round-trip them through Arrow IPC files, answer the fixture
//! queries with the generic join, and cross-check the results against the
//! binary hash join and the planted-match count. Each step reports what
//! it does and what it verified.
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
use coln_batch::{binary_join, fixtures, generate, generic_join, io};

fn main() -> Result<()> {
    let dir = std::env::temp_dir().join("coln-batch-demo");
    std::fs::create_dir_all(&dir)?;

    println!("coln-batch end-to-end demo");
    println!("--------------------------");
    println!("Steps for each workload:");
    println!("  [1] generate relations with a known number of planted matches");
    println!("  [2] save each relation as an Arrow IPC file, load it back, and");
    println!("      verify the loaded copy is identical");
    println!("  [3] answer the fixture query with the generic join");
    println!("  [4] cross-check with the binary hash join and the planted count");
    println!();
    println!("Arrow files: {}", dir.display());

    demo(
        &dir,
        "f(a, g(a)), the acyclic e-matching pattern",
        "Q(f_id, a, g_id) <- R_f(f_id, a, g_id), R_g(g_id, a)",
        10_000,
        generate::f_g_pattern(200_000, 500_000, 10_000, 42).to_vec(),
        fixtures::fg_query(),
    )?;
    demo(
        &dir,
        "triangle, the cyclic join",
        "Q(x, y, z) <- R_f(x, y), R_g(y, z), R_h(z, x)",
        10_000,
        generate::triangle(200_000, 500_000, 10_000, 42).to_vec(),
        fixtures::triangle_query(),
    )?;

    println!("\nAll checks passed.");
    Ok(())
}

fn demo(
    dir: &Path,
    title: &str,
    datalog: &str,
    planted: usize,
    relations: Vec<Relation>,
    query: Query,
) -> Result<()> {
    println!("\n=== {title} ===");

    println!(
        "[1] generated {} relations (seed 42, {planted} planted matches):",
        relations.len()
    );
    for rel in &relations {
        println!(
            "      {:<4} {:>8} rows, {} columns",
            rel.name,
            rel.len(),
            rel.arity()
        );
    }

    println!("[2] Arrow IPC round-trip:");
    let mut catalog = Catalog::new();
    for rel in &relations {
        let path = dir.join(format!("{}.arrow", rel.name));
        io::save_relation(rel, &path)?;
        let loaded = io::load_relation(&rel.name, &path)?;
        anyhow::ensure!(&loaded == rel, "{}: reloaded relation differs", rel.name);
        println!("      {:<4} saved and reloaded, identical", loaded.name);
        catalog.insert(loaded);
    }

    println!("[3] query: {datalog}");
    let started = Instant::now();
    let generic = generic_join::execute(&query, &catalog)?;
    let elapsed = started.elapsed();
    println!(
        "      generic join: {} result rows in {elapsed:.2?}",
        generic.len()
    );
    for i in 0..generic.len().min(3) {
        println!("      sample row: {:?}", generic.row(i));
    }

    println!("[4] cross-checks:");
    let started = Instant::now();
    let binary = binary_join::execute(&query, &catalog)?;
    let elapsed = started.elapsed();
    anyhow::ensure!(binary == generic, "executors disagree");
    println!(
        "      binary hash join: {} result rows in {elapsed:.2?}, identical result",
        binary.len()
    );
    anyhow::ensure!(
        generic.len() >= planted,
        "expected at least the {planted} planted matches, found {}",
        generic.len()
    );
    println!(
        "      planted matches recovered: {} found, {planted} planted (noise adds extras)",
        generic.len()
    );
    Ok(())
}
