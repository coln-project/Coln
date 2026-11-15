// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::engine::packed::{PackedRowId, StoreScalarValue, StoreTuple, WithRowId};
use coln_flir_rs::hash::CommitHash;
use coln_flir_rs::public::{PublicRowId, PublicScalarValue};
use rstest::rstest;

use super::*;
use crate::test_utils::expect_data_chunk;
use crate::{
    ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Materialization, Path, Schema},
    op::PublicOp,
    table::ValidationError,
};
mod tables {
    use super::*;

    #[test]
    fn create_table() {
        let path = Path::from("table1");
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::RowId { path: path.clone() },
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        let oid0 = store
            .create_table(path.clone(), schema)
            .expect("create table");
        assert_eq!(oid0, 0);

        {
            let read = store.begin_read();
            let t = read.table(oid0).expect("table at oid 0");
            assert_eq!(t.schema().columns.len(), 1);
            assert_eq!(t.row_count(), 0);
        }

        // Second registration gets the next oid.
        let schema2 = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        };
        let oid1 = store
            .create_table(Path::from("Other"), schema2)
            .expect("create second table");
        assert_eq!(oid1, 1);
    }

    #[test]
    fn resolve_table_oid() {
        let path = Path::from("G.E");
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::RowId { path: path.clone() },
            }],
            primary_key: None,
        };

        let mut store = Store::new();
        let oid = store
            .create_table(path.clone(), schema)
            .expect("create table");

        assert_eq!(store.resolve_table(&path), Some(oid));
        assert_eq!(store.resolve_table(&Path::from("missing")), None);
    }
}

mod root_metadata {
    use super::*;
    use crate::test_utils::non_empty_root_commit_data;

    #[rstest]
    fn json_ir_serializes_root_ir(non_empty_root_commit_data: RootCommitData) {
        let root = non_empty_root_commit_data;
        let expected = serde_json::to_string(&root.ir).expect("serialize expected IR");
        let store = Store::try_from_ir(&root.ir, root.coln_def).expect("build store");

        assert_eq!(store.json_ir(), expected);
    }

    #[rstest]
    fn coln_def_returns_root_coln_def(non_empty_root_commit_data: RootCommitData) {
        let root = non_empty_root_commit_data;
        let expected_theory = root.coln_def.theory.clone();
        let expected_realm = root.coln_def.realm.clone();
        let store = Store::try_from_ir(&root.ir, root.coln_def).expect("build store");

        let actual = store.coln_def();
        assert_eq!(actual.theory, expected_theory);
        assert_eq!(actual.realm, expected_realm);
    }
}

mod writes {
    use super::*;
    use crate::test_utils::single_int_store;

    #[rstest]
    fn store_add_inserts_row(#[from(single_int_store)] store: Store) {
        let path = Path::from("T");

        let mut txn = store.begin_write();
        txn.add(&path, vec![42i32]).expect("add row");
        txn.try_commit().expect("prepare").commit();

        assert_eq!(store.begin_read().scan_table(&path).expect("T").len(), 1);
    }
}

mod query {
    use super::*;
    use crate::test_utils::{commit_int_store, single_int_store};

    #[rstest]
    fn empty_store_returns_empty_scan(#[from(single_int_store)] store: Store) {
        let path = Path::from("T");

        assert_eq!(
            store.begin_read().scan_table(&path).expect("known table"),
            vec![]
        );
        assert!(
            store
                .begin_read()
                .scan_table(&Path::from("missing"))
                .is_none()
        );
    }

    #[rstest]
    fn scan_table_returns_rows_for_known_table(commit_int_store: (Store, CommitHash)) {
        let path = Path::from("T");
        let (store, commit) = commit_int_store;
        let row_id = store
            .begin_read()
            .id_lookup()
            .packed(&PublicRowId { commit, counter: 0 })
            .expect("committed id is packed");

        assert_eq!(
            store.begin_read().scan_table(&path).expect("known table"),
            vec![StoreTuple::from_id_values(
                row_id,
                vec![StoreScalarValue::I32(42)]
            )]
        );
    }
}

mod rowing {
    use rstest::{fixture, rstest};

    use super::*;
    use crate::test_utils::row_id_from;

