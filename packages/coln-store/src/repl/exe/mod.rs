// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};

use crate::repl::{
    Session, ShellMode, Step,
    parse::coln::{self, BatchAssignment, parse_cell_value, parse_cell_value_batch},
    parse::{ColnCommand, MetaCommand, SqlCommand},
};
use crate::{
    commit::pst::{decode_store, encode_store},
    ir::{BuiltinTy, ColType, ColumnEntry, FlatRealm},
    store::Store,
    table::{RowId, TableRef},
    txn::ops::{TempRowId, TxnCellValue},
};

mod sql;

fn help_text(mode: ShellMode) -> String {
    let mut lines = vec![
        "Commands:",
        "  .help",
        "  .exit",
        "  .quit",
        "  .load <schema-json-path>",
        "  .open <store-path>",
        "  .save <store-path>",
        "  .tables",
        "  .rules",
        "  .schema [table]",
        "  .dump <table>",
    ];

    match mode {
        ShellMode::Coln => lines.extend([
            "  add <table> values (...), (...);",
            "  begin transact; name = add <table> values (...); ... commit;",
            "",
            "Examples:",
            "  .help",
            "  .load tests/data/paths.json",
            "  .schema",
            "  .rules",
            "  .dump T",
            "  add T values (7 \"alice\"), (8 \"bob\");",
            "  begin transact; g = add Graphs values (); e = add G0 values (g); commit;",
        ]),
        ShellMode::Sql => lines.extend([
            "  create table <table> (<column> <type>, ...);",
            "  copy <table> from '<file.csv>' with (format csv, header true);",
            "  copy <table> from '<file.tsv>' with (format csv, header true, delimiter E'\\t');",
            "",
            "Examples:",
            "  .help",
            "  .schema",
            "  .dump Person",
            "  create table Person (name text, age integer);",
            "  copy Person from 'tests/data/people.csv' with (format csv, header true);",
            "  copy Decl from 'decl.csv' with (format csv, header true, delimiter E'\\t');",
        ]),
    }

    lines.join("\n")
}

pub(super) fn execute_sql(session: &mut Session, command: SqlCommand) -> Result<String> {
    sql::execute_sql(session, command)
}

pub(super) fn execute_meta(session: &mut Session, command: MetaCommand) -> Result<Step> {
    match command {
        MetaCommand::Help => Ok(Step::Continue(help_text(session.shell_mode))),
        MetaCommand::Load { path } => {
            let loaded = load_schema(std::path::Path::new(&path))?;
            tracing::info!(
                source = %loaded.schema.source.display(),
                table_count = loaded.store.table_count(),
                law_count = loaded.schema.law_count,
                "loaded schema"
            );
            let message = format!(
                "loaded schema from {} (tables: {}, laws: {})",
                loaded.schema.source.display(),
                loaded.store.table_count(),
                loaded.schema.law_count
            );
            session.loaded = Some(loaded);
            Ok(Step::Continue(message))
        }
        MetaCommand::Open { path } => {
            let loaded = load_store(std::path::Path::new(&path))?;
            tracing::info!(
                source = %loaded.schema.source.display(),
                table_count = loaded.store.table_count(),
                law_count = loaded.schema.law_count,
                "loaded store"
            );
            let message = format!(
                "loaded store from {} (tables: {}, laws: {})",
                loaded.schema.source.display(),
                loaded.store.table_count(),
                loaded.schema.law_count
            );
            session.loaded = Some(loaded);
            Ok(Step::Continue(message))
        }
        MetaCommand::Schema { table } => {
            let schema = session.loaded.as_ref().map(|loaded| &loaded.schema);
            let message = match table {
                Some(table) => render_table_schema(schema, &table)?,
                None => render_schema_summary(schema),
            };
            Ok(Step::Continue(message))
        }
        MetaCommand::Rules => {
            let store = session.loaded.as_ref().map(|loaded| &loaded.store);
            Ok(Step::Continue(render_rules(store)?))
        }
        MetaCommand::Dump { table } => {
            let loaded = session
                .loaded
                .as_mut()
                .ok_or_else(|| anyhow!("no schema loaded"))?;
            let tbl = loaded
                .store
                .table_at(&crate::ir::Path::from(table.clone()))
                .ok_or_else(|| anyhow!("unknown table: {table}"))?;
            Ok(Step::Continue(tbl.dump()))
        }
        MetaCommand::Tables => {
            let loaded = session
                .loaded
                .as_mut()
                .ok_or_else(|| anyhow!("no schema loaded"))?;
            Ok(Step::Continue(loaded.store.dump()))
        }
        MetaCommand::Save { path } => {
            let loaded = session
                .loaded
                .as_mut()
                .ok_or_else(|| anyhow!("no schema loaded"))?;
            let bytes = encode_store(&loaded.store)?;
            let mut path = PathBuf::from(&path);
            if path.extension().is_none() {
                path.add_extension("colnstore");
            }

            fs::write(&path, &bytes).with_context(|| format!("cannot find path {path:?}"))?;
            Ok(Step::Continue(format!(
                "saved store to {}",
                path.as_os_str().display()
            )))
        }
        MetaCommand::Exit => Ok(Step::Exit),
    }
}

