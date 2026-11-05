// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_bouncer::bouncer::Bouncer;
use coln_bouncer::rw::{DbRead, DbWrite};
use coln_flir_rs::engine::{schema::ColnDef, tx::TxTuple};
use coln_flir_rs::ir::{FlatRealm, Path};
use coln_flir_rs::{public::PublicScalarValue, query::WhereClause};
use coln_store::txn::{
    id::Promote,
    rw::{StoreRead, StoreWrite},
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
    let mut bouncer = Bouncer::try_new(ir, coln_def).expect("create bouncer success");
    bouncer.transaction();
    let va = bouncer
        .add(&Path::from("root.V"), TxTuple::empty())
        .expect("add successful");
    let vb = bouncer
        .add(&Path::from("root.V"), TxTuple::empty())
        .expect("add successful");
    let vc = bouncer
        .add(&Path::from("root.V"), TxTuple::empty())
        .expect("add successful");

    // TODO change the API so user does not need to clone
    let _e1 = bouncer
        .add(&Path::from("root.E"), vec![va.clone(), vb.clone()])
        .expect("add edge successful");
    let _e2 = bouncer
        .add(&Path::from("root.E"), vec![vb.clone(), vc.clone()])
        .expect("add edge successful");

    let r = bouncer.commit().expect("commit success");
    let [va, vb] = bouncer
        .promote(vec![va, vb], r.cmt_hash)
        .try_into()
        .unwrap();

    let connected = bouncer
        .all_proj(
            &WhereClause {
                table_name: Path::from("init.trans-closure.connected"),
                row_id: None,
                values: vec![PublicScalarValue::RowId(va), PublicScalarValue::RowId(vb)].into(),
            },
            &[1, 2],
        )
        .unwrap()
        .len();
    assert_eq!(connected, 1);
}
