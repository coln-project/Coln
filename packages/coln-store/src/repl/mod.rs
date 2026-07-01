// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};
use tracing::{info, warn};

use crate::commit::pst::encode_store;
use crate::ir::Path;
use crate::repl::error::ReplError;
use crate::repl::exe::{
    LoadedState, add_rows, load_schema, load_store, render_rules, render_schema_summary,
    render_table_schema, run_transact,
};
use crate::repl::parse::parse_command;
use crate::repl::parse::{ColnCommand, Command, MetaCommand, SqlCommand};

pub mod error;
pub mod exe;
pub mod parse;

const COMMANDS: &[&str] = &[
    ".help", ".exit", ".quit", ".load", ".open", ".save", ".tables", ".rules", ".schema", ".dump",
    "add", "begin",
];

const SECRET_MODE: &str = "ILOVESQL";

#[derive(Debug, Default, PartialEq, Eq, Copy, Clone)]
pub enum ShellMode {
    #[default]
    Coln,
    Sql,
}

#[derive(Default)]
struct Session {
    loaded: Option<LoadedState>,
    shell_mode: ShellMode,
}

#[derive(Debug)]
enum Step {
    Continue(String),
    Exit,
}

struct CommandHelper;

fn is_statement_start(input: &str) -> bool {
    !input.trim_start().starts_with('.')
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut editor = Editor::<CommandHelper, DefaultHistory>::new()?;
    editor.set_helper(Some(CommandHelper));
    let mut session = Session::default();
    let mut pending_statement: Option<String> = None;

    println!("coln-store repl");
    println!("Type .help for commands.");

    loop {
        let prompt = if pending_statement.is_some() {
            "....> "
        } else if session.shell_mode == ShellMode::Sql {
            "coln-sql>"
        } else {
            "coln-store> "
        };

        let line = match editor.readline(prompt) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                if pending_statement.is_some() {
                    pending_statement = None;
                    println!("cancelled pending statement");
                } else {
                    println!("Use `.exit` to quit.");
                }
                continue;
            }
            Err(ReadlineError::Eof) => break,
            Err(err) => return Err(err.into()),
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if pending_statement.is_none()
            && trimmed == SECRET_MODE
            && session.shell_mode == ShellMode::Coln
        {
            session.shell_mode = ShellMode::Sql;
            println!("Welcome to SQL mode!");
            continue;
        }

        let maybe_command = if pending_statement.is_some() || is_statement_start(trimmed) {
            let command = push_statement_line(&mut pending_statement, trimmed);
            if let Some(command) = command {
                let _ = editor.add_history_entry(command.as_str());
                Some(command)
            } else {
                continue;
            }
        } else {
            let _ = editor.add_history_entry(trimmed);
            Some(trimmed.to_string())
        };

        match parse_command(session.shell_mode, &maybe_command.expect("command")) {
            Ok(command) => match execute(&mut session, command) {
                Ok(Step::Continue(message)) => println!("{message}"),
                Ok(Step::Exit) => break,
                Err(err) => {
                    warn!(error = %err, "repl command failed");
                    eprintln!("error: {err}");
                }
            },
            Err(err) => eprintln!("error: {err}"),
        }
    }

    Ok(())
}

fn push_statement_line(pending: &mut Option<String>, line: &str) -> Option<String> {
    let pending_line = pending.get_or_insert_with(String::new);
    if !pending_line.is_empty() {
        pending_line.push(' ');
    }
    pending_line.push_str(line.trim());

    let buf = pending_line.trim_start();
    if buf.starts_with("begin transact") {
        if pending_line.trim_end().ends_with("commit;") {
            return pending.take();
        }
        return None;
    }

    if pending_line.trim_end().ends_with(';') {
        pending.take()
    } else {
        None
    }
}

impl Helper for CommandHelper {}

impl Hinter for CommandHelper {
    type Hint = String;
}

impl Highlighter for CommandHelper {}

impl Validator for CommandHelper {}

impl Completer for CommandHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let (start, candidates) = complete_command(line, pos);
        Ok((start, candidates))
    }
}

