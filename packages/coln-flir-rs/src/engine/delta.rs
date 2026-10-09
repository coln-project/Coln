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
    /// Converts the contained tuple via `f`, keeping the
    /// [`zweight`](Self::zweight) as is.
    pub fn try_map<U, E>(self, f: impl FnOnce(Tuple) -> Result<U, E>) -> Result<ZRow<U>, E> {
        Ok(ZRow {
            zweight: self.zweight,
            tuple: f(self.tuple)?,
        })
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
    /// Converts each contained tuple via `f`, which also receives the
    /// [entity](Self::for_entity()) the tuple belongs to.
    pub fn try_map<U, E>(self, f: impl Fn(Tuple) -> Result<U, E>) -> Result<TableDelta<U>, E> {
        let Self { entity, inner } = self;
        let inner = inner
            .into_iter()
            .map(|zrow| zrow.try_map(|tuple| f(tuple)))
            .collect::<Result<_, _>>()?;
        Ok(TableDelta { entity, inner })
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
/// may be not [fully consolidated](StoreDelta::is_fully_consolidated()) but
/// still partially consolidated. This means that subsequent insertions for the
/// same entity are guaranteed to be collapsed into one but never across
/// entities. Example:
///
/// ```text
/// T1 u
/// T1 v
/// T2 w
/// T1 x
/// ```
/// becomes
/// ```text
/// T1 (u, v)
/// T2 w
/// T1 x      // T2 prevents this one from being collapsed into the earlier T1.
/// ```
#[derive(Debug, Clone, Copy)]
pub struct PartialConsolidated;
/// A marker indicating that the [`TableDelta`]s stored in [`StoreDelta`]
/// are guaranteed to be [fully consolidated](StoreDelta::is_fully_consolidated()).
/// See [StoreDelta::consolidate()] for its meaning.
#[derive(Debug, Clone, Copy)]
pub struct Consolidated;

/// An update of the EDB or IDB, that is, insertions or deletions of either
/// base or derived facts. The context determines of which exactly.
#[derive(Clone, Debug)]
pub struct StoreDelta<Marker, Tuple> {
    inner: Vec<TableDelta<Tuple>>,
    marker: PhantomData<Marker>,
}

impl<M, Tuple> Default for StoreDelta<M, Tuple> {
    fn default() -> Self {
        StoreDelta::<M, Tuple>::empty()
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
    pub fn is_fully_consolidated(&self) -> bool {
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
    /// Converts each contained tuple via `f`, which also receives the
    /// [entity](TableDelta::for_entity()) the tuple belongs to. The `Marker`
    /// is preserved as entities are left untouched.
    pub fn try_map<U, E>(
        self,
        f: impl Fn(Tuple) -> Result<U, E>,
    ) -> Result<StoreDelta<Marker, U>, E> {
        let inner = self
            .inner
            .into_iter()
            .map(|table_delta| table_delta.try_map(&f))
            .collect::<Result<_, _>>()?;
        Ok(StoreDelta {
            inner,
            marker: PhantomData,
        })
    }

    pub fn empty() -> Self {
        Self {
            inner: Vec::new(),
            marker: PhantomData,
        }
    }

    /// The number of TableDelta's currently in the store, whether consolidated
    /// or not
    pub fn size(&self) -> usize {
        self.inner.len()
    }

    /// The caller has to make sure that the provided [`TableDelta`]s are not
    /// from an entity which is already present.
    pub fn unsafe_extend(&mut self, deltas: impl IntoIterator<Item = TableDelta<Tuple>>) {
        self.inner.extend(deltas);
    }
}

impl<Tuple> StoreDelta<PartialConsolidated, Tuple> {
    pub fn new(deltas: impl IntoIterator<Item = TableDelta<Tuple>>) -> Self {
        let mut store_delta = Self {
            inner: Vec::new(),
            marker: PhantomData,
        };
        store_delta.extend(deltas);
        store_delta
    }
    pub fn extend(&mut self, deltas: impl IntoIterator<Item = TableDelta<Tuple>>) {
        let deltas = deltas.into_iter();
        // Merging adjacent deltas of the same entity only shrinks the count,
        // so the iterator's length is an upper bound on what we push.
        self.inner.reserve(deltas.size_hint().0);
        for delta in deltas {
            self.push(delta);
        }
    }
    pub fn push(&mut self, delta: TableDelta<Tuple>) {
        if let Some(last) = self.inner.last_mut()
            && last.for_entity() == delta.for_entity()
        {
            last.extend(delta.inner);
        } else {
            self.inner.push(delta);
        }
    }

    /// Having called this function ensures that in case there are multiple
    /// [`TableDelta`]s for the same [Entity](ir::Path), there is only one
    /// [`TableDelta`] for each [Entity](ir::Path) left. Duplicated entries
    /// have been merged into one.
    ///
    /// # Safety
    ///
    /// Note that this changes the order of **[`ZRow`]s** _across entities_ but
    /// not _within an entity_. Also, the order of the entities themselves is
    /// preserved. Example:
    /// ```text
    /// T1 u
    /// T1 v
    /// T2 w
    /// T1 x
    /// ```
    /// becomes
    /// ```text
    /// T1 (u, v, x) // T1's zrows keep their order.
    /// T2 w         // T2 still comes after T1.
    ///              // But global order is now u, v, x, w and not u, v, w, x.
    /// ```
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

impl<Tuple> FromIterator<TableDelta<Tuple>> for StoreDelta<PartialConsolidated, Tuple> {
    fn from_iter<T: IntoIterator<Item = TableDelta<Tuple>>>(iter: T) -> Self {
        Self::new(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(entity: &str, tuples: &[&'static str]) -> TableDelta<&'static str> {
        TableDelta::new(
            entity,
            tuples.iter().map(|&tuple| ZRow::new_unchecked(1, tuple)),
        )
    }

    /// Flattens a [`StoreDelta`] into its entities and their tuples, in order.
    fn layout<Marker>(
        store_delta: StoreDelta<Marker, &'static str>,
    ) -> Vec<(String, Vec<&'static str>)> {
        store_delta
            .into_table_deltas()
            .into_iter()
            .map(|table_delta| {
                let entity = table_delta.for_entity().to_string();
                let tuples = table_delta.into_iter().map(ZRow::into_tuple).collect();
                (entity, tuples)
            })
            .collect()
    }

    #[test]
    fn new_merges_adjacent_deltas_of_same_entity_only() {
        let store_delta = StoreDelta::new([
            table("T1", &["u"]),
            table("T1", &["v"]),
            table("T2", &["w"]),
            table("T1", &["x"]),
        ]);

        assert_eq!(store_delta.size(), 3);
        assert!(!store_delta.is_fully_consolidated());
        assert_eq!(
            layout(store_delta),
            [
                ("T1".to_string(), vec!["u", "v"]),
                ("T2".to_string(), vec!["w"]),
                ("T1".to_string(), vec!["x"]),
            ]
        );
    }

    #[test]
    fn extend_merges_into_last_existing_delta() {
        let mut store_delta = StoreDelta::new([table("T1", &["u"])]);
        store_delta.extend([table("T1", &["v"]), table("T2", &["w"])]);
        store_delta.push(table("T2", &["x"]));

        assert_eq!(
            layout(store_delta),
            [
                ("T1".to_string(), vec!["u", "v"]),
                ("T2".to_string(), vec!["w", "x"]),
            ]
        );
    }

    #[test]
    fn consolidate_merges_across_entities_preserving_order() {
        let store_delta = StoreDelta::new([
            table("T1", &["u"]),
            table("T1", &["v"]),
            table("T2", &["w"]),
            table("T1", &["x"]),
            table("T3", &["y"]),
            table("T2", &["z"]),
        ])
        .consolidate();

        assert_eq!(store_delta.size(), 3);
        assert!(store_delta.is_fully_consolidated());
        assert_eq!(
            layout(store_delta),
            [
                ("T1".to_string(), vec!["u", "v", "x"]),
                ("T2".to_string(), vec!["w", "z"]),
                ("T3".to_string(), vec!["y"]),
            ]
        );
    }
}
