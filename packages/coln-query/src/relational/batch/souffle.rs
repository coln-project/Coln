// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Soufflé as a third opinion on a realm's views, next to both backends.
//!
//! The program is the frontend's own Datalog, as its printer writes it
//! ([`LogicalProgram::display`]). Soufflé accepts that nearly as is. This
//! module only renames what Soufflé does not allow in a name (`.`, `-`, `#`,
//! `/`), names the column types Soufflé's way, and marks the base tables as
//! input. Base rows go in as fact files, and each view comes back as a file.
//!
//! Soufflé is a separate program. Where it is not installed, as in CI,
//! [`Souffle::new`] returns `None` and the tests compare the two backends
//! only.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail, ensure};

use crate::api::deltas::ZRow;
use crate::api::query::FlirProgram;
use crate::frontend::{Identifiable, LogicalProgram};
use crate::relational::relation::TupleValue;
use crate::scalarial::{ScalarType, ScalarTypedValue};

/// One realm's program for Soufflé, and the base rows to run it on.
pub(super) struct Souffle {
    /// A fresh directory with the program, the fact files and the results.
    dir: PathBuf,
    /// Every base table: its name for Soufflé, column types and live rows.
    tables: HashMap<String, Table>,
    /// Every predicate's name for Soufflé and column types.
    predicates: HashMap<String, (String, Vec<ScalarType>)>,
}

struct Table {
    name: String,
    types: Vec<ScalarType>,
    /// Each row with its weight summed over all commits so far.
    rows: BTreeMap<TupleValue, i64>,
}

