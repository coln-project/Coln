// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::super::super::relation::{RelationRef, TupleValue};
use super::super::dbsp::{AsDbspRelation, new_relation};
use super::super::schema::{SchemaTuple, TupleKey};
use std::rc::Rc;

/// If the schema is not coalesced, this helper will compact the tuple key and
/// tuple value of the relation to _only_ carry the active fields of the schema.
///
/// This is important for set operations like union, intersection, and difference,
/// which require equality of schemas to function correctly.
pub fn coalesce_helper(relation: RelationRef) -> RelationRef {
    let relation_ref = relation.borrow();

    if relation_ref.as_dbsp().schema().is_coalesced() {
        drop(relation_ref);
        return relation;
    }

    let schema = relation_ref.as_dbsp().schema().coalesce();
    let coalesced = relation_ref.as_dbsp().stream().map_index({
        let relation = Rc::clone(&relation);
        move |(key, tuple)| {
            let relation_ref = relation.borrow();
            let schema = relation_ref.as_dbsp().schema();
            let key: TupleKey = SchemaTuple::new(&schema.key, key).coalesce().collect();
            let tuple: TupleValue = SchemaTuple::new(&schema.tuple, tuple).coalesce().collect();
            (key, tuple)
        }
    });

    new_relation(schema, coalesced)
}