pub(super) fn execute_coln(session: &mut Session, command: ColnCommand) -> Result<String> {
    match command {
        ColnCommand::Add { table, rows } => {
            let loaded = session
                .loaded
                .as_mut()
                .ok_or_else(|| anyhow!("no schema loaded"))?;
            let row_ids = add_rows(&mut loaded.store, &table, &rows)?;
            let row_ids = row_ids
                .into_iter()
                .map(|row_id| format!("#{row_id}"))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(format!("inserted into {table} rows [{row_ids}]"))
        }
        ColnCommand::Batch { assignments } => {
            let loaded = session
                .loaded
                .as_mut()
                .ok_or_else(|| anyhow!("no schema loaded"))?;
            run_transact(&mut loaded.store, &assignments)
        }
    }
}

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
    pub(crate) fn from_store(source: PathBuf, store: &Store) -> Self {
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

pub fn load_store(path: &Path) -> Result<LoadedState> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read store {}", path.display()))?;
    let store = decode_store(&bytes)
        .with_context(|| format!("failed to decode store {}", path.display()))?;
    let schema = SchemaSummary::from_store(path.to_path_buf(), &store);
    Ok(LoadedState { store, schema })
}

pub fn load_schema(path: &Path) -> Result<LoadedState> {
    let input = fs::read_to_string(path)
        .with_context(|| format!("failed to read schema {}", path.display()))?;
    let theory: FlatRealm = serde_json::from_str(&input)
        .with_context(|| format!("failed to parse schema {}", path.display()))?;
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
        lines.push(String::new());
        lines.push(render_table_schema_summary(table));
    }

    lines.join("\n")
}

fn render_table_schema_summary(table: &TableSummary) -> String {
    let mut lines = vec![
        format!("table: {}", table.path),
        format!("columns: {}", table.column_count),
        format!("primary key: {}", format_primary_key(&table.primary_key)),
    ];
    lines.extend(table.columns.iter().map(|column| format!("- {column}")));

    lines.join("\n")
}

pub fn render_table_schema(schema: Option<&SchemaSummary>, table_name: &str) -> Result<String> {
    let schema = schema.ok_or_else(|| anyhow!("no schema loaded"))?;
    let table = schema
        .tables
        .iter()
        .find(|table| table.path == table_name)
        .ok_or_else(|| anyhow!("unknown table: {table_name}"))?;

    Ok(render_table_schema_summary(table))
}

