// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::BTreeSet;

use coln_bouncer::bouncer::{Bouncer, ReadTxnable as _};
use coln_bouncer::error::{BouncerError, LawViolation};
use coln_flir_rs::engine::{
    schema::ColnDef,
    tx::{TxRowId, TxTuple},
};
use coln_flir_rs::ir::{FlatRealm, Path};
use coln_flir_rs::public::{PublicRowId, PublicScalarValue};
use coln_store::store::{
    Store,
    error::{CommitApplyError, StoreError},
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
    // create a bouncer
    let bouncer = Bouncer::try_new(ir, coln_def).expect("create bouncer success");
    // start a write txn
    let mut tx = bouncer.begin_write();
    // Add a bunch of stuff
    let va = tx
        .add(&Path::from("root.V"), TxTuple::empty())
        .expect("add successful");
    let vb = tx
        .add(&Path::from("root.V"), TxTuple::empty())
        .expect("add successful");
    let vc = tx
        .add(&Path::from("root.V"), TxTuple::empty())
        .expect("add successful");

    let _e1 = tx
        .add(&Path::from("root.E"), vec![va, vb])
        .expect("add edge successful");
    let _e2 = tx
        .add(&Path::from("root.E"), vec![vb, vc])
        .expect("add edge successful");

    // try_commit makes both base and derived data tentatively readable.
    let pending_tx = tx.try_commit().expect("try success");
    let tuples = pending_tx
        .scan_table(&Path::from("root.V"))
        .expect("table exists");
    // We can read them
    assert_eq!(tuples.len(), 3);
    let derived = pending_tx
        .scan_table(&Path::from("init.trans-closure.connected"))
        .expect("derived view");
    let pending_va = pending_tx.promote_one(va, pending_tx.prepared_hash());
    let pending_vc = pending_tx.promote_one(vc, pending_tx.prepared_hash());
    assert!(derived.iter().any(|row| {
        row[1..]
            == [
                PublicScalarValue::RowId(pending_va),
                PublicScalarValue::RowId(pending_vc),
            ]
    }));
    // Now commit the transaction
    let r = pending_tx.commit().expect("commit success");
    assert_eq!(
        bouncer
            .begin_read()
            .scan_table(&Path::from("init.trans-closure.connected"))
            .expect("derived view"),
        derived
    );
    let [va, _vb, vc] = bouncer
        .begin_read()
        .promote(vec![va, vb, vc], r.cmt_hash())
        .try_into()
        .unwrap();

    assert!(connected_pairs(&bouncer).contains(&(va, vc)));
}

#[fixture]
fn one_edge(ir: FlatRealm, coln_def: ColnDef) -> (Bouncer, [PublicRowId; 2]) {
    let source = Bouncer::try_new(ir, coln_def).expect("source bouncer");
    let mut tx = source.begin_write();
    let a = tx
        .add(&Path::from("root.V"), TxTuple::empty())
        .expect("vertex a");
    let b = tx
        .add(&Path::from("root.V"), TxTuple::empty())
        .expect("vertex b");
    tx.add(&Path::from("root.E"), vec![a, b])
        .expect("edge a -> b");
    let committed = tx.commit().expect("commit edge");
    let ids = source
        .begin_read()
        .promote([a, b], committed.cmt_hash())
        .try_into()
        .expect("two vertices");
    (source, ids)
}

