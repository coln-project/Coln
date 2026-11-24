// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Check that batch, incremental and Soufflé compute the same views of a
//! realm, on random rows. This is what the `coln-check` binary runs:
//!
//! ```text
//! cargo run -p coln-query --bin coln-check -- TransitiveClosureRealm --seeds 50
//! ```
//!
//! Each seed is one case: a few commits of random rows for the realm's base
//! tables (see `relational/batch/random_rows.rs`), compared view by view
//! after every commit. Soufflé takes part where it is installed. The first
//! case that differs stops the check and is reported with its seed, so it
//! can be rerun alone.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use coln_flir_rs::ir::FlatRealm;

use crate::relational::batch::random_rows::{RandomRows, max_inserts};
use crate::relational::batch::twin::Twin;

/// What to check.
#[derive(Debug, Clone)]
pub struct Options {
    /// The seeds to run, one case each.
    pub seeds: Vec<u64>,
    /// Commits per case.
    pub commits: usize,
    /// The most rows a commit inserts per table. Later seeds grow up to it.
    pub max_rows: u64,
    /// Whether to ask Soufflé as well, if it is installed.
    pub souffle: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            seeds: (0..20).collect(),
            commits: 6,
            max_rows: 3,
            souffle: true,
        }
    }
}

/// The outcome of a check.
#[derive(Debug)]
pub struct Report {
    pub realm: String,
    /// Every view with the most rows it held after any commit, so that an
    /// agreement on empty views shows as such.
    pub views: Vec<(String, usize)>,
    /// Whether Soufflé took part.
    pub souffle: bool,
    /// Whether Soufflé was asked for but is not installed.
    pub souffle_missing: bool,
    pub cases: usize,
    pub commits: usize,
    /// The first case that differs, if any.
    pub failure: Option<Failure>,
}

/// A case in which the views differ.
#[derive(Debug)]
pub struct Failure {
    pub seed: u64,
    pub commit: usize,
    /// Which views differ, and how.
    pub message: String,
    /// The table sizes after each commit up to the failing one.
    pub commits: Vec<String>,
}

impl Report {
    pub fn agrees(&self) -> bool {
        self.failure.is_none()
    }
}

/// Load the realm at `path` and [check](check_realm) it. A `path` that does
/// not exist names one of the compiler's test realms instead, e.g.
/// `TransitiveClosureRealm`.
pub fn check_file(path: &Path, options: &Options) -> Result<Report> {
    let path = resolve(path);
    let json =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let realm: FlatRealm =
        serde_json::from_str(&json).with_context(|| format!("parse {}", path.display()))?;
    let name = path.file_stem().map_or_else(
        || path.display().to_string(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    check_realm(&realm, &name, options)
}

/// Run every case of `options` on `realm`, called `name` in the report.
pub fn check_realm(realm: &FlatRealm, name: &str, options: &Options) -> Result<Report> {
    let twin = Twin::new(realm, name, options.souffle)?;
    let mut report = Report {
        realm: name.to_string(),
        views: twin.views().map(|view| (view.to_string(), 0)).collect(),
        souffle: twin.has_souffle(),
        souffle_missing: options.souffle && !twin.has_souffle(),
        cases: 0,
        commits: options.commits,
        failure: None,
    };
    for &seed in &options.seeds {
        let mut twin = Twin::new(realm, name, options.souffle)?;
        let mut rows = RandomRows::new(realm, seed)?;
        let most = max_inserts(seed, options.max_rows);
        let mut commits = Vec::new();
        report.cases += 1;
        for commit in 0..options.commits {
            let (delta, sizes) = rows.commit(most);
            commits.push(format!("{commit}: {sizes}"));
            match twin.commit(delta) {
                Ok(sizes) => {
                    for ((_, most), (_, size)) in report.views.iter_mut().zip(sizes) {
                        *most = (*most).max(size);
                    }
                }
                Err(error) => {
                    report.failure = Some(Failure {
                        seed,
                        commit,
                        message: format!("{error:#}"),
                        commits,
                    });
                    return Ok(report);
                }
            }
        }
    }
    Ok(report)
}

/// `path` itself, or the compiler's test realm it names.
fn resolve(path: &Path) -> PathBuf {
    if path.exists() {
        return path.to_path_buf();
    }
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../coln-flir-rs/tests/data")
        .join(path)
        .with_extension("json");
    match fixture.exists() {
        true => fixture,
        false => path.to_path_buf(),
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let backends = match (self.souffle, self.souffle_missing) {
            (true, _) => "batch, incremental and Soufflé",
            (false, true) => "batch and incremental (Soufflé not found)",
            (false, false) => "batch and incremental",
        };
        writeln!(
            f,
            "{}: {} views, {} cases of {} commits on {backends}",
            self.realm,
            self.views.len(),
            self.cases,
            self.commits
        )?;
        let views: Vec<String> = self
            .views
            .iter()
            .map(|(view, most)| format!("{view} {most}"))
            .collect();
        writeln!(f, "most rows per view: {}", views.join(", "))?;
        match &self.failure {
            None => write!(f, "all views agree"),
            Some(failure) => {
                writeln!(
                    f,
                    "seed {}, commit {}: {}",
                    failure.seed, failure.commit, failure.message
                )?;
                writeln!(f, "rows per table after each commit:")?;
                for commit in &failure.commits {
                    writeln!(f, "  {commit}")?;
                }
                write!(f, "rerun this case with --seed {}", failure.seed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every test realm the frontend translates agrees on random rows. This
    /// is the only data test of `GraphOfGraphsRealm`.
    #[test]
    fn every_test_realm_agrees() -> Result<()> {
        let options = Options {
            seeds: (0..10).collect(),
            ..Options::default()
        };
        for realm in [
            "GraphRealm",
            "TriangleRealm",
            "TransitiveClosureRealm",
            "GraphOfGraphsRealm",
            "BipartiteGraphRealm",
            "BipartiteGraphOverRealm",
        ] {
            let report = check_file(Path::new(realm), &options)?;
            assert!(report.agrees(), "{report}");
        }
        Ok(())
    }

    #[test]
    fn a_missing_file_is_an_error() {
        let error = check_file(Path::new("NoSuchRealm"), &Options::default()).unwrap_err();
        assert!(
            format!("{error:#}").contains("read NoSuchRealm"),
            "{error:#}"
        );
    }
}