    /// Structural `Term(value)`, `Plus(Term, Term)`, and `Mult(Plus, Plus)`
    /// tables, and a non-structural `Note(Term)` table.
    #[fixture]
    fn structural_store() -> Store {
        let int_col = |name: &str| ColumnEntry {
            path: Path::from(name),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            },
        };
        let id_col = |name: &str, target: &str| ColumnEntry {
            path: Path::from(name),
            col_type: ColType::RowId {
                path: Path::from(target),
            },
        };
        let schema = |columns: Vec<ColumnEntry>, structural: bool| Schema {
            entity_variant: if structural {
                EntityVariant::View {
                    materialization: Materialization::Memoized,
                }
            } else {
                EntityVariant::Table
            },
            columns,
            primary_key: None,
        };

        let mut store = Store::new();
        for (path, table_schema) in [
            ("Term", schema(vec![int_col("value")], true)),
            (
                "Plus",
                schema(vec![id_col("left", "Term"), id_col("right", "Term")], true),
            ),
            (
                "Mult",
                schema(vec![id_col("left", "Plus"), id_col("right", "Plus")], true),
            ),
            ("Note", schema(vec![id_col("term", "Term")], false)),
        ] {
            store
                .create_table(Path::from(path), table_schema)
                .expect("create table");
        }
        store
    }

    /// `Term(value)` and `F(x, y)` are both structural. `F` keys on `x`, so each `x`
    /// maps to at most one `y`.
    fn structural_pk_store() -> Store {
        let int_col = |name: &str| ColumnEntry {
            path: Path::from(name),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            },
        };
        let id_col = |name: &str, target: &str| ColumnEntry {
            path: Path::from(name),
            col_type: ColType::RowId {
                path: Path::from(target),
            },
        };

        let mut store = Store::new();
        for (path, table_schema) in [
            (
                "Term",
                Schema {
                    entity_variant: EntityVariant::View {
                        materialization: Materialization::Memoized,
                    },
                    columns: vec![int_col("value")],
                    primary_key: None,
                },
            ),
            (
                "F",
                Schema {
                    entity_variant: EntityVariant::View {
                        materialization: Materialization::Memoized,
                    },
                    columns: vec![id_col("x", "Term"), id_col("y", "Term")],
                    primary_key: Some(vec![0u64]),
                },
            ),
        ] {
            store
                .create_table(Path::from(path), table_schema)
                .expect("create table");
        }
        store
    }

    fn add_op(
        store: &Store,
        table: &str,
        rid: PublicRowId,
        values: Vec<PublicScalarValue>,
    ) -> PublicOp {
        PublicOp::Add {
            table: store
                .resolve_table(&Path::from(table))
                .expect("test table exists"),
            values: std::iter::once(PublicScalarValue::RowId(rid))
                .chain(values)
                .collect(),
        }
    }

    fn apply_ops_and_rebuild(store: &mut Store, ops: Vec<PublicOp>) -> Result<(), StoreError> {
        let data = store.data.get_mut();
        data.apply_commit_ops(ops)?;
        data.rebuild_to_fixpoint()
    }

    /// When a smaller structurally equal row swaps a class's canonical id, the
    /// rebuild renames the row in its own table and rewrites the id cells of
    /// every table that references it.
    #[rstest]
    fn swap_rewrites_referencing_table_cells(#[from(structural_store)] mut store: Store) {
        let t_high = row_id_from(2, 0);
        let ops = vec![add_op(
            &store,
            "Term",
            t_high,
            vec![PublicScalarValue::I32(7)],
        )];
        apply_ops_and_rebuild(&mut store, ops).unwrap();

        let plus = row_id_from(3, 0);
        let note = row_id_from(4, 0);
        let ops = vec![
            add_op(
                &store,
                "Plus",
                plus,
                vec![
                    PublicScalarValue::RowId(t_high),
                    PublicScalarValue::RowId(t_high),
                ],
            ),
            add_op(&store, "Note", note, vec![PublicScalarValue::RowId(t_high)]),
        ];
        apply_ops_and_rebuild(&mut store, ops).unwrap();

        // A smaller equal term swaps the class canonical from t_high to t_low.
        let t_low = row_id_from(1, 0);
        let ops = vec![add_op(
            &store,
            "Term",
            t_low,
            vec![PublicScalarValue::I32(7)],
        )];
        apply_ops_and_rebuild(&mut store, ops).unwrap();

        let read = store.begin_read();
        // The stored row is now t_low; the stale id t_high resolves to it.
        let t_low_packed = read.id_lookup().packed(&t_low).expect("t_low is packed");
        let term_row = Some(StoreTuple::from_id_values(
            t_low_packed,
            vec![StoreScalarValue::I32(7)],
        ));
        let term = read.table_at(&Path::from("Term")).expect("Term");
        assert_eq!(term.row_by_id(&t_low_packed), term_row);
        assert_eq!(
            term.row_by_id(&read.id_lookup().packed(&t_high).expect("t_high is packed")),
            term_row
        );
        // An id that was never observed still misses, whether its commit is
        // unknown to the store or only its counter is.
        assert_eq!(read.id_lookup().packed(&row_id_from(9, 0)), None);
        assert_eq!(
            term.row_by_id(&PackedRowId {
                counter: 9,
                ..t_low_packed
            }),
            None
        );

        // Both referencing tables now name the new canonical id.
        let plus_packed = read.id_lookup().packed(&plus).expect("plus is packed");
        assert_eq!(
            read.table_at(&Path::from("Plus"))
                .expect("Plus")
                .row_by_id(&plus_packed),
            Some(StoreTuple::from_id_values(
                plus_packed,
                vec![
                    StoreScalarValue::RowId(t_low_packed),
                    StoreScalarValue::RowId(t_low_packed),
                ],
            ))
        );
        let note_packed = read.id_lookup().packed(&note).expect("note is packed");
        assert_eq!(
            read.table_at(&Path::from("Note"))
                .expect("Note")
                .row_by_id(&note_packed),
            Some(StoreTuple::from_id_values(
                note_packed,
                vec![StoreScalarValue::RowId(t_low_packed)]
            ))
        );
    }

    /// In some cases it is possible for a row with both a stale rowid and referring
    /// to stale ids. We should make sure that this row's ids are canonicalised in one og.
    /// Term t_low  = (1,0) value 7
    /// Term t_high = (2,0) value 7          -> union, canonical t_low, displaces t_high
    /// Plus keep   = (3,0) [t_high, t_high]
    /// Plus dup    = (4,0) [t_high, t_high] -> union, canonical keep, displaces dup
    /// In this case t_high will stage a union(t_low, t_high), and dup will stage union(keep,dup)
    /// And the second union will find that the dup row has a stale rowid (because the
    /// canonical one should be keep) AND referring to a stale rowid (t_high).
    #[test]
    fn row_stale_by_its_own_id_and_by_its_cells() {
        let mut store = structural_store();

        let t_low = row_id_from(1, 0);
        let t_high = row_id_from(2, 0);
        let keep = row_id_from(3, 0);
        let dup = row_id_from(4, 0);
        let ops = vec![
            add_op(&store, "Term", t_low, vec![PublicScalarValue::I32(7)]),
            add_op(&store, "Term", t_high, vec![PublicScalarValue::I32(7)]),
            add_op(
                &store,
                "Plus",
                keep,
                vec![
                    PublicScalarValue::RowId(t_high),
                    PublicScalarValue::RowId(t_high),
                ],
            ),
            add_op(
                &store,
                "Plus",
                dup,
                vec![
                    PublicScalarValue::RowId(t_high),
                    PublicScalarValue::RowId(t_high),
                ],
            ),
        ];
        apply_ops_and_rebuild(&mut store, ops)
            .expect("duplicates merge rather than failing the commit");

        let read = store.begin_read();
        let terms = read.scan_table(&Path::from("Term")).unwrap();
        let plus = read.scan_table(&Path::from("Plus")).unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(plus.len(), 1);

        // The surviving row keeps the canonical id and names canonical children.
        let t_low_packed = read.id_lookup().packed(&t_low).expect("t_low is packed");
        assert_eq!(
            plus[0].row_id(),
            read.id_lookup().packed(&keep).expect("keep is packed")
        );
        assert_eq!(
            plus[0].values(),
            [
                StoreScalarValue::RowId(t_low_packed),
                StoreScalarValue::RowId(t_low_packed)
            ]
        );
        // Both stale ids still resolve to what replaced them.
        let plus_tbl = read.table_at(&Path::from("Plus")).expect("Plus");
        assert_eq!(
            plus_tbl.row_by_id(&read.id_lookup().packed(&dup).expect("dup is packed")),
            plus_tbl.row_by_id(&read.id_lookup().packed(&keep).expect("keep is packed"))
        );
    }

    /// A row holding two displaced ids is recorded against both of them, so a
    /// rebuild pass reaches it once per displaced id. It still has to be replaced
    /// once, with every cell canonicalised in the same replacement.
    #[test]
    fn row_referring_to_two_displaced_ids() {
        let mut store = structural_store();

        let t_low = row_id_from(1, 0);
        let u_low = row_id_from(1, 1);
        let t_high = row_id_from(2, 0);
        let u_high = row_id_from(2, 1);
        let plus = row_id_from(3, 0);
        let ops = vec![
            add_op(&store, "Term", t_low, vec![PublicScalarValue::I32(7)]),
            add_op(&store, "Term", u_low, vec![PublicScalarValue::I32(8)]),
            add_op(&store, "Term", t_high, vec![PublicScalarValue::I32(7)]),
            add_op(&store, "Term", u_high, vec![PublicScalarValue::I32(8)]),
            add_op(
                &store,
                "Plus",
                plus,
                vec![
                    PublicScalarValue::RowId(t_high),
                    PublicScalarValue::RowId(u_high),
                ],
            ),
        ];
        apply_ops_and_rebuild(&mut store, ops)
            .expect("duplicates merge rather than failing the commit");

        let read = store.begin_read();
        let terms = read.scan_table(&Path::from("Term")).unwrap();
        assert_eq!(terms.len(), 2);
        let plus_packed = read.id_lookup().packed(&plus).expect("plus is packed");
        assert_eq!(
            read.table_at(&Path::from("Plus"))
                .expect("Plus")
                .row_by_id(&plus_packed),
            Some(StoreTuple::from_id_values(
                plus_packed,
                vec![
                    StoreScalarValue::RowId(
                        read.id_lookup().packed(&t_low).expect("t_low is packed")
                    ),
                    StoreScalarValue::RowId(
                        read.id_lookup().packed(&u_low).expect("u_low is packed")
                    ),
                ],
            ))
        );
    }

    /// Store can deduplicate identical commits correctly, up to three levels up.
    #[test]
    fn add_duplicate_commits_on_structural_tables() {
        let store = structural_store();
        let term_path: Path = Path::from("Term");
        let plus_path = Path::from("Plus");
        let mult_path = Path::from("Mult");

        let mut txn = store.begin_write();
        let t7 = txn.add(&term_path, vec![7i32]).unwrap();
        let t8 = txn.add(&term_path, vec![8i32]).unwrap();
        let tp = txn.add(&plus_path, vec![t7, t8]).unwrap();
        txn.add(&mult_path, vec![tp, tp]).unwrap();
        txn.commit().unwrap();

        let mut txn2 = store.begin_write();
        let t7 = txn2.add(&term_path, vec![7i32]).unwrap();
        let t8 = txn2.add(&term_path, vec![8i32]).unwrap();
        let tp = txn2.add(&plus_path, vec![t7, t8]).unwrap();
        txn2.add(&mult_path, vec![tp, tp]).unwrap();
        txn2.commit().unwrap();

        let terms = store.begin_read().scan_table(&term_path).unwrap();
        let plus = store.begin_read().scan_table(&plus_path).unwrap();
        let mult = store.begin_read().scan_table(&mult_path).unwrap();

        // The second commit adds no rows: every row it names is structurally
        // identical to one the first commit already stored.
        assert_eq!(terms.len(), 2);
        assert_eq!(plus.len(), 1);
        assert_eq!(mult.len(), 1);

        let term_id = |value: i32| {
            let matching: Vec<&StoreTuple> = terms
                .iter()
                .filter(|row| row.values() == [StoreScalarValue::I32(value)])
                .collect();
            assert_eq!(matching.len(), 1, "exactly one Term({value})");
            matching[0].row_id()
        };
        let t7 = term_id(7);
        let t8 = term_id(8);

        // Each surviving row references the canonical id of its children, not the
        // duplicate the second commit allocated for them.
        assert_eq!(
            plus[0].values(),
            [StoreScalarValue::RowId(t7), StoreScalarValue::RowId(t8)]
        );
        let plus_id = plus[0].row_id();
        assert_eq!(
            mult[0].values(),
            [
                StoreScalarValue::RowId(plus_id),
                StoreScalarValue::RowId(plus_id)
            ]
        );
    }

    #[test]
    fn rebuild_failure_rolls_back_and_discards_only_the_failed_commit() {
        let mut target = structural_pk_store();
        let source = structural_pk_store();
        let root = source.begin_read().heads();
        let term = Path::from("Term");
        let f = Path::from("F");

        let mut txn = target.begin_write();
        let t1 = txn.add(&term, vec![1i32]).expect("Term(1)");
        let t2 = txn.add(&term, vec![2i32]).expect("Term(2)");
        txn.add(&f, vec![t1, t2]).expect("F(Term1, Term2)");
        txn.commit().expect("initial data");
        let terms_before = target.begin_read().scan_table(&term).expect("Term");
        let f_before = target.begin_read().scan_table(&f).expect("F");
        let heads = target.begin_read().heads();

        // These fresh ids pass precheck. Canonicalising the two Term(1) rows
        // during rebuild makes F's primary keys collide in the target.
        let mut txn = source.begin_write();
        let t1 = txn.add(&term, vec![1i32]).expect("Term(1)");
        let t4 = txn.add(&term, vec![4i32]).expect("Term(4)");
        txn.add(&f, vec![t1, t4]).expect("F(Term1, Term4)");
        let conflicting = txn.commit().expect("valid in source").commit_hash();
        let mut txn = source.begin_write();
        txn.add(&term, vec![99i32]).expect("child's data");
        txn.commit().expect("child commit");

        let batch = target
            .prepare_commits(
                source
                    .begin_read()
                    .chunks_after(&root)
                    .into_iter()
                    .map(expect_data_chunk),
                true,
            )
            .expect("batch");
        assert!(matches!(
            batch.prepare_next(),
            Err(StoreError::Validation(ValidationError::DuplicatePrimaryKey))
        ));

        assert_eq!(target.begin_read().heads(), heads);
        assert_eq!(
            target.begin_read().scan_table(&term).expect("Term"),
            terms_before
        );
        assert_eq!(target.begin_read().scan_table(&f).expect("F"), f_before);
        assert!(target.begin_read().commit_by_hash(&conflicting).is_none());
        target
            .apply_commits_plain([])
            .expect("failed commit must not be retried");
        let batch = target.prepare_commits([], true).expect("remaining queue");
        assert_eq!(batch.size(), 1);
        assert!(!batch.has_ready());
    }

    /// Tests structural tables with primary key constraints
    #[test]
    fn structural_with_primary_key() {
        // Suppose we have table Term(value: Int) and F(X: Id, Y: Id), both structural
        // there is a primary key constraint on F, so each X can only map to single Y
        // The first commit creates Term(1), Term(2), Term(3), F(Term1, Term2)
        // Second commit creates Term(1), Term(4) F(Term1, Term4)
        // So two of the Term1 will have different ids initially, but will canonicalise to the same
        // And this will cause a primary key violation when we add the second commit
        let store = structural_pk_store();
        let term = Path::from("Term");
        let f = Path::from("F");

        let mut first = store.begin_write();
        let t1 = first.add(&term, vec![1i32]).expect("Term(1)");
        let t2 = first.add(&term, vec![2i32]).expect("Term(2)");
        first.add(&term, vec![3i32]).expect("Term(3)");
        first.add(&f, vec![t1, t2]).expect("F(Term1, Term2)");
        first.commit().expect("x is mapped only once");

        let terms_before = store.begin_read().scan_table(&term).expect("Term").len();
        let f_before = store.begin_read().scan_table(&f).expect("F");
        assert_eq!(terms_before, 3);
        assert_eq!(f_before.len(), 1);

        // Term(1) is added again under a fresh row id, so F(Term1, Term4) still
        // looks unique on x when it is inserted. The conflict only appears once
        // the two Term(1) rows canonicalise onto one id and F's x cell is
        // rewritten, which is why the check cannot live in the pre-apply pass.
        let mut second = store.begin_write();
        let t1_again = second.add(&term, vec![1i32]).expect("Term(1)");
        let t4 = second.add(&term, vec![4i32]).expect("Term(4)");
        second.add(&f, vec![t1_again, t4]).expect("F(Term1, Term4)");

        let err = second.commit().err().expect("primary key violation");
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::DuplicatePrimaryKey)
        ));

        // The rejected commit rolls back whole, including Term(4), which was
        // legal on its own.
        assert_eq!(
            store.begin_read().scan_table(&term).expect("Term").len(),
            terms_before
        );
        assert_eq!(store.begin_read().scan_table(&f).expect("F"), f_before);
    }
}

