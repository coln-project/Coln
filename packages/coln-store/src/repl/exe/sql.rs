// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

use crate::ir::{ColumnEntry, EntityVariant, Path, Schema};
use crate::repl::Session;
use crate::repl::exe::{LoadedState, SchemaSummary, add_rows};
use crate::repl::parse::{SqlCol as Col, SqlCommand as Command};
use crate::store::Store;

#[derive(Debug, thiserror::Error)]
enum SQLModeError {
    #[error("table already exists: {table}")]
    TableExists { table: String },
    #[error("cannot create table after data commits have been recorded")]
    SchemaChangeAfterData,
}

pub(super) fn execute_sql(session: &mut Session, command: Command) -> Result<String> {
    match command {
        Command::CreateTable {
            table_name,
            columns,
        } => create_sql_table(session, table_name, columns),
        Command::Insert { .. } => Ok("SQL INSERT is not implemented yet".to_string()),
        Command::CopyFromCsv {
            table_name,
            path,
            delimiter,
        } => copy_from_csv(session, &table_name, &path, delimiter),
    }
}

/// Load a delimited file with a header row into an existing table, mapping
/// file columns to schema columns by name.
fn copy_from_csv(
    session: &mut Session,
    table_name: &str,
    path: &str,
    delimiter: u8,
) -> Result<String> {
    let loaded = session
        .loaded
        .as_mut()
        .ok_or_else(|| anyhow!("no schema loaded"))?;
    let column_names: Vec<String> = loaded
        .store
        .table_at(&Path::from(table_name))
        .ok_or_else(|| anyhow!("unknown table: {table_name}"))?
        .schema()
        .columns
        .iter()
        .map(|column| column.path.to_string())
        .collect();

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .delimiter(delimiter)
        .from_path(path)
        .with_context(|| format!("failed to open csv {path}"))?;
    let headers = reader
        .headers()
        .with_context(|| format!("failed to read csv header from {path}"))?
        .clone();

    let mut indices = Vec::with_capacity(column_names.len());
    for name in &column_names {
        let mut matches = headers
            .iter()
            .enumerate()
            .filter(|(_, header)| header == name);
        let Some((index, _)) = matches.next() else {
            bail!("csv {path} is missing column: {name}");
        };
        if matches.next().is_some() {
            bail!("csv {path} has duplicate column: {name}");
        }
        indices.push(index);
    }

    let mut rows = Vec::new();
    for (row_index, record) in reader.records().enumerate() {
        let record = record
            .with_context(|| format!("failed to read csv row {} from {path}", row_index + 1))?;
        let mut row = Vec::with_capacity(indices.len());
        for (&index, name) in indices.iter().zip(&column_names) {
            let field = record.get(index).ok_or_else(|| {
                anyhow!("csv row {}: missing field for column {name}", row_index + 1)
            })?;
            row.push(field.to_string());
        }
        rows.push(row);
    }

    if rows.is_empty() {
        return Ok(format!("copied 0 rows into {table_name}"));
    }
    tracing::debug!("ready to copy {} rows into the store", rows.len());
    let row_ids = add_rows(&mut loaded.store, table_name, &rows)?;
    Ok(format!("copied {} rows into {table_name}", row_ids.len()))
}

fn create_sql_table(
    session: &mut Session,
    table_name: String,
    columns: Vec<Col>,
) -> Result<String> {
    let loaded = session.loaded.get_or_insert_with(empty_sql_state);
    let path = Path::from(table_name.as_str());
    let schema = sql_schema_from_cols(columns);

    if loaded.store.resolve_table(&path).is_some() {
        return Err(SQLModeError::TableExists { table: table_name }.into());
    }
    if loaded
        .store
        .commits()
        .iter_topological()
        .any(|commit| !commit.is_root())
    {
        return Err(SQLModeError::SchemaChangeAfterData.into());
    }

    loaded.store.create_table(path, schema)?;
    loaded.schema = SchemaSummary::from_store(loaded.schema.source.clone(), &loaded.store);

    Ok(format!("created table {table_name}"))
}

