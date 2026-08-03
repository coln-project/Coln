use super::*;
use crate::commit::hash::CommitHash;
use crate::ir::{self, Path};
use crate::ir::{BuiltinTy, ColType};
use crate::op::Op;

fn test_row_id(counter: u32) -> RowId {
    RowId {
        commit: CommitHash([0; 32]),
        counter,
    }
}

fn row_id_from(commit_byte: u8, counter: u32) -> RowId {
    RowId {
        commit: CommitHash([commit_byte; 32]),
        counter,
    }
}

fn id_schema(columns: &[&str]) -> ir::Schema {
    ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: columns
            .iter()
            .map(|name| ir::ColumnEntry {
                path: Path::from(*name),
                col_type: ColType::RowId {
                    path: Path::from("T"),
                },
            })
            .collect(),
        primary_key: None,
    }
}

/// A [`Table`] paired with its own dictionary, packing mutations at the
/// same boundary as [`Store`](crate::store::Store).
struct TestTable {
    table: Table,
    dict: IdPacker,
    rowing: Rowing,
}

impl TestTable {
    fn new(path: Path, schema: ir::Schema) -> Self {
        Self {
            table: Table::new(path, 0, schema),
            dict: IdPacker::new(),
            rowing: Rowing::new(),
        }
    }

    /// A table that unions structurally identical rows on insert.
    fn with_hashcons(path: Path, schema: ir::Schema) -> Self {
        let mut tbl = Self::new(path, schema);
        tbl.table.set_hashcons_for_test(true);
        tbl
    }

    fn row_count(&self) -> usize {
        self.table.row_count()
    }

    fn row_id_at(&self, row_idx: usize) -> Option<RowId> {
        self.table.row_id_at(row_idx, &self.dict)
    }

    fn cell_at(&self, row_idx: usize, col_idx: usize) -> Option<CellValue> {
        self.table.cell_at(row_idx, col_idx, &self.dict)
    }

    fn row_at(&self, row_idx: usize) -> Option<RowView> {
        self.table.row_at(row_idx, &self.dict)
    }

    fn row_position(&self, row_id: RowId) -> Option<usize> {
        let row_id = self.dict.lookup_row_id(row_id)?;
        self.table.row_idx(row_id)
    }

    /// Rows the table records as referring to `child`, in row id order.
    fn referring_rows(&self, child: RowId) -> Vec<RowId> {
        let Some(child) = self.dict.lookup_row_id(child) else {
            return Vec::new();
        };
        let mut rows: Vec<RowId> = self
            .table
            .rebuild_index
            .get(&child)
            .map(|rows| {
                rows.iter()
                    .map(|rid| self.dict.unpack_row_id(*rid))
                    .collect()
            })
            .unwrap_or_default();
        rows.sort_unstable();
        rows
    }

    fn validate_insert(&self, values: &[CellValue]) -> Result<(), ValidationError> {
        self.table.validate_insert(values, &self.dict)
    }

    fn insert_row(&mut self, values: Vec<CellValue>, row_id: RowId) {
        let row_id = self.dict.pack_row_id(row_id);
        let values = values
            .into_iter()
            .map(|value| self.dict.pack_cell(value))
            .collect();
        self.table
            .insert_row(values, row_id, &mut self.rowing)
            .expect("test row satisfies the primary key")
    }

    fn stage_update(&mut self, op: Op) {
        let Op::Add {
            row_id,
            table,
            values,
        } = op;
        debug_assert_eq!(table, self.table.oid());
        let row_id = self.dict.pack_row_id(row_id);
        let values = values
            .into_iter()
            .map(|value| self.dict.pack_cell(value))
            .collect();
        self.table.stage_update(PackedOp::Add { row_id, values });
    }

    fn stage_delete(&mut self, row_id: RowId) {
        let row_id = self.dict.pack_row_id(row_id);
        self.table.stage_update(PackedOp::Delete { row_id });
    }