mod commits {
    use crate::test_utils::{commit_int, commit_int_store, single_int_store};

    use super::*;

    /// Compare row values without depending on packed dictionary indexes.
    fn row_values(store: &Store, table: &Path) -> Vec<Vec<StoreScalarValue>> {
        store
            .begin_read()
            .scan_table(table)
            .expect("table")
            .iter()
            .map(|row| row.values().to_vec())
            .collect()
    }

    #[rstest]
    fn heads_and_commit_by_hash_track_current_frontier(#[from(single_int_store)] mut store: Store) {
        let root = store.begin_read().heads();
        assert_eq!(root.len(), 1);
        assert_eq!(
            store.begin_read().data.commits.root_commit().hash(),
            root[0]
        );
        assert!(store.begin_read().data.commits.contains(&root[0]));
        assert!(store.begin_read().commit_by_hash(&root[0]).is_none());

        let commit = commit_int(&mut store, 42);

        assert_eq!(store.begin_read().heads(), vec![commit]);
        assert_eq!(
            store
                .begin_read()
                .commit_by_hash(&commit)
                .expect("data commit")
                .hash(),
            commit
        );
    }

    #[rstest]
    fn chunks_after_returns_descendants_in_topological_order(
        #[from(single_int_store)] mut store: Store,
    ) {
        let root = store.begin_read().heads();
        let first = commit_int(&mut store, 1);
        let second = commit_int(&mut store, 2);

        let chunks = store.begin_read().chunks_after(&root);
        let hashes = chunks
            .into_iter()
            .map(expect_data_chunk)
            .map(|commit| commit.hash())
            .collect::<Vec<_>>();

        assert_eq!(hashes, vec![first, second]);
        assert!(
            store
                .begin_read()
                .chunks_after(&store.begin_read().heads())
                .is_empty()
        );
    }

