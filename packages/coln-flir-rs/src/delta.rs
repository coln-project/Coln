// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An interface for passing deltas of row-oriented data. There is
//! [ZRow], [TableDelta], and [StoreDelta].

use crate::ir;
use indexmap::{IndexMap, IndexSet};
use std::{fmt, marker::PhantomData};

pub type ZWeight = i64;

/// An update of a row of some table. It either represents an insertion or a
/// deletion of a row from a table, see [`zweight`](Self::zweight()) documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZRow<Tuple> {
    /// See [Self::zweight] documentation.
    zweight: ZWeight,
    /// The row-oriented data.
    tuple: Tuple,
}

impl<Tuple> ZRow<Tuple> {
    /// Allows [`ZRow`]s with a `zweight` of 0.
    pub fn new_unchecked(zweight: ZWeight, tuple: Tuple) -> Self {
        Self { zweight, tuple }
    }
    /// Create a new [`ZRow`] but filters out deltas with a `zweight` of 0
    /// in which case `None` is returned.
    pub fn new(zweight: ZWeight, tuple: Tuple) -> Option<Self> {
        if zweight == 0 {
            None
        } else {
            Some(Self::new_unchecked(zweight, tuple))
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
    pub fn into_tuple(self) -> Tuple {
        self.tuple
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
    /// Flips the [ZWeight](Self::zweight) to retract a previously fed fact.
    /// Useful for rolling back a transaction.
    fn retract(&mut self) {
        self.zweight = -self.zweight;
    }
    /// Whether its [`zweight`](Self::zweight) is zero.
    #[inline]
    pub fn is_consolidated(&self) -> bool {
        self.zweight != 0
    }
    /// Returns `None` if its [`zweight`](Self::zweight) is zero and thus,
    /// the entry is meaningless and can be filtered out.
    /// Otherwise, `Some(self)` is returned.
    pub fn consolidate(self) -> Option<Self> {
        self.is_consolidated().then_some(self)
    }
}

impl<Tuple: fmt::Display> fmt::Display for ZRow<Tuple> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "zweight: {:02}, row: {}", self.zweight(), self.tuple)
    }
}

/// An update to some [entity](Self::for_entity()).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDelta<Tuple> {
    /// A unique identifier of an entity.
    entity: ir::Path,
    /// The row-oriented updates of the table.
    inner: Vec<ZRow<Tuple>>,
}

