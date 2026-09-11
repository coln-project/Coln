// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use rstest::{fixture, rstest};

use super::*;
use crate::ir::{self, Path};
use crate::ir::{BuiltinTy, ColType};
use crate::op::Op;
use crate::table::table_handle::{TableMut, WireRowView};
use crate::test_utils::{
    id_col_type, id_schema, idonly_schema, int_schema, row_id_from, zerohash_row_id,
};

/// A [`Table`] paired with its own dictionary, packing mutations at the
/// same boundary as [`Store`](crate::store::Store).
struct TestTable {
    table: Table,
    dict: IdPacker,
    rowing: Rowing,
}

impl TestTable {
    fn as_mut(&mut self) -> TableMut<'_> {
        TableMut::new(&mut self.table, &mut self.dict, &mut self.rowing)
    }

    fn handle(&self) -> TableHandle<'_> {
        TableHandle::new(&self.table, &self.dict, &self.rowing)
    }
}

#[fixture]
fn test_table(
    #[default("foo")] path: impl Into<Path>,
    #[default(idonly_schema(id_col_type(Path::from("T"))))] schema: ir::Schema,
) -> TestTable {
    TestTable {
        table: Table::new(path.into(), 0, schema),
        dict: IdPacker::new(),
        rowing: Rowing::new(),
    }
}

/// Rows recorded in the packed rebuild index, converted for assertions.
fn referring_rows(test_table: &TestTable, child: WireRowId) -> Vec<WireRowId> {
    let Some(child) = test_table.dict.lookup_row_id(&child) else {
        return Vec::new();
    };
    let mut rows = test_table
        .table
        .rebuild_index
        .get(&child)
        .into_iter()
        .flatten()
        .map(|row_id| test_table.dict.unpack_row_id(*row_id))
        .collect::<Vec<_>>();
    rows.sort_unstable();
    rows
}

/// Tables with no data columns still allocate row ids on insert; `row_count` must reflect
/// those rows (it cannot use column length when `cols` is empty).
#[rstest]
fn row_count_matches_inserts_when_schema_has_no_columns(
    #[with("id_only")] mut test_table: TestTable,
) {
    assert!(test_table.table.cols.is_empty());
    assert_eq!(test_table.handle().row_count(), 0);

    let r0 = zerohash_row_id(0);
    test_table
        .as_mut()
        .insert_row(vec![], r0)
        .expect("id-only row is valid");
    assert_eq!(test_table.handle().row_count(), 1);
    assert_eq!(
        test_table
            .handle()
            .row_by_id(r0)
            .expect("rowid present")
            .row_id,
        r0
    );

    let r1 = zerohash_row_id(1);
    test_table
        .as_mut()
        .insert_row(vec![], r1)
        .expect("id-only row is valid");
    assert_eq!(test_table.handle().row_count(), 2);
    assert_eq!(
        test_table
            .handle()
            .row_by_id(r1)
            .expect("rowid present")
            .row_id,
        r1
    );
}

#[rstest]
fn rollback_removes_applied_rows_and_index_entries(
    #[with("rollback", int_schema(vec!["value"], Some(vec![0])))] mut test_table: TestTable,
) {
    let existing = zerohash_row_id(0);
    let first_added = zerohash_row_id(1);
    let second_added = zerohash_row_id(2);
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Int(1)], existing)
        .expect("existing row is valid");

    let snapshot = test_table.table.snapshot();
    test_table.as_mut().stage(Op::Add {
        row_id: first_added,
        table: 0,
        values: vec![WireValue::Int(2)],
    });
    test_table.as_mut().stage(Op::Add {
        row_id: second_added,
        table: 0,
        values: vec![WireValue::Int(3)],
    });
    test_table
        .as_mut()
        .apply_staged()
        .expect("the added rows have distinct keys");

    assert_eq!(test_table.handle().row_count(), 3);
    assert_eq!(
        test_table
            .table
            .validate_insert(&[WireValue::Int(2)], &test_table.dict),
        Err(ValidationError::DuplicatePrimaryKey)
    );

    test_table.table.rollback_to(snapshot);

    let handle = test_table.handle();
    assert_eq!(handle.row_count(), 1);
    assert_eq!(
        handle
            .row_by_id(existing)
            .expect("existing row present")
            .row_id,
        existing
    );
    assert_eq!(handle.row_by_id(first_added), None);
    assert_eq!(handle.row_by_id(second_added), None);
    assert!(
        test_table
            .table
            .validate_insert(&[WireValue::Int(2)], &test_table.dict)
            .is_ok()
    );
    assert!(
        test_table
            .table
            .validate_insert(&[WireValue::Int(3)], &test_table.dict)
            .is_ok()
    );
    assert!(test_table.table.undo_log.is_none());
}

