use coln_flir_rs::engine::delta::{StoreDelta, TableDelta, ZRow};
use coln_flir_rs::engine::packed::{StoreScalarValue, StoreTuple};
use coln_flir_rs::engine::table::{STReadTxn, SortedTable, StoreST};
use coln_flir_rs::engine::tx::TxTuple;
use rstest::rstest;

use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
use crate::pack::id_packer::IdLookup;
use crate::store::Store;
use crate::store::error::StoreError;
use crate::store::txn::ReadTxnable;
use crate::table::ValidationError;
use crate::test_utils::{nodes_edges_store, single_int_store, single_memoized_int_store};

#[rstest]
fn sorted_table_at_reports_arity(#[from(single_int_store)] store: Store) {
    let path = Path::from("T");
    let read = StoreST::begin_read(&store);
    let table = read.sorted_tables_at(&path).expect("T");

    assert_eq!(table.arity(), 2); // Row ID plus the integer column.
    assert!(read.sorted_tables_at(&Path::from("missing")).is_none());
}

#[rstest]
fn validates_then_applies(#[from(single_int_store)] store: Store) {
    let path = Path::from("T");

    let mut txn = store.begin_write();
    txn.add(&path, vec![1i32]).expect("first add");
    txn.add(&path, vec![2i32]).expect("second add");

    txn.commit().expect("commit");

    assert_eq!(
        store.begin_read().table_at(&path).expect("T").row_count(),
        2
    );
}

/// Dropping a transaction after an add fails discards its earlier pending rows.
#[rstest]
fn unknown_table_leaves_store_unchanged(#[from(single_int_store)] store: Store) {
    let path = Path::from("T");

    let err = {
        let mut txn = store.begin_write();
        txn.add(&path, vec![1i32]).expect("first add");
        txn.add(&Path::from("missing"), vec![2i32]).unwrap_err()
    };

    assert!(matches!(
        err,
        StoreError::Validation(ValidationError::UnknownTable { .. })
    ));
    assert_eq!(
        store.begin_read().table_at(&path).expect("T").row_count(),
        0
    );
}

#[test]
fn duplicate_primary_key_within_batch() {
    let path = Path::from("T");
    let schema = Schema {
        entity_variant: EntityVariant::Table,
        columns: vec![ColumnEntry {
            path: Path::from("c0"),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            },
        }],
        primary_key: Some(vec![0u64]),
    };
    let mut store = Store::new();
    store
        .create_table(path.clone(), schema)
        .expect("create table");

    let mut txn = store.begin_write();
    txn.add(&path, vec![1i32]).expect("first add");
    txn.add(&path, vec![1i32]).expect("second add");
    let err = txn.commit().err().expect("commit should fail");

    assert!(matches!(
        err,
        StoreError::Validation(ValidationError::DuplicatePrimaryKey)
    ));
    assert_eq!(
        store.begin_read().table_at(&path).expect("T").row_count(),
        0
    );
}

#[rstest]
fn single_insert_commits(#[from(single_int_store)] store: Store) {
    let path = Path::from("T");

    let mut txn = store.begin_write();
    let row_id = txn.add(&path, vec![42i32]).expect("add");
    let h = txn.commit().expect("commit").commit_hash();
    let read_txn = store.begin_read();
    let row_id = read_txn
        .id_lookup()
        .packed(&read_txn.promote_one(row_id, h))
        .unwrap();

    assert_eq!(
        read_txn.row_by_id(&path, &row_id),
        Some(StoreTuple::from_id_values(row_id, vec![42i32.into()]))
    );
}

#[rstest]
fn transaction_resolves_pending_row_references_with_commit_hash(
    #[from(nodes_edges_store)] store: Store,
) {
    let nodes = Path::from("Nodes");
    let edges = Path::from("Edges");

    let mut tx = store.begin_write();
    let node_temp = tx.add(&nodes, TxTuple::empty()).expect("add node");
    let edge_temp = tx.add(&edges, vec![node_temp]).expect("add edge");
    let h = tx.commit().expect("commit").commit_hash();
    let read = store.begin_read();
    let ids = read.promote([node_temp, edge_temp], h);
    let [node_id, edge_id] = ids.try_into().unwrap();

    assert_eq!(node_id.commit, h);
    assert_eq!(node_id.counter, 0);
    assert_eq!(edge_id.commit, h);
    assert_eq!(edge_id.counter, 1);

    let node_id = read.id_lookup().packed(&node_id).unwrap();
    let edge_id = read.id_lookup().packed(&edge_id).unwrap();
    assert_eq!(
        read.row_by_id(&edges, &edge_id),
        Some(StoreTuple::from_id_values(
            edge_id,
            vec![StoreScalarValue::RowId(node_id)]
        ))
    );
}