#[rstest]
fn derived_row_ids_do_not_overlap_base_operations(one_edge: (Bouncer, [PublicRowId; 2])) {
    let (bouncer, _) = one_edge;
    let read = bouncer.begin_read();
    let rows = ["root.V", "root.E", "init.trans-closure.connected"]
        .into_iter()
        .flat_map(|path| read.scan_table(&Path::from(path)).expect("table"))
        .collect::<Vec<_>>();
    let ids = rows
        .iter()
        .map(|row| match row[0] {
            PublicScalarValue::RowId(id) => id,
            ref other => panic!("expected row id, got {other:?}"),
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(rows.len(), 6, "three base rows and three derived rows");
    assert_eq!(ids.len(), rows.len(), "every row has a distinct public id");
}

fn extend_chain(bouncer: &mut Bouncer, tail: &PublicRowId) -> PublicRowId {
    let mut tx = bouncer.begin_write();
    let next = tx
        .add(&Path::from("root.V"), TxTuple::empty())
        .expect("new vertex");
    tx.add(&Path::from("root.E"), vec![TxRowId::from(*tail), next])
        .expect("edge from old tail");
    let committed = tx.commit().expect("extend chain");
    bouncer
        .begin_read()
        .promote([next], committed.cmt_hash())
        .pop()
        .expect("new vertex id")
}

fn connected_pairs(bouncer: &Bouncer) -> BTreeSet<(PublicRowId, PublicRowId)> {
    let rows = bouncer
        .begin_read()
        .scan_table(&Path::from("init.trans-closure.connected"))
        .expect("derived view");
    let pairs = rows
        .iter()
        .map(|row| match (&row[1], &row[2]) {
            (PublicScalarValue::RowId(from), PublicScalarValue::RowId(to)) => (*from, *to),
            other => panic!("expected two vertex references, got {other:?}"),
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(pairs.len(), rows.len(), "derived pairs must be unique");
    pairs
}

#[rstest]
fn repeated_sync_is_a_noop_for_base_and_derived_data(
    one_edge: (Bouncer, [PublicRowId; 2]),
    ir: FlatRealm,
    coln_def: ColnDef,
) {
    let (source, [a, b]) = one_edge;
    let mut target = Bouncer::try_new(ir, coln_def).expect("target bouncer");
    let root = target.begin_read().fragments_after(&[])[0].hash;
    let chunks = source.begin_read().fragments_after(&[root]);
    target
        .begin_import()
        .apply_chunk_bytes(chunks.iter().map(|chunk| chunk.bytes.clone()))
        .expect("first sync");
    assert_eq!(
        connected_pairs(&target),
        BTreeSet::from([(a, a), (a, b), (b, b)])
    );
    let vertices = target
        .begin_read()
        .scan_table(&Path::from("root.V"))
        .expect("vertices");
    let edges = target
        .begin_read()
        .scan_table(&Path::from("root.E"))
        .expect("edges");
    let derived = target
        .begin_read()
        .scan_table(&Path::from("init.trans-closure.connected"))
        .expect("derived rows");
    let encoded = target.begin_read().encode_store().expect("snapshot");

    assert!(
        target
            .begin_import()
            .apply_chunk_bytes(chunks.into_iter().map(|chunk| chunk.bytes))
            .expect("repeat sync")
            .is_empty()
    );
    assert_eq!(
        target
            .begin_read()
            .scan_table(&Path::from("root.V"))
            .expect("vertices"),
        vertices
    );
    assert_eq!(
        target
            .begin_read()
            .scan_table(&Path::from("root.E"))
            .expect("edges"),
        edges
    );
    assert_eq!(
        target
            .begin_read()
            .scan_table(&Path::from("init.trans-closure.connected"))
            .expect("derived rows"),
        derived
    );
    assert_eq!(
        target
            .begin_read()
            .encode_store()
            .expect("unchanged commit history"),
        encoded
    );

    let c = extend_chain(&mut target, &b);
    assert_eq!(
        connected_pairs(&target),
        BTreeSet::from([(a, a), (a, b), (a, c), (b, b), (b, c), (c, c)])
    );
}

#[derive(Clone, Copy)]
enum FailedDelivery {
    MalformedBytes,
    ForeignRoot,
    HardViolation,
}

#[rstest]
#[case::malformed_bytes(FailedDelivery::MalformedBytes)]
#[case::foreign_root(FailedDelivery::ForeignRoot)]
#[case::hard_violation(FailedDelivery::HardViolation)]
fn buffered_child_survives_failed_delivery_and_derives_after_parent_arrives(
    one_edge: (Bouncer, [PublicRowId; 2]),
    ir: FlatRealm,
    coln_def: ColnDef,
    #[case] failure: FailedDelivery,
) {
    let (mut source, [a, b]) = one_edge;
    let c = extend_chain(&mut source, &b);
    let target = Bouncer::try_new(ir.clone(), coln_def.clone()).expect("target bouncer");
    let root = target.begin_read().fragments_after(&[])[0].hash;
    let chunks = source.begin_read().fragments_after(&[root]);
    assert_eq!(chunks.len(), 2);
    assert!(
        target
            .begin_import()
            .apply_chunk_bytes([chunks[1].bytes.clone()])
            .expect("buffer child")
            .is_empty()
    );
    let before = target.begin_read().encode_store().expect("empty snapshot");

    let invalid_bytes = match failure {
        FailedDelivery::MalformedBytes => vec![0xde, 0xad],
        FailedDelivery::ForeignRoot => {
            let foreign = Store::try_from_ir(
                &ir,
                ColnDef {
                    realm: "AnotherRealm".into(),
                    ..coln_def
                },
            )
            .expect("foreign store");
            foreign
                .begin_read()
                .commit_chunks_after(&[])
                .pop()
                .expect("foreign root")
                .bytes
        }
        FailedDelivery::HardViolation => {
            let invalid = Store::try_from_ir(&ir, coln_def).expect("plain source");
            let missing_vertex = PublicRowId {
                commit: root,
                counter: 0,
            };
            let mut txn = invalid.begin_write();
            let vertex = txn
                .add(&Path::from("root.V"), TxTuple::empty())
                .expect("valid vertex");
            txn.add(&Path::from("root.E"), vec![vertex, missing_vertex.into()])
                .expect("dangling edge");
            txn.try_commit()
                .expect("plain store skips foreign-key rule")
                .commit();
            invalid
                .begin_read()
                .commit_chunks_after(&[root])
                .pop()
                .expect("invalid commit")
                .bytes
        }
    };
    let error = target
        .begin_import()
        .apply_chunk_bytes([invalid_bytes])
        .expect_err("invalid delivery");
    match failure {
        FailedDelivery::MalformedBytes => assert!(matches!(
            error,
            BouncerError::StoreError(StoreError::Encode(_))
        )),
        FailedDelivery::ForeignRoot => assert!(matches!(
            error,
            BouncerError::StoreError(StoreError::Commit(CommitApplyError::RootCommit(_)))
        )),
        FailedDelivery::HardViolation => assert!(
            matches!(error, BouncerError::Law(LawViolation::Hard(violations)) if !violations.is_empty())
        ),
    }
    assert_eq!(
        target
            .begin_read()
            .encode_store()
            .expect("snapshot after error"),
        before
    );
    assert!(
        target
            .begin_read()
            .scan_table(&Path::from("root.V"))
            .expect("vertices")
            .is_empty()
    );
    assert!(
        target
            .begin_read()
            .scan_table(&Path::from("root.E"))
            .expect("edges")
            .is_empty()
    );
    assert!(connected_pairs(&target).is_empty());

    let reports = target
        .begin_import()
        .apply_chunk_bytes([chunks[0].bytes.clone()])
        .expect("parent unlocks buffered child");
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|report| report.is_empty()));
    assert_eq!(
        target
            .begin_read()
            .scan_table(&Path::from("root.V"))
            .expect("vertices")
            .len(),
        3
    );
    assert_eq!(
        target
            .begin_read()
            .scan_table(&Path::from("root.E"))
            .expect("edges")
            .len(),
        2
    );
    assert_eq!(
        connected_pairs(&target),
        BTreeSet::from([(a, a), (a, b), (a, c), (b, b), (b, c), (c, c)])
    );
    assert_eq!(
        target.begin_read().encode_store().expect("target snapshot"),
        source.begin_read().encode_store().expect("source snapshot")
    );
}

#[rstest]
fn snapshot_round_trip_restores_derived_data_and_query_state(
    one_edge: (Bouncer, [PublicRowId; 2]),
) {
    let (source, [a, b]) = one_edge;
    let encoded = source.begin_read().encode_store().expect("encode bouncer");
    let (mut restored, reports) = Bouncer::decode_store(&encoded).expect("decode bouncer");
    assert_eq!(reports.len(), 1);
    assert!(reports[0].is_empty());
    for table in ["root.V", "root.E"] {
        assert_eq!(
            restored.begin_read().scan_table(&Path::from(table)),
            source.begin_read().scan_table(&Path::from(table))
        );
    }
    assert_eq!(
        connected_pairs(&restored),
        BTreeSet::from([(a, a), (a, b), (b, b)])
    );
    assert_eq!(
        restored.begin_read().encode_store().expect("re-encode"),
        encoded
    );

    // Deriving a -> c needs the old a -> b fact inside query, not just store.
    let c = extend_chain(&mut restored, &b);
    assert_eq!(
        connected_pairs(&restored),
        BTreeSet::from([(a, a), (a, b), (a, c), (b, b), (b, c), (c, c)])
    );
    assert_eq!(
        restored
            .begin_read()
            .scan_table(&Path::from("root.V"))
            .expect("vertices")
            .len(),
        3
    );
    assert_eq!(
        restored
            .begin_read()
            .scan_table(&Path::from("root.E"))
            .expect("edges")
            .len(),
        2
    );
}