fn execute(session: &mut Session, command: Command) -> Result<Step, ReplError> {
    match command {
        Command::MetaCommand(MetaCommand::Help) => Ok(Step::Continue(help_text())),
        Command::MetaCommand(MetaCommand::Load { path }) => {
            let loaded = load_schema(std::path::Path::new(&path))?;
            info!(
                source = %loaded.schema.source.display(),
                table_count = loaded.store.table_count(),
                law_count = loaded.schema.law_count,
                "loaded schema"
            );
            let message = format!(
                "loaded schema from {} (tables: {}, laws: {})",
                loaded.schema.source.display(),
                loaded.store.table_count(),
                loaded.schema.law_count
            );
            session.loaded = Some(loaded);
            Ok(Step::Continue(message))
        }
        Command::MetaCommand(MetaCommand::Open { path }) => {
            let loaded = load_store(std::path::Path::new(&path))?;
            info!(
                source = %loaded.schema.source.display(),
                table_count = loaded.store.table_count(),
                law_count = loaded.schema.law_count,
                "loaded store"
            );
            let message = format!(
                "loaded store from {} (tables: {}, laws: {})",
                loaded.schema.source.display(),
                loaded.store.table_count(),
                loaded.schema.law_count
            );
            session.loaded = Some(loaded);
            Ok(Step::Continue(message))
        }
        Command::MetaCommand(MetaCommand::Schema { table }) => {
            let schema = session.loaded.as_ref().map(|loaded| &loaded.schema);
            let message = match table {
                Some(table) => render_table_schema(schema, &table)?,
                None => render_schema_summary(schema),
            };
            Ok(Step::Continue(message))
        }
        Command::MetaCommand(MetaCommand::Rules) => {
            let store = session.loaded.as_ref().map(|loaded| &loaded.store);
            Ok(Step::Continue(render_rules(store)?))
        }
        Command::ColnCommand(ColnCommand::Add { table, rows }) => {
            let loaded = session.loaded.as_mut().ok_or(ReplError::NoSchemaLoaded)?;
            let row_ids = add_rows(&mut loaded.store, &table, &rows)?;
            let row_ids = row_ids
                .into_iter()
                .map(|row_id| format!("#{row_id}"))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(Step::Continue(format!(
                "inserted into {table} rows [{row_ids}]"
            )))
        }
        Command::ColnCommand(ColnCommand::Batch { assignments }) => {
            let loaded = session.loaded.as_mut().ok_or(ReplError::NoSchemaLoaded)?;
            let message = run_transact(&mut loaded.store, &assignments)?;
            Ok(Step::Continue(message))
        }
        Command::MetaCommand(MetaCommand::Dump { table }) => {
            let loaded = session.loaded.as_mut().ok_or(ReplError::NoSchemaLoaded)?;
            let tbl = loaded
                .store
                .table_at(&Path::from(table.clone()))
                .ok_or(ReplError::UnknownTable(table))?;
            let message = tbl.dump();
            Ok(Step::Continue(message))
        }
        Command::MetaCommand(MetaCommand::Tables) => {
            let loaded = session.loaded.as_mut().ok_or(ReplError::NoSchemaLoaded)?;
            let message = loaded.store.dump();
            Ok(Step::Continue(message))
        }
        Command::MetaCommand(MetaCommand::Save { path }) => {
            let loaded = session.loaded.as_mut().ok_or(ReplError::NoSchemaLoaded)?;
            let bytes = encode_store(&loaded.store)?;
            std::fs::write(&path, &bytes)?;
            Ok(Step::Continue(format!("saved store to {path}")))
        }
        Command::MetaCommand(MetaCommand::Exit) => Ok(Step::Exit),
        Command::SqlCommand(SqlCommand::CreateTable) => Ok(Step::Continue(
            "SQL CREATE TABLE is not implemented yet".to_string(),
        )),
    }
}

fn help_text() -> String {
    [
        "Commands:",
        "  .help",
        "  .exit",
        "  .quit",
        "  .load <schema-json-path>",
        "  .open <store-path>",
        "  .save <store-path>",
        "  .tables",
        "  .rules",
        "  .schema [table]",
        "  .dump <table>",
        "  add <table> values (...), (...);",
        "  begin transact; name = add <table> values (...); ... commit;",
        "",
        "Examples:",
        "  .help",
        "  .load tests/data/paths.json",
        "  .schema",
        "  .rules",
        "  .dump T",
        "  add T values (7 \"alice\"), (8 \"bob\");",
        "  begin transact; g = add Graphs values (); e = add G0 values (g); commit;",
    ]
    .join("\n")
}

