use std::path::PathBuf;

use geomerge::{
    ir::{FlatTheory, Path},
    store::{Store, StoreIntError},
    table::CellValue,
};

static PATHS_IR: &str = "paths.json";

fn fixture_theory(name: &str) -> FlatTheory {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);

    let json = std::fs::read_to_string(p).expect("read tests/data/paths.json");
    serde_json::from_str(&json).expect("parse FlatTheory from JSON")
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
        11,
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

    let graphs = store.table_at(&Path::from("Graphs")).expect("Graph table");
    let g0 = store.table_at(&Path::from("G0")).expect("G0 table");
    let g1 = store.table_at(&Path::from("G1")).expect("G1 table");
    let gv = store.table_at(&Path::from("G.V")).expect("G.V table");

    // One explicit column (Graphs); row id will be assigned by the db.
    let op0 = graphs.add(vec![]);
    let g0_op = g0.add(vec![CellValue::Id(0)]);
    let g1_op = g1.add(vec![CellValue::Id(0)]);

    let op1 = gv.add(vec![CellValue::Id(1)]);
    let op2 = gv.add(vec![CellValue::Id(1)]);

    let gid = store.apply_batch(vec![op0, g0_op, g1_op]).unwrap()[0];
    let vids = store
        .apply_batch(vec![op1, op2])
        .expect("inserting vs successful");

    let ge = store.table_at(&Path::from("G.E")).expect("G.E table");
    assert_eq!(ge.schema().columns.len(), 3);
    assert_eq!(ge.row_count(), 0);

    let op3 = ge.add(vec![
        CellValue::Id(gid),
        CellValue::Id(vids[0]),
        CellValue::Id(vids[1]),
    ]);
    assert!(store.apply_batch(vec![op3]).is_ok());
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

    let op = graphs.add(vec![]);
    let err = store
        .apply_batch(vec![op])
        .expect_err("missing witness rows");
    assert!(matches!(err, StoreIntError::Law(_)));

    assert_eq!(
        store.table_at(&Path::from("Graphs")).unwrap().row_count(),
        0
    );
    assert_eq!(store.table_at(&Path::from("G0")).unwrap().row_count(), 0);
    assert_eq!(store.table_at(&Path::from("G1")).unwrap().row_count(), 0);
}