    #[test]
    fn chunks_after_empty_heads_returns_root_without_data() {
        let store = Store::new();
        assert_eq!(
            store.begin_read().chunks_after(&[]),
            vec![crate::commit::chunk::Chunk::Root(
                store.begin_read().data.commits.root_commit().clone()
            )]
        );
        assert!(
            store
                .begin_read()
                .chunks_after(&store.begin_read().heads())
                .is_empty()
        );
    }

    #[rstest]
    fn chunks_after_empty_heads_returns_root_before_data(
        #[from(single_int_store)] mut store: Store,
    ) {
        use crate::commit::chunk::Chunk;

        let root = store.begin_read().data.commits.root_commit().clone();
        let first_hash = commit_int(&mut store, 1);
        let second_hash = commit_int(&mut store, 2);
        let first = store
            .begin_read()
            .commit_by_hash(&first_hash)
            .expect("first")
            .clone();
        let second = store
            .begin_read()
            .commit_by_hash(&second_hash)
            .expect("second")
            .clone();

        assert_eq!(
            store.begin_read().chunks_after(&[]),
            vec![
                Chunk::Root(root.clone()),
                Chunk::Data(first.clone()),
                Chunk::Data(second.clone())
            ]
        );
        assert_eq!(
            store.begin_read().chunks_after(&[root.hash()]),
            vec![Chunk::Data(first), Chunk::Data(second.clone())]
        );
        assert_eq!(
            store.begin_read().chunks_after(&[CommitHash([0xff; 32])]),
            store.begin_read().chunks_after(&[])
        );
        assert_eq!(
            store.begin_read().chunks_after(&[first_hash]),
            vec![Chunk::Data(second)]
        );
        assert!(store.begin_read().chunks_after(&[second_hash]).is_empty());
    }

