use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    commit::pst::decode_store,
    ir::{BuiltinTy, ColType, ColumnEntry, FlatRealm},
    repl::{
        error::ReplError,
        parse::{BatchAssignment, parse_cell_value, parse_cell_value_batch},
    },
    store::Store,
    table::{RowId, Table},
    txn::ops::{TempRowId, TxnCellValue},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSummary {
    pub(crate) source: PathBuf,
    pub(crate) table_count: usize,
    pub(crate) law_count: usize,
    pub(crate) tables: Vec<TableSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSummary {
    pub(crate) path: String,
    pub(crate) column_count: usize,
    pub(crate) primary_key: PrimaryKeySummary,
    pub(crate) columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimaryKeySummary {
    None,
    Singleton,
    Columns(Vec<crate::ir::Path>),
}

pub struct LoadedState {
    pub(crate) store: Store,
    pub(crate) schema: SchemaSummary,
}

impl LoadedState {
    pub fn store(&self) -> &Store {
        &self.store
    }
}

fn format_primary_key(primary_key: &PrimaryKeySummary) -> String {
    match primary_key {
        PrimaryKeySummary::None => "none".to_string(),
        PrimaryKeySummary::Singleton => "singleton".to_string(),
        PrimaryKeySummary::Columns(columns) => format!("{columns:?}"),
    }
}

fn format_col_type(col_type: &ColType) -> String {
    match col_type {
        ColType::RowId { path } => format!("entity({path})"),
        ColType::BuiltinTy { builtin_ty } => match builtin_ty {
            BuiltinTy::BuiltinInt => "int".to_string(),
            BuiltinTy::BuiltinStr => "string".to_string(),
        },
    }
}

fn format_column(column: &ColumnEntry) -> String {
    format!("{}: {}", column.path, format_col_type(&column.col_type))
}

impl SchemaSummary {
    fn from_store(source: PathBuf, store: &Store) -> Self {
        let mut tables: Vec<TableSummary> = store
            .tables()
            .map(|(_, table)| TableSummary {
                path: table.path().to_string(),
                column_count: table.schema().columns.len(),
                primary_key: match &table.schema().primary_key {
                    None => PrimaryKeySummary::None,
                    Some(columns) if columns.is_empty() => PrimaryKeySummary::Singleton,
                    Some(columns) => PrimaryKeySummary::Columns(columns.clone()),
                },
                columns: table.schema().columns.iter().map(format_column).collect(),
            })
            .collect();
        tables.sort_by(|a, b| a.path.cmp(&b.path));
        let law_count = store.rule_entries().len();
        let table_count = tables.len();
        Self {
            source,
            table_count,
            law_count,
            tables,
        }
    }

    fn from_theory(source: PathBuf, theory: &FlatRealm) -> Self {
        let tables = theory
            .tables
            .iter()
            .map(|entry| TableSummary {
                path: entry.path.to_string(),
                column_count: entry.table.columns.len(),
                primary_key: match &entry.table.primary_key {
                    None => PrimaryKeySummary::None,
                    Some(columns) if columns.is_empty() => PrimaryKeySummary::Singleton,
                    Some(columns) => PrimaryKeySummary::Columns(columns.clone()),
                },
                columns: entry.table.columns.iter().map(format_column).collect(),
            })
            .collect();

        Self {
            source,
            table_count: theory.tables.len(),
            law_count: theory.rules.len(),
            tables,
        }
    }
}

pub fn load_store(path: &Path) -> Result<LoadedState, ReplError> {
    let bytes = fs::read(path)?;
    let store = decode_store(&bytes)?;
    let schema = SchemaSummary::from_store(path.to_path_buf(), &store);
    Ok(LoadedState { store, schema })
}

pub fn load_schema(path: &Path) -> Result<LoadedState, ReplError> {
    let input = fs::read_to_string(path)?;
    let theory: FlatRealm = serde_json::from_str(&input)?;
    let summary = SchemaSummary::from_theory(path.to_path_buf(), &theory);
    let store = Store::try_from_theory(theory)?;
    Ok(LoadedState {
        store,
        schema: summary,
    })
}

pub fn render_schema_summary(schema: Option<&SchemaSummary>) -> String {
    let Some(schema) = schema else {
        return "no schema loaded".to_string();
    };

    let mut lines = vec![
        format!("source: {}", schema.source.display()),
        format!("tables: {}", schema.table_count),
        format!("laws: {}", schema.law_count),
    ];

    for table in &schema.tables {
        lines.push(format!(
            "- {} | cols={} | pk={} | [{}]",
            table.path,
            table.column_count,
            format_primary_key(&table.primary_key),
            table.columns.join(", ")
        ));
    }

    lines.join("\n")
}

pub fn add_rows(
    store: &mut Store,
    table_name: &str,
    raw_rows: &[Vec<String>],
) -> Result<Vec<RowId>, ReplError> {
    let table_path = crate::ir::Path::from(table_name);
    let oid = store
        .resolve_table(&table_path)
        .ok_or_else(|| ReplError::UnknownTable(table_name.to_string()))?;

    let table = store
        .table(oid)
        .ok_or_else(|| ReplError::UnknownTable(table_name.to_string()))?;
    let mut rows = Vec::new();
    for raw_values in raw_rows {
        rows.push(parse_txn_values(table, raw_values)?);
    }

    let mut tx = store.transaction();
    let mut temp_ids = Vec::new();
    for values in rows {
        temp_ids.push(tx.add_internal(&table_path, values)?);
    }
    let commit = tx.commit()?;
    let row_ids = temp_ids
        .into_iter()
        .map(|temp_id| temp_id.resolve(commit))
        .collect();
    Ok(row_ids)
}

/// Parse and commit a batch transaction, allowing later rows to refer to earlier bindings.
pub fn run_transact(
    store: &mut Store,
    assignments: &[BatchAssignment],
) -> Result<String, ReplError> {
    let mut bindings: HashMap<String, TempRowId> = HashMap::new();
    let mut pending = Vec::new();

    for (index, a) in assignments.iter().enumerate() {
        if bindings.contains_key(&a.name) {
            return Err(ReplError::DuplicateBinding(a.name.clone()));
        }
        let table_path = crate::ir::Path::from(a.table.as_str());
        let table = store
            .table_at(&table_path)
            .ok_or_else(|| ReplError::UnknownTable(a.table.clone()))?;

        let expected = table.schema().columns.len();
        if a.row.len() != expected {
            return Err(ReplError::ColumnCountMismatch {
                expected,
                got: a.row.len(),
            });
        }

        let mut values = Vec::with_capacity(expected);
        for (idx, column) in table.schema().columns.iter().enumerate() {
            let v =
                parse_cell_value_batch(&column.col_type, &a.row[idx], &bindings).map_err(|e| {
                    let err: ReplError = e.into();
                    match err {
                        ReplError::BadValue { message, .. } => ReplError::BadValue {
                            column: idx,
                            message,
                        },
                        other => other,
                    }
                })?;
            values.push(v);
        }

        let temp_id = TempRowId::from(index as u32);
        bindings.insert(a.name.clone(), temp_id);
        pending.push((a.name.clone(), table_path, values, temp_id));
    }

    let mut tx = store.transaction();
    for (_, table_path, values, _) in pending.iter() {
        tx.add_internal(table_path, values.clone())?;
    }
    let commit = tx.commit()?;

    let parts = pending
        .into_iter()
        .map(|(name, _, _, temp_id)| format!("{}=#{}", name, temp_id.resolve(commit)))
        .collect::<Vec<_>>();
    let message = format!("committed batch: {}", parts.join(", "));
    Ok(message)
}

fn parse_txn_values(table: &Table, raw_values: &[String]) -> Result<Vec<TxnCellValue>, ReplError> {
    let expected = table.schema().columns.len();
    if raw_values.len() != expected {
        return Err(ReplError::ColumnCountMismatch {
            expected,
            got: raw_values.len(),
        });
    }

    table
        .schema()
        .columns
        .iter()
        .enumerate()
        .map(|(idx, column)| -> Result<TxnCellValue, ReplError> {
            let raw = &raw_values[idx];
            parse_cell_value(&column.col_type, raw)
                .map(Into::into)
                .map_err(|message| ReplError::BadValue {
                    column: idx,
                    message,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    static PATHS_IR: &str = "Path.json";

    #[test]
    fn lists_no_schema_message() {
        assert_eq!(render_schema_summary(None), "no schema loaded");
    }

    #[test]
    fn loads_schema_summary_from_fixture() {
        let loaded = load_schema(&Path::new("tests/data/").join(PATHS_IR)).expect("load schema");
        assert_eq!(loaded.store.table_count(), 16);
        assert_eq!(loaded.schema.table_count, 16);
        assert_eq!(loaded.schema.law_count, 27);
        assert_eq!(loaded.schema.tables[0].path, "Path.G0");
    }
}