/// A staged delete drops the row and its index entries, and undoing it
/// restores the cells the delete returned. This is the pair canonicalisation
/// stages to move a row, so both directions have to keep indexes in step.
#[rstest]
fn staged_delete_removes_row_and_undo_restores_it(
    #[with("deleting", int_schema(vec!["value"], Some(vec![0])))] mut test_table: TestTable,
) {
    let kept = zerohash_row_id(0);
    let removed = zerohash_row_id(1);
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Int(1)], kept)
        .expect("kept row is valid");
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Int(2)], removed)
        .expect("removed row is valid");

    let snapshot = test_table.table.snapshot();
    test_table.as_mut().stage_delete(removed);
    test_table
        .as_mut()
        .apply_staged()
        .expect("a delete cannot duplicate a key");

    assert_eq!(test_table.handle().row_count(), 1);
    assert_eq!(test_table.handle().row_by_id(removed), None);
    // The primary key index gave up the key, so it is free to reuse.
    assert!(
        test_table
            .table
            .validate_insert(&[WireValue::Int(2)], &test_table.dict)
            .is_ok()
    );

    test_table.table.rollback_to(snapshot);

    let handle = test_table.handle();
    assert_eq!(handle.row_count(), 2);
    assert!(handle.row_by_id(kept).is_some());
    assert_eq!(
        handle.row_by_id(removed),
        Some(WireRowView {
            row_id: removed,
            values: vec![WireValue::Int(2)],
        })
    );
    assert_eq!(
        test_table
            .table
            .validate_insert(&[WireValue::Int(2)], &test_table.dict),
        Err(ValidationError::DuplicatePrimaryKey)
    );
}

/// The rebuild index maps each referenced id to the rows holding it, so a
/// canonicalisation pass can find the rows it has to rewrite without
/// scanning the table. Inserts and deletes are the only ways in and out of
/// storage, so both have to keep it in step.
#[rstest]
#[ignore = "rebuild index disabled until we need it"]
fn rebuild_index_tracks_rows_referring_to_an_id(
    #[with("edge", id_schema(vec!["left", "right"], None, id_col_type(Path::from("T"))))]
    mut test_table: TestTable,
) {
    let a = row_id_from(1, 0);
    let b = row_id_from(1, 1);
    let pair = zerohash_row_id(0);
    let doubled = zerohash_row_id(1);

    test_table
        .as_mut()
        .insert_row(vec![WireValue::Id(a), WireValue::Id(b)], pair)
        .expect("pair row is valid");
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Id(a), WireValue::Id(a)], doubled)
        .expect("doubled row is valid");

    // `doubled` refers to `a` twice but is recorded against it once, so a
    // rebuild pass restages it once rather than deleting it twice.
    assert_eq!(referring_rows(&test_table, a), vec![pair, doubled]);
    assert_eq!(referring_rows(&test_table, b), vec![pair]);

    test_table.as_mut().stage_delete(pair);
    test_table
        .as_mut()
        .apply_staged()
        .expect("a delete cannot duplicate a key");

    assert_eq!(referring_rows(&test_table, a), vec![doubled]);
    assert!(referring_rows(&test_table, b).is_empty());

    test_table.as_mut().stage_delete(doubled);
    test_table
        .as_mut()
        .apply_staged()
        .expect("a delete cannot duplicate a key");

    assert!(
        test_table.table.rebuild_index.is_empty(),
        "index entries outlived the rows that held them"
    );
}