    fn apply_staged_ops(&mut self) -> Result<(), ValidationError> {
        self.table.apply_staged_ops(&mut self.rowing)
    }

    fn dump(&self) -> String {
        self.table.dump(&self.dict)
    }
}

/// Tables with no data columns still allocate row ids on insert; `row_count` must reflect
/// those rows (it cannot use column length when `cols` is empty).
#[test]
fn row_count_matches_inserts_when_schema_has_no_columns() {
    let schema = ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: vec![],
        primary_key: None,
    };
    let mut tbl = TestTable::new(Path::from("id_only"), schema);
    assert!(tbl.table.cols.is_empty());
    assert_eq!(tbl.row_count(), 0);

    let r0 = test_row_id(0);
    tbl.insert_row(vec![], r0);
    assert_eq!(tbl.row_count(), 1);
    assert_eq!(tbl.row_id_at(0), Some(r0));

    let r1 = test_row_id(1);
    tbl.insert_row(vec![], r1);
    assert_eq!(tbl.row_count(), 2);
    assert_eq!(tbl.row_id_at(1), Some(r1));
}

#[test]
fn rollback_removes_applied_rows_and_index_entries() {
    let path = Path::from("rollback");
    let mut tbl = TestTable::new(path.clone(), int_schema(&["value"], Some(&["value"])));
    let existing = test_row_id(0);
    let first_added = test_row_id(1);
    let second_added = test_row_id(2);
    tbl.insert_row(vec![CellValue::Int(1)], existing);

    let snapshot = tbl.table.snapshot();
    tbl.stage_update(Op::Add {
        row_id: first_added,
        table: 0,
        values: vec![CellValue::Int(2)],
    });
    tbl.stage_update(Op::Add {
        row_id: second_added,
        table: 0,
        values: vec![CellValue::Int(3)],
    });
    tbl.apply_staged_ops()
        .expect("the added rows have distinct keys");

    assert_eq!(tbl.row_count(), 3);
    assert_eq!(
        tbl.validate_insert(&[CellValue::Int(2)]),
        Err(ValidationError::DuplicatePrimaryKey)
    );

    tbl.table.rollback(snapshot);

    assert_eq!(tbl.row_count(), 1);
    assert_eq!(tbl.row_id_at(0), Some(existing));
    assert_eq!(tbl.row_position(first_added), None);
    assert_eq!(tbl.row_position(second_added), None);
    assert!(tbl.validate_insert(&[CellValue::Int(2)]).is_ok());
    assert!(tbl.validate_insert(&[CellValue::Int(3)]).is_ok());
    assert!(tbl.table.undo_log.is_none());
}

/// A staged delete drops the row and its index entries, and undoing it
/// restores the cells the delete returned. This is the pair canonicalisation
/// stages to move a row, so both directions have to keep indexes in step.
#[test]
fn staged_delete_removes_row_and_undo_restores_it() {
    let mut tbl = TestTable::new(
        Path::from("deleting"),
        int_schema(&["value"], Some(&["value"])),
    );
    let kept = test_row_id(0);
    let removed = test_row_id(1);
    tbl.insert_row(vec![CellValue::Int(1)], kept);
    tbl.insert_row(vec![CellValue::Int(2)], removed);

    let snapshot = tbl.table.snapshot();
    tbl.stage_delete(removed);
    tbl.apply_staged_ops()
        .expect("a delete cannot duplicate a key");

    assert_eq!(tbl.row_count(), 1);
    assert_eq!(tbl.row_position(removed), None);
    // The primary key index gave up the key, so it is free to reuse.
    assert!(tbl.validate_insert(&[CellValue::Int(2)]).is_ok());

    tbl.table.rollback(snapshot);

    assert_eq!(tbl.row_count(), 2);
    assert_eq!(tbl.row_position(kept), Some(0));
    let idx = tbl.row_position(removed).expect("row is restored");
    assert_eq!(tbl.cell_at(idx, 0), Some(CellValue::Int(2)));
    assert_eq!(
        tbl.validate_insert(&[CellValue::Int(2)]),
        Err(ValidationError::DuplicatePrimaryKey)
    );
}

