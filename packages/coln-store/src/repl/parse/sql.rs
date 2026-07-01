use sqlparser::{dialect::GenericDialect, parser::Parser};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    CreateTable,
}

pub(crate) fn parse_sql_statement(input: &str) -> Result<Command, String> {
    let dialect = GenericDialect {};
    let ast= Parser::parse_sql(&dialect, input)?;



    Err(format!("unsupported SQL statement: {input}"))
}