    #[rstest]
    fn commit_chunks_include_root_only_when_missing(#[from(single_int_store)] mut store: Store) {
        let root = store.begin_read().data.commits.root_commit().hash();
        let first = commit_int(&mut store, 1);
        let second = commit_int(&mut store, 2);

        let all = store.begin_read().commit_chunks_after(&[]);
        assert_eq!(
            all.iter().map(|chunk| chunk.hash).collect::<Vec<_>>(),
            vec![root, first, second]
        );
        let after_root = store.begin_read().commit_chunks_after(&[root]);
        assert_eq!(
            after_root
                .iter()
                .map(|chunk| chunk.hash)
                .collect::<Vec<_>>(),
            vec![first, second]
        );
        let after_first = store.begin_read().commit_chunks_after(&[first]);
        assert_eq!(
            after_first
                .iter()
                .map(|chunk| chunk.hash)
                .collect::<Vec<_>>(),
            vec![second]
        );
        assert!(store.begin_read().commit_chunks_after(&[second]).is_empty());
    }

    #[rstest]
    fn applying_root_chunks_preserves_the_existing_root(
        #[from(single_int_store)] mut store: Store,
    ) {
        let root = store.begin_read().data.commits.root_commit().hash();
        let known =
            crate::commit::frame::Frame::from(store.begin_read().data.commits.root_commit())
                .encoded();
        store
            .apply_chunk_bytes_plain([known])
            .expect("known root is ignored");

        let foreign_store = Store::new();
        let foreign_read = foreign_store.begin_read();
        let foreign_root = foreign_read.data.commits.root_commit();
        let foreign = crate::commit::frame::Frame::from(foreign_root).encoded();
        let error = store
            .apply_chunk_bytes_plain([foreign])
            .expect_err("foreign root");
        assert!(matches!(
            error,
            StoreError::Commit(crate::store::error::CommitApplyError::RootCommit(hash))
                if hash == foreign_root.hash()
        ));
        assert_eq!(store.begin_read().data.commits.root_commit().hash(), root);
        assert_eq!(store.begin_read().heads(), vec![root]);
    }