/// The rebuild index maps each referenced id to the rows holding it, so a
/// canonicalisation pass can find the rows it has to rewrite without
/// scanning the table. Inserts and deletes are the only ways in and out of
/// storage, so both have to keep it in step.
#[test]
#[ignore = "indexing disabled until we need it"]
fn rebuild_index_tracks_rows_referring_to_an_id() {
    let mut tbl = TestTable::new(Path::from("edge"), id_schema(&["left", "right"]));
    let a = row_id_from(1, 0);
    let b = row_id_from(1, 1);
    let pair = test_row_id(0);
    let doubled = test_row_id(1);

    tbl.insert_row(vec![CellValue::Id(a), CellValue::Id(b)], pair);
    tbl.insert_row(vec![CellValue::Id(a), CellValue::Id(a)], doubled);

    // `doubled` refers to `a` twice but is recorded against it once, so a
    // rebuild pass restages it once rather than deleting it twice.
    assert_eq!(tbl.referring_rows(a), vec![pair, doubled]);
    assert_eq!(tbl.referring_rows(b), vec![pair]);

    tbl.stage_delete(pair);
    tbl.apply_staged_ops()
        .expect("a delete cannot duplicate a key");

    assert_eq!(tbl.referring_rows(a), vec![doubled]);
    assert!(tbl.referring_rows(b).is_empty());

    tbl.stage_delete(doubled);
    tbl.apply_staged_ops()
        .expect("a delete cannot duplicate a key");

    assert!(
        tbl.table.rebuild_index.is_empty(),
        "index entries outlived the rows that held them"
    );
}

#[test]
fn full_rebuild_rewrites_stale_id_cells() {
    let mut tbl = TestTable::new(Path::from("edge"), id_schema(&["child"]));
    let canonical_child = row_id_from(1, 0);
    let stale_child = row_id_from(2, 0);
    let owner = test_row_id(0);
    tbl.insert_row(vec![CellValue::Id(stale_child)], owner);

    let stale_child = tbl.dict.lookup_row_id(stale_child).unwrap();
    let canonical_child = tbl.dict.pack_row_id(canonical_child);
    tbl.rowing.stage_union(0, stale_child, canonical_child);
    tbl.rowing.apply_unions(&tbl.dict);

    tbl.table.rebuild_full(&tbl.rowing, &tbl.dict);
    tbl.apply_staged_ops().unwrap();

    assert_eq!(tbl.row_count(), 1);
    assert_eq!(tbl.row_id_at(0), Some(owner));
    assert_eq!(tbl.cell_at(0, 0), Some(CellValue::Id(row_id_from(1, 0))));
}

#[test]
fn full_rebuild_collapses_a_displaced_row_onto_its_canonical_row() {
    let mut tbl = TestTable::with_hashcons(Path::from("term"), int_schema(&["value"], None));
    let canonical = row_id_from(1, 0);
    let displaced = row_id_from(2, 0);
    tbl.insert_row(vec![CellValue::Int(7)], displaced);
    tbl.insert_row(vec![CellValue::Int(7)], canonical);
    tbl.rowing.apply_unions(&tbl.dict);

    tbl.table.rebuild_full(&tbl.rowing, &tbl.dict);
    tbl.apply_staged_ops().unwrap();

    assert_eq!(tbl.row_count(), 1);
    assert_eq!(tbl.row_id_at(0), Some(canonical));
    assert_eq!(tbl.cell_at(0, 0), Some(CellValue::Int(7)));
}