#[rstest]
fn full_rebuild_rewrites_stale_id_cells(
    #[with("edge", id_schema(vec!["child"], None, id_col_type(Path::from("T"))))]
    mut test_table: TestTable,
) {
    let canonical_child = row_id_from(1, 0);
    let stale_child = row_id_from(2, 0);
    let owner = zerohash_row_id(0);
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Id(stale_child)], owner)
        .expect("owner row is valid");

    let stale_child = test_table.dict.lookup_row_id(&stale_child).unwrap();
    let canonical_child = test_table.dict.pack_row_id(canonical_child);
    test_table
        .rowing
        .stage_union(0, stale_child, canonical_child);
    test_table.rowing.apply_unions(&test_table.dict);

    test_table.as_mut().rebuild();
    test_table.as_mut().apply_staged().unwrap();

    let handle = test_table.handle();
    assert_eq!(
        handle.row_by_id(owner),
        Some(WireRowView {
            row_id: owner,
            values: vec![WireValue::Id(row_id_from(1, 0))],
        })
    );
}

#[rstest]
fn full_rebuild_collapses_a_displaced_row_onto_its_canonical_row(
    #[with("term", int_schema(vec!["value"], None))] mut test_table: TestTable,
) {
    test_table.table.set_structural_index_for_test(true);
    let canonical = row_id_from(1, 0);
    let displaced = row_id_from(2, 0);
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Int(7)], displaced)
        .expect("displaced row is valid");
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Int(7)], canonical)
        .expect("canonical row is valid");
    test_table.rowing.apply_unions(&test_table.dict);

    test_table.as_mut().rebuild();
    test_table.as_mut().apply_staged().unwrap();

    let handle = test_table.handle();
    assert_eq!(
        handle.row_by_id(canonical),
        Some(WireRowView {
            row_id: canonical,
            values: vec![WireValue::Int(7)],
        })
    );
}

/// Rollback replays the undo log through the same insert path, so the
/// rebuild index has to come back with the rows it restores.
#[rstest]
#[ignore = "rebuild index disabled until we need it"]
fn rollback_restores_rebuild_index_entries(
    #[with("edge", id_schema(vec!["left", "right"], None, id_col_type(Path::from("T"))))]
    mut test_table: TestTable,
) {
    let a = row_id_from(1, 0);
    let b = row_id_from(1, 1);
    let row = zerohash_row_id(0);
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Id(a), WireValue::Id(b)], row)
        .expect("row is valid");

    let snapshot = test_table.table.snapshot();
    test_table.as_mut().stage_delete(row);
    test_table
        .as_mut()
        .apply_staged()
        .expect("a delete cannot duplicate a key");
    assert!(test_table.table.rebuild_index.is_empty());

    test_table.table.rollback_to(snapshot);

    assert_eq!(referring_rows(&test_table, a), vec![row]);
    assert_eq!(referring_rows(&test_table, b), vec![row]);
}

#[rstest]
fn commit_snapshot_keeps_rows_and_discards_undo_log(
    #[with("commit_snapshot", int_schema(vec!["value"], None))] mut test_table: TestTable,
) {
    let row_id = zerohash_row_id(0);

    let snapshot = test_table.table.snapshot();
    test_table.as_mut().stage(Op::Add {
        row_id,
        table: 0,
        values: vec![WireValue::Int(7)],
    });
    test_table
        .as_mut()
        .apply_staged()
        .expect("a table without a primary key accepts the row");
    test_table.table.commit(snapshot);

    assert_eq!(test_table.handle().row_count(), 1);
    assert_eq!(
        test_table.handle().row_by_id(row_id),
        Some(WireRowView {
            row_id,
            values: vec![WireValue::Int(7)],
        })
    );
    assert!(test_table.table.undo_log.is_none());
}

#[rstest]
fn rollback_discards_updates_staged_after_snapshot(
    #[with("staged_rollback", int_schema(vec!["value"], None))] mut test_table: TestTable,
) {
    let snapshot = test_table.table.snapshot();
    test_table.as_mut().stage(Op::Add {
        row_id: zerohash_row_id(0),
        table: 0,
        values: vec![WireValue::Int(7)],
    });
    test_table.table.rollback_to(snapshot);

    assert_eq!(test_table.handle().row_count(), 0);
    assert!(test_table.table.pending_updates.is_empty());
}

