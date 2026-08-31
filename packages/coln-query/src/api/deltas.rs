// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An interface for passing deltas of row-oriented data. There is
//! [ZRow], [TableDelta], [StoreDelta], and [DerivedDataDelta].

use crate::relational::{TupleValue, schema::EntityRef};
pub use crate::scalarial::ScalarTypedValue;

pub type ZWeight = i64;

/// An update of a row of some base table.
/// It either represents an insertion or a deletion of a row from a table,
/// see [`zweight`](`Self::zweight`) documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZRow {
    /// A ZWeight value ...
    /// - `== 0` behaves as if there was no change happening at all.
    /// - `n if n > 0` represents an insertion. If `n > 1` it is a duplicated
    ///   insertion, that is, the row is inserted n-times.
    /// - `n if n < 0` represents a deletion. If `n < 1` we remove the row
    ///   n-times.
    zweight: ZWeight,
    /// The row-oriented data.
    row: TupleValue,
}

impl ZRow {
    /// Create a new [`RowDelta`] but filters out deltas with a `zweight` of 0
    /// in which case `None` is returned.
    pub fn new(zweight: ZWeight, row: TupleValue) -> Option<Self> {
        if zweight == 0 {
            None
        } else {
            Some(Self { zweight, row })
        }
    }
    pub fn zweight(&self) -> ZWeight {
        self.zweight
    }
    pub fn into_row(self) -> TupleValue {
        self.row
    }
    /// Inverses the [ZWeight](Self::zweight) to retract a previously
    /// fed fact. Useful for rolling back a transaction.
    fn retract(&mut self) {
        self.zweight = -self.zweight;
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
    delta: Vec<ZRow>,
}

impl TableDelta {
    pub fn new<T: Into<EntityRef>>(for_entity: T, delta: Vec<ZRow>) -> Self {
        Self {
            entity: for_entity.into(),
            delta,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.delta.is_empty()
    }
    pub fn for_entity(&self) -> &EntityRef {
        &self.entity
    }
    pub fn delta(&self) -> &[ZRow] {
        &self.delta
    }
    pub fn into_delta(self) -> Vec<ZRow> {
        self.delta
    }
    /// Retracts all contained [`RowDelta`]s.
    fn retract(&mut self) {
        self.delta.iter_mut().for_each(|delta| delta.retract());
    }
}

// TODO: Offer proper cli-table based formatting, for anything that implements
// AsRef<Vec<TableDelta>>.
impl std::fmt::Display for TableDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Entity {}{}",
            self.for_entity(),
            if self.is_empty() { " <empty>" } else { "\n" }
        )?;
        for delta in self.delta() {
            writeln!(f, "{}", delta)?;
        }
        Ok(())
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
    pub fn with_deltas(deltas: Vec<TableDelta>) -> Self {
        Self { inner: deltas }
    }
    pub fn extend<I: IntoIterator<Item = TableDelta>>(&mut self, deltas: I) {
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
    /// # use coln_query::api::deltas::{ZRow, TableDelta, StoreDelta};
    /// # use coln_query::relational::TupleValue;
    /// # use coln_query::scalarial::ScalarTypedValue;
    /// #
    /// # let row: TupleValue = [ScalarTypedValue::from(9_i64)].into_iter().collect();
    /// # let row_delta = ZRow::new(1, row).unwrap();
    /// # let row_deltas = vec![row_delta.clone(), row_delta.clone()];
    /// # let table_delta = TableDelta::new("SomeTable", row_deltas);
    /// # let store_delta = StoreDelta::with_deltas(vec![table_delta]);
    /// assert_eq!(store_delta, store_delta.clone().retract().retract());
    /// ```
    pub fn retract(mut self) -> Self {
        self.inner.iter_mut().for_each(|table| table.retract());
        self
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
    pub fn with_deltas(deltas: Vec<TableDelta>) -> Self {
        Self { inner: deltas }
    }
    pub fn extend<I: IntoIterator<Item = TableDelta>>(&mut self, deltas: I) {
        self.inner.extend(deltas);
    }
    pub fn into_table_deltas(self) -> Vec<TableDelta> {
        self.inner
    }
}

impl std::fmt::Display for DerivedDataDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DerivedDataDelta{}",
            if self.is_empty() { " <empty>" } else { "\n" }
        )?;
        for delta in &self.inner {
            write!(f, "{}", delta)?;
        }
        Ok(())
    }
}

// TODO: Snapshot with row and column views.

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
        .expect("non-zero z-weight")
    }

    fn table_delta<T: Into<EntityRef>>(name: T) -> TableDelta {
        TableDelta::new(name.into(), vec![row_delta(), row_delta()])
    }

    #[test]
    fn retracting_twice_restores_the_original_state() {
        let store_delta =
            StoreDelta::with_deltas(vec![table_delta("BaseTable1"), table_delta("BaseTable2")]);

        assert_eq!(store_delta, store_delta.clone().retract().retract());
    }
}