    #[rstest]
    fn commits_added_returns_commits_in_other_store(
        #[from(single_int_store)] base: Store,
        #[from(commit_int_store)]
        #[with(7)]
        other: (Store, CommitHash),
    ) {
        let (other, commit) = other;

        let commits = base.begin_read().commits_added(&other.begin_read());
        let hashes = commits.iter().map(DataCommit::hash).collect::<Vec<_>>();

        assert_eq!(hashes, vec![commit]);
    }

    #[test]
    fn commit_chunks_create_empty_store() {
        let source = Store::new();
        let chunks = source
            .begin_read()
            .commit_chunks_after(&[])
            .into_iter()
            .map(|chunk| chunk.bytes)
            .collect::<Vec<_>>();

        let restored = Store::try_from_commit_bytes_plain(chunks).expect("store from chunks");

        assert_eq!(restored.begin_read().data.pending_commits.len(), 0);
        assert_eq!(restored.table_count(), 0);
        assert_eq!(restored.begin_read().heads(), source.begin_read().heads());
    }

    #[rstest]
    fn commit_chunks_create_store_from_out_of_order_data(
        #[from(commit_int_store)]
        #[with(99)]
        source: (Store, CommitHash),
    ) {
        let (source, commit) = source;
        let mut chunks = source
            .begin_read()
            .commit_chunks_after(&[])
            .into_iter()
            .map(|chunk| chunk.bytes)
            .collect::<Vec<_>>();
        chunks.reverse();

        let restored = Store::try_from_commit_bytes_plain(chunks).expect("store from chunks");

        assert_eq!(restored.begin_read().data.pending_commits.len(), 0);
        assert_eq!(
            row_values(&restored, &Path::from("T")),
            vec![vec![StoreScalarValue::I32(99)]]
        );
        assert_eq!(restored.begin_read().heads(), vec![commit]);
    }

