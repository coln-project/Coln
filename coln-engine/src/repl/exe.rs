use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    ir::{ColType, FlatTheory, PrimType},
    repl::{error::ReplError, parse::parse_cell_value},
    store::Store,
    table::CellValue,
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
    Columns(Vec<i64>),
}

pub struct LoadedState {
    pub(crate) store: Store,
    pub(crate) schema: SchemaSummary,
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
        ColType::EntityType { path } => format!("entity({path})"),
        ColType::PrimType { prim } => match prim {
            PrimType::PrimInt => "int".to_string(),
            PrimType::PrimString => "string".to_string(),
        },
        ColType::Tuple { fields } => format!("tuple({} fields)", fields.len()),
    }
}

impl SchemaSummary {
    fn from_theory(source: PathBuf, theory: &FlatTheory) -> Self {
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
                columns: entry.table.columns.iter().map(format_col_type).collect(),
            })
            .collect();

        Self {
            source,
            table_count: theory.tables.len(),
            law_count: theory.laws.len(),
            tables,
        }
    }
}

pub fn load_schema(path: &Path) -> Result<LoadedState, ReplError> {
    let input = fs::read_to_string(path)?;
    let theory: FlatTheory = serde_json::from_str(&input)?;
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
) -> Result<Vec<u64>, ReplError> {
    let table_path = crate::ir::Path::from(table_name);
    let table = store
        .table_at(&table_path)
        .ok_or_else(|| ReplError::UnknownTable(table_name.to_string()))?;

    let ops = raw_rows
        .iter()
        .map(|raw_values| build_add_op(table, raw_values))
        .collect::<Result<Vec<_>, _>>()?;

    store.apply_batch(ops).map_err(Into::into)
}

fn build_add_op(
    table: &crate::table::Table,
    raw_values: &[String],
) -> Result<crate::ops::Op, ReplError> {
    let expected = table.schema().columns.len();
    if raw_values.len() != expected {
        return Err(ReplError::ColumnCountMismatch {
            expected,
            got: raw_values.len(),
        });
    }

    let values = table
        .schema()
        .columns
        .iter()
        .enumerate()
        .map(|(idx, col_type)| -> Result<CellValue, ReplError> {
            let raw = &raw_values[idx];
            parse_cell_value(col_type, raw).map_err(|message| ReplError::BadValue {
                column: idx,
                message,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(table.add(values))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_no_schema_message() {
        assert_eq!(render_schema_summary(None), "no schema loaded");
    }

    #[test]
    fn loads_schema_summary_from_fixture() {
        let loaded = load_schema(Path::new("tests/data/paths.json")).expect("load schema");
        assert_eq!(loaded.store.table_count(), 10);
        assert_eq!(loaded.schema.table_count, 10);
        assert_eq!(loaded.schema.law_count, 12);
        assert_eq!(loaded.schema.tables[0].path, "G.E");
    }
}
