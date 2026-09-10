// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{collections::BTreeSet, sync::Once};

use coln_flir_rs::ir::{self, FlatRealm, Path};
use coln_store::{
    commit::{hash::CommitHash, pst},
    store::{ColnDef, Store, error::StoreError},
    table::{WireRowId, WireValue},
    txn::{empty_row, rw::StoreWrite},
    value::Value,
};
use rstest::{fixture, rstest};
use tracing_subscriber::EnvFilter;

static GRAPH_IR: &str = include_str!("../../coln-flir-rs/tests/data/Graph.json");

// For testing only
#[allow(dead_code)]
static INIT: Once = Once::new();
#[allow(dead_code)]
fn init_test_logging() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("coln_store=debug")),
            )
            .with_test_writer()
            .init();
    });
}

#[fixture]
#[once]
fn graph_ir() -> FlatRealm {
    serde_json::from_str(GRAPH_IR).expect("parse Graph FlatRealm from JSON")
}

#[fixture]
#[once]
fn graph_coln_def() -> ColnDef {
    ColnDef {
        theory: String::new(),
        realm: String::from("Graph"),
    }
}

struct GraphData {
    v1: WireRowId,
    v2: WireRowId,
}

fn add_basic_data_to_graph(store: &mut Store) -> Result<GraphData, StoreError> {
    let gv = Path::from("Graph.V");
    let ge = Path::from("Graph.E");

    let mut tx = store.transaction();
    let v1 = tx.add(&gv, empty_row())?;
    let v2 = tx.add(&gv, empty_row())?;
    tx.add(&ge, vec![v1.clone(), v2.clone()])?;
    tx.commit()?;

    Ok(GraphData {
        v1: v1.row_id()?,
        v2: v2.row_id()?,
    })
}

fn add_graph_vertex(store: &mut Store) -> Result<CommitHash, StoreError> {
    let mut tx = store.transaction();
    tx.add(&Path::from("Graph.V"), empty_row())?;
    tx.commit()
}

fn add_graph_edge(
    store: &mut Store,
    v1: WireRowId,
    v2: WireRowId,
) -> Result<CommitHash, StoreError> {
    let gv = Path::from("Graph.V");
    let ge = Path::from("Graph.E");
    let tv = store.table_at(&gv).expect("Graph.V table");
    let v1 = tv.row_by_id(v1).expect("vertex 1");
    let v2 = tv.row_by_id(v2).expect("vertex 2");

    let mut txn = store.transaction();
    txn.add(&ge, vec![Value::Id(v1.row_id), Value::Id(v2.row_id)])?;
    txn.commit()
}

#[rstest]
fn test_read_graph_json(#[from(graph_ir)] theory: &FlatRealm) {
    assert_eq!(
        theory.tables.len(),
        2,
        "expected table count from Graph.json"
    );
    assert_eq!(theory.rules.len(), 2, "expected law count from Graph.json");

    let edge = theory
        .tables
        .iter()
        .find(|t| t.path == Path::from("Graph.E"))
        .expect("Graph.E table");
    assert_eq!(edge.table.columns.len(), 2);
    assert_eq!(edge.table.primary_key, None);

    let vertices = theory
        .tables
        .iter()
        .find(|t| t.path == Path::from("Graph.V"))
        .expect("Graph.V table");
    assert!(vertices.table.columns.is_empty());

    let edge_fk = theory
        .rules
        .iter()
        .find(|e| e.path == Path::from("Graph.E.foreignKey"))
        .expect("Graph.E.foreignKey law path");
    assert!(
        !edge_fk.rule.vars.is_empty(),
        "Graph.E foreignKey law should bind variables"
    );
}

#[rstest]
// Builds a minimal valid graph dataset from the fixture, including the witness
// rows required by the fixture's totality rules before inserting vertices/edges.
fn test_add_data_and_law_enforce(
    #[from(graph_ir)] theory: &FlatRealm,
    #[from(graph_coln_def)] coln_def: &ColnDef,
) {
    let n_tables = theory.tables.len();
    let n_rules = theory.rules.len();

    let mut store = Store::try_from_ir(theory.clone(), coln_def.clone()).expect("valid theory");

    assert_eq!(store.table_count(), n_tables);
    assert_eq!(store.rule_entries().len(), n_rules);
    assert!(store.resolve_table(&Path::from("Path.Graphs")).is_some());

    add_basic_data_to_graph(&mut store).expect("add basic data");

    let ge = store
        .table_at(&Path::from("Graph.E"))
        .expect("get table Graph.E");
    assert_eq!(ge.schema().columns.len(), 2);
    assert_eq!(ge.row_count(), 1);
}

#[rstest]
fn test_add_edge_referencing_vertices_from_previous_commit(
    #[from(graph_ir)] theory: &FlatRealm,
    #[from(graph_coln_def)] coln_def: &ColnDef,
) {
    let mut store = Store::try_from_ir(theory.clone(), coln_def.clone()).expect("valid theory");

    let data = add_basic_data_to_graph(&mut store).expect("add basic data");

    let edge_commit =
        add_graph_edge(&mut store, data.v1, data.v2).expect("add edge in later transaction");

    let edges = store
        .table_at(&Path::from("Graph.E"))
        .expect("Graph.E table");
    assert_eq!(edges.row_count(), 2);
    let second_edge = WireRowId {
        commit: edge_commit,
        counter: 0,
    };
    let row = edges.row_by_id(second_edge).expect("second edge row");
    assert_eq!(row.row_id, second_edge);
    assert_eq!(
        row.values,
        vec![WireValue::Id(data.v1), WireValue::Id(data.v2)]
    );
}

