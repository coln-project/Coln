// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `coln-check`: do batch, incremental and Soufflé compute the same views of
//! a realm? See [`coln_query::check`].

use std::path::PathBuf;
use std::process::ExitCode;

use coln_query::check::{Options, check_file};

const USAGE: &str = "\
usage: coln-check <realm.json>... [options]

Runs each realm's views on batch, incremental and Soufflé (if installed)
over random rows, and reports the first case in which they differ.
A realm may also be one of the compiler's test realms by name, e.g.
TransitiveClosureRealm.

options:
  --seeds <n>     number of cases, seeds 0 to n-1 (default 20)
  --seed <n>      run only the case with seed n
  --commits <n>   commits per case (default 6)
  --max-rows <n>  most rows a commit inserts per table (default 3)
  --no-souffle    compare batch and incremental only";

fn main() -> ExitCode {
    let (paths, options) = match parse(std::env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut agree = true;
    for path in &paths {
        match check_file(path, &options) {
            Ok(report) => {
                println!("{report}\n");
                agree &= report.agrees();
            }
            Err(error) => {
                eprintln!("{}: {error:#}\n", path.display());
                return ExitCode::from(2);
            }
        }
    }
    match agree {
        true => ExitCode::SUCCESS,
        false => ExitCode::FAILURE,
    }
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<(Vec<PathBuf>, Options), String> {
    let mut options = Options::default();
    let mut paths = Vec::new();
    let number = |flag: &str, value: Option<String>| -> Result<u64, String> {
        value
            .ok_or_else(|| format!("{flag} needs a number"))?
            .parse()
            .map_err(|_| format!("{flag} needs a number"))
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seeds" => options.seeds = (0..number(&arg, args.next())?).collect(),
            "--seed" => options.seeds = vec![number(&arg, args.next())?],
            "--commits" => options.commits = number(&arg, args.next())? as usize,
            "--max-rows" => options.max_rows = number(&arg, args.next())?,
            "--no-souffle" => options.souffle = false,
            "-h" | "--help" => return Err("coln-check".to_string()),
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            path => paths.push(PathBuf::from(path)),
        }
    }
    if paths.is_empty() {
        return Err("no realm given".to_string());
    }
    Ok((paths, options))
}
