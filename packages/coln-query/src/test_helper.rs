// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This module is only available if compiling with the `test` feature.
//! It provides helpers for testing and benchmarking purposes.

use crate::{
    api::deltas::ZRow,
    host::QueryIr,
    program::QueryProgram,
    relational::{
        Column, EntityRef, TableSchema, TupleValue,
        catalog::{Catalog, SourceSchemas},
        expr::SourceId,
        incremental::{TupleKey, dbsp::ZWeight},
    },
    scalarial::{ScalarType, ScalarTypedValue},
};
use std::borrow::Cow;
use std::fmt::Debug;

pub mod graph_flir {
    use crate::{
        api::deltas::{StoreDelta, TableDelta, ZRow},
        relational::TupleValue,
        scalarial::ScalarTypedValue,
    };
    use coln_flir_rs::ir;

    pub trait JsonFlir {
        const FILENAME: &'static str;

        fn load(&self) -> ir::FlatRealm {
            coln_flir_rs::test_utils::load_theory_from_json(Self::FILENAME)
        }
    }

    pub struct GraphFlir {
        hash: u64,
        ctr: u64,
        store_delta: StoreDelta,
    }

    impl GraphFlir {
        pub fn init() -> Self {
            Self {
                hash: 0,
                ctr: 0,
                store_delta: StoreDelta::empty(),
            }
        }
        pub fn epoch(&self) -> u64 {
            self.hash
        }
        pub fn ctr(&self) -> u64 {
            self.ctr
        }
        pub fn next_epoch(&mut self) -> StoreDelta {
            self.hash += 1;
            self.ctr = 0;
            std::mem::take(&mut self.store_delta)
        }
        pub fn next_ctr(&mut self) -> u64 {
            let ctr = self.ctr;
            self.ctr += 1;
            ctr
        }
        fn with_zweight(zweight: i64, row: TupleValue) -> ZRow {
            ZRow::new(zweight, row).expect("non-zero zweight")
        }
        pub fn insert_vertex(&mut self) -> Vertex {
            let vertex = Vertex::new(self.hash, self.next_ctr());
            self.insert_to_table_delta(&vertex);
            vertex
        }
        pub fn insert_edge(&mut self, from: &Vertex, to: &Vertex) -> Edge {
            let edge = Edge::with_vertices(self.hash, self.next_ctr(), from, to);
            self.insert_to_table_delta(&edge);
            edge
        }
        pub fn insert_raw_edge(&mut self, edge: Edge) -> Edge {
            self.insert_to_table_delta(&edge);
            edge
        }
        fn insert_to_table_delta<T: Entity>(&mut self, entry: &T) {
            // Maybe improve by collecting all vertices of this epoch in a single
            // table delta but maybe it's good to test this not-so-pretty code
            // path as well..
            let table_delta =
                TableDelta::new(&T::ir_path(), vec![Self::with_zweight(1, entry.to_row())]);
            self.store_delta.extend(Some(table_delta));
        }
    }

    impl JsonFlir for GraphFlir {
        const FILENAME: &'static str = "Graph.json";
    }

    pub trait Entity {
        const NAME: &'static str;

        fn ir_path() -> ir::Path {
            ir::Path::from(Self::NAME)
        }

        fn to_row(&self) -> TupleValue;

        fn row_id(&self) -> &RowId;
    }

    pub struct RowId {
        hash: u64,
        ctr: u64,
    }

    impl RowId {
        pub fn hash(&self) -> u64 {
            self.hash
        }
        pub fn ctr(&self) -> u64 {
            self.ctr
        }
    }

    pub struct Vertex {
        row_id: RowId,
    }

    impl Vertex {
        fn new(hash: u64, ctr: u64) -> Vertex {
            Self {
                row_id: RowId { hash, ctr },
            }
        }
    }

    impl Entity for Vertex {
        const NAME: &'static str = "Graph.V";

        fn to_row(&self) -> TupleValue {
            [
                ScalarTypedValue::from(self.row_id.hash()),
                ScalarTypedValue::from(self.row_id.ctr()),
            ]
            .into_iter()
            .collect()
        }

        fn row_id(&self) -> &RowId {
            &self.row_id
        }
    }

    pub struct Edge {
        row_id: RowId,
        from_hash: u64,
        from_ctr: u64,
        to_hash: u64,
        to_ctr: u64,
    }