#[rstest]
fn committed_row_handle_can_be_used_in_later_transaction(#[from(nodes_edges_store)] store: Store) {
    let nodes = Path::from("Nodes");
    let edges = Path::from("Edges");

    let mut tx = store.begin_write();
    let node = tx.add(&nodes, TxTuple::empty()).expect("add node");
    let first_commit = tx.commit().expect("commit node").commit_hash();

    let node_id = store.begin_read().promote_one(node, first_commit);
    assert_eq!(node_id.commit, first_commit);

    let mut tx = store.begin_write();
    let edge = tx.add(&edges, vec![node_id]).expect("add edge");
    let h = tx.commit().expect("commit edge").commit_hash();

    let read = store.begin_read();
    let edge_id = read.id_lookup().packed(&read.promote_one(edge, h)).unwrap();
    let node_id = read.id_lookup().packed(&node_id).unwrap();

    assert_eq!(
        read.row_by_id(&edges, &edge_id),
        Some(StoreTuple::from_id_values(
            edge_id,
            vec![StoreScalarValue::RowId(node_id)]
        ))
    );
}

/// A pending id whose row deduplicates into an existing structural class
/// promotes to the id the store actually kept, not to its raw
/// `(commit, counter)` id, which names no stored row. The first id
/// may still go stale when the second commit wins the merge; promoting
/// the existing id again resolves it to the canonical id.
#[rstest]
fn deduplicated_row_handle_finalizes_to_canonical_id(
    #[from(single_memoized_int_store)] store: Store,
) {
    let term = Path::from("T");

    let mut tx = store.begin_write();
    let first = tx.add(&term, vec![7_i32]).expect("add first term");
    let h1 = tx.commit().expect("commit first term").commit_hash();
    let first = store.begin_read().promote_one(first, h1);

    // Structurally equal row: deduplicates into the first row's class.
    // Which id wins the merge depends on the commit hash ordering.
    let mut tx = store.begin_write();
    let second = tx.add(&term, vec![7_i32]).expect("add equal term");
    let h2 = tx.commit().expect("commit equal term").commit_hash();

    let read = store.begin_read();
    let stored = read.promote_one(second, h2);
    assert_eq!(read.scan_table(&term).expect("T").len(), 1);

    // Reads return the canonical id. Promoting an already-existing id
    // also canonicalises, so a stale first id catches up.
    let view = read
        .row_by_id(&term, &read.id_lookup().packed(&first).unwrap())
        .expect("class row is stored");
    assert_eq!(
        view.first(),
        Some(&StoreScalarValue::RowId(
            read.id_lookup().packed(&stored).unwrap()
        ))
    );
    let first = read.promote_one(first, h1);
    assert_eq!(first, stored);
}

#[rstest]
fn transaction_commit_updates_commit_graph_heads_and_deps(#[from(single_int_store)] store: Store) {
    let path = Path::from("T");
    let root = store.begin_read().data.commits.root_commit().hash();

    let mut tx = store.begin_write();
    tx.add(&path, vec![1i32]).expect("add first row");
    let first = tx.commit().expect("first commit").commit_hash();

    {
        let read = store.begin_read();
        let commit = read.commit_by_hash(&first).expect("first commit is stored");
        assert_eq!(commit.deps, vec![root]);
        assert_eq!(read.heads(), vec![first]);
    }

    let mut tx = store.begin_write();
    tx.add(&path, vec![2i32]).expect("add second row");
    let second = tx.commit().expect("second commit").commit_hash();

    let read = store.begin_read();
    let commit = read
        .commit_by_hash(&second)
        .expect("second commit is stored");
    assert_eq!(commit.deps, vec![first]);
    assert_eq!(read.heads(), vec![second]);
}