/// Rollback replays the undo log through the same insert path, so the
/// rebuild index has to come back with the rows it restores.
#[test]
#[ignore = "rebuild index disabled until we need it"]
fn rollback_restores_rebuild_index_entries() {
    let mut tbl = TestTable::new(Path::from("edge"), id_schema(&["left", "right"]));
    let a = row_id_from(1, 0);
    let b = row_id_from(1, 1);
    let row = test_row_id(0);
    tbl.insert_row(vec![CellValue::Id(a), CellValue::Id(b)], row);

    let snapshot = tbl.table.snapshot();
    tbl.stage_delete(row);
    tbl.apply_staged_ops()
        .expect("a delete cannot duplicate a key");
    assert!(tbl.table.rebuild_index.is_empty());

    tbl.table.rollback(snapshot);

    assert_eq!(tbl.referring_rows(a), vec![row]);
    assert_eq!(tbl.referring_rows(b), vec![row]);
}

#[test]
fn commit_snapshot_keeps_rows_and_discards_undo_log() {
    let path = Path::from("commit_snapshot");
    let mut tbl = TestTable::new(path.clone(), int_schema(&["value"], None));
    let row_id = test_row_id(0);

    let snapshot = tbl.table.snapshot();
    tbl.stage_update(Op::Add {
        row_id,
        table: 0,
        values: vec![CellValue::Int(7)],
    });
    tbl.apply_staged_ops()
        .expect("a table without a primary key accepts the row");
    tbl.table.commit_snapshot(snapshot);

    assert_eq!(tbl.row_count(), 1);
    assert_eq!(tbl.row_id_at(0), Some(row_id));
    assert!(tbl.table.undo_log.is_none());
}

#[test]
fn rollback_discards_updates_staged_after_snapshot() {
    let path = Path::from("staged_rollback");
    let mut tbl = TestTable::new(path.clone(), int_schema(&["value"], None));

    let snapshot = tbl.table.snapshot();
    tbl.stage_update(Op::Add {
        row_id: test_row_id(0),
        table: 0,
        values: vec![CellValue::Int(7)],
    });
    tbl.table.rollback(snapshot);

    assert_eq!(tbl.row_count(), 0);
    assert!(tbl.table.pending_updates.is_empty());
}

/// `primary_key: Some([])` marks a singleton table (at most one row).
#[test]
fn empty_primary_key_rejects_second_row() {
    let schema = ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: vec![ir::ColumnEntry {
            path: Path::from("c0"),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            },
        }],
        primary_key: Some(vec![]),
    };
    let mut tbl = TestTable::new(Path::from("singleton"), schema);

    tbl.insert_row(vec![CellValue::Int(0)], test_row_id(0));
    assert_eq!(tbl.row_count(), 1);

    let values1 = vec![CellValue::Int(1)];
    let err = tbl.validate_insert(&values1).unwrap_err();
    assert_eq!(err, ValidationError::DuplicatePrimaryKey);
    assert_eq!(tbl.row_count(), 1);
}

#[test]
fn row_read_helpers_return_row_id_and_cells() {
    let schema = ir::Schema {
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
    };
    let mut tbl = TestTable::new(Path::from("readable"), schema);

    let row_id = test_row_id(0);
    tbl.insert_row(
        vec![CellValue::Int(7), CellValue::Str("x".to_string())],
        row_id,
    );

    assert_eq!(
        tbl.row_at(0),
        Some(RowView {
            row_id,
            values: vec![CellValue::Int(7), CellValue::Str("x".to_string())],
        })
    );
    assert_eq!(tbl.row_id_at(0), Some(row_id));
    assert_eq!(tbl.cell_at(0, 0), Some(CellValue::Int(7)));
    assert_eq!(tbl.cell_at(0, 1), Some(CellValue::Str("x".to_string())));
    let packed = tbl
        .dict
        .lookup_row_id(row_id)
        .expect("insert packed the row id");
    assert_eq!(
        tbl.table.packed_row_at(packed),
        Some(vec![PackedCell::Int(7), PackedCell::Str("x".to_string())])
    );
    assert_eq!(tbl.row_at(1), None);
    assert_eq!(tbl.row_id_at(1), None);
    assert_eq!(tbl.cell_at(0, 2), None);
}

