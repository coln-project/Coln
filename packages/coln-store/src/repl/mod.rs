// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

mod cli;
pub mod exe;
pub mod parse;

use anyhow::{Result, bail};
use rustyline::Editor;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use tracing::warn;

use crate::repl::cli::{
    CommandHelper, history_path, is_statement_start, prompt, push_statement_line,
};
use crate::repl::exe::{LoadedState, execute_coln, execute_meta, execute_sql};
use crate::repl::parse::Command;
use crate::repl::parse::parse_command;

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

impl Session {
    fn new(mode: ShellMode) -> Self {
        Self {
            loaded: None,
            shell_mode: mode,
        }
    }

    fn with_flags(enable_sql: bool) -> Self {
        if enable_sql {
            Self::new(ShellMode::Sql)
        } else {
            Self::new(ShellMode::Coln)
        }
    }
}

#[derive(Debug)]
enum Step {
    Continue(String),
    Exit,
}

/// Run `-c` / `--command` strings non-interactively and exit.
///
/// Each string is split on newlines and fed through the same statement buffer as
/// the interactive REPL. Pending statements may span multiple `-c` arguments.
/// Stops on the first parse or execution error.
pub fn run_commands(enable_sql: bool, commands: &[String]) -> Result<()> {
    let mut session = Session::with_flags(enable_sql);
    run_commands_into(&mut session, commands)
}

fn run_commands_into(session: &mut Session, commands: &[String]) -> Result<()> {
    let mut pending: Option<String> = None;

    for command_arg in commands {
        for line in command_arg.lines() {
            if !process_line(session, &mut pending, line)? {
                return Ok(());
            }
        }
    }

    if pending.is_some() {
        bail!("incomplete statement: missing `;` or `commit;`");
    }
    Ok(())
}

/// Parse and execute one completed command string.
fn dispatch(session: &mut Session, command_src: &str) -> Result<Step> {
    let command = parse_command(session.shell_mode, command_src)?;
    execute(session, command)
}

/// Feed one input line through the statement buffer. Returns `Ok(false)` on `.exit` / `.quit`.
fn process_line(session: &mut Session, pending: &mut Option<String>, line: &str) -> Result<bool> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(true);
    }

    let command_src = if pending.is_some() || is_statement_start(trimmed) {
        match push_statement_line(pending, trimmed) {
            Some(command) => command,
            None => return Ok(true),
        }
    } else {
        trimmed.to_string()
    };

    match dispatch(session, &command_src)? {
        Step::Continue(message) => {
            println!("{message}");
            Ok(true)
        }
        Step::Exit => Ok(false),
    }
}

pub fn run(enable_sql: bool) -> Result<()> {
    let mut editor = Editor::<CommandHelper, DefaultHistory>::new()?;
    editor.set_helper(Some(CommandHelper::new()));
    let _ = editor.load_history(&history_path());

    let mut session = Session::with_flags(enable_sql);
    let mut pending_statement: Option<String> = None;

    println!("coln-store repl");
    println!("Type .help for commands.");

    loop {
        let prompt = prompt(session.shell_mode, pending_statement.is_some());

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
            println!("Welcome to SQL mode! EXPERIMENTAL ONLY. ");
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

        match dispatch(&mut session, &maybe_command.expect("command")) {
            Ok(Step::Continue(message)) => println!("{message}"),
            Ok(Step::Exit) => break,
            Err(err) => {
                warn!(error = %err, "repl command failed");
                eprintln!("error: {err:#}");
            }
        }
    }

    let _ = editor.append_history(&history_path());
    Ok(())
}

