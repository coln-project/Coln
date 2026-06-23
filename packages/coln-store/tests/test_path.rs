// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::{collections::BTreeSet, path::PathBuf, sync::Once};

use coln_store::{
    commit::hash::CommitHash,
    commit::pst,
    ir::{self, FlatRealm, Path},
    store::{Store, error::StoreIntError},
    table::{CellValue, RowId},
};
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

fn fixture_theory(name: &str) -> FlatRealm {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);

    let json = std::fs::read_to_string(p).expect("read tests/data/paths.json");
    serde_json::from_str(&json).expect("parse FlatRealm from JSON")
}

fn add_basic_data_to_path(store: &mut Store) -> Result<(), Box<StoreIntError>> {
    let graphs = Path::from("Path.Graphs");
    let g0 = Path::from("Path.G0");
    let g1 = Path::from("Path.G1");
    let gv = Path::from("Path.G.V");
    let ge = Path::from("Path.G.E");

    let mut tx = store.transaction();
    let gid1 = tx.add(&graphs, vec![])?;
    let gid2 = tx.add(&graphs, vec![])?;
    tx.add(&g0, vec![gid2.clone().into()])?;
    tx.add(&g1, vec![gid2.clone().into()])?;
    let v1 = tx.add(&gv, vec![gid1.clone().into()])?;
    let v2 = tx.add(&gv, vec![gid1.clone().into()])?;
    tx.add(&ge, vec![gid1.into(), v1.into(), v2.into()])?;
    tx.commit()?;

    Ok(())
}

fn add_vertex_to_graph(
    store: &mut Store,
    graph_row: usize,
) -> Result<CommitHash, Box<StoreIntError>> {
    let graphs = Path::from("Path.Graphs");
    let gv = Path::from("Path.G.V");
    let graph = store
        .table_at(&graphs)
        .expect("Path.Graphs table")
        .row_id_at(graph_row)
        .expect("graph row");

    let mut tx = store.transaction();
    tx.add(&gv, vec![graph.into()])?;
    tx.commit()
}

fn add_extra_edge_to_first_graph(store: &mut Store) -> Result<CommitHash, Box<StoreIntError>> {
    let graphs = Path::from("Path.Graphs");
    let gv = Path::from("Path.G.V");
    let ge = Path::from("Path.G.E");

    let tv = store.table_at(&gv).expect("Path.G.V table");
    let v1 = tv.row_id_at(0).expect("vertex 1");
    let v2 = tv.row_id_at(1).expect("vertex 2");
    let graph = store
        .table_at(&graphs)
        .expect("Path.Graphs table")
        .row_id_at(0)
        .expect("first graph row");

    let mut txn = store.transaction();
    txn.add(&ge, vec![graph.into(), v1.into(), v2.into()])?;
    txn.commit()
}

