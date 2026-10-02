// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::engine::packed::{PackedRowId, StoreScalarValue, StoreTuple};
use coln_flir_rs::public::PublicScalarValue;
use rstest::{fixture, rstest};

use super::*;
use crate::ir::{self, Path};
use crate::ir::{BuiltinTy, ColType};
use crate::op::Op;
use crate::pack::id_packer::IdLookup;
use crate::table::handle::TableMut;
use crate::test_utils::{
    id_col_type, id_schema, idonly_schema, int_schema, memoized_int_schema, row_id_from,
    zerohash_row_id,
};

/// A [`Table`] with the dictionary and canonicaliser [`TableHandle`] and
/// [`TableMut`] require. These tests pass [`PackedRowId`]s straight through.
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

/// Rows recorded in the rebuild index against `child`, in packed-id order.
fn referring_rows(test_table: &TestTable, child: PackedRowId) -> Vec<PackedRowId> {
    let mut rows = test_table
        .table
        .rebuild_index
        .get(&child)
        .into_iter()
        .flatten()
        .copied()
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

    let r0 = PackedRowId {
        commit_idx: 0,
        counter: 0,
    };
    test_table
        .as_mut()
        .insert_row(StoreTuple::from(vec![]), r0)
        .expect("id-only row is valid");
    assert_eq!(test_table.handle().row_count(), 1);
    assert_eq!(
        test_table.handle().row_by_id(&r0),
        Some(StoreTuple::from(vec![StoreScalarValue::RowId(r0)]))
    );

    let r1 = PackedRowId {
        commit_idx: 0,
        counter: 1,
    };
    test_table
        .as_mut()
        .insert_row(StoreTuple::from(vec![]), r1)
        .expect("id-only row is valid");
    assert_eq!(test_table.handle().row_count(), 2);
    assert_eq!(
        test_table.handle().row_by_id(&r1),
        Some(StoreTuple::from(vec![StoreScalarValue::RowId(r1)]))
    );
}

