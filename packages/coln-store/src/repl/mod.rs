// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use anyhow::Result;
use rustyline::Editor;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use tracing::warn;

use crate::repl::cli::{CommandHelper, is_statement_start, prompt, push_statement_line};
use crate::repl::exe::{LoadedState, execute_coln, execute_meta, execute_sql};
use crate::repl::parse::Command;
use crate::repl::parse::parse_command;

mod cli;
pub mod error;
pub mod exe;
pub mod parse;

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

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut editor = Editor::<CommandHelper, DefaultHistory>::new()?;
    editor.set_helper(Some(CommandHelper));
    let mut session = Session::default();
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

    use crate::{
        ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant},
        repl::{
            exe::add_rows,
            exe::{PrimaryKeySummary, SchemaSummary, TableSummary},
            parse::{ColnCommand, Command, SqlCol, SqlCommand},
        },
        store::Store,
    };

    use super::*;

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
                    col_typ: BuiltinTy::BuiltinStr,
                },
                SqlCol {
                    col_name: "age".to_string(),
                    col_typ: BuiltinTy::BuiltinInt,
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
            "column 0: expected entity id like #<commit>:<counter>"
        );
    }
}