impl Souffle {
    /// Prepare `program` for Soufflé, or `None` if Soufflé is not installed.
    pub(super) fn new(program: &FlirProgram) -> Result<Option<Self>> {
        if Command::new("souffle").arg("--version").output().is_err() {
            return Ok(None);
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "coln-souffle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("facts"))?;

        let logical = program.logical();
        let mut tables = HashMap::new();
        let mut predicates = HashMap::new();
        let mut inputs = String::new();
        for predicate in logical.predicates() {
            let path = predicate.id().to_string();
            let name = escape(&path);
            let types: Vec<ScalarType> = predicate
                .columns()
                .map(|column| column.scalar_type())
                .collect();
            if predicate.is_edb_predicate() {
                write!(inputs, "\n.input {name}")?;
                let table = Table {
                    name: name.clone(),
                    types: types.clone(),
                    rows: BTreeMap::new(),
                };
                tables.insert(path.clone(), table);
            }
            predicates.insert(path, (name, types));
        }
        let arities = predicates
            .iter()
            .map(|(path, (_, types))| (path.as_str(), types.len()))
            .collect();
        let source = dialect(&logical.display().to_string(), &arities)? + &inputs + "\n";
        std::fs::write(dir.join("program.dl"), source)?;
        Ok(Some(Self {
            dir,
            tables,
            predicates,
        }))
    }

    /// Add one commit's rows of the base table `path`.
    pub(super) fn feed(&mut self, path: &str, delta: &[ZRow]) -> Result<()> {
        let table = self
            .tables
            .get_mut(path)
            .with_context(|| format!("Soufflé has no base table {path}"))?;
        for row in delta {
            let weight = table.rows.entry(row.clone().into_row()).or_insert(0);
            *weight += row.zweight();
        }
        table.rows.retain(|_, weight| *weight != 0);
        Ok(())
    }

    /// Run the program on the live base rows and return the rows of each of
    /// `views`.
    pub(super) fn evaluate(&self, views: &[&str]) -> Result<HashMap<String, BTreeSet<TupleValue>>> {
        let facts = self.dir.join("facts");
        for table in self.tables.values() {
            let mut file = String::new();
            for (row, weight) in &table.rows {
                ensure!(*weight == 1, "{} holds {row:?} {weight} times", table.name);
                ensure!(row.data.len() == table.types.len(), "the width of {row:?}");
                let fields: Vec<String> = row.data.iter().map(field).collect::<Result<_>>()?;
                writeln!(file, "{}", fields.join("\t"))?;
            }
            std::fs::write(facts.join(format!("{}.facts", table.name)), file)?;
        }

        let output = self.dir.join("output");
        let _ = std::fs::remove_dir_all(&output);
        std::fs::create_dir_all(&output)?;
        let run = Command::new("souffle")
            .arg("--no-warn")
            .arg("--fact-dir")
            .arg(&facts)
            .arg("--output-dir")
            .arg(&output)
            .arg(self.dir.join("program.dl"))
            .output()
            .context("run souffle")?;
        if !run.status.success() {
            let stderr = String::from_utf8_lossy(&run.stderr);
            let errors: Vec<&str> = stderr
                .lines()
                .filter(|line| line.starts_with("Error"))
                .collect();
            bail!(
                "Soufflé fails on {}: {}",
                self.dir.join("program.dl").display(),
                errors.join(" ")
            );
        }

        let mut results = HashMap::new();
        for &view in views {
            let (name, types) = self
                .predicates
                .get(view)
                .with_context(|| format!("Soufflé has no view {view}"))?;
            let text = std::fs::read_to_string(output.join(format!("{name}.csv")))
                .with_context(|| format!("Soufflé's output of {view}"))?;
            let rows = text
                .lines()
                .map(|line| {
                    let fields: Vec<&str> = line.split('\t').collect();
                    ensure!(fields.len() == types.len(), "the width of {line:?}");
                    let data = fields
                        .iter()
                        .zip(types)
                        .map(|(text, scalar_type)| value(text, *scalar_type))
                        .collect::<Result<_>>()?;
                    Ok(TupleValue::new(data))
                })
                .collect::<Result<_>>()?;
            results.insert(view.to_string(), rows);
        }
        Ok(results)
    }
}

impl Drop for Souffle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A name the frontend prints, as a Soufflé identifier. Every character
/// Soufflé does not allow becomes `_` and a letter, and `_` itself `__`, so
/// two different names stay different.
fn escape(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '_' => escaped.push_str("__"),
            '.' => escaped.push_str("_d"),
            '-' => escaped.push_str("_h"),
            '#' => escaped.push_str("_s"),
            '/' => escaped.push_str("_f"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// Leo's Datalog in Soufflé's dialect: every name [escaped](escape) and every
/// column type spelled as Soufflé spells it. Directives (`.decl`, `.output`),
/// literals, `_` and the punctuation stay as they are.
///
/// The printer leaves out the unbound positions at the end of an atom, as an
/// atom does not know its predicate's width. Soufflé needs them, so every atom
/// of a predicate in `arities` gets `_` up to that width.
fn dialect(program: &str, arities: &HashMap<&str, usize>) -> Result<String> {
    let mut out = String::with_capacity(program.len());
    let mut chars = program.chars().peekable();
    // The open atom: its predicate's width and the positions it has so far.
    let mut atom: Option<(usize, usize)> = None;
    let mut line_is_decl = false;
    while let Some(c) = chars.next() {
        if let Some((arity, positions)) = &mut atom {
            match c {
                '(' => {}
                ',' => *positions += 1,
                ')' => {
                    for position in *positions..*arity {
                        out.push_str(if position == 0 { "_" } else { ", _" });
                    }
                    atom = None;
                }
                c if !c.is_whitespace() && *positions == 0 => *positions = 1,
                _ => {}
            }
        }
        if c == '\n' {
            line_is_decl = false;
        }
        if c == '"' {
            // A string literal, copied up to its closing quote.
            out.push(c);
            while let Some(c) = chars.next() {
                out.push(c);
                match c {
                    '\\' => out.extend(chars.next()),
                    '"' => break,
                    _ => {}
                }
            }
        } else if c == '.' && chars.peek().is_some_and(char::is_ascii_alphabetic) {
            // A directive: `.decl` or `.output`.
            out.push(c);
            let mut directive = String::new();
            while let Some(c) = chars.next_if(char::is_ascii_alphabetic) {
                directive.push(c);
            }
            line_is_decl = directive == "decl";
            out.push_str(&directive);
        } else if c.is_ascii_alphabetic() {
            let mut name = String::from(c);
            while let Some(c) = chars.next_if(|c| c.is_ascii_alphanumeric() || "_.-#/".contains(*c))
            {
                name.push(c);
            }
            // A rule ends with `.`, which is not part of its last name.
            let ends_rule = name.ends_with('.');
            if ends_rule {
                name.pop();
            }
            if out.ends_with(": ") {
                out.push_str(souffle_type(&name)?);
            } else {
                out.push_str(&escape(&name));
            }
            if !line_is_decl && chars.peek() == Some(&'(') {
                atom = arities.get(name.as_str()).map(|&arity| (arity, 0));
            }
            if ends_rule {
                out.push('.');
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// A column type as the frontend prints it, as Soufflé's type.
fn souffle_type(name: &str) -> Result<&'static str> {
    Ok(match name {
        "uint" => "unsigned",
        "iint" => "number",
        "string" => "symbol",
        _ => bail!("Soufflé has no type for {name}"),
    })
}

/// A value as a field of a fact file.
fn field(value: &ScalarTypedValue) -> Result<String> {
    Ok(match value {
        ScalarTypedValue::Uint(value) => value.to_string(),
        ScalarTypedValue::Iint(value) => value.to_string(),
        ScalarTypedValue::String(value) if !value.contains(['\t', '\n']) => value.clone(),
        _ => bail!("no fact file field for {value:?}"),
    })
}

/// A field of Soufflé's output as a value of `scalar_type`.
fn value(text: &str, scalar_type: ScalarType) -> Result<ScalarTypedValue> {
    Ok(match scalar_type {
        ScalarType::Uint => ScalarTypedValue::Uint(text.parse()?),
        ScalarType::Iint => ScalarTypedValue::Iint(text.parse()?),
        ScalarType::String => ScalarTypedValue::String(text.to_string()),
        _ => bail!("no value of {scalar_type} from Soufflé"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_keeps_names_apart() {
        assert_eq!(
            escape("root.E.foreignKey#consequent"),
            "root_dE_dforeignKey_sconsequent"
        );
        assert_eq!(escape("trans-closure"), "trans_hclosure");
        assert_ne!(escape("a_d"), escape("a.d"));
    }

    #[test]
    fn the_dialect_renames_names_and_types() -> Result<()> {
        let leo = ".decl root.E(a.RowIdHash: uint, b: string)\n\
                   .output root.E\n\
                   path-to(x.y, z) :- root.E(_, x.y), !no#t(z), z != 3, x.y = \"a.b\".";
        assert_eq!(
            dialect(leo, &HashMap::new())?,
            ".decl root_dE(a_dRowIdHash: unsigned, b: symbol)\n\
             .output root_dE\n\
             path_hto(x_dy, z) :- root_dE(_, x_dy), !no_st(z), z != 3, x_dy = \"a.b\"."
        );
        Ok(())
    }

    #[test]
    fn the_dialect_fills_atoms_up_to_their_width() -> Result<()> {
        let leo = ".decl e(a: uint, b: uint, c: uint)\n\
                   p(x) :- e(x), !e(_, \"a,b\"), e(), q(x).";
        let arities = HashMap::from([("e", 3), ("p", 1)]);
        assert_eq!(
            dialect(leo, &arities)?,
            ".decl e(a: unsigned, b: unsigned, c: unsigned)\n\
             p(x) :- e(x, _, _), !e(_, \"a,b\", _), e(_, _, _), q(x)."
        );
        Ok(())
    }
}