/// Row ids and id cells survive the pack/unpack round trip across rows
/// from different commits.
#[test]
fn packed_row_ids_round_trip_across_commits() {
    let mut tbl = TestTable::new(Path::from("edges"), id_schema(&["src", "dst"]));

    let rows = [
        (row_id_from(1, 0), row_id_from(3, 7), row_id_from(4, 8)),
        (row_id_from(2, 1), row_id_from(3, 9), row_id_from(1, 0)),
        (row_id_from(1, 2), row_id_from(2, 1), row_id_from(3, 7)),
    ];
    for (rid, src, dst) in rows {
        tbl.insert_row(vec![CellValue::Id(src), CellValue::Id(dst)], rid);
    }

    for (rid, src, dst) in rows {
        let idx = tbl.row_position(rid).expect("row is stored");
        assert_eq!(tbl.row_id_at(idx), Some(rid));
        assert_eq!(tbl.cell_at(idx, 0), Some(CellValue::Id(src)));
        assert_eq!(tbl.cell_at(idx, 1), Some(CellValue::Id(dst)));
    }

    // Four distinct commit hashes, each interned exactly once.
    assert_eq!(tbl.dict.len(), 4);
}

/// Rows are stored sorted by packed row id regardless of insertion order,
/// and `row_position` reports presence and absence accordingly.
#[test]
fn rows_stay_sorted_by_row_id() {
    let schema = ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: vec![ir::ColumnEntry {
            path: Path::from("c0"),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            },
        }],
        primary_key: None,
    };
    let mut tbl = TestTable::new(Path::from("sorted"), schema);

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
        tbl.insert_row(vec![CellValue::Int(v)], rid);
    }

    let stored: Vec<RowId> = (0..tbl.row_count())
        .map(|idx| tbl.row_id_at(idx).expect("row id"))
        .collect();
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
        let idx = tbl.row_position(rid).expect("row is stored");
        assert_eq!(tbl.cell_at(idx, 0), Some(CellValue::Int(v)));
    }

    // Absent ids: known commit with unused counter, and unknown commit.
    assert_eq!(tbl.row_position(row_id_from(1, 3)), None);
    assert_eq!(tbl.row_position(row_id_from(9, 0)), None);
}

/// Primary key comparison works on dictionary-encoded id columns, and an
/// id with an unseen commit hash never collides.
#[test]
fn primary_key_detects_duplicates_in_id_columns() {
    let mut schema = id_schema(&["src", "dst"]);
    schema.primary_key = Some(vec![Path::from("src")]);
    let mut tbl = TestTable::new(Path::from("edges"), schema);

    let src = row_id_from(3, 7);
    tbl.insert_row(
        vec![CellValue::Id(src), CellValue::Id(row_id_from(4, 8))],
        row_id_from(1, 0),
    );

    let duplicate = vec![CellValue::Id(src), CellValue::Id(row_id_from(4, 9))];
    assert_eq!(
        tbl.validate_insert(&duplicate),
        Err(ValidationError::DuplicatePrimaryKey)
    );

    let unseen_commit = vec![
        CellValue::Id(row_id_from(9, 7)),
        CellValue::Id(row_id_from(4, 8)),
    ];
    assert!(tbl.validate_insert(&unseen_commit).is_ok());
}

fn int_schema(columns: &[&str], primary_key: Option<&[&str]>) -> ir::Schema {
    ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: columns
            .iter()
            .map(|name| ir::ColumnEntry {
                path: Path::from(*name),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            })
            .collect(),
        primary_key: primary_key.map(|pk| pk.iter().map(|name| Path::from(*name)).collect()),
    }
}