    #[rstest]
    fn plain_constructor_keeps_missing_dependencies_pending(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
    ) {
        let (mut source, first) = source;
        let second = commit_int(&mut source, 2);
        let chunks = source.begin_read().commit_chunks_after(&[]);
        let mut restored = Store::try_from_commit_bytes_plain(
            chunks
                .iter()
                .filter(|chunk| chunk.hash != first)
                .map(|chunk| chunk.bytes.as_slice()),
        )
        .expect("restore with missing parent");

        assert_eq!(restored.begin_read().data.pending_commits.len(), 1);
        assert_eq!(restored.begin_read().data.pending_commits[0].hash(), second);
        assert!(
            restored
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table")
                .is_empty()
        );

        restored
            .apply_chunk_bytes_plain(
                chunks
                    .into_iter()
                    .filter(|chunk| chunk.hash == first)
                    .map(|chunk| chunk.bytes),
            )
            .expect("apply missing parent");

        assert!(restored.begin_read().data.pending_commits.is_empty());
        assert_eq!(restored.begin_read().heads(), source.begin_read().heads());
        assert_eq!(
            row_values(&restored, &Path::from("T")),
            row_values(&source, &Path::from("T"))
        );
    }

    #[rstest]
    fn apply_commits_applies_rows_and_updates_heads(
        #[from(commit_int_store)]
        #[with(99)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (source, _) = source;

        let commits = source
            .begin_read()
            .chunks_after(&target.begin_read().heads())
            .into_iter()
            .map(expect_data_chunk)
            .collect::<Vec<_>>();
        target.apply_commits_plain(commits).expect("apply commits");

        assert_eq!(
            row_values(&target, &Path::from("T")),
            vec![vec![StoreScalarValue::I32(99)]]
        );
        assert_eq!(target.begin_read().heads(), source.begin_read().heads());
    }

