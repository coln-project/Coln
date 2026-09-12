// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::{FlatRealm, Path};
use coln_store::{
    store::{ColnDef, auto::AutoStore},
    txn::{
        empty_row,
        rw::{StoreRead, StoreWrite, WhereClause},
    },
    value::Value,
};
use rstest::{fixture, rstest};

static TC_IR: &str = include_str!("../../coln-flir-rs/tests/data/TransitiveClosureRealm.json");

#[fixture]
fn ir() -> FlatRealm {
    serde_json::from_str(TC_IR).expect("valid tc ir")
}

#[fixture]
fn coln_def() -> ColnDef {
    ColnDef {
        theory: String::new(),
        realm: "TransitiveClosureRealm".into(),
    }
}

#[rstest]
fn test_tc_computation(ir: FlatRealm, coln_def: ColnDef) {
    let mut auto_store = AutoStore::try_from_ir(ir, coln_def).expect("create store successful");
    let va = auto_store
        .add(&Path::from("root.V"), empty_row())
        .expect("add successful");
    let vb = auto_store
        .add(&Path::from("root.V"), empty_row())
        .expect("add successful");
    let vc = auto_store
        .add(&Path::from("root.V"), empty_row())
        .expect("add successful");

    // TODO change the API so user does not need to clone
    let _e1 = auto_store
        .add(&Path::from("root.E"), vec![va.clone(), vb.clone()])
        .expect("add edge successful");
    let _e2 = auto_store
        .add(&Path::from("root.E"), vec![vb.clone(), vc])
        .expect("add edge successful");

    auto_store.commit().expect("commit success");

    // TODO change the API so user does not need to manually construct WireValue?
    let connected = auto_store
        .all(
            &WhereClause {
                table_name: Path::from("init.trans-closure.connected"),
                row_id: None,
                values: vec![
                    Value::Id(va.row_id().unwrap()),
                    Value::Id(vb.row_id().unwrap()),
                ],
            },
            &[1, 2],
        )
        .unwrap()
        .len();
    assert_eq!(connected, 1);
}
