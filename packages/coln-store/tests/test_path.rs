// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{collections::BTreeSet, path::PathBuf, sync::Once};

use coln_flir_rs::ir::{self, FlatRealm, Path};
use coln_store::{
    commit::{hash::CommitHash, pst},
    store::{Store, error::StoreError},
    table::{WireRowId, WireValue},
    txn::empty_row,
    value::Value,
};
use rstest::{fixture, rstest};
use tracing_subscriber::EnvFilter;

static PATHS_IR: &str = "Path.json";

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
fn path_theory() -> FlatRealm {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(PATHS_IR);

    let json = std::fs::read_to_string(p).expect("read tests/data/paths.json");
    serde_json::from_str(&json).expect("parse FlatRealm from JSON")
}

struct PathData {
    graph1: WireRowId,
    v1: WireRowId,
    v2: WireRowId,
}

fn add_basic_data_to_path(store: &mut Store) -> Result<PathData, StoreError> {
    let graphs = Path::from("Path.Graphs");
    let g0 = Path::from("Path.G0");
    let g1 = Path::from("Path.G1");
    let gv = Path::from("Path.G.V");
    let ge = Path::from("Path.G.E");

    let mut tx = store.transaction();
    let gid1 = tx.add(&graphs, empty_row())?;
    let gid2 = tx.add(&graphs, empty_row())?;
    tx.add(&g0, vec![gid2.clone()])?;
    tx.add(&g1, vec![gid2.clone()])?;
    let v1 = tx.add(&gv, vec![gid1.clone()])?;
    let v2 = tx.add(&gv, vec![gid1.clone()])?;
    tx.add(&ge, vec![gid1.clone(), v1.clone(), v2.clone()])?;
    tx.commit()?;

    Ok(PathData {
        graph1: gid1.row_id()?,
        v1: v1.row_id()?,
        v2: v2.row_id()?,
    })
}

fn add_vertex_to_graph(store: &mut Store, graph: WireRowId) -> Result<CommitHash, StoreError> {
    let graphs = Path::from("Path.Graphs");
    let gv = Path::from("Path.G.V");
    let graph = store
        .table_at(&graphs)
        .expect("Path.Graphs table")
        .row_by_id(graph)
        .expect("graph row");

    let mut tx = store.transaction();
    tx.add(&gv, vec![Value::Id(graph.row_id)])?;
    tx.commit()
}

fn add_extra_edge(
    store: &mut Store,
    graph: WireRowId,
    v1: WireRowId,
    v2: WireRowId,
) -> Result<CommitHash, StoreError> {
    let graphs = Path::from("Path.Graphs");
    let gv = Path::from("Path.G.V");
    let ge = Path::from("Path.G.E");

    let graph = store
        .table_at(&graphs)
        .expect("Path.Graphs table")
        .row_by_id(graph)
        .expect("graph row");
    let tv = store.table_at(&gv).expect("Path.G.V table");
    let v1 = tv.row_by_id(v1).expect("vertex 1");
    let v2 = tv.row_by_id(v2).expect("vertex 2");

    let mut txn = store.transaction();
    txn.add(
        &ge,
        vec![
            Value::Id(graph.row_id),
            Value::Id(v1.row_id),
            Value::Id(v2.row_id),
        ],
    )?;
    txn.commit()
}

#[rstest]
fn test_read_path_coln(#[from(path_theory)] theory: &FlatRealm) {
    assert_eq!(
        theory.tables.len(),
        16,
        "expected table count from coln paths.json"
    );
    assert_eq!(
        theory.rules.len(),
        27,
        "expected law count from coln paths.json"
    );

    let g_edge = theory
        .tables
        .iter()
        .find(|t| t.path == Path::from("Path.G.E"))
        .expect("G,E table");
    assert_eq!(g_edge.table.columns.len(), 3);
    assert_eq!(g_edge.table.primary_key, None);

    let graphs = theory
        .tables
        .iter()
        .find(|t| t.path == Path::from("Path.Graphs"))
        .expect("Path.Graphs table");
    assert!(graphs.table.columns.is_empty());

    let hom_e_fk = theory
        .rules
        .iter()
        .find(|e| e.path == Path::from("Path.Hom.E.foreignKey"))
        .expect("Path.Hom.E.foreignKey law path");
    assert!(
        !hom_e_fk.rule.var_names.is_empty(),
        "Hom,E foreignKeys law should bind variables"
    );
}

#[rstest]
fn test_compile_path_rules(#[from(path_theory)] theory: &FlatRealm) {
    let expected_law_count = theory.rules.len();
    let store = Store::try_from_ir(theory.clone()).expect("valid theory");

    assert!(expected_law_count > 0, "fixture should contain rules");
    assert_eq!(store.rules().len(), expected_law_count);
}

#[rstest]
// Builds a minimal valid graph dataset from the fixture, including the witness
// rows required by the fixture's totality rules before inserting vertices/edges.
fn test_add_data_and_law_enforce(#[from(path_theory)] theory: &FlatRealm) {
    let n_tables = theory.tables.len();
    let n_rules = theory.rules.len();

    let mut store = Store::try_from_ir(theory.clone()).expect("valid theory");

    assert_eq!(store.table_count(), n_tables);
    assert_eq!(store.rules().len(), n_rules);
    assert!(store.resolve_table(&Path::from("Path.Graphs")).is_some());

    // One explicit column (Graphs); row id will be assigned by the db.

    add_basic_data_to_path(&mut store).expect("add basic data");

    let ge = store
        .table_at(&Path::from("Path.G.E"))
        .expect("get table Path.G.E");
    assert_eq!(ge.schema().columns.len(), 3);
    assert_eq!(ge.row_count(), 1);
}