#[test]
fn test_read_path_coln() {
    let theory = fixture_theory(PATHS_IR);

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

#[test]
fn test_compile_path_rules() {
    let theory = fixture_theory(PATHS_IR);
    let expected_law_count = theory.rules.len();
    let store = Store::try_from_theory(theory).expect("valid theory");

    assert!(expected_law_count > 0, "fixture should contain rules");
    assert_eq!(store.rules().len(), expected_law_count);
}

#[test]
// Builds a minimal valid graph dataset from the fixture, including the witness
// rows required by the fixture's totality rules before inserting vertices/edges.
fn test_add_data_and_law_enforce() {
    let theory = fixture_theory(PATHS_IR);
    let n_tables = theory.tables.len();
    let n_rules = theory.rules.len();

    let mut store = Store::try_from_theory(theory).expect("valid theory");

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

#[test]
fn test_add_edge_referencing_vertices_from_previous_commit() {
    let theory = fixture_theory(PATHS_IR);
    let mut store = Store::try_from_theory(theory).expect("valid theory");

    add_basic_data_to_path(&mut store).expect("add basic data");

    let graph = store
        .table_at(&Path::from("Path.Graphs"))
        .expect("Path.Graphs table")
        .row_id_at(0)
        .expect("first graph row");
    let vertices = store
        .table_at(&Path::from("Path.G.V"))
        .expect("Path.G.V table");
    let v1 = vertices.row_id_at(0).expect("first vertex row");
    let v2 = vertices.row_id_at(1).expect("second vertex row");

    let edge_commit =
        add_extra_edge_to_first_graph(&mut store).expect("add edge in later transaction");

    let edges = store
        .table_at(&Path::from("Path.G.E"))
        .expect("Path.G.E table");
    assert_eq!(edges.row_count(), 2);
    assert_eq!(
        edges.row_id_at(1).expect("second edge row"),
        RowId {
            commit: edge_commit,
            counter: 0,
        }
    );
    assert_eq!(edges.cell_at(1, 0), Some(&CellValue::Id(graph)));
    assert_eq!(edges.cell_at(1, 1), Some(&CellValue::Id(v1)));
    assert_eq!(edges.cell_at(1, 2), Some(&CellValue::Id(v2)));
}

#[test]
// Rejects a graph insert when the corresponding witness rows are missing and
// confirms the failed batch leaves the store unchanged.
fn test_missing_graph_witness_rejects_batch_without_mutation() {
    let theory = fixture_theory(PATHS_IR);
    let mut store = Store::try_from_theory(theory).expect("valid theory");

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
    tx.add(&Path::from("Path.Graphs"), vec![])
        .expect("add graph row");
    let err = tx.commit().expect_err("missing g0 and g1");
    assert!(matches!(*err, StoreIntError::Law(_)));

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

#[test]
fn test_fk() {
    let theory = fixture_theory(PATHS_IR);
    let mut store = Store::try_from_theory(theory).expect("valid theory");

    let graphs = Path::from("Path.Graphs");
    let gv = Path::from("Path.G.V");
    let ge = Path::from("Path.G.E");

    add_basic_data_to_path(&mut store).expect("add valid baseline data");
    let gid = store
        .table_at(&graphs)
        .expect("Path.Graphs table")
        .row_id_at(0)
        .expect("graph row");
    let vid = store
        .table_at(&gv)
        .expect("Path.G.V table")
        .row_id_at(0)
        .expect("vertex row");

    let dummy_vid = RowId {
        commit: CommitHash([0xff; 32]),
        counter: u32::MAX,
    };
    let mut tx = store.transaction();
    tx.add(&ge, vec![gid.into(), vid.into(), dummy_vid.into()])
        .expect("add edge");
    let err = tx.commit().expect_err("missing v2");

    assert!(matches!(*err, StoreIntError::Law(_)));
}

#[test]
fn test_persist_roundtrip() {
    let theory = fixture_theory(PATHS_IR);

    let mut store = Store::try_from_theory(theory).expect("valid theory");

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

#[test]
fn test_divergent_commits_merge_between_stores() {
    let theory = fixture_theory(PATHS_IR);
    let mut base = Store::try_from_theory(theory).expect("valid theory");
    add_basic_data_to_path(&mut base).expect("add shared baseline data");

    // Add a second law-free graph so we can add vertices to different graphs to
    // make two commits different
    {
        let mut tx = base.transaction();
        tx.add(&Path::from("Path.Graphs"), vec![])
            .expect("add a graph");
        tx.commit().expect("commit second graph");
    }

    let mut left = base.clone();
    let mut right = base.clone();
    let left_commit = add_vertex_to_graph(&mut left, 0).expect("left branch commit");
    let right_commit = add_vertex_to_graph(&mut right, 2).expect("right branch commit");
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

    let left_row_ids = (0..left_vertices.row_count())
        .map(|row| left_vertices.row_id_at(row).expect("left row id"))
        .collect::<BTreeSet<_>>();
    let right_row_ids = (0..right_vertices.row_count())
        .map(|row| right_vertices.row_id_at(row).expect("right row id"))
        .collect::<BTreeSet<_>>();
    assert_eq!(left_row_ids, right_row_ids);
}
