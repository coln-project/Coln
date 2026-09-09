// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

pub use cli_table::{Cell, CellStruct, TableDisplay, format::Justify};
use cli_table::{
    Table,
    format::{Border, HorizontalLine, Separator},
};
use std::{fmt, io, iter};

/// Names the columns of a table. Deliberately decoupled from [`CliTableRow`]:
/// a row knows how to render its own cells but not necessarily how its columns
/// are named, so the header may come from any other type, potentially a schema
/// that is only known at runtime.
pub trait CliTableHeader {
    fn cli_table_header(&self) -> impl Iterator<Item = CellStruct>;
}

impl<T: ?Sized + CliTableHeader> CliTableHeader for &T {
    fn cli_table_header(&self) -> impl Iterator<Item = CellStruct> {
        T::cli_table_header(*self)
    }
}

impl<T: AsRef<str>> CliTableHeader for [T] {
    fn cli_table_header(&self) -> impl Iterator<Item = CellStruct> {
        self.iter().map(|name| name.as_ref().cell())
    }
}

impl<T: AsRef<str>, const N: usize> CliTableHeader for [T; N] {
    fn cli_table_header(&self) -> impl Iterator<Item = CellStruct> {
        self.as_slice().cli_table_header()
    }
}

impl<T: AsRef<str>> CliTableHeader for Vec<T> {
    fn cli_table_header(&self) -> impl Iterator<Item = CellStruct> {
        self.as_slice().cli_table_header()
    }
}

/// A header for rows whose column names are not known, such as a base table
/// delta that carries no schema. The columns are numbered by position.
pub struct PositionalHeader {
    columns: usize,
}

impl PositionalHeader {
    pub fn new(columns: usize) -> Self {
        Self { columns }
    }
}

impl CliTableHeader for PositionalHeader {
    fn cli_table_header(&self) -> impl Iterator<Item = CellStruct> {
        (0..self.columns).map(|index| format!("field {index}").cell())
    }
}

/// Prefixes another header with a z-weight column, such that a schema only
/// has to name its own columns.
pub struct ZWeightedHeader<Header>(pub Header);

impl<Header: CliTableHeader> CliTableHeader for ZWeightedHeader<Header> {
    fn cli_table_header(&self) -> impl Iterator<Item = CellStruct> {
        iter::once("zweight".cell()).chain(self.0.cli_table_header())
    }
}

/// An absent header contributes no columns, for tables that show a part of
/// their data only in some views.
impl<Header: CliTableHeader> CliTableHeader for Option<Header> {
    fn cli_table_header(&self) -> impl Iterator<Item = CellStruct> {
        self.iter().flat_map(|header| header.cli_table_header())
    }
}

/// Two headers side by side, for a table which shows the columns of more than
/// one schema.
pub struct ChainedHeader<First, Second>(pub First, pub Second);

impl<First: CliTableHeader, Second: CliTableHeader> CliTableHeader
    for ChainedHeader<First, Second>
{
    fn cli_table_header(&self) -> impl Iterator<Item = CellStruct> {
        self.0.cli_table_header().chain(self.1.cli_table_header())
    }
}

pub trait CliTableRow {
    fn as_cli_table_row(&self) -> impl Iterator<Item = CellStruct> + use<Self>;
}

// The forwarded iterator owns its cells, so it does not borrow `self`.
// Saying so is a refinement of the trait, which is what makes `.map()` over
// owned rows work in `to_cli_table_with`.
#[allow(refining_impl_trait)]
impl<T: ?Sized + CliTableRow> CliTableRow for &T {
    fn as_cli_table_row(&self) -> impl Iterator<Item = CellStruct> + use<T> {
        T::as_cli_table_row(*self)
    }
}

pub trait ToCliTableIterExt {
    fn to_cli_table_with(self, header: impl CliTableHeader) -> io::Result<TableDisplay>;
}

impl<Iter> ToCliTableIterExt for Iter
where
    Iter: IntoIterator,
    Iter::Item: CliTableRow,
{
    fn to_cli_table_with(self, header: impl CliTableHeader) -> io::Result<TableDisplay> {
        let double_h_line = HorizontalLine::new('=', '=', '≠', '=');
        let single_h_line = HorizontalLine::new('-', '-', '+', '-');
        self.into_iter()
            .map(|row| row.as_cli_table_row())
            .table()
            .border(
                Border::builder()
                    .top(double_h_line)
                    .bottom(double_h_line)
                    .build(),
            )
            .separator(Separator::builder().title(Some(single_h_line)).build())
            .title(header.cli_table_header())
            .display()
    }
}

/// An ordered collection of titled tables, the way the contents of a
/// transaction or of a query output are reported: one table per relation,
/// each under its own title, and sub-reports for parts that group several
/// of them.
///
/// A report is built eagerly because rendering a table is fallible while
/// [`Display`](std::fmt::Display) is not. Constructing the report keeps the
/// failure at the point where the rows are known and leaves printing
/// infallible.
pub struct CliReport {
    title: Option<String>,
    sections: Vec<CliSection>,
}

enum CliSection {
    Table { title: String, table: TableDisplay },
    Empty { title: String },
    Report(CliReport),
}

impl CliReport {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: Some(title.into()),
            sections: Vec::new(),
        }
    }
    /// A report without a heading of its own, for a single table or for
    /// sections that are meant to be spliced into another report.
    pub fn untitled() -> Self {
        Self {
            title: None,
            sections: Vec::new(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
    /// Renders `rows` into a titled section. Rows that are empty render as
    /// `<empty>` under their title rather than as a table without content.
    pub fn section<Rows>(
        &mut self,
        title: impl Into<String>,
        header: impl CliTableHeader,
        rows: Rows,
    ) -> io::Result<&mut Self>
    where
        Rows: IntoIterator,
        Rows::Item: CliTableRow,
    {
        let title = title.into();
        let mut rows = rows.into_iter().peekable();
        let section = if rows.peek().is_none() {
            CliSection::Empty { title }
        } else {
            CliSection::Table {
                title,
                table: rows.to_cli_table_with(header)?,
            }
        };
        self.sections.push(section);
        Ok(self)
    }
    /// Adds `report` as a sub-report, keeping its title as a sub-heading.
    pub fn nest(&mut self, report: CliReport) -> &mut Self {
        self.sections.push(CliSection::Report(report));
        self
    }
    /// Splices the sections of each `report` into this one, dropping its title.
    pub fn extend(&mut self, reports: impl IntoIterator<Item = CliReport>) -> &mut Self {
        self.sections
            .extend(reports.into_iter().flat_map(|report| report.sections));
        self
    }
    fn fmt_at_depth(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let rule = if depth == 0 { "======" } else { "------" };
        if let Some(title) = &self.title {
            writeln!(f, "{rule} {title} {rule}")?;
        }
        for section in &self.sections {
            match section {
                // A rendered table ends in a newline, so the `writeln!` leaves
                // a blank line between the sections.
                CliSection::Table { title, table } => write!(f, "{title}\n{table}")?,
                CliSection::Empty { title } => writeln!(f, "{title} <empty>")?,
                CliSection::Report(report) => report.fmt_at_depth(f, depth + 1)?,
            }
        }
        Ok(())
    }
}

impl fmt::Display for CliReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_at_depth(f, 0)
    }
}

/// Implemented by anything that describes itself as one or more tables,
/// such as the deltas that make up a transaction.
pub trait ToCliReport {
    fn to_cli_report(&self) -> io::Result<CliReport>;
}
