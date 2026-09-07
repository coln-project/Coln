// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::Path;
use coln_flir_rs::test_utils;

// TODO add more theory json files
const THEORY_FIXTURES: &[&str] = &["Graph.json", "Prim.json"];

#[test]
fn deserialises_all_theory_fixtures() {
    for name in THEORY_FIXTURES {
        test_utils::load_theory_from_json(name);
    }
}

#[test]
fn deserialises_graph_theory() {
    let theory = test_utils::load_theory_from_json("Graph.json");

    assert_eq!(theory.tables.len(), 2);
    assert_eq!(theory.rules.len(), 2);

    assert_eq!(theory.tables[0].path, Path::from("Graph.E"));
    assert_eq!(theory.tables[1].path, Path::from("Graph.V"));

    let rules = &theory.rules[0];
    assert_eq!(rules.path, Path::from("Graph.E.foreignKey"));
    assert_eq!(rules.rule.var_names.len(), 2);
    assert_eq!(rules.rule.var_types.len(), 2);
}