    #[rstest]
    fn apply_commits_accepts_out_of_order_input(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, _) = source;
        commit_int(&mut source, 2);

        let mut commits = source
            .begin_read()
            .chunks_after(&target.begin_read().heads())
            .into_iter()
            .map(expect_data_chunk)
            .collect::<Vec<_>>();
        commits.reverse();
        target.apply_commits_plain(commits).expect("apply commits");

        assert_eq!(
            row_values(&target, &Path::from("T")),
            vec![
                vec![StoreScalarValue::I32(1)],
                vec![StoreScalarValue::I32(2)]
            ]
        );
        assert_eq!(target.begin_read().heads(), source.begin_read().heads());
    }

    #[rstest]
    fn apply_commits_ignores_known_commits(
        #[from(commit_int_store)]
        #[with(5)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (source, _) = source;

        let commits = source
            .begin_read()
            .chunks_after(&target.begin_read().heads())
            .into_iter()
            .map(expect_data_chunk)
            .collect::<Vec<_>>();
        target
            .apply_commits_plain(commits.clone())
            .expect("first apply commits");
        target
            .apply_commits_plain(commits)
            .expect("second apply commits");

        assert_eq!(
            row_values(&target, &Path::from("T")),
            vec![vec![StoreScalarValue::I32(5)]]
        );
    }

    #[rstest]
    fn apply_commits_skips_missing_dependency_without_changing_store(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, _) = source;
        let second = commit_int(&mut source, 2);
        let second_commit = source
            .begin_read()
            .commit_by_hash(&second)
            .expect("second commit")
            .clone();

        target
            .apply_commits_plain([second_commit])
            .expect("skip missing dependency");
        let pending_hashes: Vec<_> = target
            .begin_read()
            .data
            .pending_commits
            .iter()
            .map(DataCommit::hash)
            .collect();

        assert_eq!(pending_hashes, vec![second]);
        assert_eq!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table"),
            vec![]
        );
    }

    #[rstest]
    fn apply_commits_applies_ready_commits_and_keeps_blocked_ones_pending(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, first) = source;
        commit_int(&mut source, 2);
        let third = commit_int(&mut source, 3);
        let first_commit = source
            .begin_read()
            .commit_by_hash(&first)
            .expect("first commit")
            .clone();
        let third_commit = source
            .begin_read()
            .commit_by_hash(&third)
            .expect("third commit")
            .clone();

        target
            .apply_commits_plain([first_commit, third_commit])
            .expect("apply ready commits");
        let pending_hashes: Vec<_> = target
            .begin_read()
            .data
            .pending_commits
            .iter()
            .map(DataCommit::hash)
            .collect();

        assert_eq!(pending_hashes, vec![third]);
        // The blocked commit's row never landed, so only the first row is stored.
        assert_eq!(
            row_values(&target, &Path::from("T")),
            vec![vec![StoreScalarValue::I32(1)]]
        );
        assert_eq!(target.begin_read().heads(), vec![first]);
    }

    #[rstest]
    fn apply_chunk_bytes_plain_retries_pending_when_missing_parent_arrives(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, _) = source;
        let second = commit_int(&mut source, 2);

        let chunks: Vec<Vec<u8>> = source
            .begin_read()
            .commit_chunks_after(&target.begin_read().heads())
            .into_iter()
            .map(|chunk| chunk.bytes)
            .collect();
        assert_eq!(chunks.len(), 2);

        target
            .apply_chunk_bytes_plain([chunks[1].clone()])
            .expect("skip child without parent");
        assert_eq!(target.begin_read().data.pending_commits.len(), 1);
        assert_eq!(
            target
                .begin_read()
                .scan_table(&Path::from("T"))
                .expect("table"),
            vec![]
        );

        target
            .apply_chunk_bytes_plain(std::iter::once(chunks[0].clone()))
            .expect("retry leftover with parent");
        assert!(target.begin_read().data.pending_commits.is_empty());

        assert_eq!(
            row_values(&target, &Path::from("T")),
            vec![
                vec![StoreScalarValue::I32(1)],
                vec![StoreScalarValue::I32(2)]
            ]
        );
        assert_eq!(target.begin_read().heads(), vec![second]);
    }
}
