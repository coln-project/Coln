// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::HashMap;

use crate::{
    commit::hash::{CommitHash, HASH_SIZE},
    ir::{BuiltinTy, ColType},
    repl::error::BatchCellParseError,
    table::{CellValue, RowId},
    txn::ops::{TempRowId, TxnCellValue},
};

/// One `name = add <table> values (...);` step inside a batch block (exactly one row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchAssignment {
    pub name: String,
    pub table: String,
    pub row: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Add {
        table: String,
        rows: Vec<Vec<String>>,
    },
    /// `begin transact;` … `commit;` with assignments using previous bindings in entity columns.
    Batch { assignments: Vec<BatchAssignment> },
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

pub fn parse_statement(input: &str) -> Result<Command, String> {
    let input = input.trim();
    if input.starts_with("begin transact") {
        return parse_batch_block(input);
    }

    let parts = shlex::split(input).ok_or_else(|| "could not parse input".to_string())?;
    let Some(command) = parts.first() else {
        return Err("empty command".to_string());
    };

    match command.as_str() {
        "add" => parse_add_statement(input),
        _ => Err(format!(
            "unknown statement: {command}. Statements must end with `;`, or use `.help` for meta commands."
        )),
    }
}

/// Resolve one cell for a batch insert: entity columns accept `#id` or a prior binding name.
pub(crate) fn parse_cell_value_batch(
    col_type: &ColType,
    raw: &str,
    bindings: &HashMap<String, TempRowId>,
) -> Result<TxnCellValue, BatchCellParseError> {
    match col_type {
        ColType::RowId { .. } => {
            if raw.starts_with('#') {
                parse_cell_value(col_type, raw)
                    .map(Into::into)
                    .map_err(BatchCellParseError::InvalidValue)
            } else if is_binding_ident(raw) {
                let id = bindings
                    .get(raw)
                    .copied()
                    .ok_or_else(|| BatchCellParseError::UnknownBinding(raw.to_string()))?;
                Ok(TxnCellValue::from(id))
            } else {
                Err(BatchCellParseError::InvalidValue(format!(
                    "expected entity id like #<commit>:<counter> or a binding name, got {raw}"
                )))
            }
        }
        _ => parse_cell_value(col_type, raw)
            .map(Into::into)
            .map_err(BatchCellParseError::InvalidValue),
    }
}

/// Parse `begin transact;` … `commit` (outer `;` already stripped by [`parse_command`]).
fn parse_batch_block(input: &str) -> Result<Command, String> {
    let input = input.trim();
    let Some(rest) = input.strip_prefix("begin transact") else {
        return Err("internal error: expected begin transact".to_string());
    };
    let mut rest = rest.trim_start();
    let Some(after_kw) = rest.strip_prefix(';') else {
        return Err("expected `begin transact;`".to_string());
    };
    rest = after_kw.trim();
    let Some(inner) = rest.strip_suffix("commit") else {
        return Err("transaction block must end with `commit`".to_string());
    };
    let inner = inner.trim().strip_suffix(';').unwrap_or(inner).trim();
    let assignments = parse_batch_assignments(inner)?;
    Ok(Command::Batch { assignments })
}

pub fn is_binding_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn split_semicolon_statements(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in s.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
        }
        if ch == ';' && !in_quotes {
            let t = cur.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    let tail = cur.trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn parse_batch_assignments(inner: &str) -> Result<Vec<BatchAssignment>, String> {
    let mut v = Vec::new();
    for stmt in split_semicolon_statements(inner) {
        v.push(parse_batch_assignment(&stmt)?);
    }
    Ok(v)
}

fn parse_batch_assignment(line: &str) -> Result<BatchAssignment, String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("empty statement inside batch block".to_string());
    }
    let Some((name, rhs)) = line.split_once(" = add ") else {
        return Err(format!(
            "expected `name = add <table> values (...)`, got: {line}"
        ));
    };
    let name = name.trim();
    if name.is_empty() || !is_binding_ident(name) {
        return Err(format!("invalid binding name: {name}"));
    }
    let rhs = rhs.trim();
    let Some((table, rows_src)) = rhs.split_once(" values ") else {
        return Err(format!(
            "expected `values (...)` after table name in: {line}"
        ));
    };
    let table = table.trim();
    if table.is_empty() {
        return Err("missing table name".to_string());
    }
    let rows = parse_add_rows(rows_src.trim())?;
    if rows.len() != 1 {
        return Err(
            "each batch assignment must insert exactly one row (one `values (...)` group)"
                .to_string(),
        );
    }
    let row = rows.into_iter().next().expect("one row");
    Ok(BatchAssignment {
        name: name.to_string(),
        table: table.to_string(),
        row,
    })
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
        ColType::RowId { .. } => parse_row_id(raw).map(CellValue::Id),
        ColType::BuiltinTy { builtin_ty } => match builtin_ty {
            BuiltinTy::BuiltinInt => raw
                .parse::<i64>()
                .map(CellValue::Int)
                .map_err(|_| format!("invalid int: {raw}")),
            BuiltinTy::BuiltinStr => Ok(CellValue::Str(raw.to_string())),
        },
    }
}

fn parse_row_id(raw: &str) -> Result<RowId, String> {
    let rest = raw
        .strip_prefix('#')
        .ok_or_else(|| "expected entity id like #<commit>:<counter>".to_string())?;
    let Some((hash_hex, counter_raw)) = rest.split_once(':') else {
        return Err("expected entity id like #<commit>:<counter>".to_string());
    };
    let hash_bytes = hex::decode(hash_hex).map_err(|_| format!("invalid entity id: {raw}"))?;
    if hash_bytes.len() != HASH_SIZE {
        return Err(format!("invalid entity id: {raw}"));
    }
    let mut hash = [0; HASH_SIZE];
    hash.copy_from_slice(&hash_bytes);
    let counter = counter_raw
        .parse::<u32>()
        .map_err(|_| format!("invalid entity id: {raw}"))?;
    Ok(RowId {
        commit: CommitHash(hash),
        counter,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_binding_name() {
        assert!(is_binding_ident("g0"));
    }
}
