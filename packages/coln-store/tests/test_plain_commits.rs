// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::engine::{schema::ColnDef, tx::TxTuple};
use coln_flir_rs::ir::{FlatRealm, Path};
use coln_store::{store::Store, txn::rw::StoreWrite};

#[test]
fn plain_replay_preserves_commits_without_materializing_derived_rows() {
    let ir: FlatRealm = serde_json::from_str(include_str!(
        "../../coln-flir-rs/tests/data/TransitiveClosureRealm.json"
    ))
    .expect("transitive closure IR");
    assert!(!ir.definitions.is_empty());
    let coln_def = ColnDef {
        theory: String::new(),
        realm: "TransitiveClosureRealm".into(),
    };
    let mut source = Store::try_from_ir(&ir, coln_def.clone()).expect("source store");
    let mut target = Store::try_from_ir(&ir, coln_def).expect("target store");
    let vertex = Path::from("root.V");
    let derived = Path::from("init.trans-closure.connected");
    let mut txn = source.transaction();
    txn.add(&vertex, TxTuple::empty()).expect("add vertex");
    txn.commit().expect("commit vertex");

    let chunks = source.commit_chunks_after(&[]);
    let restored = Store::try_from_commit_bytes_plain(chunks.iter().map(|chunk| &chunk.bytes))
        .expect("restore commit bytes");
    target
        .apply_chunk_bytes_plain(
            source
                .commit_chunks_after(&target.heads())
                .into_iter()
                .map(|chunk| chunk.bytes),
        )
        .expect("apply commit bytes");
    let (mut decoded, commits) =
        Store::decode(&source.encode().expect("encode snapshot")).expect("decode snapshot");
    decoded
        .apply_commits_plain(commits)
        .expect("replay snapshot");

    for store in [restored, target, decoded] {
        assert_eq!(
            store.table_at(&vertex).expect("vertex table").row_count(),
            1
        );
        // The reflexive closure would contain a row if the query engine ran.
        assert_eq!(
            store.table_at(&derived).expect("derived view").row_count(),
            0
        );
        assert_eq!(store.heads(), source.heads());
        assert_eq!(
            store.encode().expect("re-encode snapshot"),
            source.encode().expect("original snapshot"),
        );
    }
}
