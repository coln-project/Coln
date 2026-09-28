// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::Path;
use coln_flir_rs::test_utils::{self, all_theory_fixtures};

#[test]
fn deserialises_all_theory_fixtures() {
    for name in all_theory_fixtures().expect("list json theory files") {
        test_utils::load_theory_from_json(name);
    }
}

#[test]
fn deserialises_graph_theory() {
    let theory = test_utils::load_theory_from_json("GraphRealm.json");

    assert_eq!(theory.tables.len(), 4);
    assert_eq!(theory.definitions.len(), 2);
    assert_eq!(theory.rules.len(), 2);

    assert_eq!(theory.tables[0].path, Path::from("root.V"));
    assert_eq!(theory.tables[1].path, Path::from("root.E"));

    let e_foreign_key_rule = &theory.rules[1];
    assert_eq!(e_foreign_key_rule.path, Path::from("root.E.foreignKey"));
    assert_eq!(e_foreign_key_rule.rule.vars.len(), 2);
}