/// `primary_key: Some([])` marks a singleton table (at most one row).
#[rstest]
fn empty_primary_key_rejects_second_row(
    #[with("singleton", int_schema(vec!["c0"], Some(vec![])))] mut test_table: TestTable,
) {
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Int(0)], zerohash_row_id(0))
        .expect("first singleton row is valid");
    assert_eq!(test_table.handle().row_count(), 1);

    let values1 = vec![WireValue::Int(1)];
    let err = test_table
        .table
        .validate_insert(&values1, &test_table.dict)
        .unwrap_err();
    assert_eq!(err, ValidationError::DuplicatePrimaryKey);
    assert_eq!(test_table.handle().row_count(), 1);
}

#[rstest]
fn row_read_helpers_return_row_id_and_cells(
    #[with("readable", ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: vec![
            ir::ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            },
            ir::ColumnEntry {
                path: Path::from("c1"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinStr,
                },
            },
        ],
        primary_key: None,
    })]
    mut test_table: TestTable,
) {
    let row_id = zerohash_row_id(0);
    test_table
        .as_mut()
        .insert_row(
            vec![WireValue::Int(7), WireValue::Str("x".to_string())],
            row_id,
        )
        .expect("row is valid");

    let handle = test_table.handle();
    assert_eq!(
        handle.row_by_id(row_id),
        Some(WireRowView {
            row_id,
            values: vec![WireValue::Int(7), WireValue::Str("x".to_string())],
        })
    );
    let packed = test_table
        .dict
        .lookup_row_id(&row_id)
        .expect("insert packed the row id");
    assert_eq!(
        test_table.table.row_by_id(packed),
        Some(vec![PackedValue::Int(7), PackedValue::Str("x".to_string())])
    );
    assert!(test_table.table.row_by_idx(1).is_none());
    assert!(test_table.table.cell_by_idx(0, 2).is_none());
}

#[rstest]
fn row_by_id_finds_inserted_row(
    #[with("T", int_schema(vec!["c0"], None))] mut test_table: TestTable,
) {
    let row_id = zerohash_row_id(0);
    test_table
        .as_mut()
        .insert_row(vec![WireValue::Int(42)], row_id)
        .expect("row is valid");

    let handle = test_table.handle();
    assert_eq!(
        handle.row_by_id(row_id),
        Some(WireRowView {
            row_id,
            values: vec![WireValue::Int(42)],
        })
    );
    assert_eq!(handle.row_by_id(zerohash_row_id(1)), None);
}

/// Row ids and id cells survive the pack/unpack round trip across rows
/// from different commits.
#[rstest]
fn packed_row_ids_round_trip_across_commits(
    #[with("edges", id_schema(vec!["src", "dst"], None, id_col_type(Path::from("T"))))]
    mut test_table: TestTable,
) {
    let rows = [
        (row_id_from(1, 0), row_id_from(3, 7), row_id_from(4, 8)),
        (row_id_from(2, 1), row_id_from(3, 9), row_id_from(1, 0)),
        (row_id_from(1, 2), row_id_from(2, 1), row_id_from(3, 7)),
    ];
    for (rid, src, dst) in rows {
        test_table
            .as_mut()
            .insert_row(vec![WireValue::Id(src), WireValue::Id(dst)], rid)
            .expect("row is valid");
    }

    let handle = test_table.handle();
    for (rid, src, dst) in rows {
        assert_eq!(
            handle.row_by_id(rid),
            Some(WireRowView {
                row_id: rid,
                values: vec![WireValue::Id(src), WireValue::Id(dst)],
            })
        );
    }

    // Four distinct commit hashes, each interned exactly once.
    assert_eq!(test_table.dict.len(), 4);
}

