// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An interface for passing deltas of row-oriented data. There is
//! [ZRow], [TableDelta], [StoreDelta], and [DerivedDataDelta].

use crate::relational::schema::EntityRef;
use indexmap::{IndexMap, IndexSet};
// Re-exported, not merely imported: [`ZRow::new`] takes a [`TupleValue`] built
// from [`ScalarTypedValue`]s, so a caller outside this crate cannot construct
// one of the deltas this module is about without both names in reach.
pub use crate::relational::TupleValue;
pub use crate::scalarial::ScalarTypedValue;
use crate::utils::cli_table::{
    Cell, CellStruct, CliReport, CliTableRow, Justify, PositionalHeader, ToCliReport,
    ZWeightedHeader,
};
use std::{borrow::Borrow, iter, marker::PhantomData};

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

#[cfg(test)]
#[macro_export]
macro_rules! zrow {
    ( $zweight:literal [$($key:expr),* $(,)?]) => {{
        let tuple = [$( ScalarTypedValue::from($key) ),*].into_iter().collect::<TupleValue>();
        ZRow::new($zweight, tuple).expect("non-zero zweight")
    }};
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

#[cfg(test)]
impl<T: AsRef<[ZRow]>> PartialEq<T> for TableDelta {
    fn eq(&self, other: &T) -> bool {
        let data = other.as_ref();
        self.delta().len() == data.len() && self.delta().iter().all(|row| data.contains(row))
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

#[derive(Debug, Clone, Copy)]
pub struct MaybeUnconsolidated(());
#[derive(Debug, Clone, Copy)]
pub struct Consolidated(());

/// An update of the IDB, that is, insertions or deletions of derived facts.
#[derive(Clone, Debug)]
pub struct DerivedDataDelta<Marker> {
    /// Contains the delta in the IDB after applying a delta in the EDB (the
    /// latter is a [`StoreDelta`]).
    inner: Vec<TableDelta>,
    marker: PhantomData<Marker>,
}

impl Default for DerivedDataDelta<Consolidated> {
    fn default() -> Self {
        DerivedDataDelta::<Consolidated>::empty()
    }
}

impl<Marker> DerivedDataDelta<Marker> {
    pub fn is_empty(&self) -> bool {
        self.inner.iter().all(|table_delta| table_delta.is_empty())
    }
    pub fn into_table_deltas(self) -> Vec<TableDelta> {
        self.inner
    }
    pub fn is_consolidated(&self) -> bool {
        let mut map = IndexSet::new();
        for table_delta in self.inner.iter() {
            if !map.insert(table_delta.for_entity()) {
                return false;
            }
        }
        true
    }
}

impl DerivedDataDelta<Consolidated> {
    pub fn empty() -> Self {
        Self {
            inner: Vec::new(),
            marker: PhantomData,
        }
    }
    /// This count is equal to the count of distinct [`TableDelta`]s.
    pub fn size(&self) -> usize {
        self.inner.len()
    }
    /// The caller has to make sure that the provided [`TableDelta`]s are new.
    pub fn unsafe_extend(&mut self, deltas: impl IntoIterator<Item = TableDelta>) {
        self.inner.extend(deltas);
    }
}

impl DerivedDataDelta<MaybeUnconsolidated> {
    pub fn new(deltas: impl IntoIterator<Item = TableDelta>) -> Self {
        Self {
            inner: deltas.into_iter().collect(),
            marker: PhantomData,
        }
    }
    /// In case there are multiple [`TableDelta`]s for the same entity,
    /// this count is larger than the amount of updated entities.
    pub fn size(&self) -> usize {
        self.inner.len()
    }
    pub fn extend(&mut self, deltas: impl IntoIterator<Item = TableDelta>) {
        self.inner.extend(deltas);
    }
    /// Having called this function ensures that in case there are multiple
    /// [`TableDelta`]s for the same [Entity](EntityRef), there is only one
    /// [`TableDelta`] for each [Entity](EntityRef) left. Duplicated entries
    /// have been merged into one.
    pub fn consolidate(mut self) -> DerivedDataDelta<Consolidated> {
        let upper_bound = self.size();
        let consolidated = self.inner.iter_mut().fold(
            IndexMap::<&EntityRef, TableDelta>::with_capacity(upper_bound),
            |mut acc, table_delta| {
                acc.entry(&table_delta.entity)
                    .and_modify(|slot| slot.extend(std::mem::take(&mut table_delta.inner)))
                    .or_insert(TableDelta::new(
                        table_delta.for_entity().clone(),
                        std::mem::take(&mut table_delta.inner),
                    ));
                acc
            },
        );
        DerivedDataDelta::<Consolidated> {
            inner: consolidated
                .into_iter()
                .map(|(_, consolidated_table_delta)| consolidated_table_delta)
                .collect(),
            marker: PhantomData,
        }
    }
}

impl<Marker> ToCliReport for DerivedDataDelta<Marker> {
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
impl<T: Into<EntityRef>> std::ops::Index<T> for DerivedDataDelta<Consolidated> {
    type Output = TableDelta;

    fn index(&self, index: T) -> &Self::Output {
        let index = index.into();
        self.inner
            .iter()
            .find(|table_delta| table_delta.for_entity() == &index)
            .unwrap_or_else(|| panic!("Entity '{index}' not found"))
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