#[rstest]
fn test_add_edge_referencing_vertices_from_previous_commit(
    #[from(path_theory)] theory: &FlatRealm,
) {
    let mut store = Store::try_from_ir(theory.clone()).expect("valid theory");

    let data = add_basic_data_to_path(&mut store).expect("add basic data");

    let edge_commit = add_extra_edge(&mut store, data.graph1, data.v1, data.v2)
        .expect("add edge in later transaction");

    let edges = store
        .table_at(&Path::from("Path.G.E"))
        .expect("Path.G.E table");
    assert_eq!(edges.row_count(), 2);
    let second_edge = WireRowId {
        commit: edge_commit,
        counter: 0,
    };
    let row = edges.row_by_id(second_edge).expect("second edge row");
    assert_eq!(row.row_id, second_edge);
    assert_eq!(
        row.values,
        vec![
            WireValue::Id(data.graph1),
            WireValue::Id(data.v1),
            WireValue::Id(data.v2),
        ]
    );
}

#[rstest]
// Rejects a graph insert when the corresponding witness rows are missing and
// confirms the failed batch leaves the store unchanged.
fn test_missing_graph_witness_rejects_batch_without_mutation(
    #[from(path_theory)] theory: &FlatRealm,
) {
    let mut store = Store::try_from_ir(theory.clone()).expect("valid theory");

    let graphs = store
        .table_at(&Path::from("Path.Graphs"))
        .expect("Path.Graphs table");
    let g0 = store
        .table_at(&Path::from("Path.G0"))
        .expect("Path.G0 table");
    let g1 = store
        .table_at(&Path::from("Path.G1"))
        .expect("Path.G1 table");

    assert_eq!(graphs.row_count(), 0);
    assert_eq!(g0.row_count(), 0);
    assert_eq!(g1.row_count(), 0);

    let mut tx = store.transaction();
    tx.add(&Path::from("Path.Graphs"), empty_row())
        .expect("add graph row");
    let err = tx.commit().expect_err("missing g0 and g1");
    assert!(matches!(err, StoreError::Rule(_)));

    assert_eq!(
        store
            .table_at(&Path::from("Path.Graphs"))
            .unwrap()
            .row_count(),
        0
    );
    assert_eq!(
        store.table_at(&Path::from("Path.G0")).unwrap().row_count(),
        0
    );
    assert_eq!(
        store.table_at(&Path::from("Path.G1")).unwrap().row_count(),
        0
    );
}

#[rstest]
fn test_fk(#[from(path_theory)] theory: &FlatRealm) {
    let mut store = Store::try_from_ir(theory.clone()).expect("valid theory");

    let ge = Path::from("Path.G.E");

    let data = add_basic_data_to_path(&mut store).expect("add valid baseline data");
    let gid = data.graph1;
    let vid = data.v1;

    let dummy_vid = Value::Id(WireRowId {
        commit: CommitHash([0xff; 32]),
        counter: u32::MAX,
    });
    let mut tx = store.transaction();
    tx.add(&ge, vec![Value::Id(gid), Value::Id(vid), dummy_vid])
        .expect("add edge");
    let err = tx.commit().expect_err("missing v2");

    assert!(matches!(err, StoreError::Rule(_)));
}

#[rstest]
fn test_persist_roundtrip(#[from(path_theory)] theory: &FlatRealm) {
    let mut store = Store::try_from_ir(theory.clone()).expect("valid theory");

    let r = add_basic_data_to_path(&mut store);
    assert!(r.is_ok());
    assert!(
        store
            .table_at(&ir::Path::from("Path.G.V"))
            .expect("table Path.G.V")
            .row_count()
            > 0
    );

    let content = store.dump();
    let data = pst::encode_store(&store).expect("encoding store success");
    let st = pst::decode_store(&data).expect("decode store success");

    assert_eq!(content, st.dump());
}

#[rstest]
fn test_divergent_commits_merge_between_stores(#[from(path_theory)] theory: &FlatRealm) {
    let mut base = Store::try_from_ir(theory.clone()).expect("valid theory");
    let data = add_basic_data_to_path(&mut base).expect("add shared baseline data");

    // Add a second rule-free graph so we can add vertices to different graphs to
    // make two commits different
    let extra_graph = {
        let mut tx = base.transaction();
        let handle = tx
            .add(&Path::from("Path.Graphs"), empty_row())
            .expect("add a graph");
        tx.commit().expect("commit second graph");
        handle.row_id().expect("extra graph row id")
    };

    let mut left = Store::try_from_ir(theory.clone()).expect("valid left-hand theory");
    let mut right = Store::try_from_ir(theory.clone()).expect("valid right-hand theory");
    let baseline_commits = base.commits_after(&left.heads());
    left.apply_commits(baseline_commits.clone())
        .expect("apply shared baseline to left");
    right
        .apply_commits(baseline_commits)
        .expect("apply shared baseline to right");

    let left_commit = add_vertex_to_graph(&mut left, data.graph1).expect("left branch commit");
    let right_commit = add_vertex_to_graph(&mut right, extra_graph).expect("right branch commit");
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

    let left_vertices = left
        .table_at(&Path::from("Path.G.V"))
        .expect("left Path.G.V");
    let right_vertices = right
        .table_at(&Path::from("Path.G.V"))
        .expect("right Path.G.V");
    assert_eq!(left_vertices.row_count(), 4);
    assert_eq!(right_vertices.row_count(), 4);

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