#[rstest]
fn rollback_removes_applied_rows_and_index_entries(
    #[with("rollback", int_schema(vec!["value"], Some(vec![0])))] mut test_table: TestTable,
) {
    let existing = PackedRowId {
        commit_idx: 0,
        counter: 0,
    };
    let first_public = zerohash_row_id(1);
    let second_public = zerohash_row_id(2);
    test_table
        .as_mut()
        .insert_row(StoreTuple::from(vec![StoreScalarValue::I32(1)]), existing)
        .expect("existing row is valid");

    let snapshot = test_table.table.snapshot();
    test_table.as_mut().stage(Op::Add {
        row_id: first_public.clone(),
        table: 0,
        values: vec![PublicScalarValue::I32(2)],
    });
    test_table.as_mut().stage(Op::Add {
        row_id: second_public.clone(),
        table: 0,
        values: vec![PublicScalarValue::I32(3)],
    });
    test_table
        .as_mut()
        .apply_staged()
        .expect("the added rows have distinct keys");
    // Staging the zerohash commit interns it at index 0, which `existing` uses.
    let first_added = test_table
        .dict
        .packed(&first_public)
        .expect("stage interned the row id");
    let second_added = test_table
        .dict
        .packed(&second_public)
        .expect("stage interned the row id");

    assert_eq!(test_table.handle().row_count(), 3);
    assert_eq!(
        test_table
            .table
            .validate_insert(&[PublicScalarValue::I32(2)], &test_table.dict),
        Err(ValidationError::DuplicatePrimaryKey)
    );

    test_table.table.rollback_to(snapshot);

    assert_eq!(test_table.handle().row_count(), 1);
    assert_eq!(
        test_table.handle().row_by_id(&existing),
        Some(StoreTuple::from(vec![
            StoreScalarValue::RowId(existing),
            StoreScalarValue::I32(1)
        ]))
    );
    assert_eq!(test_table.handle().row_by_id(&first_added), None);
    assert_eq!(test_table.handle().row_by_id(&second_added), None);
    assert!(
        test_table
            .table
            .validate_insert(&[PublicScalarValue::I32(2)], &test_table.dict)
            .is_ok()
    );
    assert!(
        test_table
            .table
            .validate_insert(&[PublicScalarValue::I32(3)], &test_table.dict)
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
    let kept = PackedRowId {
        commit_idx: 0,
        counter: 0,
    };
    let removed = PackedRowId {
        commit_idx: 0,
        counter: 1,
    };
    test_table
        .as_mut()
        .insert_row(StoreTuple::from(vec![StoreScalarValue::I32(1)]), kept)
        .expect("kept row is valid");
    test_table
        .as_mut()
        .insert_row(StoreTuple::from(vec![StoreScalarValue::I32(2)]), removed)
        .expect("removed row is valid");

    let snapshot = test_table.table.snapshot();
    test_table.as_mut().stage_delete(removed);
    test_table
        .as_mut()
        .apply_staged()
        .expect("a delete cannot duplicate a key");

    assert_eq!(test_table.handle().row_count(), 1);
    assert_eq!(test_table.handle().row_by_id(&removed), None);
    // The primary key index gave up the key, so it is free to reuse.
    assert!(
        test_table
            .table
            .validate_insert(&[PublicScalarValue::I32(2)], &test_table.dict)
            .is_ok()
    );

    test_table.table.rollback_to(snapshot);

    assert_eq!(test_table.handle().row_count(), 2);
    assert!(test_table.handle().row_by_id(&kept).is_some());
    assert_eq!(
        test_table.handle().row_by_id(&removed),
        Some(StoreTuple::from(vec![
            StoreScalarValue::RowId(removed),
            StoreScalarValue::I32(2)
        ]))
    );
    assert_eq!(
        test_table
            .table
            .validate_insert(&[PublicScalarValue::I32(2)], &test_table.dict),
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
    let a = PackedRowId {
        commit_idx: 1,
        counter: 0,
    };
    let b = PackedRowId {
        commit_idx: 1,
        counter: 1,
    };
    let pair = PackedRowId {
        commit_idx: 0,
        counter: 0,
    };
    let doubled = PackedRowId {
        commit_idx: 0,
        counter: 1,
    };

    test_table
        .as_mut()
        .insert_row(
            StoreTuple::from(vec![StoreScalarValue::RowId(a), StoreScalarValue::RowId(b)]),
            pair,
        )
        .expect("pair row is valid");
    test_table
        .as_mut()
        .insert_row(
            StoreTuple::from(vec![StoreScalarValue::RowId(a), StoreScalarValue::RowId(a)]),
            doubled,
        )
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
    // Rowing picks the canonical id by comparing unpacked commit hashes, so
    // these ids have to be interned. Hash byte 1 sorts before byte 2.
    let owner = test_table.dict.pack_row_id(zerohash_row_id(0));
    let stale_child = test_table.dict.pack_row_id(row_id_from(2, 0));
    let canonical_child = test_table.dict.pack_row_id(row_id_from(1, 0));
    test_table
        .as_mut()
        .insert_row(
            StoreTuple::from(vec![StoreScalarValue::RowId(stale_child)]),
            owner,
        )
        .expect("owner row is valid");

    test_table
        .rowing
        .stage_union(0, stale_child, canonical_child);
    test_table.rowing.apply_unions(&test_table.dict);

    test_table.as_mut().rebuild();
    test_table.as_mut().apply_staged().unwrap();

    assert_eq!(
        test_table.handle().row_by_id(&owner),
        Some(StoreTuple::from(vec![
            StoreScalarValue::RowId(owner),
            StoreScalarValue::RowId(canonical_child)
        ]))
    );
}

#[rstest]
fn full_rebuild_collapses_a_displaced_row_onto_its_canonical_row(
    #[with("term", memoized_int_schema(vec!["value"], None))] mut test_table: TestTable,
) {
    // Rowing picks the canonical id by comparing unpacked commit hashes, so
    // these ids have to be interned. Hash byte 1 sorts before byte 2.
    let canonical = test_table.dict.pack_row_id(row_id_from(1, 0));
    let displaced = test_table.dict.pack_row_id(row_id_from(2, 0));
    test_table
        .as_mut()
        .insert_row(StoreTuple::from(vec![StoreScalarValue::I32(7)]), displaced)
        .expect("displaced row is valid");
    test_table
        .as_mut()
        .insert_row(StoreTuple::from(vec![StoreScalarValue::I32(7)]), canonical)
        .expect("canonical row is valid");
    test_table.rowing.apply_unions(&test_table.dict);

    test_table.as_mut().rebuild();
    test_table.as_mut().apply_staged().unwrap();

    assert_eq!(
        test_table.handle().row_by_id(&canonical),
        Some(StoreTuple::from(vec![
            StoreScalarValue::RowId(canonical),
            StoreScalarValue::I32(7)
        ]))
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
    let a = PackedRowId {
        commit_idx: 1,
        counter: 0,
    };
    let b = PackedRowId {
        commit_idx: 1,
        counter: 1,
    };
    let row = PackedRowId {
        commit_idx: 0,
        counter: 0,
    };
    test_table
        .as_mut()
        .insert_row(
            StoreTuple::from(vec![StoreScalarValue::RowId(a), StoreScalarValue::RowId(b)]),
            row,
        )
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
    let public_id = zerohash_row_id(0);

    let snapshot = test_table.table.snapshot();
    test_table.as_mut().stage(Op::Add {
        row_id: public_id.clone(),
        table: 0,
        values: vec![PublicScalarValue::I32(7)],
    });
    test_table
        .as_mut()
        .apply_staged()
        .expect("a table without a primary key accepts the row");
    test_table.table.commit(snapshot);
    let row_id = test_table
        .dict
        .packed(&public_id)
        .expect("stage interned the row id");

    assert_eq!(test_table.handle().row_count(), 1);
    assert_eq!(
        test_table.handle().row_by_id(&row_id),
        Some(StoreTuple::from(vec![
            StoreScalarValue::RowId(row_id),
            StoreScalarValue::I32(7)
        ]))
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
        values: vec![PublicScalarValue::I32(7)],
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
        .insert_row(
            StoreTuple::from(vec![StoreScalarValue::I32(0)]),
            PackedRowId {
                commit_idx: 0,
                counter: 0,
            },
        )
        .expect("first singleton row is valid");
    assert_eq!(test_table.handle().row_count(), 1);

    let values1 = vec![PublicScalarValue::I32(1)];
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
    let row_id = PackedRowId {
        commit_idx: 0,
        counter: 0,
    };
    test_table
        .as_mut()
        .insert_row(
            StoreTuple::from(vec![
                StoreScalarValue::I32(7),
                StoreScalarValue::String("x".to_string()),
            ]),
            row_id,
        )
        .expect("row is valid");

    // The handle leads with the row id; the table's own lookup returns cells only.
    assert_eq!(
        test_table.handle().row_by_id(&row_id),
        Some(StoreTuple::from(vec![
            StoreScalarValue::RowId(row_id),
            StoreScalarValue::I32(7),
            StoreScalarValue::String("x".to_string()),
        ]))
    );
    assert_eq!(
        test_table.table.row_by_id(row_id),
        Some(StoreTuple::from(vec![
            StoreScalarValue::I32(7),
            StoreScalarValue::String("x".to_string())
        ]))
    );
    assert!(test_table.table.row_by_idx(1).is_none());
    assert!(test_table.table.cell_by_idx(0, 2).is_none());
}

#[rstest]
fn row_by_id_finds_inserted_row(
    #[with("T", int_schema(vec!["c0"], None))] mut test_table: TestTable,
) {
    let row_id = PackedRowId {
        commit_idx: 0,
        counter: 0,
    };
    test_table
        .as_mut()
        .insert_row(StoreTuple::from(vec![StoreScalarValue::I32(42)]), row_id)
        .expect("row is valid");

    assert_eq!(
        test_table.handle().row_by_id(&row_id),
        Some(StoreTuple::from(vec![
            StoreScalarValue::RowId(row_id),
            StoreScalarValue::I32(42)
        ]))
    );
    assert_eq!(
        test_table.handle().row_by_id(&PackedRowId {
            commit_idx: 0,
            counter: 1
        }),
        None
    );
}

/// Rows are stored sorted by packed row id regardless of insertion order.
#[rstest]
fn rows_stay_sorted_by_row_id(
    #[with("sorted", int_schema(vec!["c0"], None))] mut test_table: TestTable,
) {
    // `(commit_idx, counter)` is the storage order.
    let rows = [
        (
            PackedRowId {
                commit_idx: 0,
                counter: 5,
            },
            0,
        ),
        (
            PackedRowId {
                commit_idx: 1,
                counter: 0,
            },
            1,
        ),
        (
            PackedRowId {
                commit_idx: 0,
                counter: 0,
            },
            2,
        ),
        (
            PackedRowId {
                commit_idx: 1,
                counter: 7,
            },
            3,
        ),
        (
            PackedRowId {
                commit_idx: 0,
                counter: 2,
            },
            4,
        ),
    ];
    for &(rid, v) in &rows {
        test_table
            .as_mut()
            .insert_row(StoreTuple::from(vec![StoreScalarValue::I32(v)]), rid)
            .expect("row is valid");
    }

    let stored_row_ids = test_table
        .handle()
        .scan()
        .map(|row| row.row_id())
        .collect::<Vec<_>>();
    assert_eq!(
        stored_row_ids,
        vec![
            PackedRowId {
                commit_idx: 0,
                counter: 0
            },
            PackedRowId {
                commit_idx: 0,
                counter: 2
            },
            PackedRowId {
                commit_idx: 0,
                counter: 5
            },
            PackedRowId {
                commit_idx: 1,
                counter: 0
            },
            PackedRowId {
                commit_idx: 1,
                counter: 7
            },
        ]
    );

    // Cells moved together with their row ids.
    for (rid, v) in rows {
        assert_eq!(
            test_table.handle().row_by_id(&rid),
            Some(StoreTuple::from(vec![
                StoreScalarValue::RowId(rid),
                StoreScalarValue::I32(v)
            ]))
        );
    }

    assert_eq!(
        test_table.handle().row_by_id(&PackedRowId {
            commit_idx: 0,
            counter: 3
        }),
        None
    );
    assert_eq!(
        test_table.handle().row_by_id(&PackedRowId {
            commit_idx: 2,
            counter: 0
        }),
        None
    );
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
    let src_public = row_id_from(3, 7);
    let src = test_table.dict.pack_row_id(src_public.clone());
    let dst = test_table.dict.pack_row_id(row_id_from(4, 8));
    let row_id = test_table.dict.pack_row_id(row_id_from(1, 0));
    test_table
        .as_mut()
        .insert_row(
            StoreTuple::from(vec![
                StoreScalarValue::RowId(src),
                StoreScalarValue::RowId(dst),
            ]),
            row_id,
        )
        .expect("row is valid");

    let duplicate = vec![
        PublicScalarValue::RowId(src_public),
        PublicScalarValue::RowId(row_id_from(4, 9)),
    ];
    assert_eq!(
        test_table
            .table
            .validate_insert(&duplicate, &test_table.dict),
        Err(ValidationError::DuplicatePrimaryKey)
    );

    let unseen_commit = vec![
        PublicScalarValue::RowId(row_id_from(9, 7)),
        PublicScalarValue::RowId(row_id_from(4, 8)),
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
        let values = vec![
            PublicScalarValue::I32(a),
            PublicScalarValue::I32(b),
            PublicScalarValue::I32(0),
        ];
        test_table
            .table
            .validate_insert(&values, &test_table.dict)
            .expect("unique pair");
        test_table
            .as_mut()
            .insert_row(
                StoreTuple::from(vec![
                    StoreScalarValue::I32(a),
                    StoreScalarValue::I32(b),
                    StoreScalarValue::I32(0),
                ]),
                PackedRowId {
                    commit_idx: 0,
                    counter: i as u32,
                },
            )
            .expect("row is valid");
    }

    for (a, b) in rows {
        let dup = vec![
            PublicScalarValue::I32(a),
            PublicScalarValue::I32(b),
            PublicScalarValue::I32(9),
        ];
        assert_eq!(
            test_table.table.validate_insert(&dup, &test_table.dict),
            Err(ValidationError::DuplicatePrimaryKey)
        );
    }
    let fresh = vec![
        PublicScalarValue::I32(3),
        PublicScalarValue::I32(2),
        PublicScalarValue::I32(0),
    ];
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
        let values = vec![PublicScalarValue::String(name.to_string())];
        test_table
            .table
            .validate_insert(&values, &test_table.dict)
            .expect("unique name");
        test_table
            .as_mut()
            .insert_row(
                StoreTuple::from(vec![StoreScalarValue::String(name.to_string())]),
                PackedRowId {
                    commit_idx: 0,
                    counter: i as u32,
                },
            )
            .expect("row is valid");
    }

    assert_eq!(
        test_table.table.validate_insert(
            &[PublicScalarValue::String("a".to_string())],
            &test_table.dict
        ),
        Err(ValidationError::DuplicatePrimaryKey)
    );
    assert!(
        test_table
            .table
            .validate_insert(
                &[PublicScalarValue::String("d".to_string())],
                &test_table.dict
            )
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
        let public_id = zerohash_row_id(i as u32);
        let row_id = test_table.dict.pack_row_id(public_id.clone());
        let values = vec![
            PublicScalarValue::RowId(public_id),
            PublicScalarValue::I32(i),
        ];
        test_table
            .table
            .validate_insert(&values, &test_table.dict)
            .expect("keys are unique");
        test_table
            .as_mut()
            .insert_row(
                StoreTuple::from(vec![
                    StoreScalarValue::RowId(row_id),
                    StoreScalarValue::I32(i),
                ]),
                row_id,
            )
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
        let row_id = PackedRowId {
            commit_idx: 0,
            counter: test_table.handle().row_count() as u32,
        };
        test_table
            .as_mut()
            .insert_row(
                StoreTuple::from(vec![
                    StoreScalarValue::I32(value),
                    StoreScalarValue::I32(value),
                ]),
                row_id,
            )
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
            .index_seek(&StoreTuple::from(vec![StoreScalarValue::I32(value)]))
            .expect("valid index lookup")
            .collect::<Vec<_>>();
        // Scanned rows lead with the row id, so the `plain` column is at 2.
        let scanned = test_table
            .handle()
            .scan()
            .filter_map(|row| (row[2] == StoreScalarValue::I32(value)).then(|| row.row_id()))
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
    // Dump prints unpacked row ids, so the commits have to be interned.
    let first = zerohash_row_id(0);
    let second = zerohash_row_id(1);
    let first_packed = test_table.dict.pack_row_id(first.clone());
    let second_packed = test_table.dict.pack_row_id(second.clone());
    test_table
        .as_mut()
        .insert_row(
            StoreTuple::from(vec![
                StoreScalarValue::I32(7),
                StoreScalarValue::String("x".to_string()),
            ]),
            first_packed,
        )
        .expect("first row is valid");
    test_table
        .as_mut()
        .insert_row(
            StoreTuple::from(vec![
                StoreScalarValue::I32(8),
                StoreScalarValue::String("y".to_string()),
            ]),
            second_packed,
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
            first, second,
        )
    );
}
