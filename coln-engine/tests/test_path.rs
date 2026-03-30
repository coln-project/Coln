use std::path::PathBuf;

use geomerge::{
    ir::{FlatTheory, Path},
    store::Store,
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
fn test_create_table_for_path() {
    let theory = fixture_theory(PATHS_IR);
    let n_tables = theory.tables.len();
    let n_laws = theory.laws.len();

    let mut store = Store::from_theory(theory);

    assert_eq!(store.table_count(), n_tables);
    assert_eq!(store.laws().len(), n_laws);
    assert!(store.resolve_table(&Path::from("Graphs")).is_some());

    {
        let ge = store.table_at_mut(&Path::from("G.E")).expect("G.E table");
        assert_eq!(ge.schema().columns.len(), 3);
        assert_eq!(ge.row_count(), 0);
    }

    let gv = store.table_at_mut(&Path::from("G.V")).expect("G.V table");
    
    let mut values = vec![CellValue::Id(1), CellValue::Id(1)];
    // add two vertices to the Grahph table of id 1, the vertex id would be implicit
    let op1 = gv.add(values);
    gv.apply(op).expect("add row to G.V");
}
