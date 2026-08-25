// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module is only available if compiling with the `test` feature.
//! It provides helpers for testing and benchmarking purposes.

use crate::{
    host::Code,
    program::QueryProgram,
    relational::{
        Column, TableRef, TableSchema, TupleValue,
        catalog::{Catalog, SourceSchemas},
        expr::SourceId,
        incremental::{TupleKey, dbsp::ZWeight},
    },
    scalarial::{ScalarType, ScalarTypedValue},
};
use std::borrow::Cow;
use std::fmt::Debug;

/// Assemble a [`TableSchema`] the way a test states one: named and typed columns
/// in physical order, plus the names of the columns forming its primary key
/// (empty for a relation that declares none).
pub fn table_schema<'a>(
    name: &str,
    columns: impl IntoIterator<Item = (&'a str, ScalarType)>,
    key: impl IntoIterator<Item = &'a str>,
) -> TableSchema {
    let columns: Vec<Column> = columns
        .into_iter()
        .map(|(name, scalar_type)| Column::new(name, scalar_type))
        .collect();
    let key: Vec<usize> = key
        .into_iter()
        .map(|key_column| {
            columns
                .iter()
                .position(|column| column.name() == key_column)
                .unwrap_or_else(|| panic!("key column '{key_column}' is not a column of '{name}'"))
        })
        .collect();
    let primary_keys = if key.is_empty() { vec![] } else { vec![key] };
    TableSchema::new(TableRef::from(name), columns, primary_keys)
}

/// A [`QueryProgram`] assembled by hand: the plan under test, plus the schemas
/// of the relations its [`SourceExpr`](crate::relational::expr::SourceExpr)
/// leaves name.
pub struct TestProgram {
    code: Code,
    sources: SourceSchemas,
}

impl TestProgram {
    /// `schemas` are keyed by their own [`name`](TableSchema::name), which is
    /// the name a [`SourceExpr` leaf](crate::relational::expr::SourceExpr)
    /// refers to them.
    pub fn new(code: impl Into<Code>, schemas: impl IntoIterator<Item = TableSchema>) -> Self {
        Self {
            code: code.into(),
            sources: schemas
                .into_iter()
                .map(|schema| (SourceId::from(schema.name().to_string()), schema))
                .collect(),
        }
    }
}

/// A test states its schemas outright, so they are already in resolved form and
/// this delegates to [`SourceSchemas`]' own [`Catalog`] impl, the
/// [`Cow::Borrowed`] side, where a stored schema is lent rather than built.
impl Catalog for TestProgram {
    fn source_schema(&self, id: &SourceId) -> Option<Cow<'_, TableSchema>> {
        self.sources.source_schema(id)
    }
}

impl QueryProgram for TestProgram {
    fn code(&self) -> &Code {
        &self.code
    }

    fn take_code(&mut self) -> Code {
        std::mem::take(&mut self.code)
    }
}

/// Convenience: turn input entities (with per-row z-weights) into the value rows
/// that [`crate::relational::Runtime::feed`] expects.
pub fn rows<E: InputEntity>(
    entities: impl IntoIterator<Item = (E, ZWeight)>,
) -> Vec<(TupleValue, ZWeight)> {
    entities
        .into_iter()
        .map(|(entity, weight)| (entity.into(), weight))
        .collect()
}

/// Convenience: turn input entities into value rows all carrying `weight`.
pub fn rows_with_weight<E: InputEntity>(
    entities: impl IntoIterator<Item = E>,
    weight: ZWeight,
) -> Vec<(TupleValue, ZWeight)> {
    entities
        .into_iter()
        .map(|entity| (entity.into(), weight))
        .collect()
}

pub trait InputEntity: Into<TupleKey> + Into<TupleValue> + Clone + Debug {
    fn schema() -> TableSchema;

    /// The name a plan's [`SourceExpr`](crate::relational::expr::SourceExpr)
    /// leaf refers to this relation by. Derived from the schema's name, so a
    /// leaf and the [`TestProgram`] catalog entry describing it cannot drift
    /// apart.
    fn id() -> SourceId {
        SourceId::from(Self::schema().name().to_string())
    }
}

#[derive(Clone, Debug)]
pub struct Person {
    person_id: u64,
    name: String,
    age: u64,
    profession_id: u64,
}

impl InputEntity for Person {
    fn schema() -> TableSchema {
        table_schema(
            "person",
            [
                ("person_id", ScalarType::Uint),
                ("name", ScalarType::String),
                ("age", ScalarType::Uint),
                ("profession_id", ScalarType::Uint),
            ],
            ["person_id"],
        )
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
    fn schema() -> TableSchema {
        table_schema(
            "profession",
            [
                ("profession_id", ScalarType::Uint),
                ("name", ScalarType::String),
            ],
            ["profession_id"],
        )
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
    fn schema() -> TableSchema {
        table_schema(
            "plain",
            [
                ("a", ScalarType::Uint),
                ("b", ScalarType::Uint),
                ("c", ScalarType::Uint),
            ],
            [],
        )
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
    fn schema() -> TableSchema {
        table_schema(
            "edges",
            [
                ("from", ScalarType::Uint),
                ("to", ScalarType::Uint),
                ("weight", ScalarType::Uint),
                ("active", ScalarType::Bool),
            ],
            ["from", "to"],
        )
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
    fn schema() -> TableSchema {
        table_schema(
            "set",
            [
                ("RepId", ScalarType::Uint),
                ("Ctr", ScalarType::Uint),
                ("Key", ScalarType::Uint),
                ("Value", ScalarType::Uint),
            ],
            ["RepId", "Ctr"],
        )
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
    fn schema() -> TableSchema {
        table_schema(
            "pred",
            [
                ("FromRepId", ScalarType::Uint),
                ("FromCtr", ScalarType::Uint),
                ("ToRepId", ScalarType::Uint),
                ("ToCtr", ScalarType::Uint),
            ],
            ["FromRepId", "FromCtr", "ToRepId", "ToCtr"],
        )
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
