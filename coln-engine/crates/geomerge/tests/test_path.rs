use std::{path::PathBuf, sync::Once};

use geomerge::{
    ir::{self, FlatTheory, Path},
    persist::pst,
    store::{Store, StoreIntError},
    table::CellValue,
};
use tracing_subscriber::EnvFilter;

static PATHS_IR: &str = "paths.json";

// For testing only
#[allow(dead_code)]
static INIT: Once = Once::new();
#[allow(dead_code)]
fn init_test_logging() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("geomerge=debug")),
            )
            .with_test_writer()
            .init();
    });
}

fn fixture_theory(name: &str) -> FlatTheory {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);

    let json = std::fs::read_to_string(p).expect("read tests/data/paths.json");
    serde_json::from_str(&json).expect("parse FlatTheory from JSON")
}

fn add_basic_data_to_path(store: &mut Store) -> Result<(), StoreIntError> {
    let graphs = store
        .table_at_mut(&Path::from("Graphs"))
        .expect("Graph table");

    let gid1 = graphs.add(vec![]);
    let gid2 = graphs.add(vec![]);
    let g0 = store.table_at_mut(&Path::from("G0")).expect("G0 table");
    let g0_op = g0.add(vec![CellValue::Id(gid2.id())]);
    let g1 = store.table_at_mut(&Path::from("G1")).expect("G1 table");
    let g1_op = g1.add(vec![CellValue::Id(gid2.id())]);

    let gv = store.table_at_mut(&Path::from("G.V")).expect("G.V table");
    let v1 = gv.add(vec![CellValue::Id(gid1.id())]);
    let v2 = gv.add(vec![CellValue::Id(gid1.id())]);

    let ge = store.table_at_mut(&Path::from("G.E")).expect("G.E table");

    let eop = ge.add(vec![
        CellValue::Id(gid1.id()),
        CellValue::Id(v1.id()),
        CellValue::Id(v2.id()),
    ]);

    store
        .apply_batch(vec![gid1, gid2, g0_op, g1_op, v1, v2, eop])
        .expect("applied successfully");
    Ok(())
}

#[test]
fn test_read_path_geolog() {
    let theory = fixture_theory(PATHS_IR);

    assert_eq!(
        theory.tables.len(),
        10,
        "expected table count from geolog paths.json"
    );
    assert_eq!(
        theory.laws.len(),
        12,
        "expected law count from geolog paths.json"
    );

    let g_edge = theory
        .tables
        .iter()
        .find(|t| t.path == Path::from("G.E"))
        .expect("G,E table");
    assert_eq!(g_edge.table.columns.len(), 3);
    assert_eq!(g_edge.table.primary_key, None);

    let graphs = theory
        .tables
        .iter()
        .find(|t| t.path == Path::from("Graphs"))
        .expect("Graphs table");
    assert!(graphs.table.columns.is_empty());

    let hom_e_fk = theory
        .laws
        .iter()
        .find(|e| e.path == Path::from("Hom.E.foreignKeys"))
        .expect("Hom E foreignKeys law path");
    assert!(
        !hom_e_fk.law.variables.is_empty(),
        "Hom,E foreignKeys law should bind variables"
    );
}

#[test]
fn test_compile_path_laws() {
    let theory = fixture_theory(PATHS_IR);
    let expected_law_count = theory.laws.len();
    let store = Store::try_from_theory(theory).expect("valid theory");

    assert!(expected_law_count > 0, "fixture should contain laws");
    assert_eq!(store.laws().len(), expected_law_count);
}

#[test]
// Builds a minimal valid graph dataset from the fixture, including the witness
// rows required by the fixture's totality laws before inserting vertices/edges.
fn test_add_data_and_law_enforce() {
    let theory = fixture_theory(PATHS_IR);
    let n_tables = theory.tables.len();
    let n_laws = theory.laws.len();

    let mut store = Store::try_from_theory(theory).expect("valid theory");

    assert_eq!(store.table_count(), n_tables);
    assert_eq!(store.laws().len(), n_laws);
    assert!(store.resolve_table(&Path::from("Graphs")).is_some());

    // One explicit column (Graphs); row id will be assigned by the db.

    let r = add_basic_data_to_path(&mut store);

    let ge = store.table_at(&Path::from("G.E")).expect("get table G.E");
    assert_eq!(ge.schema().columns.len(), 3);
    assert_eq!(ge.row_count(), 1);
}

#[test]
// Rejects a graph insert when the corresponding witness rows are missing and
// confirms the failed batch leaves the store unchanged.
fn test_missing_graph_witness_rejects_batch_without_mutation() {
    let theory = fixture_theory(PATHS_IR);
    let mut store = Store::try_from_theory(theory).expect("valid theory");

    let graphs = store.table_at(&Path::from("Graphs")).expect("Graph table");
    let g0 = store.table_at(&Path::from("G0")).expect("G0 table");
    let g1 = store.table_at(&Path::from("G1")).expect("G1 table");

    assert_eq!(graphs.row_count(), 0);
    assert_eq!(g0.row_count(), 0);
    assert_eq!(g1.row_count(), 0);

    let op = store
        .table_at_mut(&Path::from("Graphs"))
        .expect("Graph table")
        .add(vec![]);
    let err = store.apply_batch(vec![op]).expect_err("missing g0 and g1");
    assert!(matches!(*err, StoreIntError::Law(_)));

    assert_eq!(
        store.table_at(&Path::from("Graphs")).unwrap().row_count(),
        0
    );
    assert_eq!(store.table_at(&Path::from("G0")).unwrap().row_count(), 0);
    assert_eq!(store.table_at(&Path::from("G1")).unwrap().row_count(), 0);
}

#[test]
fn test_fk() {
    let theory = fixture_theory(PATHS_IR);
    let mut store = Store::try_from_theory(theory).expect("valid theory");

    let op0 = store
        .table_at_mut(&Path::from("Graphs"))
        .expect("Graph table")
        .add(vec![]);
    let gid = op0.id();
    let g0_op = store
        .table_at_mut(&Path::from("G0"))
        .expect("G0")
        .add(vec![CellValue::Id(0)]);
    let g1_op = store
        .table_at_mut(&Path::from("G1"))
        .expect("G1")
        .add(vec![CellValue::Id(0)]);

    let op1 = store
        .table_at_mut(&Path::from("G.V"))
        .expect("G.V")
        .add(vec![CellValue::Id(1)]);

    store.apply_batch(vec![op0, g0_op, g1_op]).unwrap();
    let vid = op1.id();
    store
        .apply_batch(vec![op1])
        .expect("inserting v1 successful");

    let dummy_vid = u64::MAX;
    let ope = store
        .table_at_mut(&Path::from("G.E"))
        .expect("G.E")
        .add(vec![
            CellValue::Id(gid),
            CellValue::Id(vid),
            CellValue::Id(dummy_vid),
        ]);
    let err = store.apply_batch(vec![ope]).expect_err("missing v2");

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
            .table_at(&ir::Path::from("G.V"))
            .expect("table G.V")
            .row_count()
            > 0
    );

    let content = store.dump();
    let data = pst::encode_store(&store).expect("encoding store success");
    let st = pst::decode_store(&data).expect("decode store success");

    assert_eq!(content, st.dump());
}