fn empty_sql_state() -> LoadedState {
    let store = Store::new();
    let source = PathBuf::from("<sql>");
    let schema = SchemaSummary::from_store(source, &store);
    LoadedState { store, schema }
}

fn sql_schema_from_cols(columns: Vec<Col>) -> Schema {
    Schema {
        entity_variant: EntityVariant::Table,
        columns: columns
            .into_iter()
            .map(|column| ColumnEntry {
                path: Path::from(column.col_name.as_str()),
                col_type: column.col_typ,
            })
            .collect(),
        primary_key: None,
    }
}

#[cfg(test)]
mod tests {
    use coln_flir_rs::ir::ColType;

    use super::*;
    use crate::repl::ShellMode;

    fn sql_session_with_person() -> Session {
        let mut session = Session {
            loaded: None,
            shell_mode: ShellMode::Sql,
        };
        create_sql_table(
            &mut session,
            "Person".to_string(),
            vec![
                Col {
                    col_name: "name".to_string(),
                    col_typ: ColType::BuiltinTy {
                        builtin_ty: coln_flir_rs::ir::BuiltinTy::BuiltinStr,
                    },
                },
                Col {
                    col_name: "age".to_string(),
                    col_typ: ColType::BuiltinTy {
                        builtin_ty: coln_flir_rs::ir::BuiltinTy::BuiltinInt,
                    },
                },
            ],
        )
        .expect("create table");
        session
    }

    #[test]
    fn copy_requires_loaded_schema() {
        let err = copy_from_csv(
            &mut Session::default(),
            "Person",
            "tests/data/people.csv",
            b',',
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "no schema loaded");
    }

    #[test]
    fn copy_rejects_unknown_table() {
        let mut session = sql_session_with_person();
        let err =
            copy_from_csv(&mut session, "Missing", "tests/data/people.csv", b',').unwrap_err();
        assert_eq!(err.to_string(), "unknown table: Missing");
    }

    #[test]
    fn copy_rejects_missing_file() {
        let mut session = sql_session_with_person();
        let err =
            copy_from_csv(&mut session, "Person", "tests/data/missing.csv", b',').unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to open csv tests/data/missing.csv")
        );
    }

    #[test]
    fn copy_imports_tab_delimited_file() {
        let mut session = sql_session_with_person();
        let message = copy_from_csv(&mut session, "Person", "tests/data/people.tsv", b'\t')
            .expect("copy tsv");
        assert_eq!(message, "copied 2 rows into Person");

        let loaded = session.loaded.as_ref().expect("loaded session");
        let dump = loaded
            .store
            .table_at(&"Person".parse().unwrap())
            .expect("Person table")
            .dump();
        assert!(dump.contains("alice"));
        assert!(dump.contains("41"));
    }

    #[test]
    fn copy_rejects_missing_csv_column() {
        let mut session = Session {
            loaded: None,
            shell_mode: ShellMode::Sql,
        };
        create_sql_table(
            &mut session,
            "Person".to_string(),
            vec![Col {
                col_name: "email".to_string(),
                col_typ: ColType::BuiltinTy {
                    builtin_ty: coln_flir_rs::ir::BuiltinTy::BuiltinStr,
                },
            }],
        )
        .expect("create table");

        let err = copy_from_csv(&mut session, "Person", "tests/data/people.csv", b',').unwrap_err();
        assert_eq!(
            err.to_string(),
            "csv tests/data/people.csv is missing column: email"
        );
    }

    #[test]
    fn copy_rejects_duplicate_csv_column() {
        let mut session = sql_session_with_person();
        let err =
            copy_from_csv(&mut session, "Person", "tests/data/people_dup.csv", b',').unwrap_err();
        assert_eq!(
            err.to_string(),
            "csv tests/data/people_dup.csv has duplicate column: name"
        );
    }

    #[test]
    fn copy_rejects_invalid_int_value() {
        let mut session = sql_session_with_person();
        let err = copy_from_csv(
            &mut session,
            "Person",
            "tests/data/people_bad_int.csv",
            b',',
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "column 1: invalid int: not-a-number");
    }
}