fn complete_command(line: &str, pos: usize) -> (usize, Vec<Pair>) {
    let prefix = &line[..pos];
    if prefix.split_whitespace().count() > 1 || prefix.contains(' ') {
        return (0, Vec::new());
    }

    let matches = COMMANDS
        .iter()
        .filter(|command| command.starts_with(prefix))
        .map(|command| Pair {
            display: (*command).to_string(),
            replacement: (*command).to_string(),
        })
        .collect();

    (0, matches)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{
        ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant},
        repl::{
            exe::{PrimaryKeySummary, SchemaSummary, TableSummary},
            parse::{ColnCommand, Command},
        },
        store::Store,
    };

    use super::*;

    fn test_loaded_state() -> LoadedState {
        use crate::ir::{Path as IrPath, Schema};
        use crate::table::Table;

        let path = IrPath::from("T");
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![
                ColumnEntry {
                    path: IrPath::from("c0"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                },
                ColumnEntry {
                    path: IrPath::from("c1"),
                    col_type: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinStr,
                    },
                },
            ],
            primary_key: None,
        };

        let mut store = Store::new();
        store.insert_table(path.clone(), Table::new(path.clone(), schema));

        LoadedState {
            store,
            schema: SchemaSummary {
                source: PathBuf::from("test.json"),
                table_count: 1,
                law_count: 0,
                tables: vec![TableSummary {
                    path: "T".to_string(),
                    column_count: 2,
                    primary_key: PrimaryKeySummary::None,
                    columns: vec!["c0: int".to_string(), "c1: string".to_string()],
                }],
            },
        }
    }

    #[test]
    fn add_inserts_rows_into_loaded_store() {
        let mut session = Session {
            loaded: Some(test_loaded_state()),
            shell_mode: ShellMode::Coln,
        };

        let message = match execute(
            &mut session,
            Command::ColnCommand(ColnCommand::Add {
                table: "T".to_string(),
                rows: vec![
                    vec!["7".to_string(), "alice".to_string()],
                    vec!["8".to_string(), "bob".to_string()],
                ],
            }),
        )
        .expect("execute add")
        {
            Step::Continue(message) => message,
            Step::Exit => panic!("unexpected exit"),
        };

        assert!(message.starts_with("inserted into T rows [#"));
        assert!(message.contains(":0, #"));
        assert!(message.ends_with(":1]"));
        let loaded = session.loaded.as_ref().expect("loaded session");
        assert_eq!(
            loaded
                .store
                .table_at(&"T".parse().unwrap())
                .unwrap()
                .row_count(),
            2
        );
    }

    #[test]
    fn add_requires_loaded_schema() {
        let err = execute(
            &mut Session::default(),
            Command::ColnCommand(ColnCommand::Add {
                table: "T".to_string(),
                rows: vec![vec!["7".to_string()]],
            }),
        )
        .unwrap_err();

        assert_eq!(err.to_string(), "no schema loaded");
    }

    #[test]
    fn add_rejects_bad_entity_id() {
        let mut store = Store::new();
        let path: crate::ir::Path = "Ref".parse().unwrap();
        store.insert_table(
            path.clone(),
            crate::table::Table::new(
                path,
                crate::ir::Schema {
                    entity_variant: EntityVariant::Table,
                    columns: vec![ColumnEntry {
                        path: "ref".parse().unwrap(),
                        col_type: ColType::RowId {
                            path: "T".parse().unwrap(),
                        },
                    }],
                    primary_key: None,
                },
            ),
        );

        let err = add_rows(&mut store, "Ref", &[vec!["7".to_string()]]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "column 0: expected entity id like #<commit>:<counter>"
        );
    }

    #[test]
    fn completes_command_prefix() {
        let (start, matches) = complete_command(".q", 2);
        assert_eq!(start, 0);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].replacement, ".quit");
    }

    #[test]
    fn does_not_complete_after_first_argument() {
        let (_, matches) = complete_command(".load te", ".load te".len());
        assert!(matches.is_empty());
    }

    #[test]
    fn statement_buffer_waits_for_semicolon() {
        let mut pending = None;
        assert_eq!(push_statement_line(&mut pending, "add T 7"), None);
        assert_eq!(pending.as_deref(), Some("add T 7"));
        assert_eq!(
            push_statement_line(&mut pending, "\"alice\";"),
            Some("add T 7 \"alice\";".to_string())
        );
        assert_eq!(pending, None);
    }

    #[test]
    fn batch_buffer_waits_for_commit_semicolon() {
        let mut pending = None;
        assert_eq!(push_statement_line(&mut pending, "begin transact;"), None);
        assert_eq!(
            push_statement_line(&mut pending, "x = add T values (1);"),
            None
        );
        assert_eq!(
            push_statement_line(&mut pending, "commit;"),
            Some("begin transact; x = add T values (1); commit;".to_string())
        );
        assert_eq!(pending, None);
    }

    #[test]
    fn meta_command_is_not_statement_start() {
        assert!(!is_statement_start(".help"));
        assert!(is_statement_start("add T 7"));
    }
}
