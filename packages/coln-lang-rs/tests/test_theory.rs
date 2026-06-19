mod common;

use coln_lang_rs::ir::Path;

// TODO add more theory json files
const THEORY_FIXTURES: &[&str] = &["Graph.json"];

#[test]
fn deserialises_all_theory_fixtures() {
    for name in THEORY_FIXTURES {
        common::load_theory(name);
    }
}

#[test]
fn deserialises_graph_theory() {
    let theory = common::load_theory("Graph.json");

    assert_eq!(theory.tables.len(), 2);
    assert_eq!(theory.rules.len(), 2);

    assert_eq!(theory.tables[0].path, Path::from("Graph.E"));
    assert_eq!(theory.tables[1].path, Path::from("Graph.V"));

    let rules = &theory.rules[0];
    assert_eq!(rules.path, Path::from("Graph.E.foreignKey"));
    assert_eq!(rules.rule.var_names.len(), 2);
    assert_eq!(rules.rule.var_types.len(), 2);
}