    impl Edge {
        pub fn with_vertices(hash: u64, ctr: u64, from: &Vertex, to: &Vertex) -> Edge {
            Edge::new(
                hash,
                ctr,
                from.row_id.hash(),
                from.row_id.ctr(),
                to.row_id.hash(),
                to.row_id.ctr(),
            )
        }
        pub fn new(
            hash: u64,
            ctr: u64,
            from_hash: u64,
            from_ctr: u64,
            to_hash: u64,
            to_ctr: u64,
        ) -> Edge {
            Self {
                row_id: RowId { hash, ctr },
                from_hash,
                from_ctr,
                to_hash,
                to_ctr,
            }
        }
    }

    impl Entity for Edge {
        const NAME: &'static str = "Graph.E";

        fn to_row(&self) -> TupleValue {
            [
                ScalarTypedValue::from(self.row_id.hash()),
                ScalarTypedValue::from(self.row_id.ctr()),
                ScalarTypedValue::from(self.from_hash),
                ScalarTypedValue::from(self.from_ctr),
                ScalarTypedValue::from(self.to_hash),
                ScalarTypedValue::from(self.to_ctr),
            ]
            .into_iter()
            .collect()
        }

        fn row_id(&self) -> &RowId {
            &self.row_id
        }
    }
}

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
    TableSchema::new(EntityRef::from(name), columns, primary_keys)
}

/// A [`QueryProgram`] assembled by hand: the plan under test, plus the schemas
/// of the relations its [`SourceExpr`](crate::relational::expr::SourceExpr)
/// leaves name.
pub struct TestProgram {
    code: QueryIr,
    sources: SourceSchemas,
}

