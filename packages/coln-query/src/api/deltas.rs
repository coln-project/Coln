// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An interface for passing deltas of row-oriented data. There is
//! [ZRow], [TableDelta], [StoreDelta], and [DerivedDataDelta].

use crate::relational::schema::EntityRef;
// Re-exported, not merely imported: [`ZRow::new`] takes a [`TupleValue`] built
// from [`ScalarTypedValue`]s, so a caller outside this crate cannot construct
// one of the deltas this module is about without both names in reach.
pub use crate::relational::TupleValue;
pub use crate::scalarial::ScalarTypedValue;
use crate::utils::cli_table::{
    Cell, CellStruct, CliReport, CliTableRow, Justify, PositionalHeader, ToCliReport,
    ZWeightedHeader,
};
use std::{borrow::Borrow, iter};

pub type ZWeight = i64;

/// An update of a row of some table. It either represents an insertion or a
/// deletion of a row from a table, see [`zweight`](Self::zweight()) documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZRow {
    /// See [Self::zweight] documentation.
    zweight: ZWeight,
    /// The row-oriented data.
    row: TupleValue,
}

impl ZRow {
    /// Allows [`ZRow`]s  with a `zweight` of 0. For internal use only.
    pub(crate) fn new_unchecked(zweight: ZWeight, row: TupleValue) -> Self {
        Self { zweight, row }
    }
    /// Create a new [`ZRow`] but filters out deltas with a `zweight` of 0
    /// in which case `None` is returned.
    pub fn new(zweight: ZWeight, row: TupleValue) -> Option<Self> {
        if zweight == 0 {
            None
        } else {
            Some(Self::new_unchecked(zweight, row))
        }
    }
    /// A ZWeight value ...
    /// - `== 0` behaves as if there was no change happening at all.
    /// - `n if n > 0` represents an insertion. If `n > 1` it is a duplicated
    ///   insertion, that is, the row is inserted n-times.
    /// - `n if n < 0` represents a deletion. If `n < 1` we remove the row
    ///   n-times.
    pub fn zweight(&self) -> ZWeight {
        self.zweight
    }
    /// Returns `true` if the [`zweight`](Self::zweight) is (strictly) positive.
    ///
    /// Can occur for both deltas (changes) or sets (snapshots).
    pub fn is_assertion(&self) -> bool {
        self.zweight() > 0
    }
    /// Returns `true` if the [`zweight`](Self::zweight) is negative in which
    /// case a previous assertion of this row is now retracted.
    ///
    /// Only occurs if the [`ZRow`] stores a delta (change).
    pub fn is_retraction(&self) -> bool {
        self.zweight() < 0
    }
    pub fn into_row(self) -> TupleValue {
        self.row
    }
    /// Flips the [ZWeight](Self::zweight) to retract a previously fed fact.
    /// Useful for rolling back a transaction.
    fn retract(&mut self) {
        self.zweight = -self.zweight;
    }
}

impl CliTableRow for ZRow {
    /// The `zweight` is prepended to the `row`.
    fn as_cli_table_row(&self) -> impl Iterator<Item = CellStruct> + use<> {
        iter::once(self.zweight().to_string().cell().justify(Justify::Right))
            .chain(self.row.data.iter().map(|field| {
                let justification = match field {
                    ScalarTypedValue::String(_) => Justify::Left,
                    _ => Justify::Right,
                };
                field.to_string().cell().justify(justification)
            }))
            // Collected so that the iterator owns its cells instead of
            // borrowing the row it was built from.
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl std::fmt::Display for ZRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "zweight: {:02}, row: {}", self.zweight(), self.row)
    }
}

/// An update to a base table (part of the EDB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDelta {
    /// A unique identifier of a table.
    entity: EntityRef,
    /// The row-oriented updates of the table.
    inner: Vec<ZRow>,
}

