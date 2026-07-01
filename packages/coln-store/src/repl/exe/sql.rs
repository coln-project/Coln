use std::path::PathBuf;

use anyhow::Result;

use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
use crate::repl::Session;
use crate::repl::exe::{LoadedState, SchemaSummary};
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
    }
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
                col_type: match column.col_typ {
                    BuiltinTy::BuiltinInt => ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                    BuiltinTy::BuiltinStr => ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinStr,
                    },
                },
            })
            .collect(),
        primary_key: None,
    }
}
