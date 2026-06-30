// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module is only available if compiling with the `test` feature.
//! It provides helpers for testing and benchmarking purposes.

use crate::{
    IncDataLog,
    optimizer::NoOptimizer,
    relation::{RelationSchema, TupleKey, TupleValue},
    scalar::ScalarTypedValue,
};
use std::fmt::Debug;

pub fn setup_inc_data_log() -> IncDataLog<NoOptimizer> {
    IncDataLog::default()
}

pub trait InputEntity: Into<TupleKey> + Into<TupleValue> + Clone + Debug {
    fn schema() -> RelationSchema;
}

#[derive(Clone, Debug)]
pub struct Person {
    person_id: u64,
    name: String,
    age: u64,
    profession_id: u64,
}

impl InputEntity for Person {
    fn schema() -> RelationSchema {
        RelationSchema::new(
            "person",
            ["person_id", "name", "age", "profession_id"],
            ["person_id"],
        )
        .expect("Correct schema definition")
    }
}

impl From<Person> for TupleKey {
    fn from(person: Person) -> Self {
        TupleKey {
            data: vec![ScalarTypedValue::Uint(person.person_id)],
        }
    }
}

impl From<Person> for TupleValue {
    fn from(person: Person) -> Self {
        TupleValue {
            data: vec![
                ScalarTypedValue::Uint(person.person_id),
                ScalarTypedValue::String(person.name),
                ScalarTypedValue::Uint(person.age),
                ScalarTypedValue::Uint(person.profession_id),
            ],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Profession {
    profession_id: u64,
    name: String,
}

impl InputEntity for Profession {
    fn schema() -> RelationSchema {
        RelationSchema::new("profession", ["profession_id", "name"], ["profession_id"])
            .expect("Correct schema definition")
    }
}

impl From<Profession> for TupleKey {
    fn from(profession: Profession) -> Self {
        TupleKey {
            data: vec![ScalarTypedValue::Uint(profession.profession_id)],
        }
    }
}

impl From<Profession> for TupleValue {
    fn from(profession: Profession) -> Self {
        TupleValue {
            data: vec![
                ScalarTypedValue::Uint(profession.profession_id),
                ScalarTypedValue::String(profession.name),
            ],
        }
    }
}

pub fn person_profession_data() -> [(Vec<Person>, Vec<Profession>); 1] {
    [(
        vec![
            Person {
                person_id: 0,
                name: "Alice".to_string(),
                age: 20,
                profession_id: 0,
            },
            Person {
                person_id: 1,
                name: "Bob".to_string(),
                age: 30,
                profession_id: 1,
            },
            Person {
                person_id: 2,
                name: "Charlie".to_string(),
                age: 40,
                profession_id: 0,
            },
        ],
        vec![
            Profession {
                profession_id: 0,
                name: "Engineer".to_string(),
            },
            Profession {
                profession_id: 1,
                name: "Doctor".to_string(),
            },
        ],
    )]
}

#[derive(Copy, Clone, Debug)]
pub struct PlainRelation {
    a: u64,
    b: u64,
    c: u64,
}

impl PlainRelation {
    pub fn new(a: u64, b: u64, c: u64) -> Self {
        Self { a, b, c }
    }
    const STEPS: usize = 1;
    pub fn test_data_1() -> [Vec<PlainRelation>; Self::STEPS] {
        [vec![
            PlainRelation::new(1, 2, 3),
            PlainRelation::new(4, 5, 6),
            PlainRelation::new(7, 8, 9),
        ]]
    }
    pub fn test_data_2() -> [Vec<PlainRelation>; Self::STEPS] {
        [vec![PlainRelation::new(1, 2, 3)]]
    }
    pub fn test_data_3() -> [Vec<PlainRelation>; Self::STEPS] {
        [vec![PlainRelation::new(4, 5, 6)]]
    }
}

impl InputEntity for PlainRelation {
    fn schema() -> RelationSchema {
        RelationSchema::new("plain", ["a", "b", "c"], []).expect("Correct schema definition")
    }
}

impl From<PlainRelation> for TupleKey {
    fn from(fact: PlainRelation) -> Self {
        TupleKey { data: vec![] }
    }
}

impl From<PlainRelation> for TupleValue {
    fn from(fact: PlainRelation) -> Self {
        TupleValue {
            data: vec![
                ScalarTypedValue::Uint(fact.a),
                ScalarTypedValue::Uint(fact.b),
                ScalarTypedValue::Uint(fact.c),
            ],
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Edge {
    from: u64,
    to: u64,
    weight: u64,
    active: bool,
}

impl Edge {
    pub fn new(from: u64, to: u64, weight: u64) -> Self {
        Self {
            from,
            to,
            weight,
            active: true,
        }
    }
}

impl InputEntity for Edge {
    fn schema() -> RelationSchema {
        RelationSchema::new("edges", ["from", "to", "weight", "active"], ["from", "to"])
            .expect("Correct schema definition")
    }
}

impl From<Edge> for TupleKey {
    fn from(edge: Edge) -> Self {
        TupleKey {
            data: vec![
                ScalarTypedValue::Uint(edge.from),
                ScalarTypedValue::Uint(edge.to),
            ],
        }
    }
}

impl From<Edge> for TupleValue {
    fn from(edge: Edge) -> Self {
        TupleValue {
            data: vec![
                ScalarTypedValue::Uint(edge.from),
                ScalarTypedValue::Uint(edge.to),
                ScalarTypedValue::Uint(edge.weight),
                ScalarTypedValue::Bool(edge.active),
            ],
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct SetOp {
    rep_id: u64,
    ctr: u64,
    key: u64,
    value: u64,
}

impl SetOp {
    pub fn new(rep_id: u64, ctr: u64, key: u64, value: u64) -> Self {
        Self {
            rep_id,
            ctr,
            key,
            value,
        }
    }
}

impl InputEntity for SetOp {
    fn schema() -> RelationSchema {
        RelationSchema::new("set", ["RepId", "Ctr", "Key", "Value"], ["RepId", "Ctr"])
            .expect("Correct schema definition")
    }
}

impl From<SetOp> for TupleKey {
    fn from(set_op: SetOp) -> Self {
        TupleKey::from_iter([set_op.rep_id, set_op.ctr])
    }
}

impl From<SetOp> for TupleValue {
    fn from(set_op: SetOp) -> Self {
        TupleValue::from_iter([set_op.rep_id, set_op.ctr, set_op.key, set_op.value])
    }
}

#[derive(Copy, Clone, Debug)]
pub struct PredRel {
    from_rep_id: u64,
    from_ctr: u64,
    to_rep_id: u64,
    to_ctr: u64,
}

impl PredRel {
    pub fn new(from_rep_id: u64, from_ctr: u64, to_rep_id: u64, to_ctr: u64) -> Self {
        Self {
            from_rep_id,
            from_ctr,
            to_rep_id,
            to_ctr,
        }
    }
}

impl InputEntity for PredRel {
    fn schema() -> RelationSchema {
        RelationSchema::new(
            "pred",
            ["FromRepId", "FromCtr", "ToRepId", "ToCtr"],
            ["FromRepId", "FromCtr", "ToRepId", "ToCtr"],
        )
        .expect("Correct schema definition")
    }
}

impl From<PredRel> for TupleKey {
    fn from(pred_rel: PredRel) -> Self {
        TupleKey::from_iter([
            pred_rel.from_rep_id,
            pred_rel.from_ctr,
            pred_rel.to_rep_id,
            pred_rel.to_ctr,
        ])
    }
}

impl From<PredRel> for TupleValue {
    fn from(pred_rel: PredRel) -> Self {
        TupleValue::from_iter([
            pred_rel.from_rep_id,
            pred_rel.from_ctr,
            pred_rel.to_rep_id,
            pred_rel.to_ctr,
        ])
    }
}

/// This function returns test data for an operation history of the MVR CRDT
/// store. The history is as follows.
/// The notation is `set_<replica_id>_<counter>(<key>, <value>)`.
///
/// 1. step (just one root operation setting register with key 1 to value 1):
///
/// ```text
/// set_0_0(1, 1)
/// ```
///
/// 2. step (concurrent writes by replica 0 and 1):
///
/// ```text
///               ---> set_0_1(1, 2)
/// set_0_0(1, 1)
///               ---> set_1_0(1, 3)
/// ```
///
/// 3. step (replica 1 does a "merge" operation overwriting the previous
///    conflict):
///
/// ```text
///               ---> set_0_1(1, 2)
/// set_0_0(1, 1)                    ---> set_1_2(1, 4)
///               ---> set_1_0(1, 3)
/// ```
///
/// 4. step (replica 0 overwrites a not-yet delivered operation):
///
/// ```text
///               ---> set_0_1(1, 2)
/// set_0_0(1, 1)                    ---> set_1_2(1, 4) ---> missing ---> set_0_4(1, 6)
///               ---> set_1_0(1, 3)
/// ```
///
/// 5. step (replica 0's missing operation arrives):
///
/// ```text
///               ---> set_0_1(1, 2)
/// set_0_0(1, 1)                    ---> set_1_2(1, 4) ---> set_0_3(1, 5) ---> set_0_4(1, 6)
///               ---> set_1_0(1, 3)
/// ```
pub fn mvr_store_operation_history() -> [(Vec<PredRel>, Vec<SetOp>); 5] {
    [
        (vec![], vec![SetOp::new(0, 0, 1, 1)]),
        (
            vec![PredRel::new(0, 0, 0, 1), PredRel::new(0, 0, 1, 0)],
            vec![SetOp::new(0, 1, 1, 2), SetOp::new(1, 0, 1, 3)],
        ),
        (
            vec![PredRel::new(0, 1, 1, 2), PredRel::new(1, 0, 1, 2)],
            vec![SetOp::new(1, 2, 1, 4)],
        ),
        (vec![PredRel::new(0, 3, 0, 4)], vec![SetOp::new(0, 4, 1, 6)]),
        (vec![PredRel::new(1, 2, 0, 3)], vec![SetOp::new(0, 3, 1, 5)]),
    ]
}