#[rstest]
fn test_missing_vertex_rejects_batch_without_mutation(
    #[from(graph_ir)] theory: &FlatRealm,
    #[from(graph_coln_def)] coln_def: &ColnDef,
) {
    let mut store = Store::try_from_ir(theory.clone(), coln_def.clone()).expect("valid theory");

    let vertices = store
        .table_at(&Path::from("Graph.V"))
        .expect("Graph.V table");
    let edges = store
        .table_at(&Path::from("Graph.E"))
        .expect("Graph.E table");

    assert_eq!(vertices.row_count(), 0);
    assert_eq!(edges.row_count(), 0);

    let dummy_vid = Value::Id(WireRowId {
        commit: CommitHash([0xff; 32]),
        counter: u32::MAX,
    });
    let mut tx = store.transaction();
    tx.add(&Path::from("Graph.E"), vec![dummy_vid.clone(), dummy_vid])
        .expect("add edge");
    let err = tx.commit().expect_err("missing vertices");
    assert!(matches!(err, StoreError::Rule(_)));

    assert_eq!(
        store.table_at(&Path::from("Graph.V")).unwrap().row_count(),
        0
    );
    assert_eq!(
        store.table_at(&Path::from("Graph.E")).unwrap().row_count(),
        0
    );
}

#[rstest]
fn test_fk(#[from(graph_ir)] theory: &FlatRealm, #[from(graph_coln_def)] coln_def: &ColnDef) {
    let mut store = Store::try_from_ir(theory.clone(), coln_def.clone()).expect("valid theory");

    let ge = Path::from("Graph.E");
    let data = add_basic_data_to_graph(&mut store).expect("add valid baseline data");

    let dummy_vid = Value::Id(WireRowId {
        commit: CommitHash([0xff; 32]),
        counter: u32::MAX,
    });
    let mut tx = store.transaction();
    tx.add(&ge, vec![Value::Id(data.v1), dummy_vid])
        .expect("add edge");
    let err = tx.commit().expect_err("missing target vertex");

    assert!(matches!(err, StoreError::Rule(_)));
}

#[rstest]
fn test_persist_roundtrip(
    #[from(graph_ir)] theory: &FlatRealm,
    #[from(graph_coln_def)] coln_def: &ColnDef,
) {
    let mut store = Store::try_from_ir(theory.clone(), coln_def.clone()).expect("valid theory");

    let r = add_basic_data_to_graph(&mut store);
    assert!(r.is_ok());
    assert!(
        store
            .table_at(&ir::Path::from("Graph.V"))
            .expect("table Graph.V")
            .row_count()
            > 0
    );

    let content = store.dump();
    let data = pst::encode_store(&store).expect("encoding store success");
    let st = pst::decode_store(&data).expect("decode store success");

    assert_eq!(content, st.dump());
}

#[rstest]
fn test_divergent_commits_merge_between_stores(
    #[from(graph_ir)] theory: &FlatRealm,
    #[from(graph_coln_def)] coln_def: &ColnDef,
) {
    let mut base = Store::try_from_ir(theory.clone(), coln_def.clone()).expect("valid theory");
    let data = add_basic_data_to_graph(&mut base).expect("add shared baseline data");

    let mut left =
        Store::try_from_ir(theory.clone(), coln_def.clone()).expect("valid left-hand theory");
    let mut right =
        Store::try_from_ir(theory.clone(), coln_def.clone()).expect("valid right-hand theory");
    let baseline_commits = base.commits_after(&left.heads());
    left.apply_commits(baseline_commits.clone())
        .expect("apply shared baseline to left");
    right
        .apply_commits(baseline_commits)
        .expect("apply shared baseline to right");

    // Divergent ops: a new vertex on the left, a second edge on the right.
    // Identical vertex inserts would content-address to the same commit.
    let left_commit = add_graph_vertex(&mut left).expect("left branch commit");
    let right_commit = add_graph_edge(&mut right, data.v1, data.v2).expect("right branch commit");
    let expected_heads = BTreeSet::from([left_commit, right_commit]);

    let left_heads = left.merge(&right).expect("merge right into left");
    assert_eq!(
        left_heads.into_iter().collect::<BTreeSet<_>>(),
        expected_heads
    );

    let right_heads = right.merge(&left).expect("merge left into right");
    assert_eq!(
        right_heads.into_iter().collect::<BTreeSet<_>>(),
        expected_heads
    );

    let left_vertices = left.table_at(&Path::from("Graph.V")).expect("left Graph.V");
    let right_vertices = right
        .table_at(&Path::from("Graph.V"))
        .expect("right Graph.V");
    assert_eq!(left_vertices.row_count(), 3);
    assert_eq!(right_vertices.row_count(), 3);

    let left_edges = left.table_at(&Path::from("Graph.E")).expect("left Graph.E");
    let right_edges = right
        .table_at(&Path::from("Graph.E"))
        .expect("right Graph.E");
    assert_eq!(left_edges.row_count(), 2);
    assert_eq!(right_edges.row_count(), 2);

    let left_row_ids = left_vertices
        .scan()
        .map(|row| row.row_id)
        .collect::<BTreeSet<_>>();
    let right_row_ids = right_vertices
        .scan()
        .map(|row| row.row_id)
        .collect::<BTreeSet<_>>();
    assert_eq!(left_row_ids, right_row_ids);
}
