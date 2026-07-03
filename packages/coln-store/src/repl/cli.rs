use std::path::PathBuf;

use anyhow::Result;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Helper};

use crate::repl::ShellMode;

const COMMANDS: &[&str] = &[
    ".help", ".exit", ".quit", ".load", ".open", ".save", ".tables", ".rules", ".schema", ".dump",
    "add", "begin",
];

const PATH_COMMANDS: &[&str] = &[".load", ".open", ".save"];

const HISTORY_FILE: &str = ".coln-store_history";

pub(super) fn history_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(HISTORY_FILE)
}

pub(super) struct CommandHelper {
    filename_completer: FilenameCompleter,
}

impl CommandHelper {
    pub(super) fn new() -> Self {
        Self {
            filename_completer: FilenameCompleter::new(),
        }
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
        ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        if wants_path_completion(line, pos) {
            return self.filename_completer.complete(line, pos, ctx);
        }
        let (start, candidates) = complete_command(line, pos);
        Ok((start, candidates))
    }
}

fn wants_path_completion(line: &str, pos: usize) -> bool {
    let prefix = &line[..pos];
    let Some(command) = prefix.split_whitespace().next() else {
        return false;
    };
    PATH_COMMANDS.contains(&command) && prefix.len() > command.len()
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

pub(super) fn prompt(shell_mode: ShellMode, pending_statement: bool) -> &'static str {
    if pending_statement {
        "....> "
    } else if shell_mode == ShellMode::Sql {
        "coln-sql> "
    } else {
        "coln-store> "
    }
}

pub(super) fn is_statement_start(input: &str) -> bool {
    !input.trim_start().starts_with('.')
}

pub(super) fn push_statement_line(pending: &mut Option<String>, line: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn path_completion_applies_after_path_command() {
        assert!(wants_path_completion(".load ", ".load ".len()));
        assert!(wants_path_completion(
            ".open tests/da",
            ".open tests/da".len()
        ));
        assert!(wants_path_completion(
            ".save out.bin",
            ".save out.bin".len()
        ));
    }

    #[test]
    fn path_completion_does_not_apply_elsewhere() {
        assert!(!wants_path_completion(".load", ".load".len()));
        assert!(!wants_path_completion(".dump te", ".dump te".len()));
        assert!(!wants_path_completion("add T 7", "add T 7".len()));
        assert!(!wants_path_completion("", 0));
    }

    #[test]
    fn path_completion_lists_matching_files() {
        let helper = CommandHelper::new();
        let history = rustyline::history::DefaultHistory::new();
        let ctx = Context::new(&history);
        let line = ".load tests/data/people.c";
        let (start, matches) = helper.complete(line, line.len(), &ctx).unwrap();
        assert_eq!(start, ".load ".len());
        assert!(
            matches
                .iter()
                .any(|pair| pair.replacement == "tests/data/people.csv"),
            "expected people.csv in completions, got: {:?}",
            matches.iter().map(|p| &p.replacement).collect::<Vec<_>>()
        );
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

    #[test]
    fn prompt_reflects_mode_and_pending_statement() {
        assert_eq!(prompt(ShellMode::Coln, false), "coln-store> ");
        assert_eq!(prompt(ShellMode::Sql, false), "coln-sql> ");
        assert_eq!(prompt(ShellMode::Sql, true), "....> ");
    }
}