impl TestProgram {
    /// `schemas` are keyed by their own [`name`](TableSchema::name), which is
    /// the name a [`SourceExpr` leaf](crate::relational::expr::SourceExpr)
    /// refers to them.
    pub fn new(code: impl Into<QueryIr>, schemas: impl IntoIterator<Item = TableSchema>) -> Self {
        Self {
            code: code.into(),
            sources: schemas
                .into_iter()
                .map(|schema| (SourceId::from(schema.name()), schema))
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
    fn code(&self) -> &QueryIr {
        &self.code
    }

    fn take_code(&mut self) -> QueryIr {
        std::mem::take(&mut self.code)
    }
}

/// Convenience: turn input entities (with per-row z-weights) into the value rows
/// that [`crate::relational::Runtime::feed`] expects.
pub fn rows<E: InputRel>(entities: impl IntoIterator<Item = (E, ZWeight)>) -> Vec<ZRow> {
    entities
        .into_iter()
        .map(|(entity, weight)| ZRow::new(weight, entity.into()).expect("non-zero zweight"))
        .collect()
}

/// Convenience: turn input entities into value rows all carrying `weight`.
pub fn rows_with_weight<E: InputRel>(
    entities: impl IntoIterator<Item = E>,
    weight: ZWeight,
) -> Vec<ZRow> {
    entities
        .into_iter()
        .map(|entity| ZRow::new(weight, entity.into()).expect("non-zero zweight"))
        .collect()
}

pub trait InputRel: Into<TupleKey> + Into<TupleValue> + Clone + Debug {
    fn schema() -> TableSchema;

    /// The name a plan's [`SourceExpr`](crate::relational::expr::SourceExpr)
    /// leaf refers to this relation by. Derived from the schema's name, so a
    /// leaf and the [`TestProgram`] catalog entry describing it cannot drift
    /// apart.
    fn id() -> SourceId {
        SourceId::from(Self::schema().name())
    }
}

#[derive(Clone, Debug)]
pub struct PersonRel {
    person_id: u64,
    name: String,
    age: u64,
    profession_id: u64,
}

impl InputRel for PersonRel {
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

impl From<PersonRel> for TupleKey {
    fn from(person: PersonRel) -> Self {
        TupleKey {
            data: vec![ScalarTypedValue::Uint(person.person_id)],
        }
    }
}

impl From<PersonRel> for TupleValue {
    fn from(person: PersonRel) -> Self {
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
pub struct ProfessionRel {
    profession_id: u64,
    name: String,
}

impl InputRel for ProfessionRel {
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

impl From<ProfessionRel> for TupleKey {
    fn from(profession: ProfessionRel) -> Self {
        TupleKey {
            data: vec![ScalarTypedValue::Uint(profession.profession_id)],
        }
    }
}

impl From<ProfessionRel> for TupleValue {
    fn from(profession: ProfessionRel) -> Self {
        TupleValue {
            data: vec![
                ScalarTypedValue::Uint(profession.profession_id),
                ScalarTypedValue::String(profession.name),
            ],
        }
    }
}

pub fn person_profession_data() -> [(Vec<PersonRel>, Vec<ProfessionRel>); 1] {
    [(
        vec![
            PersonRel {
                person_id: 0,
                name: "Alice".to_string(),
                age: 20,
                profession_id: 0,
            },
            PersonRel {
                person_id: 1,
                name: "Bob".to_string(),
                age: 30,
                profession_id: 1,
            },
            PersonRel {
                person_id: 2,
                name: "Charlie".to_string(),
                age: 40,
                profession_id: 0,
            },
        ],
        vec![
            ProfessionRel {
                profession_id: 0,
                name: "Engineer".to_string(),
            },
            ProfessionRel {
                profession_id: 1,
                name: "Doctor".to_string(),
            },
        ],
    )]
}

#[derive(Copy, Clone, Debug)]
pub struct PlainRel {
    a: u64,
    b: u64,
    c: u64,
}

impl PlainRel {
    pub fn new(a: u64, b: u64, c: u64) -> Self {
        Self { a, b, c }
    }
    const STEPS: usize = 1;
    pub fn test_data_1() -> [Vec<PlainRel>; Self::STEPS] {
        [vec![
            PlainRel::new(1, 2, 3),
            PlainRel::new(4, 5, 6),
            PlainRel::new(7, 8, 9),
        ]]
    }
    pub fn test_data_2() -> [Vec<PlainRel>; Self::STEPS] {
        [vec![PlainRel::new(1, 2, 3)]]
    }
    pub fn test_data_3() -> [Vec<PlainRel>; Self::STEPS] {
        [vec![PlainRel::new(4, 5, 6)]]
    }
}

impl InputRel for PlainRel {
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

impl From<PlainRel> for TupleKey {
    fn from(fact: PlainRel) -> Self {
        TupleKey { data: vec![] }
    }
}

impl From<PlainRel> for TupleValue {
    fn from(fact: PlainRel) -> Self {
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
pub struct EdgeRel {
    from: u64,
    to: u64,
    weight: u64,
    active: bool,
}

impl EdgeRel {
    pub fn new(from: u64, to: u64, weight: u64) -> Self {
        Self {
            from,
            to,
            weight,
            active: true,
        }
    }
}

impl InputRel for EdgeRel {
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

impl From<EdgeRel> for TupleKey {
    fn from(edge: EdgeRel) -> Self {
        TupleKey {
            data: vec![
                ScalarTypedValue::Uint(edge.from),
                ScalarTypedValue::Uint(edge.to),
            ],
        }
    }
}

impl From<EdgeRel> for TupleValue {
    fn from(edge: EdgeRel) -> Self {
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
pub struct SetRel {
    rep_id: u64,
    ctr: u64,
    key: u64,
    value: u64,
}

impl SetRel {
    pub fn new(rep_id: u64, ctr: u64, key: u64, value: u64) -> Self {
        Self {
            rep_id,
            ctr,
            key,
            value,
        }
    }
}

impl InputRel for SetRel {
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

impl From<SetRel> for TupleKey {
    fn from(set_op: SetRel) -> Self {
        TupleKey::from_iter([set_op.rep_id, set_op.ctr])
    }
}

impl From<SetRel> for TupleValue {
    fn from(set_op: SetRel) -> Self {
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

impl InputRel for PredRel {
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
pub fn mvr_store_operation_history() -> [(Vec<PredRel>, Vec<SetRel>); 5] {
    [
        (vec![], vec![SetRel::new(0, 0, 1, 1)]),
        (
            vec![PredRel::new(0, 0, 0, 1), PredRel::new(0, 0, 1, 0)],
            vec![SetRel::new(0, 1, 1, 2), SetRel::new(1, 0, 1, 3)],
        ),
        (
            vec![PredRel::new(0, 1, 1, 2), PredRel::new(1, 0, 1, 2)],
            vec![SetRel::new(1, 2, 1, 4)],
        ),
        (
            vec![PredRel::new(0, 3, 0, 4)],
            vec![SetRel::new(0, 4, 1, 6)],
        ),
        (
            vec![PredRel::new(1, 2, 0, 3)],
            vec![SetRel::new(0, 3, 1, 5)],
        ),
    ]
}
