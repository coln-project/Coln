// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Typed sorted maps backed by parallel Hexane columns.

use std::ops::Range;

use hexane::AsColumnRef;

pub(crate) trait ColIndex {
    type Columns;
    type Ref<'a>: Copy;

    fn new_columns() -> Self::Columns;
    fn scope(columns: &Self::Columns, key: Self::Ref<'_>, range: Range<usize>) -> Range<usize>;
    fn iter_range(
        columns: &Self::Columns,
        range: Range<usize>,
    ) -> impl Iterator<Item = Self::Ref<'_>>;

    fn len(columns: &Self::Columns) -> usize;

    fn insert(columns: &mut Self::Columns, index: usize, key: Self::Ref<'_>);
    fn remove(columns: &mut Self::Columns, index: usize);
}


impl<T> ColIndex for T
where
    T: hexane::ColumnValueRef,
    for<'a> T::Get<'a>: Ord + AsColumnRef<T>,
{
    type Columns = hexane::Column<T>;
    type Ref<'a> = T::Get<'a>;

    fn new_columns() -> Self::Columns {
        hexane::Column::<T>::new()
    }

    fn scope(columns: &Self::Columns, key: Self::Ref<'_>, range: Range<usize>) -> Range<usize> {
        columns.scope_to_value(key, range)
    }

    fn iter_range(
        columns: &Self::Columns,
        range: Range<usize>,
    ) -> impl Iterator<Item = Self::Ref<'_>> {
        columns.iter_range(range)
    }

    fn insert(columns: &mut Self::Columns, index: usize, key: Self::Ref<'_>) {
        columns.insert(index, key);
    }

    fn remove(columns: &mut Self::Columns, index: usize) {
        columns.remove(index);
    }

    fn len(columns: &Self::Columns) -> usize {
        columns.len()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ColumnMap<K: ColIndex, V: ColIndex> {
    keys: K::Columns,
    values: V::Columns,
}

impl<K, V> ColumnMap<K, V>
where
    K: ColIndex,
    V: ColIndex,
{
    pub(crate) fn new() -> Self {
        ColumnMap {
            keys: K::new_columns(),
            values: V::new_columns(),
        }
    }

    fn len(&self) -> usize {
        K::len(&self.keys)
    }

    pub(crate) fn get(&self, key: K::Ref<'_>) -> impl Iterator<Item = V::Ref<'_>> {
        let range = K::scope(&self.keys, key, 0..self.len());
        V::iter_range(&self.values, range)
    }

    pub(crate) fn contains_key(&self, key: K::Ref<'_>) -> bool {
        self.get(key).next().is_some()
    }

    pub(crate) fn insert(&mut self, key: K::Ref<'_>, value: V::Ref<'_>) {
        let range = K::scope(&self.keys, key, 0..self.len());
        let value_range = V::scope(&self.values, value, range);
        let index = value_range.end;

        K::insert(&mut self.keys, index, key);
        V::insert(&mut self.values, index, value);
    }

    pub(crate) fn remove(&mut self, key: K::Ref<'_>) {
        let range = K::scope(&self.keys, key, 0..self.len());
        for index in range.rev() {
            K::remove(&mut self.keys, index);
            V::remove(&mut self.values, index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_lookup() {
        let mut map: ColumnMap<u32, u32> = ColumnMap::new();
        map.insert(1, 3);
        map.insert(4, 10);

        assert_eq!(map.get(1).next(), Some(3));
        assert_eq!(map.get(4).next(), Some(10));
        assert_eq!(map.get(2).next(), None);
        assert!(!map.contains_key(2));
    }
}
