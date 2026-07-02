use anyhow::{Context, Result, bail};
use sqlparser::{
    ast::{
        CopyLegacyOption, CopyOption, CopySource, CopyTarget, DataType, Expr, SetExpr, Statement,
        TableObject, Value,
    },
    dialect::GenericDialect,
    parser::Parser,
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Col {
    pub(crate) col_name: String,
    pub(crate) col_typ: coln_flir_rs::ir::BuiltinTy,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    CreateTable {
        table_name: String,
        columns: Vec<Col>,
    },
    Insert {
        table_name: String,
        col_names: Vec<String>,
        values: Vec<Vec<String>>,
    },
    /// `COPY <table> FROM '<file>' WITH (FORMAT csv, HEADER true)`, with an
    /// optional `DELIMITER` such as `E'\t'` for tab-separated files.
    // TODO use duckdb style import
    CopyFromCsv {
        table_name: String,
        path: String,
        delimiter: u8,
    },
}

pub(crate) fn parse_sql_statement(input: &str) -> Result<Command> {
    let dialect = GenericDialect {};
    let ast = Parser::parse_sql(&dialect, input).with_context(|| "failed to parse sql")?;

    // TODO support multi statement
    let [statement] = ast.as_slice() else {
        bail!("expected exactly one SQL statement");
    };

    if let Statement::CreateTable(sqlparser::ast::CreateTable { name, columns, .. }) = statement {
        let mut cols = Vec::new();
        for col in columns {
            let col_typ = match &col.data_type {
                DataType::Int(_) | DataType::Integer(_) => coln_flir_rs::ir::BuiltinTy::BuiltinInt,
                DataType::Text | DataType::String(_) | DataType::Varchar(_) => {
                    coln_flir_rs::ir::BuiltinTy::BuiltinStr
                }
                other => bail!("unsupported data type: {other}"),
            };
            cols.push(Col {
                col_name: col.name.to_string(),
                col_typ,
            })
        }
        Ok(Command::CreateTable {
            table_name: name.to_string(),
            columns: cols,
        })
    } else if let Statement::Insert(insert) = statement {
        let TableObject::TableName(table) = &insert.table else {
            bail!("unsupported INSERT target: {}", insert.table);
        };
        if insert.columns.is_empty() {
            bail!("INSERT must specify column names");
        }
        if !insert.assignments.is_empty() {
            bail!("INSERT assignments are not supported");
        }
        if insert.returning.is_some() {
            bail!("INSERT RETURNING is not supported");
        }

        let source = insert
            .source
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("INSERT must provide VALUES"))?;
        let SetExpr::Values(values) = source.body.as_ref() else {
            bail!("only INSERT ... VALUES is supported");
        };

        let col_names = insert
            .columns
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let rows = values
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(sql_expr_to_raw_value)
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Command::Insert {
            table_name: table.to_string(),
            col_names,
            values: rows,
        })
    } else if let Statement::Copy {
        source,
        to,
        target,
        options,
        legacy_options,
        values,
    } = statement
    {
        parse_copy_from_csv(source, *to, target, options, legacy_options, values)
    } else {
        bail!("Unsupported statements {}", input);
    }
}

fn parse_copy_from_csv(
    source: &CopySource,
    to: bool,
    target: &CopyTarget,
    options: &[CopyOption],
    legacy_options: &[CopyLegacyOption],
    values: &[Option<String>],
) -> Result<Command> {
    if to {
        bail!("COPY TO is not supported");
    }
    let CopySource::Table {
        table_name,
        columns,
    } = source
    else {
        bail!("COPY FROM only supports a table source");
    };
    if !columns.is_empty() {
        bail!("COPY column lists are not supported");
    }
    let CopyTarget::File { filename } = target else {
        bail!("COPY FROM only supports a file source");
    };
    if !legacy_options.is_empty() {
        bail!("legacy COPY options are not supported; use WITH (FORMAT csv, HEADER true)");
    }
    if !values.is_empty() {
        bail!("inline COPY values are not supported");
    }

    let mut format_csv = false;
    let mut header = false;
    let mut delimiter = b',';
    for option in options {
        match option {
            CopyOption::Format(format) => {
                if !format.value.eq_ignore_ascii_case("csv") {
                    bail!("unsupported COPY format: {format}");
                }
                format_csv = true;
            }
            CopyOption::Header(value) => header = *value,
            CopyOption::Delimiter(value) => delimiter = *value as u8,
            other => bail!("unsupported COPY option: {other}"),
        }
    }
    if !format_csv {
        bail!("COPY requires FORMAT csv");
    }
    if !header {
        bail!("COPY requires HEADER true");
    }

    Ok(Command::CopyFromCsv {
        table_name: table_name.to_string(),
        path: filename.clone(),
        delimiter,
    })
}

fn sql_expr_to_raw_value(expr: &Expr) -> Result<String> {
    let Expr::Value(value) = expr else {
        bail!("only literal INSERT values are supported");
    };

    match &value.value {
        Value::SingleQuotedString(value) | Value::DoubleQuotedString(value) => Ok(value.clone()),
        Value::Number(value, _) => Ok(value.clone()),
        other => bail!("unsupported INSERT value: {other}"),
    }
}
