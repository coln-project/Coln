// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The batch backend's store contract can be implemented outside
//! coln-query with nothing but `coln_query::api::batch`, the way coln-store
//! would implement it.

use std::collections::HashMap;

use coln_query::api::batch::{
    ColId, Dictionary, Key, RelationSource, SortedTable, SourceId, TableHandle, check_contract,
};

/// A table as encoded rows, sorted per request.
struct Table {
    arity: usize,
    rows: Vec<Vec<Key>>,
}

struct Store {
    dictionary: Dictionary,
    tables: HashMap<SourceId, Table>,
}

struct Handle<'a>(&'a Table);

struct Sorted {
    order: Vec<ColId>,
    rows: Vec<Vec<Key>>,
}

impl SortedTable for Sorted {
    fn arity(&self) -> usize {
        self.order.len()
    }

    fn len(&self) -> usize {
        self.rows.len()
    }

    fn sort_order(&self) -> &[ColId] {
        &self.order
    }

    fn value(&self, row: usize, col: ColId) -> Key {
        self.rows[row][col]
    }
}

impl TableHandle for Handle<'_> {
    fn arity(&self) -> usize {
        self.0.arity
    }

    fn sorted(&self, order: &[ColId]) -> Box<dyn SortedTable + '_> {
        let mut rows = self.0.rows.clone();
        rows.sort_by(|a, b| order.iter().map(|&c| a[c]).cmp(order.iter().map(|&c| b[c])));
        rows.dedup();
        Box::new(Sorted {
            order: order.to_vec(),
            rows,
        })
    }
}

impl RelationSource for Store {
    fn table(&self, table: &SourceId) -> Option<Box<dyn TableHandle + '_>> {
        let found = self.tables.get(table)?;
        Some(Box::new(Handle(found)))
    }

    fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }
}

#[test]
fn a_store_outside_coln_query_can_serve_the_batch_backend() {
    let mut dictionary = Dictionary::new();
    let (a, b) = (dictionary.intern("a"), dictionary.intern("b"));
    let edge = SourceId::from("edge");
    let store = Store {
        dictionary,
        tables: HashMap::from([(
            edge.clone(),
            Table {
                arity: 2,
                rows: vec![vec![b, a], vec![a, b], vec![a, a]],
            },
        )]),
    };

    let handle = store.table(&edge).expect("the store has this table");
    assert_eq!(handle.arity(), 2);
    for order in [[0, 1], [1, 0]] {
        let table = handle.sorted(&order);
        check_contract(&*table);
        assert_eq!(table.sort_order(), order);
    }
    assert!(store.table(&SourceId::from("missing")).is_none());
}