fn execute(session: &mut Session, command: Command) -> Result<Step> {
    match command {
        Command::Meta(command) => execute_meta(session, command),
        Command::Coln(command) => Ok(Step::Continue(execute_coln(session, command)?)),
        Command::Sql(command) => Ok(Step::Continue(execute_sql(session, command)?)),
    }
}
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{
        ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant},
        repl::{
            exe::add_rows,
            exe::{PrimaryKeySummary, SchemaSummary, TableSummary},
            parse::{ColnCommand, Command, SqlCol, SqlCommand},
        },
        store::Store,
    };

    fn test_loaded_state() -> LoadedState {
        use crate::ir::{Path as IrPath, Schema};

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
        store
            .create_table(path.clone(), schema)
            .expect("create test table");

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
            Command::Coln(ColnCommand::Add {
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
            Command::Coln(ColnCommand::Add {
                table: "T".to_string(),
                rows: vec![vec!["7".to_string()]],
            }),
        )
        .unwrap_err();

        assert_eq!(err.to_string(), "no schema loaded");
    }

    fn sql_create_table_command(table_name: &str) -> Command {
        Command::Sql(SqlCommand::CreateTable {
            table_name: table_name.to_string(),
            columns: vec![
                SqlCol {
                    col_name: "name".to_string(),
                    col_typ: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinStr,
                    },
                },
                SqlCol {
                    col_name: "age".to_string(),
                    col_typ: ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                },
            ],
        })
    }

    #[test]
    fn sql_create_table_registers_schema() {
        let mut session = Session {
            loaded: None,
            shell_mode: ShellMode::Sql,
        };

        let message = match execute(&mut session, sql_create_table_command("Person"))
            .expect("execute create table")
        {
            Step::Continue(message) => message,
            Step::Exit => panic!("unexpected exit"),
        };

        assert_eq!(message, "created table Person");
        let loaded = session.loaded.as_ref().expect("sql store loaded");
        assert!(loaded.store.table_at(&"Person".parse().unwrap()).is_some());
        assert_eq!(loaded.schema.table_count, 1);
        assert_eq!(loaded.schema.tables[0].path, "Person");
        assert_eq!(
            loaded.schema.tables[0].columns,
            vec!["name: string".to_string(), "age: int".to_string()]
        );
    }

    #[test]
    fn sql_create_table_rejects_duplicate_name() {
        let mut session = Session {
            loaded: None,
            shell_mode: ShellMode::Sql,
        };

        execute(&mut session, sql_create_table_command("Person")).expect("first create");
        let err = execute(&mut session, sql_create_table_command("Person")).unwrap_err();

        assert_eq!(err.to_string(), "table already exists: Person");
    }

    #[test]
    fn sql_create_table_rejects_schema_change_after_data_commit() {
        let mut session = Session {
            loaded: None,
            shell_mode: ShellMode::Sql,
        };

        execute(&mut session, sql_create_table_command("Person")).expect("create table");
        execute(
            &mut session,
            Command::Coln(ColnCommand::Add {
                table: "Person".to_string(),
                rows: vec![vec!["alice".to_string(), "7".to_string()]],
            }),
        )
        .expect("insert row");

        let err = execute(&mut session, sql_create_table_command("Other")).unwrap_err();
        assert_eq!(
            err.to_string(),
            "cannot create table after data commits have been recorded"
        );
    }

    #[test]
    fn sql_copy_from_csv_inserts_rows() {
        let mut session = Session {
            loaded: None,
            shell_mode: ShellMode::Sql,
        };
        execute(&mut session, sql_create_table_command("Person")).expect("create table");

        let message = match execute(
            &mut session,
            Command::Sql(SqlCommand::CopyFromCsv {
                table_name: "Person".to_string(),
                path: "tests/data/people.csv".to_string(),
                delimiter: b',',
            }),
        )
        .expect("copy csv")
        {
            Step::Continue(message) => message,
            Step::Exit => panic!("unexpected exit"),
        };

        assert_eq!(message, "copied 2 rows into Person");
        let loaded = session.loaded.as_ref().expect("loaded session");
        let table = loaded
            .store
            .table_at(&"Person".parse().unwrap())
            .expect("Person table");
        assert_eq!(table.row_count(), 2);
        // The fixture header order (age, name) differs from the schema order.
        let dump = table.dump();
        assert!(dump.contains("alice"));
        assert!(dump.contains("30"));
    }

    #[test]
    fn add_rejects_bad_entity_id() {
        let mut store = Store::new();
        let path: crate::ir::Path = "Ref".parse().unwrap();
        store
            .create_table(
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
            )
            .expect("create test table");

        let err = add_rows(&mut store, "Ref", &[vec!["7".to_string()]]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "column 0: invalid input value expected entity id like #<commit>:<counter>"
        );
    }

    #[test]
    fn run_commands_executes_sql_sequence() {
        let mut session = Session::new(ShellMode::Sql);
        run_commands_into(
            &mut session,
            &[
                "create table Person (name text, age integer);".to_string(),
                "copy Person from 'tests/data/people.csv' with (format csv, header true);"
                    .to_string(),
            ],
        )
        .expect("run commands");

        let loaded = session.loaded.as_ref().expect("loaded session");
        let table = loaded
            .store
            .table_at(&"Person".parse().unwrap())
            .expect("Person table");
        assert_eq!(table.row_count(), 2);
    }

    #[test]
    fn run_commands_multiline_string() {
        let mut session = Session::new(ShellMode::Sql);
        run_commands_into(
            &mut session,
            &["create table Person (name text, age integer);\n.tables".to_string()],
        )
        .expect("run multiline command");

        assert!(session.loaded.is_some());
        assert_eq!(session.loaded.as_ref().unwrap().schema.table_count, 1);
    }

    #[test]
    fn run_commands_pending_spans_arguments() {
        let mut session = Session {
            loaded: Some(test_loaded_state()),
            shell_mode: ShellMode::Coln,
        };
        run_commands_into(
            &mut session,
            &[
                "begin transact;".to_string(),
                "x = add T values (1 \"a\");".to_string(),
                "commit;".to_string(),
            ],
        )
        .expect("spanned batch");

        let loaded = session.loaded.as_ref().expect("loaded session");
        assert_eq!(
            loaded
                .store
                .table_at(&"T".parse().unwrap())
                .unwrap()
                .row_count(),
            1
        );
    }

    #[test]
    fn run_commands_rejects_incomplete_statement() {
        let mut session = Session {
            loaded: Some(test_loaded_state()),
            shell_mode: ShellMode::Coln,
        };
        let err =
            run_commands_into(&mut session, &["add T values (1 \"a\")".to_string()]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "incomplete statement: missing `;` or `commit;`"
        );
    }

    #[test]
    fn run_commands_stops_on_first_error() {
        let mut session = Session::new(ShellMode::Sql);
        let err = run_commands_into(
            &mut session,
            &[
                "create table Person (name text, age integer);".to_string(),
                "not a valid statement;".to_string(),
                "create table Other (name text);".to_string(),
            ],
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("failed to parse sql") || err.to_string().contains("parse")
        );
        let loaded = session
            .loaded
            .as_ref()
            .expect("first create should have succeeded");
        assert_eq!(loaded.schema.table_count, 1);
        assert!(loaded.store.table_at(&"Other".parse().unwrap()).is_none());
    }

    #[test]
    fn run_commands_exit_stops_early() {
        let mut session = Session::new(ShellMode::Sql);
        run_commands_into(
            &mut session,
            &[
                "create table Person (name text, age integer);".to_string(),
                ".exit".to_string(),
                "create table Other (name text);".to_string(),
            ],
        )
        .expect("exit is success");

        let loaded = session.loaded.as_ref().expect("loaded");
        assert_eq!(loaded.schema.table_count, 1);
        assert!(loaded.store.table_at(&"Other".parse().unwrap()).is_none());
    }
}