pub fn render_rules(store: Option<&Store>) -> Result<String> {
    let store = store.ok_or_else(|| anyhow!("no schema loaded"))?;
    if store.rules().is_empty() {
        return Ok("no rules".to_string());
    }

    Ok(store
        .rules()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn add_rows(
    store: &mut Store,
    table_name: &str,
    raw_rows: &[Vec<String>],
) -> Result<Vec<RowId>> {
    let table_path = crate::ir::Path::from(table_name);
    let oid = store
        .resolve_table(&table_path)
        .ok_or_else(|| anyhow!("unknown table: {table_name}"))?;

    let table = store
        .table(oid)
        .ok_or_else(|| anyhow!("unknown table: {table_name}"))?;
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
pub fn run_transact(store: &mut Store, assignments: &[BatchAssignment]) -> Result<String> {
    let mut bindings: HashMap<String, TempRowId> = HashMap::new();
    let mut pending = Vec::new();

    for (index, a) in assignments.iter().enumerate() {
        if bindings.contains_key(&a.name) {
            bail!("duplicate binding: {}", a.name);
        }
        let table_path = crate::ir::Path::from(a.table.as_str());
        let table = store
            .table_at(&table_path)
            .ok_or_else(|| anyhow!("unknown table: {}", a.table))?;

        let expected = table.schema().columns.len();
        if a.row.len() != expected {
            bail!(
                "column count mismatch: expected {expected}, got {}",
                a.row.len()
            );
        }

        let mut values = Vec::with_capacity(expected);
        for (idx, column) in table.schema().columns.iter().enumerate() {
            let v =
                parse_cell_value_batch(&column.col_type, &a.row[idx], &bindings).map_err(|e| {
                    match e {
                        coln::ParserError::UnknownBinding(name) => {
                            anyhow!("unknown binding: {name}")
                        }
                        coln::ParserError::InvalidValue(message) => {
                            anyhow!("column {idx}: {message}")
                        }
                        coln::ParserError::InvalidInput(message) => {
                            anyhow!("invalid intput: {message}")
                        }
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

fn parse_txn_values(table: TableRef<'_>, raw_values: &[String]) -> Result<Vec<TxnCellValue>> {
    let expected = table.schema().columns.len();
    if raw_values.len() != expected {
        bail!(
            "column count mismatch: expected {expected}, got {}",
            raw_values.len()
        );
    }

    table
        .schema()
        .columns
        .iter()
        .enumerate()
        .map(|(idx, column)| -> Result<TxnCellValue> {
            let raw = &raw_values[idx];
            parse_cell_value(&column.col_type, raw)
                .map(Into::into)
                .map_err(|message| anyhow!("column {idx}: {message}"))
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
    fn coln_help_lists_only_coln_statements() {
        let help = help_text(ShellMode::Coln);
        assert!(help.contains(".load <schema-json-path>"));
        assert!(help.contains("add <table> values"));
        assert!(help.contains("begin transact;"));
        assert!(!help.contains("create table"));
        assert!(!help.contains("copy <table> from"));
    }

    #[test]
    fn sql_help_lists_only_sql_statements() {
        let help = help_text(ShellMode::Sql);
        assert!(help.contains(".load <schema-json-path>"));
        assert!(help.contains("create table <table>"));
        assert!(help.contains("copy <table> from '<file.csv>' with (format csv, header true);"));
        assert!(!help.contains("add <table> values"));
        assert!(!help.contains("begin transact;"));
    }

    #[test]
    fn renders_all_table_schemas() {
        let loaded = load_schema(&Path::new("tests/data/").join(PATHS_IR)).expect("load schema");
        let rendered = render_schema_summary(Some(&loaded.schema));
        assert!(rendered.contains("source: tests/data/Path.json"));
        assert!(rendered.contains("table: Path.G.V"));
        assert!(rendered.contains("- a: entity(Path.Graphs)"));
        assert!(rendered.contains("table: Path.G.E"));
        assert!(rendered.contains("- b: entity(Path.G.V)"));
    }

    #[test]
    fn loads_schema_summary_from_fixture() {
        let loaded = load_schema(&Path::new("tests/data/").join(PATHS_IR)).expect("load schema");
        assert_eq!(loaded.store.table_count(), 16);
        assert_eq!(loaded.schema.table_count, 16);
        assert_eq!(loaded.schema.law_count, 27);
        assert_eq!(loaded.schema.tables[0].path, "Path.G0");
    }

    #[test]
    fn renders_single_table_schema() {
        let loaded = load_schema(&Path::new("tests/data/").join(PATHS_IR)).expect("load schema");
        let rendered =
            render_table_schema(Some(&loaded.schema), "Path.G.V").expect("render table schema");
        assert!(rendered.contains("table: Path.G.V"));
        assert!(rendered.contains("primary key:"));
        assert!(rendered.contains("- a: entity(Path.Graphs)"));
    }

    #[test]
    fn renders_rules_one_per_line() {
        let loaded = load_schema(&Path::new("tests/data/").join(PATHS_IR)).expect("load schema");
        let rendered = render_rules(Some(&loaded.store)).expect("render rules");
        let lines = rendered.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), loaded.store.rules().len());
        assert!(lines[0].contains(" := forall"));
        assert!(lines[0].contains(" |- "));
    }
}