/// Rows are stored sorted by packed row id regardless of insertion order,
/// and `row_position` reports presence and absence accordingly.
#[rstest]
fn rows_stay_sorted_by_row_id(
    #[with("sorted", int_schema(vec!["c0"], None))] mut test_table: TestTable,
) {
    // Commit A is interned first, so its rows sort before commit B's, and
    // counters order rows within a commit.
    let rows = [
        (row_id_from(1, 5), 0),
        (row_id_from(2, 0), 1),
        (row_id_from(1, 0), 2),
        (row_id_from(2, 7), 3),
        (row_id_from(1, 2), 4),
    ];
    for (rid, v) in rows {
        test_table
            .as_mut()
            .insert_row(vec![WireValue::Int(v)], rid)
            .expect("row is valid");
    }

    let handle = test_table.handle();
    let stored: Vec<WireRowId> = handle.scan().map(|row| row.row_id).collect();
    assert_eq!(
        stored,
        vec![
            row_id_from(1, 0),
            row_id_from(1, 2),
            row_id_from(1, 5),
            row_id_from(2, 0),
            row_id_from(2, 7),
        ]
    );

    // Cells moved together with their row ids.
    for (rid, v) in rows {
        assert_eq!(
            handle.row_by_id(rid),
            Some(WireRowView {
                row_id: rid,
                values: vec![WireValue::Int(v)],
            })
        );
    }

    // Absent ids: known commit with unused counter, and unknown commit.
    assert_eq!(handle.row_by_id(row_id_from(1, 3)), None);
    assert_eq!(handle.row_by_id(row_id_from(9, 0)), None);
}

/// Primary key comparison works on dictionary-encoded id columns, and an
/// id with an unseen commit hash never collides.
#[rstest]
fn primary_key_detects_duplicates_in_id_columns(
    #[with(
        "edges",
        id_schema(vec!["src", "dst"], Some(vec![0]), id_col_type(Path::from("T")))
    )]
    mut test_table: TestTable,
) {
    let src = row_id_from(3, 7);
    test_table
        .as_mut()
        .insert_row(
            vec![WireValue::Id(src), WireValue::Id(row_id_from(4, 8))],
            row_id_from(1, 0),
        )
        .expect("row is valid");

    let duplicate = vec![WireValue::Id(src), WireValue::Id(row_id_from(4, 9))];
    assert_eq!(
        test_table
            .table
            .validate_insert(&duplicate, &test_table.dict),
        Err(ValidationError::DuplicatePrimaryKey)
    );

    let unseen_commit = vec![
        WireValue::Id(row_id_from(9, 7)),
        WireValue::Id(row_id_from(4, 8)),
    ];
    assert!(
        test_table
            .table
            .validate_insert(&unseen_commit, &test_table.dict)
            .is_ok()
    );
}

/// Multi-column primary keys reject a duplicate pair but accept rows
/// sharing only one key column, regardless of insert order.
#[rstest]
fn multi_column_primary_key_checks_all_columns(
    #[with("pairs", int_schema(vec!["c0", "c1", "c2"], Some(vec![0, 1])))]
    mut test_table: TestTable,
) {
    let rows = [(3, 1), (1, 2), (1, 1), (2, 1), (2, 2)];
    for (i, (a, b)) in rows.into_iter().enumerate() {
        let values = vec![WireValue::Int(a), WireValue::Int(b), WireValue::Int(0)];
        test_table
            .table
            .validate_insert(&values, &test_table.dict)
            .expect("unique pair");
        test_table
            .as_mut()
            .insert_row(values, zerohash_row_id(i as u32))
            .expect("row is valid");
    }

    for (a, b) in rows {
        let dup = vec![WireValue::Int(a), WireValue::Int(b), WireValue::Int(9)];
        assert_eq!(
            test_table.table.validate_insert(&dup, &test_table.dict),
            Err(ValidationError::DuplicatePrimaryKey)
        );
    }
    let fresh = vec![WireValue::Int(3), WireValue::Int(2), WireValue::Int(0)];
    assert!(
        test_table
            .table
            .validate_insert(&fresh, &test_table.dict)
            .is_ok()
    );
}

