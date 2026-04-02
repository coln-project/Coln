use crate::{
    ir::{ColType, PrimType},
    table::CellValue,
};

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    LoadSchema {
        path: String,
    },
    ListSchema,
    Add {
        table: String,
        rows: Vec<Vec<String>>,
    },
    Exit,
}

pub fn parse_add_statement(input: &str) -> Result<Command, String> {
    let Some(rest) = input.strip_prefix("add ") else {
        return Err("usage: add <table> values (...), (...);".to_string());
    };
    let Some((table, rows_src)) = rest.split_once(" values ") else {
        return Err("usage: add <table> values (...), (...);".to_string());
    };
    let table = table.trim();
    if table.is_empty() {
        return Err("usage: add <table> values (...), (...);".to_string());
    }
    let rows = parse_add_rows(rows_src.trim())?;
    if rows.is_empty() {
        return Err("add requires at least one row".to_string());
    }
    Ok(Command::Add {
        table: table.to_string(),
        rows,
    })
}

pub fn parse_command(input: &str) -> Result<Command, String> {
    let input = input.trim();
    if input.starts_with('/') {
        parse_meta_command(input)
    } else {
        let Some(input) = input.strip_suffix(';') else {
            return Err("statements must end with `;`".to_string());
        };
        let input = input.trim_end();
        parse_statement(input)
    }
}

pub fn parse_meta_command(input: &str) -> Result<Command, String> {
    let parts = shlex::split(input).ok_or_else(|| "could not parse input".to_string())?;
    let Some(command) = parts.first() else {
        return Err("empty command".to_string());
    };

    match command.as_str() {
        "/help" => {
            if parts.len() == 1 {
                Ok(Command::Help)
            } else {
                Err("usage: /help".to_string())
            }
        }
        "/exit" | "/quit" => {
            if parts.len() == 1 {
                Ok(Command::Exit)
            } else {
                Err(format!("usage: {command}"))
            }
        }
        _ => Err(format!(
            "unknown meta command: {command}. Type `/help` for commands."
        )),
    }
}

pub fn parse_statement(input: &str) -> Result<Command, String> {
    let parts = shlex::split(input).ok_or_else(|| "could not parse input".to_string())?;
    let Some(command) = parts.first() else {
        return Err("empty command".to_string());
    };

    match command.as_str() {
        "load-schema" => match parts.as_slice() {
            [_, path] => Ok(Command::LoadSchema { path: path.clone() }),
            _ => Err("usage: load-schema <path>;".to_string()),
        },
        "list-schema" => {
            if parts.len() == 1 {
                Ok(Command::ListSchema)
            } else {
                Err("usage: list-schema;".to_string())
            }
        }
        "add" => parse_add_statement(input),
        _ => Err(format!(
            "unknown statement: {command}. Statements must end with `;`, or use `/help` for meta commands."
        )),
    }
}

/// Split `values ( ... ), ( ... )` into row bodies between outer parentheses.
///
/// We only need to find the matching `)` for each `(`. Inside double-quoted strings, `)` and `,`
/// are ignored so they do not end the row. We do not support backslash escapes; `"` delimits
/// quoted strings.
///
/// Row *values* are split with [`split_add_row_tokens`], not `shlex::split`: Unix shell rules
/// treat `#` as starting a comment, which would drop entity ids like `#11`.
///
/// Regex is a poor fit here: a pattern like `\(([^)]*)\)` breaks when a string column contains
/// `)`.
pub fn parse_add_rows(input: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some((_, ch)) = chars.peek().copied() {
        if ch.is_whitespace() || ch == ',' {
            chars.next();
            continue;
        }
        if ch != '(' {
            return Err("expected `(` to start a row".to_string());
        }

        chars.next();
        let start = chars.peek().map(|(idx, _)| *idx).unwrap_or(input.len());
        let mut in_quotes = false;
        let mut end = None;

        for (idx, ch) in chars.by_ref() {
            if in_quotes {
                if ch == '"' {
                    in_quotes = false;
                }
                continue;
            }

            match ch {
                '"' => in_quotes = true,
                ')' => {
                    end = Some(idx);
                    break;
                }
                _ => {}
            }
        }

        let end = end.ok_or_else(|| "unterminated row in add statement".to_string())?;
        let row_src = &input[start..end];
        let row = split_add_row_tokens(row_src);
        rows.push(row);
    }

    Ok(rows)
}

/// Split the inside of one `( ... )` into values: whitespace-separated, with `"..."` for
/// tokens that contain spaces. Does not treat `#` as a comment (entity ids are `#12`-style).
fn split_add_row_tokens(row_src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = row_src.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let mut s = String::new();
            for ch in chars.by_ref() {
                if ch == '"' {
                    break;
                }
                s.push(ch);
            }
            out.push(s);
        } else {
            let mut s = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_whitespace() {
                    break;
                }
                s.push(chars.next().expect("peeked"));
            }
            out.push(s);
        }
    }
    out
}

pub fn parse_cell_value(col_type: &ColType, raw: &str) -> Result<CellValue, String> {
    match col_type {
        ColType::EntityType { .. } => raw
            .strip_prefix('#')
            .ok_or_else(|| "expected entity id like #12".to_string())
            .and_then(|rest| {
                rest.parse::<u64>()
                    .map(CellValue::Id)
                    .map_err(|_| format!("invalid entity id: {raw}"))
            }),
        ColType::PrimType { prim } => match prim {
            PrimType::PrimInt => raw
                .parse::<i64>()
                .map(CellValue::Int)
                .map_err(|_| format!("invalid int: {raw}")),
            PrimType::PrimString => Ok(CellValue::Str(raw.to_string())),
        },
        ColType::Tuple { .. } => Err("tuple columns are not supported yet".to_string()),
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn parses_help() {
        assert_eq!(parse_command("/help").unwrap(), Command::Help);
    }

    #[test]
    fn parses_load_schema_with_quotes() {
        assert_eq!(
            parse_command("load-schema \"tests/data/paths.json\";").unwrap(),
            Command::LoadSchema {
                path: "tests/data/paths.json".to_string()
            }
        );
    }

    #[test]
    fn parses_add_command() {
        assert_eq!(
            parse_command("add T values (7 \"alice\"), (8 \"bob\");").unwrap(),
            Command::Add {
                table: "T".to_string(),
                rows: vec![
                    vec!["7".to_string(), "alice".to_string()],
                    vec!["8".to_string(), "bob".to_string()],
                ],
            }
        );
    }

    #[test]
    fn parses_add_command_single_value() {
        assert_eq!(
            parse_command("add T values (#11);").unwrap(),
            Command::Add {
                table: "T".to_string(),
                rows: vec![vec!["#11".to_string()],],
            }
        );
    }

    #[test]
    fn rejects_unknown_command() {
        let err = parse_command("wat;").unwrap_err();
        assert!(err.contains("unknown statement"));
    }

    #[test]
    fn rejects_missing_semicolon() {
        let err = parse_command("help").unwrap_err();
        assert_eq!(err, "statements must end with `;`");
    }

    #[test]
    fn parses_quit_without_semicolon() {
        assert_eq!(parse_command("/quit").unwrap(), Command::Exit);
    }
}