impl<Tuple> TableDelta<Tuple> {
    pub fn new(
        for_entity: impl Into<ir::Path>,
        delta: impl IntoIterator<Item = ZRow<Tuple>>,
    ) -> Self {
        Self {
            entity: for_entity.into(),
            inner: delta.into_iter().collect(),
        }
    }
    pub fn extend(&mut self, rows: impl IntoIterator<Item = ZRow<Tuple>>) {
        self.inner.extend(rows);
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn for_entity(&self) -> &ir::Path {
        &self.entity
    }
    pub fn delta(&self) -> &[ZRow<Tuple>] {
        &self.inner
    }
    pub fn iter(&self) -> impl Iterator<Item = &ZRow<Tuple>> {
        self.into_iter()
    }
    pub fn into_delta(self) -> Vec<ZRow<Tuple>> {
        self.inner
    }
    /// Retracts all contained [`ZRow`]s.
    fn retract(&mut self) {
        self.inner.iter_mut().for_each(|delta| delta.retract());
    }
    /// If each contained [`ZRow::zweight()`] is non-zero.
    pub fn is_consolidated(&self) -> bool {
        self.inner.iter().all(|zrow| zrow.is_consolidated())
    }
    /// After calling this, it is guaranteed that each contained
    /// [`ZRow::zweight()`] is non-zero.
    pub fn consolidate(mut self) -> Self {
        self.inner = self
            .inner
            .into_iter()
            .filter_map(|zrow| zrow.consolidate())
            .collect();
        self
    }
}

impl<Tuple> IntoIterator for TableDelta<Tuple> {
    type Item = ZRow<Tuple>;
    type IntoIter = std::vec::IntoIter<ZRow<Tuple>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, Tuple> IntoIterator for &'a TableDelta<Tuple> {
    type Item = &'a ZRow<Tuple>;
    type IntoIter = std::slice::Iter<'a, ZRow<Tuple>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

/// A marker indicating that the [`TableDelta`]s stored in [`StoreDelta`]
/// may be [unconsolidated](StoreDelta::is_consolidated()).
#[derive(Debug, Clone, Copy)]
pub struct MaybeUnconsolidated(());
/// A marker indicating that the [`TableDelta`]s stored in [`StoreDelta`]
/// are guaranteed to be [consolidated](StoreDelta::is_consolidated()).
#[derive(Debug, Clone, Copy)]
pub struct Consolidated(());

/// An update of the EDB or IDB, that is, insertions or deletions of either
/// base or derived facts. The context determines of which exactly.
#[derive(Clone, Debug)]
pub struct StoreDelta<Marker, Tuple> {
    inner: Vec<TableDelta<Tuple>>,
    marker: PhantomData<Marker>,
}

impl<Tuple> Default for StoreDelta<Consolidated, Tuple> {
    fn default() -> Self {
        StoreDelta::<Consolidated, Tuple>::empty()
    }
}

impl<Marker, Tuple> StoreDelta<Marker, Tuple> {
    pub fn is_empty(&self) -> bool {
        self.inner.iter().all(|table_delta| table_delta.is_empty())
    }
    pub fn into_table_deltas(self) -> Vec<TableDelta<Tuple>> {
        self.inner
    }
    /// Returns `true` if there is exactly one [`TableDelta`] for each
    /// [`TableDelta::for_entity()`].
    pub fn is_consolidated(&self) -> bool {
        let mut map = IndexSet::new();
        for table_delta in self.inner.iter() {
            if !map.insert(table_delta.for_entity()) {
                return false;
            }
        }
        true
    }
    /// Inverses all facts of each contained [`TableDelta`]. Useful for
    /// retractions and rolling back a transaction. Applying this twice yields
    /// back the original state.
    pub fn retract(mut self) -> Self {
        self.inner.iter_mut().for_each(|table| table.retract());
        self
    }
}

impl<Tuple> StoreDelta<Consolidated, Tuple> {
    pub fn empty() -> Self {
        Self {
            inner: Vec::new(),
            marker: PhantomData,
        }
    }
    /// This count is guaranteed to be equal to the count of distinct,
    /// contained [`TableDelta`]s.
    pub fn size(&self) -> usize {
        self.inner.len()
    }
    /// The caller has to make sure that the provided [`TableDelta`]s are not
    /// from an entity which is already present.
    pub fn unsafe_extend(&mut self, deltas: impl IntoIterator<Item = TableDelta<Tuple>>) {
        self.inner.extend(deltas);
    }
}

impl<Tuple> StoreDelta<MaybeUnconsolidated, Tuple> {
    pub fn new(deltas: impl IntoIterator<Item = TableDelta<Tuple>>) -> Self {
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
    pub fn extend(&mut self, deltas: impl IntoIterator<Item = TableDelta<Tuple>>) {
        self.inner.extend(deltas);
    }
    /// Having called this function ensures that in case there are multiple
    /// [`TableDelta`]s for the same [Entity](ir::Path), there is only one
    /// [`TableDelta`] for each [Entity](ir::Path) left. Duplicated entries
    /// have been merged into one.
    pub fn consolidate(mut self) -> StoreDelta<Consolidated, Tuple> {
        let upper_bound = self.size();
        let consolidated = self.inner.iter_mut().fold(
            IndexMap::<&ir::Path, TableDelta<Tuple>>::with_capacity(upper_bound),
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
        StoreDelta::<Consolidated, Tuple> {
            inner: consolidated
                .into_iter()
                .map(|(_, consolidated_table_delta)| consolidated_table_delta)
                .collect(),
            marker: PhantomData,
        }
    }
}

impl<Tuple> FromIterator<TableDelta<Tuple>> for StoreDelta<MaybeUnconsolidated, Tuple> {
    fn from_iter<T: IntoIterator<Item = TableDelta<Tuple>>>(iter: T) -> Self {
        Self::new(iter)
    }
}