/// Multi-column primary keys reject a duplicate pair but accept rows
/// sharing only one key column, regardless of insert order.
#[test]
fn multi_column_primary_key_checks_all_columns() {
    let schema = int_schema(&["c0", "c1", "c2"], Some(&["c0", "c1"]));
    let mut tbl = TestTable::new(Path::from("pairs"), schema);

    let rows = [(3, 1), (1, 2), (1, 1), (2, 1), (2, 2)];
    for (i, (a, b)) in rows.into_iter().enumerate() {
        let values = vec![CellValue::Int(a), CellValue::Int(b), CellValue::Int(0)];
        tbl.validate_insert(&values).expect("unique pair");
        tbl.insert_row(values, test_row_id(i as u32));
    }

    for (a, b) in rows {
        let dup = vec![CellValue::Int(a), CellValue::Int(b), CellValue::Int(9)];
        assert_eq!(
            tbl.validate_insert(&dup),
            Err(ValidationError::DuplicatePrimaryKey)
        );
    }
    let fresh = vec![CellValue::Int(3), CellValue::Int(2), CellValue::Int(0)];
    assert!(tbl.validate_insert(&fresh).is_ok());
}

/// String primary keys go through the sorted index as well.
#[test]
fn string_primary_key_detects_duplicates() {
    let schema = ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: vec![ir::ColumnEntry {
            path: Path::from("name"),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinStr,
            },
        }],
        primary_key: Some(vec![Path::from("name")]),
    };
    let mut tbl = TestTable::new(Path::from("named"), schema);

    for (i, name) in ["b", "a", "c"].into_iter().enumerate() {
        let values = vec![CellValue::Str(name.to_string())];
        tbl.validate_insert(&values).expect("unique name");
        tbl.insert_row(values, test_row_id(i as u32));
    }

    assert_eq!(
        tbl.validate_insert(&[CellValue::Str("a".to_string())]),
        Err(ValidationError::DuplicatePrimaryKey)
    );
    assert!(
        tbl.validate_insert(&[CellValue::Str("d".to_string())])
            .is_ok()
    );
}

/// Schemas are compiler-generated, so a primary key referencing an
/// unknown column is a bug and fails table construction.
#[test]
#[should_panic(expected = "schema pk spec is correct")]
fn invalid_primary_key_name_panics_at_construction() {
    let schema = int_schema(&["c0"], Some(&["missing"]));
    Table::new(Path::from("broken"), 0, schema);
}

/// Manual benchmark for the primary key duplicate check on insert.
/// Run with:
/// cargo test -p coln-store --release pk_insert_benchmark -- --ignored --nocapture
#[test]
#[ignore = "manual benchmark"]
fn pk_insert_benchmark() {
    let schema = ir::Schema {
        entity_variant: ir::EntityVariant::Table,
        columns: vec![ir::ColumnEntry {
            path: Path::from("c0"),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            },
        }],
        primary_key: Some(vec![Path::from("c0")]),
    };
    let mut tbl = TestTable::new(Path::from("bench"), schema);
    let n = 50_000;
    let start = std::time::Instant::now();
    for i in 0..n {
        let values = vec![CellValue::Int(i)];
        tbl.validate_insert(&values).expect("keys are unique");
        tbl.insert_row(values, test_row_id(i as u32));
    }
    println!("inserted {n} rows with pk check in {:?}", start.elapsed());
}