/// Reads through a preparing write transaction exclude its pending operations.
#[rstest]
fn transaction_store_read_sees_committed_rows_not_pending(#[from(single_int_store)] store: Store) {
    let path = Path::from("T");

    let mut tx = store.begin_write();
    let pending_id = tx.add(&path, vec![1i32]).expect("add");
    let h = tx.commit().expect("commit").commit_hash();

    let mut tx = store.begin_write();
    let row_id = tx
        .read()
        .id_lookup()
        .packed(&tx.read().promote_one(pending_id, h))
        .unwrap();

    assert_eq!(
        tx.read().row_by_id(&path, &row_id),
        Some(StoreTuple::from_id_values(row_id, vec![1i32.into()]))
    );

    tx.add(&path, vec![2i32]).expect("add pending");
    assert_eq!(tx.read().scan_table(&path).expect("T").len(), 1);
    drop(tx);
}

/// Committing an empty transaction does not modify the commit graph
#[test]
fn txn_empty_commit_not_added() {
    let store = Store::new();
    let (root, heads, commits) = {
        let read = store.begin_read();
        let graph = &read.data.commits;
        (
            graph.root_commit().hash(),
            read.heads(),
            graph
                .iter_topological()
                .map(|c| c.hash())
                .collect::<Vec<_>>(),
        )
    };

    let error = store.begin_write().commit().err().expect("empty commit");
    assert!(matches!(
        error,
        StoreError::Commit(crate::store::error::CommitApplyError::EmptyCommit)
    ));
    let read = store.begin_read();
    let graph = &read.data.commits;
    assert_eq!(graph.root_commit().hash(), root);
    assert_eq!(read.heads(), heads);
    assert_eq!(
        graph
            .iter_topological()
            .map(|c| c.hash())
            .collect::<Vec<_>>(),
        commits
    );
}

#[rstest]
fn pending_read_reuses_write_lock_and_sees_tentative_rows(#[from(single_int_store)] store: Store) {
    let path = Path::from("T");
    let heads = store.begin_read().heads();
    let mut tx = store.begin_write();
    let id = tx.add(&path, vec![42i32]).expect("add");
    assert!(tx.read().scan_table(&path).expect("T").is_empty());
    let mut pending = tx.try_commit().expect("prepare");
    let hash = pending.prepared_hash();
    assert!(
        store.data.try_read().is_none(),
        "pending holds the write lock"
    );
    {
        let read = pending.read();
        let public = read.promote_one(id, hash);
        let packed = read.id_lookup().packed(&public).expect("packed id");
        assert_eq!(
            read.row_by_id(&path, &packed),
            Some(StoreTuple::from_id_values(packed, vec![42i32.into()]))
        );
        assert_eq!(read.heads(), heads);
        assert!(read.commit_by_hash(&hash).is_none());
    }
    assert!(!pending.store_delta().is_empty());
    assert_eq!(pending.read().scan_table(&path).expect("T").len(), 1);
    assert_eq!(pending.commit().commit_hash(), hash);
    assert_eq!(store.begin_read().heads(), vec![hash]);
}

#[rstest]
#[case::commit(0)]
#[case::abort(1)]
#[case::drop(2)]
fn derived_rows_follow_pending_transaction_outcome(
    #[from(single_int_store)] store: Store,
    #[case] outcome: u8,
) {
    let path = Path::from("T");
    let heads = store.begin_read().heads();
    let mut tx = store.begin_write();
    let id = tx.add(&path, vec![1i32]).expect("add base row");
    let mut pending = tx.try_commit().expect("prepare");
    let hash = pending.prepared_hash();
    let derived = {
        let read = pending.read();
        let mut id = read.promote_one(id, hash);
        id.counter += 1;
        let id = read.id_lookup().packed(&id).expect("derived id");
        StoreTuple::from_id_values(id, vec![2i32.into()])
    };
    pending
        .apply_derived_view(
            StoreDelta::new([TableDelta::new(
                path.clone(),
                [ZRow::new_unchecked(1, derived)],
            )])
            .consolidate(),
        )
        .expect("apply derived row");
    assert_eq!(pending.read().scan_table(&path).expect("T").len(), 2);
    match outcome {
        0 => {
            pending.commit();
        }
        1 => {
            pending.abort();
        }
        _ => drop(pending),
    }
    let read = store.begin_read();
    assert_eq!(
        read.scan_table(&path).expect("T").len(),
        if outcome == 0 { 2 } else { 0 }
    );
    assert_eq!(read.heads(), if outcome == 0 { vec![hash] } else { heads });
    assert!(!read.has_pending());
}