/// String primary keys go through the sorted index as well.
#[rstest]
fn string_primary_key_detects_duplicates(
    #[with("named", ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: vec![ir::ColumnEntry {
            path: Path::from("name"),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinStr,
            },
        }],
        primary_key: Some(vec![0]),
    })]
    mut test_table: TestTable,
) {
    for (i, name) in ["b", "a", "c"].into_iter().enumerate() {
        let values = vec![WireValue::Str(name.to_string())];
        test_table
            .table
            .validate_insert(&values, &test_table.dict)
            .expect("unique name");
        test_table
            .as_mut()
            .insert_row(values, zerohash_row_id(i as u32))
            .expect("row is valid");
    }

    assert_eq!(
        test_table
            .table
            .validate_insert(&[WireValue::Str("a".to_string())], &test_table.dict),
        Err(ValidationError::DuplicatePrimaryKey)
    );
    assert!(
        test_table
            .table
            .validate_insert(&[WireValue::Str("d".to_string())], &test_table.dict)
            .is_ok()
    );
}

/// Manual benchmark for the primary key duplicate check on insert.
/// Inserting `n` rows of one integer (the primary key) and one row id.
/// Run with:
/// cargo test -p coln-store --release pk_insert_benchmark -- --ignored --nocapture
#[rstest]
#[ignore = "manual benchmark"]
fn pk_insert_benchmark(
    #[with("bench", ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: vec![
            ir::ColumnEntry {
                path: Path::from("rid"),
                col_type: ColType::RowId {
                    path: Path::from("T"),
                },
            },
            ir::ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            },
        ],
        primary_key: Some(vec![0]),
    })]
    mut test_table: TestTable,
) {
    let n = 50_000;
    let start = std::time::Instant::now();
    for i in 0..n {
        let row_id = zerohash_row_id(i as u32);
        let values = vec![WireValue::Id(row_id), WireValue::Int(i)];
        test_table
            .table
            .validate_insert(&values, &test_table.dict)
            .expect("keys are unique");
        test_table
            .as_mut()
            .insert_row(values, row_id)
            .expect("row is valid");
    }
    println!("inserted {n} rows with pk check in {:?}", start.elapsed());
}

/// Creates a table with index, and requests a index lookup. Also do a
/// non-index lookup on a different column but looks for the same row(s)
/// Indexed and non-index lookup should return the same results (positive and
/// negative)
#[rstest]
fn table_index_non_index_give_same_results(
    #[with("lookup", int_schema(vec!["indexed", "plain"], Some(vec![0])))]
    mut test_table: TestTable,
) {
    for value in [7, 8] {
        let row_id = zerohash_row_id(test_table.handle().row_count() as u32);
        test_table
            .as_mut()
            .insert_row(vec![WireValue::Int(value), WireValue::Int(value)], row_id)
            .expect("row is valid");
    }
    let index = test_table
        .handle()
        .unique_columns()
        .expect("primary-key index");
    assert_eq!(index, 1);

    for value in [7, 9] {
        let indexed = test_table
            .handle()
            .index_seek(&[WireValue::Int(value)])
            .expect("valid index lookup")
            .collect::<Vec<_>>();
        let scanned = test_table
            .handle()
            .scan()
            .filter_map(|r| (r.values[1] == WireValue::Int(value)).then_some(r.row_id))
            .collect::<Vec<_>>();
        assert_eq!(indexed, scanned);
    }
}

#[rstest]
fn debug_dumps_rows(
    #[with("debug.table", ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: vec![
            ir::ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            },
            ir::ColumnEntry {
                path: Path::from("c1"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinStr,
                },
            },
        ],
        primary_key: None,
    })]
    mut test_table: TestTable,
) {
    test_table
        .as_mut()
        .insert_row(
            vec![WireValue::Int(7), WireValue::Str("x".to_string())],
            zerohash_row_id(0),
        )
        .expect("first row is valid");
    test_table
        .as_mut()
        .insert_row(
            vec![WireValue::Int(8), WireValue::Str("y".to_string())],
            zerohash_row_id(1),
        )
        .expect("second row is valid");

    assert_eq!(
        test_table.handle().dump(),
        format!(
            concat!(
                "table debug.table (rows: 2, cols: 2)\n",
                "[0] row_id={} | c0=7 | c1=\"x\"\n",
                "[1] row_id={} | c0=8 | c1=\"y\"\n",
            ),
            zerohash_row_id(0),
            zerohash_row_id(1),
        )
    );
}
