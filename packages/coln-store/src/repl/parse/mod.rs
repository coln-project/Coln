use crate::repl::ShellMode;

mod coln;
mod meta;
mod sql;

pub use coln::{BatchAssignment, Command as ColnCommand};
pub(crate) use coln::{parse_cell_value, parse_cell_value_batch};
pub(crate) use meta::Command as MetaCommand;
pub(crate) use sql::Command as SqlCommand;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    ColnCommand(ColnCommand),
    SqlCommand(SqlCommand),
    MetaCommand(MetaCommand),
}

pub(crate) fn parse_command(mode: ShellMode, input: &str) -> Result<Command, String> {
    let input = input.trim();
    if input.starts_with('.') {
        meta::parse_meta_command(input).map(|mc| Command::MetaCommand(mc))
    } else {
        let Some(input) = input.strip_suffix(';') else {
            return Err("statements must end with `;`".to_string());
        };
        let input = input.trim_end();

        match mode {
            ShellMode::Coln => coln::parse_statement(input).map(|cc| Command::ColnCommand(cc)),
            ShellMode::Sql => sql::parse_sql_statement(input).map(|sc| Command::SqlCommand(sc)),
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
            Command::MetaCommand(meta::Command::Help)
        );
    }

    #[test]
    fn parses_load_with_quotes() {
        assert_eq!(
            parse_command(ShellMode::Coln, ".load \"tests/data/paths.json\"").unwrap(),
            Command::MetaCommand(MetaCommand::Load {
                path: "tests/data/paths.json".to_string()
            })
        );
    }

    #[test]
    fn parses_add_command() {
        assert_eq!(
            parse_command(ShellMode::Coln, "add T values (7 \"alice\"), (8 \"bob\");").unwrap(),
            Command::ColnCommand(ColnCommand::Add {
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
            Command::ColnCommand(ColnCommand::Add {
                table: "T".to_string(),
                rows: vec![vec!["#11".to_string()],],
            })
        );
    }

    #[test]
    fn rejects_unknown_command() {
        let err = parse_command(ShellMode::Coln, "wat;").unwrap_err();
        assert!(err.contains("unknown statement"));
    }

    #[test]
    fn rejects_missing_semicolon() {
        let err = parse_command(ShellMode::Coln, "help").unwrap_err();
        assert_eq!(err, "statements must end with `;`");
    }

    #[test]
    fn parses_quit_without_semicolon() {
        assert_eq!(
            parse_command(ShellMode::Coln, ".quit").unwrap(),
            Command::MetaCommand(MetaCommand::Exit)
        );
    }

    #[test]
    fn parses_meta_commands_without_semicolons() {
        assert_eq!(
            parse_command(ShellMode::Coln, ".open paths.bin").unwrap(),
            Command::MetaCommand(MetaCommand::Open {
                path: "paths.bin".to_string()
            })
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".save paths.bin").unwrap(),
            Command::MetaCommand(MetaCommand::Save {
                path: "paths.bin".to_string()
            })
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".tables").unwrap(),
            Command::MetaCommand(MetaCommand::Tables)
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".rules").unwrap(),
            Command::MetaCommand(MetaCommand::Rules)
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".schema Path.G.V").unwrap(),
            Command::MetaCommand(MetaCommand::Schema {
                table: Some("Path.G.V".to_string())
            })
        );
        assert_eq!(
            parse_command(ShellMode::Coln, ".dump Path.G.V").unwrap(),
            Command::MetaCommand(MetaCommand::Dump {
                table: "Path.G.V".to_string()
            })
        );
    }

    #[test]
    fn parses_batch_empty() {
        assert_eq!(
            parse_command(ShellMode::Coln, "begin transact; commit;").unwrap(),
            Command::ColnCommand(ColnCommand::Batch {
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
            Command::ColnCommand(ColnCommand::Batch {
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
    fn rejects_batch_without_commit_keyword() {
        let err =
            parse_command(ShellMode::Coln, "begin transact; g = add T values (1);").unwrap_err();
        assert!(err.contains("transaction block must end with `commit`") || err.contains("commit"));
    }
}
