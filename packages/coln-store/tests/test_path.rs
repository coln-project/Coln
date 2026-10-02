// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{collections::BTreeSet, sync::Once};

use coln_flir_rs::engine::packed::StoreScalarValue;
use coln_flir_rs::engine::schema::ColnDef;
use coln_flir_rs::engine::tx::TxTuple;
use coln_flir_rs::hash::CommitHash;
use coln_flir_rs::ir::{self, FlatRealm, Path};
use coln_flir_rs::public::PublicRowId;
use coln_store::{
    IdLookup,
    commit::pst,
    store::{Store, error::StoreError},
    txn::{id::Promote, rw::StoreWrite},
};
use rstest::{fixture, rstest};
use tracing_subscriber::EnvFilter;

static GRAPH_IR: &str = include_str!("../../coln-flir-rs/tests/data/GraphRealm.json");

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
        realm: String::from("GraphRealm"),
    }
}

struct GraphData {
    v1: PublicRowId,
    v2: PublicRowId,
}
fn add_basic_data_to_graph(store: &mut Store) -> Result<GraphData, StoreError> {
    let gv = Path::from("root.V");
    let ge = Path::from("root.E");

    let mut tx = store.transaction();
    let v1 = tx.add(&gv, TxTuple::empty())?;
    let v2 = tx.add(&gv, TxTuple::empty())?;
    tx.add(&ge, vec![v1.clone(), v2.clone()])?;
    let h = tx.commit()?;
    let [v1, v2] = store.promote(vec![v1, v2], h).try_into().unwrap();

    Ok(GraphData { v1, v2 })
}

fn add_graph_vertex(store: &mut Store) -> Result<CommitHash, StoreError> {
    let mut tx = store.transaction();
    tx.add(&Path::from("root.V"), TxTuple::empty())?;
    tx.commit()
}

fn add_graph_edge(
    store: &mut Store,
    v1: PublicRowId,
    v2: PublicRowId,
) -> Result<CommitHash, StoreError> {
    let gv = Path::from("root.V");
    let ge = Path::from("root.E");
    let tv = store.table_at(&gv).expect("root.V table");
    let ids = store.id_lookup();
    let v1 = tv
        .row_by_id(&ids.packed(&v1).expect("v1 is packed"))
        .expect("vertex 1");
    let v2 = tv
        .row_by_id(&ids.packed(&v2).expect("v2 is packed"))
        .expect("vertex 2");
    let v1 = ids.unpacked(&v1.row_id()).expect("v1 is unpacked");
    let v2 = ids.unpacked(&v2.row_id()).expect("v2 is unpacked");

    let mut txn = store.transaction();
    txn.add(&ge, vec![v1, v2])?;
    txn.commit()
}

#[rstest]
fn test_read_graph_realm_json(#[from(graph_ir)] theory: &FlatRealm) {
    assert_eq!(
        theory.tables.len(),
        2,
        "expected table count from GraphRealm.json"
    );
    assert_eq!(
        theory.rules.len(),
        2,
        "expected law count from GraphRealm.json"
    );

    let edge = theory
        .tables
        .iter()
        .find(|t| t.path == Path::from("root.E"))
        .expect("root.E table");
    assert_eq!(edge.table.columns.len(), 2);
    assert_eq!(edge.table.primary_key, None);

    let vertices = theory
        .tables
        .iter()
        .find(|t| t.path == Path::from("root.V"))
        .expect("root.V table");
    assert!(vertices.table.columns.is_empty());

    let edge_fk = theory
        .rules
        .iter()
        .find(|e| e.path == Path::from("root.E.foreignKey"))
        .expect("root.E.foreignKey law path");
    assert!(
        !edge_fk.rule.vars.is_empty(),
        "root.E foreignKey law should bind variables"
    );
}

#[rstest]
fn test_add_edge_referencing_vertices_from_previous_commit(
    #[from(graph_ir)] theory: &FlatRealm,
    #[from(graph_coln_def)] coln_def: &ColnDef,
) {
    let mut store = Store::try_from_ir(theory.clone(), coln_def.clone()).expect("valid theory");

    let data = add_basic_data_to_graph(&mut store).expect("add basic data");

    let edge_commit = add_graph_edge(&mut store, data.v1.clone(), data.v2.clone())
        .expect("add edge in later transaction");

    let edges = store.table_at(&Path::from("root.E")).expect("root.E table");
    assert_eq!(edges.row_count(), 2);
    let ids = store.id_lookup();
    let second_edge = ids
        .packed(&PublicRowId {
            commit: edge_commit,
            counter: 0,
        })
        .expect("second edge is packed");
    let row = edges.row_by_id(&second_edge).expect("second edge row");
    assert_eq!(row.row_id(), second_edge);
    assert_eq!(
        row.values(),
        [
            StoreScalarValue::RowId(ids.packed(&data.v1).expect("v1 is packed")),
            StoreScalarValue::RowId(ids.packed(&data.v2).expect("v2 is packed")),
        ]
    );
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
            .table_at(&ir::Path::from("root.V"))
            .expect("table root.V")
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

    let left_vertices = left.table_at(&Path::from("root.V")).expect("left root.V");
    let right_vertices = right.table_at(&Path::from("root.V")).expect("right root.V");
    assert_eq!(left_vertices.row_count(), 3);
    assert_eq!(right_vertices.row_count(), 3);

    let left_edges = left.table_at(&Path::from("root.E")).expect("left root.E");
    let right_edges = right.table_at(&Path::from("root.E")).expect("right root.E");
    assert_eq!(left_edges.row_count(), 2);
    assert_eq!(right_edges.row_count(), 2);

    // Packed ids follow each store's dictionary insertion order, so compare
    // the unpacked ids.
    let left_row_ids = left_vertices
        .scan()
        .map(|row| left.id_lookup().unpacked(&row.row_id()))
        .collect::<Option<BTreeSet<_>>>()
        .expect("left row ids are unpacked");
    let right_row_ids = right_vertices
        .scan()
        .map(|row| right.id_lookup().unpacked(&row.row_id()))
        .collect::<Option<BTreeSet<_>>>()
        .expect("right row ids are unpacked");
    assert_eq!(left_row_ids, right_row_ids);
}