/// Creates a table with an index, and does an indexed lookup as well
/// a non-indexed lookup and both should work.
#[test]
fn table_performs_index_lookup() {
    let schema = int_schema(&["indexed", "plain"], Some(&["indexed"]));
    let mut tbl = TestTable::new(Path::from("lookup"), schema);
    tbl.insert_row(vec![CellValue::Int(7), CellValue::Int(70)], test_row_id(0));
    tbl.insert_row(vec![CellValue::Int(8), CellValue::Int(80)], test_row_id(1));

    let index = tbl.table.primary_index().expect("primary-key index");
    assert_eq!(
        tbl.table
            .index_lookup(index, &[CellValue::Int(7)], &tbl.dict),
        Ok(true)
    );
    assert_eq!(
        tbl.table
            .index_lookup(index, &[CellValue::Int(9)], &tbl.dict),
        Ok(false)
    );
    assert_eq!(
        tbl.table.lookup(
            &[SeekKey {
                column: 1,
                value: CellValue::Int(80),
            }],
            &tbl.dict,
        ),
        Ok(true)
    );
    assert_eq!(
        tbl.table.lookup(
            &[SeekKey {
                column: 1,
                value: CellValue::Int(90),
            }],
            &tbl.dict,
        ),
        Ok(false)
    );
}

/// Creates a table with an index, but passes an index id that does not exist
/// which is then rejected by the table.
#[test]
fn table_index_lookup_non_existing_index() {
    let schema = int_schema(&["indexed"], Some(&["indexed"]));
    let tbl = TestTable::new(Path::from("lookup"), schema);

    assert_eq!(
        tbl.table.index_lookup(99, &[CellValue::Int(7)], &tbl.dict),
        Err(ValidationError::InvalidIndex { index: 99 })
    );
}

/// Creates a table with an index, but gives a key that does not match the
/// index shape, which should be rejected as an error.
#[test]
fn table_index_lookup_incorrect_key() {
    let schema = int_schema(&["indexed", "plain"], Some(&["indexed"]));
    let tbl = TestTable::new(Path::from("lookup"), schema);
    let index = tbl.table.primary_index().expect("primary-key index");

    assert_eq!(
        tbl.table
            .index_lookup(index, &[CellValue::Int(7), CellValue::Int(8)], &tbl.dict,),
        Err(ValidationError::InvalidIndexKey {
            index,
            expected: 1,
            got: 2,
        })
    );
}

/// Creates a table with index, and requests a index lookup. Also do a
/// non-index lookup on a different column but looks for the same row(s)
/// Indexed and non-index lookup should return the same results (positive and
/// negative)
#[test]
fn table_index_non_index_give_same_results() {
    let schema = int_schema(&["indexed", "plain"], Some(&["indexed"]));
    let mut tbl = TestTable::new(Path::from("lookup"), schema);
    for value in [7, 8] {
        let row_id = test_row_id(tbl.row_count() as u32);
        tbl.insert_row(vec![CellValue::Int(value), CellValue::Int(value)], row_id);
    }
    let index = tbl.table.primary_index().expect("primary-key index");

    for value in [7, 9] {
        let indexed = tbl
            .table
            .index_seek(index, &[CellValue::Int(value)], &tbl.dict)
            .expect("valid index lookup")
            .collect::<Vec<_>>();
        let scanned = tbl
            .table
            .seek(
                &[SeekKey {
                    column: 1,
                    value: CellValue::Int(value),
                }],
                &tbl.dict,
            )
            .expect("valid table scan")
            .collect::<Vec<_>>();
        assert_eq!(indexed, scanned);
    }
}

#[test]
fn debug_dumps_rows() {
    let schema = ir::Schema {
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
    };
    let mut tbl = TestTable::new(Path::from("debug.table"), schema);

    tbl.insert_row(
        vec![CellValue::Int(7), CellValue::Str("x".to_string())],
        test_row_id(0),
    );
    tbl.insert_row(
        vec![CellValue::Int(8), CellValue::Str("y".to_string())],
        test_row_id(1),
    );

    assert_eq!(
        tbl.dump(),
        format!(
            concat!(
                "table debug.table (rows: 2, cols: 2)\n",
                "[0] row_id={} | c0=7 | c1=\"x\"\n",
                "[1] row_id={} | c0=8 | c1=\"y\"\n",
            ),
            test_row_id(0),
            test_row_id(1),
        )
    );
}
