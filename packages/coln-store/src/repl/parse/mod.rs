use crate::repl::ShellMode;

use anyhow::{Result, bail};
pub use coln::{BatchAssignment, Command as ColnCommand};
pub(crate) use coln::{parse_cell_value, parse_cell_value_batch};
pub(crate) use meta::Command as MetaCommand;
pub(crate) use sql::{Col as SqlCol, Command as SqlCommand};

mod coln;
mod meta;
mod sql;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Coln(ColnCommand),
    Sql(SqlCommand),
    Meta(MetaCommand),
}

pub(crate) fn parse_command(mode: ShellMode, input: &str) -> Result<Command> {
    let input = input.trim();
    if input.starts_with('.') {
        meta::parse_meta_command(input).map(Command::Meta)
    } else {
        let Some(input) = input.strip_suffix(';') else {
            bail!("statements must end with `;`");
        };
        let input = input.trim_end();

        match mode {
            ShellMode::Coln => coln::parse_statement(input).map(Command::Coln),
            ShellMode::Sql => sql::parse_sql_statement(input).map(Command::Sql),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn parses_help() {
        assert_eq!(
            parse_command(ShellMode::Coln, ".help").unwrap(),
            Command::Meta(meta::Command::Help)
        );
    }

    #[test]
    fn parses_load_with_quotes() {
        assert_eq!(
            parse_command(ShellMode::Coln, ".load \"tests/data/paths.json\"").unwrap(),
            Command::Meta(MetaCommand::Load {
                path: "tests/data/paths.json".to_string()
            })
        );
    }

    #[test]
    fn parses_add_command() {
        assert_eq!(
            parse_command(ShellMode::Coln, "add T values (7 \"alice\"), (8 \"bob\");").unwrap(),
            Command::Coln(ColnCommand::Add {
                table: "T".to_string(),
                rows: vec![
                    vec!["7".to_string(), "alice".to_string()],
                    vec!["8".to_string(), "bob".to_string()],
                ],
            })
        );
    }

    #[test]
    fn parses_add_command_single_value() {
        assert_eq!(
            parse_command(ShellMode::Coln, "add T values (#11);").unwrap(),
            Command::Coln(ColnCommand::Add {
                table: "T".to_string(),
                rows: vec![vec!["#11".to_string()],],
            })
        );
    }

    #[test]
    fn rejects_unknown_command() {
        let err = parse_command(ShellMode::Coln, "wat;").unwrap_err();
        assert!(err.to_string().contains("unknown statement"));
    }

    #[test]
    fn rejects_missing_semicolon() {
        let err = parse_command(ShellMode::Coln, "help").unwrap_err();
        assert_eq!(err.to_string(), "statements must end with `;`");
    }

    #[test]
    fn parses_quit_without_semicolon() {
        assert_eq!(
            parse_command(ShellMode::Coln, ".quit").unwrap(),
            Command::Meta(MetaCommand::Exit)
        );
    }

    #[test]
    fn parses_meta_commands_without_semicolons() {
        assert_eq!(
            parse_command(ShellMode::Coln, ".open paths.bin").unwrap(),
            Command::Meta(MetaCommand::Open {
                path: "paths.bin".to_string()
            })
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".save paths.bin").unwrap(),
            Command::Meta(MetaCommand::Save {
                path: "paths.bin".to_string()
            })
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".tables").unwrap(),
            Command::Meta(MetaCommand::Tables)
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".rules").unwrap(),
            Command::Meta(MetaCommand::Rules)
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".schema Path.G.V").unwrap(),
            Command::Meta(MetaCommand::Schema {
                table: Some("Path.G.V".to_string())
            })
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".dump Path.G.V").unwrap(),
            Command::Meta(MetaCommand::Dump {
                table: "Path.G.V".to_string()
            })
        );
    }

    #[test]
    fn parses_batch_empty() {
        assert_eq!(
            parse_command(ShellMode::Coln, "begin transact; commit;").unwrap(),
            Command::Coln(ColnCommand::Batch {
                assignments: vec![]
            })
        );
    }

    #[test]
    fn parses_batch_with_bindings() {
        let cmd = parse_command(
            ShellMode::Coln,
            "begin transact; g = add Graphs values (); x = add G0 values (g); commit;",
        )
        .unwrap();
        assert_eq!(
            cmd,
            Command::Coln(ColnCommand::Batch {
                assignments: vec![
                    BatchAssignment {
                        name: "g".to_string(),
                        table: "Graphs".to_string(),
                        row: vec![],
                    },
                    BatchAssignment {
                        name: "x".to_string(),
                        table: "G0".to_string(),
                        row: vec!["g".to_string()],
                    },
                ]
            })
        );
    }

    #[test]
    fn parses_sql_create_table() {
        let cmd = parse_command(
            ShellMode::Sql,
            "create table Person (name text, age integer);",
        )
        .unwrap();

        let Command::Sql(SqlCommand::CreateTable {
            table_name,
            columns,
        }) = cmd
        else {
            panic!("expected SQL create table");
        };

        assert_eq!(table_name, "Person");
        assert_eq!(columns.len(), 2);
        assert_eq!(columns[0].col_name, "name");
        assert_eq!(columns[0].col_typ, crate::ir::BuiltinTy::BuiltinStr);
        assert_eq!(columns[1].col_name, "age");
        assert_eq!(columns[1].col_typ, crate::ir::BuiltinTy::BuiltinInt);
    }

    #[test]
    fn rejects_batch_without_commit_keyword() {
        let err =
            parse_command(ShellMode::Coln, "begin transact; g = add T values (1);").unwrap_err();
        assert!(
            err.to_string()
                .contains("transaction block must end with `commit`")
                || err.to_string().contains("commit")
        );
    }
}