impl TableDelta {
    pub fn new(for_entity: impl Into<EntityRef>, delta: impl IntoIterator<Item = ZRow>) -> Self {
        Self {
            entity: for_entity.into(),
            inner: delta.into_iter().collect(),
        }
    }
    pub fn extend(&mut self, rows: impl IntoIterator<Item = ZRow>) {
        self.inner.extend(rows);
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &ZRow> {
        self.into_iter()
    }
    pub fn for_entity(&self) -> &EntityRef {
        &self.entity
    }
    pub fn delta(&self) -> &[ZRow] {
        &self.inner
    }
    pub fn into_delta(self) -> Vec<ZRow> {
        self.inner
    }
    /// Retracts all contained [`ZRow`]s.
    fn retract(&mut self) {
        self.inner.iter_mut().for_each(|delta| delta.retract());
    }
    /// A base table delta carries no schema, so its columns are numbered by
    /// position, with the arity taken from the first row.
    fn cli_table_header(&self) -> ZWeightedHeader<PositionalHeader> {
        let columns = self.inner.first().map_or(0, |row| row.row.data.len());
        ZWeightedHeader(PositionalHeader::new(columns))
    }
}

impl IntoIterator for TableDelta {
    type Item = ZRow;
    type IntoIter = std::vec::IntoIter<ZRow>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a TableDelta {
    type Item = &'a ZRow;
    type IntoIter = std::slice::Iter<'a, ZRow>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

pub trait ZRowIterExt: Iterator + Sized {
    fn assertions(self) -> impl Iterator<Item = Self::Item>;
    fn retractions(self) -> impl Iterator<Item = Self::Item>;
}

impl<I> ZRowIterExt for I
where
    I: Iterator + Sized,
    I::Item: Borrow<ZRow>,
{
    fn assertions(self) -> impl Iterator<Item = Self::Item> {
        self.filter(|row| row.borrow().is_assertion())
    }

    fn retractions(self) -> impl Iterator<Item = Self::Item> {
        self.filter(|row| row.borrow().is_retraction())
    }
}

impl ToCliReport for TableDelta {
    fn to_cli_report(&self) -> std::io::Result<CliReport> {
        let mut report = CliReport::untitled();
        report.section(
            self.for_entity().to_string(),
            self.cli_table_header(),
            self.delta(),
        )?;
        Ok(report)
    }
}

/// An update of the EDB, that is, insertions or deletions of base facts.
///
/// Ideally, there is at most one entry per table.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct StoreDelta {
    inner: Vec<TableDelta>,
}

impl StoreDelta {
    pub fn empty() -> Self {
        Self { inner: Vec::new() }
    }
    pub fn new(deltas: impl IntoIterator<Item = TableDelta>) -> Self {
        Self {
            inner: deltas.into_iter().collect(),
        }
    }
    pub fn extend(&mut self, deltas: impl IntoIterator<Item = TableDelta>) {
        self.inner.extend(deltas);
    }
    pub fn into_table_deltas(self) -> Vec<TableDelta> {
        self.inner
    }
    /// Inverses all facts of each contained [`TableDelta`]. Useful for
    /// retractions and rolling back a transaction. Applying this twice yields
    /// the original state, that is:
    ///
    /// ```
    /// # use coln_query::api::deltas::{
    /// #     ScalarTypedValue, StoreDelta, TableDelta, TupleValue, ZRow,
    /// # };
    /// #
    /// # let row: TupleValue = [ScalarTypedValue::from(9_i64)].into_iter().collect();
    /// # let row_delta = ZRow::new(1, row).unwrap();
    /// # let row_deltas = [row_delta.clone(), row_delta.clone()];
    /// # let table_delta = TableDelta::new("SomeTable", row_deltas);
    /// # let store_delta = StoreDelta::new([table_delta]);
    /// assert_eq!(store_delta, store_delta.clone().retract().retract());
    /// ```
    pub fn retract(mut self) -> Self {
        self.inner.iter_mut().for_each(|table| table.retract());
        self
    }
}

impl FromIterator<TableDelta> for StoreDelta {
    fn from_iter<T: IntoIterator<Item = TableDelta>>(iter: T) -> Self {
        Self::new(iter)
    }
}

/// This is useful if keying the TableDeltas in a [`std::collections::HashMap`]
/// to avoid duplicate [`TableDelta`]s per [`EntityRef`].
impl<U> FromIterator<(U, TableDelta)> for StoreDelta {
    fn from_iter<T: IntoIterator<Item = (U, TableDelta)>>(iter: T) -> Self {
        Self::new(iter.into_iter().map(|(_, table_delta)| table_delta))
    }
}

impl ToCliReport for StoreDelta {
    fn to_cli_report(&self) -> std::io::Result<CliReport> {
        let mut report = CliReport::new("StoreDelta");
        report.extend(
            self.inner
                .iter()
                .map(|delta| delta.to_cli_report())
                .collect::<std::io::Result<Vec<_>>>()?,
        );
        Ok(report)
    }
}

/// An update of the IDB, that is, insertions or deletions of derived facts.
#[derive(Default, Clone, Debug)]
pub struct DerivedDataDelta {
    /// Contains the delta in the IDB after applying a delta in the EDB (the
    /// latter is a [`StoreDelta`]).
    inner: Vec<TableDelta>,
}

impl DerivedDataDelta {
    pub fn empty() -> Self {
        Self { inner: Vec::new() }
    }
    pub fn is_empty(&self) -> bool {
        self.inner.iter().all(|table_delta| table_delta.is_empty())
    }
    pub fn new(deltas: impl IntoIterator<Item = TableDelta>) -> Self {
        Self {
            inner: deltas.into_iter().collect(),
        }
    }
    pub fn extend(&mut self, deltas: impl IntoIterator<Item = TableDelta>) {
        self.inner.extend(deltas);
    }
    pub fn into_table_deltas(self) -> Vec<TableDelta> {
        self.inner
    }
}

impl ToCliReport for DerivedDataDelta {
    fn to_cli_report(&self) -> std::io::Result<CliReport> {
        let mut report = CliReport::new("DerivedDataDelta");
        report.extend(
            self.inner
                .iter()
                .map(|delta| delta.to_cli_report())
                .collect::<std::io::Result<Vec<_>>>()?,
        );
        Ok(report)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn row_delta() -> ZRow {
        ZRow::new(
            2,
            [
                ScalarTypedValue::from("String"),
                ScalarTypedValue::from(1_i64),
            ]
            .into_iter()
            .collect(),
        )
        .expect("non-zero zweight")
    }

    fn table_delta(name: impl Into<EntityRef>) -> TableDelta {
        TableDelta::new(name.into(), [row_delta(), row_delta()])
    }

    #[test]
    fn retracting_twice_restores_the_original_state() {
        let store_delta = StoreDelta::new([table_delta("BaseTable1"), table_delta("BaseTable2")]);

        assert_eq!(store_delta, store_delta.clone().retract().retract());
    }

    #[test]
    fn report_titles_one_table_per_delta() {
        let store_delta =
            StoreDelta::new([table_delta("BaseTable1"), TableDelta::new("BaseTable2", [])]);

        let report = store_delta
            .to_cli_report()
            .expect("report renders")
            .to_string();

        println!("{report}");

        assert!(report.starts_with("====== StoreDelta ======\n"));
        assert!(report.contains("BaseTable1\n"));
        assert!(report.contains("zweight"));
        // The columns of a base table delta are numbered, as it carries no
        // schema to name them by.
        assert!(report.contains("field 0"));
        assert!(report.contains("field 1"));
        assert!(report.contains("String"));
        // A delta without rows says so instead of rendering an empty table.
        assert!(report.contains("BaseTable2 <empty>"));
    }

    #[test]
    fn nested_reports_are_sub_headings() {
        let store_delta = StoreDelta::new([table_delta("BaseTable1")]);
        let derived_delta = DerivedDataDelta::new([table_delta("DerivedTable1")]);

        let mut transaction = CliReport::new("Transaction");
        transaction
            .nest(store_delta.to_cli_report().expect("report renders"))
            .nest(derived_delta.to_cli_report().expect("report renders"));
        let report = transaction.to_string();

        println!("{report}");

        assert!(report.starts_with("====== Transaction ======\n"));
        assert!(report.contains("------ StoreDelta ------"));
        assert!(report.contains("------ DerivedDataDelta ------"));
        assert!(report.contains("BaseTable1\n"));
        assert!(report.contains("DerivedTable1\n"));
    }
}
