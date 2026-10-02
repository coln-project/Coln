// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::engine::packed::StoreScalarValue;
use coln_flir_rs::public::PublicTuple;
use rstest::rstest;

use super::*;
use crate::{
    ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Materialization, Path, Schema},
    txn::rw::{StoreRead, StoreWrite},
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

        let t = store.table(oid0).expect("table at oid 0");
        assert_eq!(t.schema().columns.len(), 1);
        assert_eq!(t.row_count(), 0);

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
        let store = Store::try_from_ir(root.ir, root.coln_def).expect("build store");

        assert_eq!(store.json_ir().expect("serialize store IR"), expected);
    }

    #[rstest]
    fn coln_def_returns_root_coln_def(non_empty_root_commit_data: RootCommitData) {
        let root = non_empty_root_commit_data;
        let expected_theory = root.coln_def.theory.clone();
        let expected_realm = root.coln_def.realm.clone();
        let store = Store::try_from_ir(root.ir, root.coln_def).expect("build store");

        let actual = store.coln_def().expect("read Coln definition");
        assert_eq!(actual.theory, expected_theory);
        assert_eq!(actual.realm, expected_realm);
    }
}

mod writes {
    use super::*;
    use crate::test_utils::single_int_autostore;

    #[rstest]
    fn store_add_inserts_row(#[from(single_int_autostore)] mut store: AutoStore) {
        let path = Path::from("T");

        store.transaction();
        store.add(&path, vec![42i32]).expect("add row");
        store.commit().expect("txn success");

        store.transaction();
        assert_eq!(store.scan_table(&path).expect("T").len(), 1);
    }
}

mod reads {

    use coln_flir_rs::engine::tx::TxTuple;

    use super::*;
    use crate::test_utils::{int_schema, nodes_edges_store};

    // Tests that store.all() returns all values satisfy requirements.
    // Test with/without rowid, and the table should contain duplicate values as well
    // Test with/without select. `select` indexes the row tuple, so column 0 is
    // the row id and schema columns start at 1.
    #[rstest]
    fn store_all_returns_all_rows(#[from(nodes_edges_store)] mut store: Store) {
        let nodes = Path::from("Nodes");
        let edges = Path::from("Edges");

        let mut txn = store.transaction();
        let n0 = txn.add(&nodes, TxTuple::empty()).expect("n0");
        let n1 = txn.add(&nodes, TxTuple::empty()).expect("n1");

        let e0 = txn.add(&edges, vec![n0.clone()]).expect("e0");
        txn.add(&edges, vec![n0.clone()])
            .expect("duplicate edge to n0");
        txn.add(&edges, vec![n1.clone()]).expect("e2");
        let h = txn.commit().expect("txn success");
        let [n0_id, n1_id, e0_id] = store.promote(vec![n0, n1, e0], h).try_into().unwrap();
        let n0_packed = store.id_lookup().packed(&n0_id).expect("n0 is packed");
        let n1_packed = store.id_lookup().packed(&n1_id).expect("n1 is packed");
        let e0_packed = store.id_lookup().packed(&e0_id).expect("e0 is packed");

        let n0_col = StoreTuple::from(vec![StoreScalarValue::RowId(n0_packed)]);
        let n1_col = StoreTuple::from(vec![StoreScalarValue::RowId(n1_packed)]);
        let no_cols = StoreTuple::from(vec![]);

        let all_edges = WhereClause {
            table_name: edges.clone(),
            row_id: None,
            values: PublicTuple::from(vec![]),
        };
        assert_eq!(
            store
                .all_proj(&all_edges, &[1])
                .expect("all edge node columns"),
            vec![n0_col.clone(), n0_col.clone(), n1_col.clone()]
        );
        assert_eq!(
            store
                .all_proj(&all_edges, &[])
                .expect("all edges with empty select"),
            vec![no_cols.clone(), no_cols.clone(), no_cols.clone()]
        );

        let by_row_id = WhereClause {
            table_name: edges,
            row_id: Some(e0_id),
            values: PublicTuple::from(vec![]),
        };
        assert_eq!(
            store
                .all_proj(&by_row_id, &[0, 1])
                .expect("edge e0 with its id"),
            vec![StoreTuple::from_id_values(
                e0_packed,
                vec![StoreScalarValue::RowId(n0_packed)]
            )]
        );
        assert_eq!(
            store.all_proj(&by_row_id, &[1]).expect("edge e0"),
            vec![n0_col]
        );
        assert_eq!(
            store
                .all_proj(&by_row_id, &[])
                .expect("edge e0 with empty select"),
            vec![no_cols]
        );
    }

    // Partial keys are a prefix of the first n consecutive columns.
    #[rstest]
    fn store_all_supports_partial_keys(
        #[from(int_schema)]
        #[with(vec!["a", "b", "c"])]
        schema: Schema,
    ) {
        let path = Path::from("T");
        let mut store = Store::new();
        store.create_table(path.clone(), schema).expect("create T");

        let mut txn = store.transaction();
        txn.add(&path, vec![1i32, 10, 100]).expect("r0");
        txn.add(&path, vec![1i32, 10, 101]).expect("r1");
        txn.add(&path, vec![1i32, 20, 200]).expect("r2");
        txn.add(&path, vec![2i32, 10, 300]).expect("r3");
        txn.commit().expect("txn success");

        let ints = |values: &[i32]| -> StoreTuple {
            values.iter().map(|v| StoreScalarValue::I32(*v)).collect()
        };
        let r0 = ints(&[1, 10, 100]);
        let r1 = ints(&[1, 10, 101]);
        let r2 = ints(&[1, 20, 200]);
        let r3 = ints(&[2, 10, 300]);
        // Skip column 0, the row id, and select the three schema columns.
        let cols = [1, 2, 3];

        let query = |values: Vec<i32>| WhereClause {
            table_name: path.clone(),
            row_id: None,
            values: PublicTuple::from(
                values
                    .into_iter()
                    .map(PublicScalarValue::from)
                    .collect::<Vec<_>>(),
            ),
        };

        assert_eq!(
            store.all_proj(&query(vec![]), &cols).expect("empty prefix"),
            vec![r0.clone(), r1.clone(), r2.clone(), r3.clone()]
        );
        assert_eq!(
            store
                .all_proj(&query(vec![1i32]), &cols)
                .expect("first column"),
            vec![r0.clone(), r1.clone(), r2.clone()]
        );
        assert_eq!(
            store
                .all_proj(&query(vec![1, 10]), &cols)
                .expect("first two columns"),
            vec![r0.clone(), r1.clone()]
        );
        assert_eq!(
            store
                .all_proj(&query(vec![1, 10, 100]), &cols)
                .expect("full key"),
            vec![r0]
        );
        assert_eq!(
            store
                .all_proj(&query(vec![1, 20]), &cols)
                .expect("first two, other b"),
            vec![r2]
        );
        assert_eq!(
            store
                .all_proj(&query(vec![2i32]), &cols)
                .expect("other first column"),
            vec![r3]
        );
        assert_eq!(
            store
                .all_proj(&query(vec![10]), &cols)
                .expect("10 is a later-column value, not a col0 prefix"),
            Vec::<StoreTuple>::new()
        );
        assert!(
            store.all_proj(&query(vec![1, 10, 100, 0]), &cols).is_err(),
            "longer than the column count is not a valid prefix"
        );
    }
}

mod query {
    use super::*;
    use crate::test_utils::{commit_int_store, single_int_store};

    #[rstest]
    fn empty_store_returns_empty_scan(#[from(single_int_store)] store: Store) {
        let path = Path::from("T");

        assert_eq!(store.scan_table(&path).expect("known table"), vec![]);
        assert!(store.scan_table(&Path::from("missing")).is_none());
    }

    #[rstest]
    fn scan_table_returns_rows_for_known_table(commit_int_store: (Store, CommitHash)) {
        let path = Path::from("T");
        let (store, commit) = commit_int_store;
        let row_id = store
            .id_lookup()
            .packed(&PublicRowId { commit, counter: 0 })
            .expect("committed id is packed");

        assert_eq!(
            store.scan_table(&path).expect("known table"),
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

    /// Store with a structural `Term` table (one int column), a structural
    /// `Plus` table (two id columns), and a non-structural `Note` table (one
    /// id column).
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
                schema(vec![id_col("left", "Term"), id_col("right", "Term")], true),
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

    fn add_op(store: &Store, table: &str, rid: PublicRowId, values: Vec<PublicScalarValue>) -> Op {
        Op::Add {
            row_id: rid,
            table: store
                .resolve_table(&Path::from(table))
                .expect("test table exists"),
            values,
        }
    }

    fn apply_ops_and_rebuild(store: &mut Store, ops: Vec<Op>) -> Result<(), StoreError> {
        store.apply_commit_ops(ops)?;
        store.rebuild_to_fixpoint()
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
            t_high.clone(),
            vec![PublicScalarValue::I32(7)],
        )];
        apply_ops_and_rebuild(&mut store, ops).unwrap();

        let plus = row_id_from(3, 0);
        let note = row_id_from(4, 0);
        let ops = vec![
            add_op(
                &store,
                "Plus",
                plus.clone(),
                vec![
                    PublicScalarValue::RowId(t_high.clone()),
                    PublicScalarValue::RowId(t_high.clone()),
                ],
            ),
            add_op(
                &store,
                "Note",
                note.clone(),
                vec![PublicScalarValue::RowId(t_high.clone())],
            ),
        ];
        apply_ops_and_rebuild(&mut store, ops).unwrap();

        // A smaller equal term swaps the class canonical from t_high to t_low.
        let t_low = row_id_from(1, 0);
        let ops = vec![add_op(
            &store,
            "Term",
            t_low.clone(),
            vec![PublicScalarValue::I32(7)],
        )];
        apply_ops_and_rebuild(&mut store, ops).unwrap();

        // The stored row is now t_low; the stale id t_high resolves to it.
        let t_low_packed = store.id_lookup().packed(&t_low).expect("t_low is packed");
        let term_row = Some(StoreTuple::from_id_values(
            t_low_packed,
            vec![StoreScalarValue::I32(7)],
        ));
        let term = store.table_at(&Path::from("Term")).expect("Term");
        assert_eq!(term.row_by_id(&t_low_packed), term_row);
        assert_eq!(
            term.row_by_id(&store.id_lookup().packed(&t_high).expect("t_high is packed")),
            term_row
        );
        // An id that was never observed still misses, whether its commit is
        // unknown to the store or only its counter is.
        assert_eq!(store.id_lookup().packed(&row_id_from(9, 0)), None);
        assert_eq!(
            term.row_by_id(&PackedRowId {
                counter: 9,
                ..t_low_packed
            }),
            None
        );

        // Both referencing tables now name the new canonical id.
        let plus_packed = store.id_lookup().packed(&plus).expect("plus is packed");
        assert_eq!(
            store
                .table_at(&Path::from("Plus"))
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
        let note_packed = store.id_lookup().packed(&note).expect("note is packed");
        assert_eq!(
            store
                .table_at(&Path::from("Note"))
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
            add_op(
                &store,
                "Term",
                t_low.clone(),
                vec![PublicScalarValue::I32(7)],
            ),
            add_op(
                &store,
                "Term",
                t_high.clone(),
                vec![PublicScalarValue::I32(7)],
            ),
            add_op(
                &store,
                "Plus",
                keep.clone(),
                vec![
                    PublicScalarValue::RowId(t_high.clone()),
                    PublicScalarValue::RowId(t_high.clone()),
                ],
            ),
            add_op(
                &store,
                "Plus",
                dup.clone(),
                vec![
                    PublicScalarValue::RowId(t_high.clone()),
                    PublicScalarValue::RowId(t_high),
                ],
            ),
        ];
        apply_ops_and_rebuild(&mut store, ops)
            .expect("duplicates merge rather than failing the commit");

        let terms = store.scan_table(&Path::from("Term")).unwrap();
        let plus = store.scan_table(&Path::from("Plus")).unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(plus.len(), 1);

        // The surviving row keeps the canonical id and names canonical children.
        let t_low_packed = store.id_lookup().packed(&t_low).expect("t_low is packed");
        assert_eq!(
            plus[0].row_id(),
            store.id_lookup().packed(&keep).expect("keep is packed")
        );
        assert_eq!(
            plus[0].values(),
            [
                StoreScalarValue::RowId(t_low_packed),
                StoreScalarValue::RowId(t_low_packed)
            ]
        );
        // Both stale ids still resolve to what replaced them.
        let plus_tbl = store.table_at(&Path::from("Plus")).expect("Plus");
        assert_eq!(
            plus_tbl.row_by_id(&store.id_lookup().packed(&dup).expect("dup is packed")),
            plus_tbl.row_by_id(&store.id_lookup().packed(&keep).expect("keep is packed"))
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
            add_op(
                &store,
                "Term",
                t_low.clone(),
                vec![PublicScalarValue::I32(7)],
            ),
            add_op(
                &store,
                "Term",
                u_low.clone(),
                vec![PublicScalarValue::I32(8)],
            ),
            add_op(
                &store,
                "Term",
                t_high.clone(),
                vec![PublicScalarValue::I32(7)],
            ),
            add_op(
                &store,
                "Term",
                u_high.clone(),
                vec![PublicScalarValue::I32(8)],
            ),
            add_op(
                &store,
                "Plus",
                plus.clone(),
                vec![
                    PublicScalarValue::RowId(t_high),
                    PublicScalarValue::RowId(u_high),
                ],
            ),
        ];
        apply_ops_and_rebuild(&mut store, ops)
            .expect("duplicates merge rather than failing the commit");

        let terms = store.scan_table(&Path::from("Term")).unwrap();
        assert_eq!(terms.len(), 2);
        let plus_packed = store.id_lookup().packed(&plus).expect("plus is packed");
        assert_eq!(
            store
                .table_at(&Path::from("Plus"))
                .expect("Plus")
                .row_by_id(&plus_packed),
            Some(StoreTuple::from_id_values(
                plus_packed,
                vec![
                    StoreScalarValue::RowId(
                        store.id_lookup().packed(&t_low).expect("t_low is packed")
                    ),
                    StoreScalarValue::RowId(
                        store.id_lookup().packed(&u_low).expect("u_low is packed")
                    ),
                ],
            ))
        );
    }

    /// Store can deduplicate identical commits correctly, up to three levels up.
    #[test]
    fn add_duplicate_commits_on_structural_tables() {
        let mut store = structural_store();
        let term_path: Path = Path::from("Term");
        let plus_path = Path::from("Plus");
        let mult_path = Path::from("Mult");

        let mut txn = store.transaction();
        let t7 = txn.add(&term_path, vec![7i32]).unwrap();
        let t8 = txn.add(&term_path, vec![8i32]).unwrap();
        let tp = txn.add(&plus_path, vec![t7, t8]).unwrap();
        txn.add(&mult_path, vec![tp.clone(), tp]).unwrap();
        txn.commit().unwrap();

        let mut txn2 = store.transaction();
        let t7 = txn2.add(&term_path, vec![7i32]).unwrap();
        let t8 = txn2.add(&term_path, vec![8i32]).unwrap();
        let tp = txn2.add(&plus_path, vec![t7, t8]).unwrap();
        txn2.add(&mult_path, vec![tp.clone(), tp]).unwrap();
        txn2.commit().unwrap();

        let terms = store.scan_table(&term_path).unwrap();
        let plus = store.scan_table(&plus_path).unwrap();
        let mult = store.scan_table(&mult_path).unwrap();

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

    /// Tests structural tables with primary key constraints
    #[test]
    fn structural_with_primary_key() {
        // Suppose we have table Term(value: Int) and F(X: Id, Y: Id), both structural
        // there is a primary key constraint on F, so each X can only map to single Y
        // The first commit creates Term(1), Term(2), Term(3), F(Term1, Term2)
        // Second commit creates Term(1), Term(4) F(Term1, Term4)
        // So two of the Term1 will have different ids initially, but will canonicalise to the same
        // And this will cause a primary key violation when we add the second commit
        let mut store = structural_pk_store();
        let term = Path::from("Term");
        let f = Path::from("F");

        let mut first = store.transaction();
        let t1 = first.add(&term, vec![1i32]).expect("Term(1)");
        let t2 = first.add(&term, vec![2i32]).expect("Term(2)");
        first.add(&term, vec![3i32]).expect("Term(3)");
        first.add(&f, vec![t1, t2]).expect("F(Term1, Term2)");
        first.commit().expect("x is mapped only once");

        let terms_before = store.scan_table(&term).expect("Term").len();
        let f_before = store.scan_table(&f).expect("F");
        assert_eq!(terms_before, 3);
        assert_eq!(f_before.len(), 1);

        // Term(1) is added again under a fresh row id, so F(Term1, Term4) still
        // looks unique on x when it is inserted. The conflict only appears once
        // the two Term(1) rows canonicalise onto one id and F's x cell is
        // rewritten, which is why the check cannot live in the pre-apply pass.
        let mut second = store.transaction();
        let t1_again = second.add(&term, vec![1i32]).expect("Term(1)");
        let t4 = second.add(&term, vec![4i32]).expect("Term(4)");
        second.add(&f, vec![t1_again, t4]).expect("F(Term1, Term4)");

        let err = second.commit().unwrap_err();
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::DuplicatePrimaryKey)
        ));

        // The rejected commit rolls back whole, including Term(4), which was
        // legal on its own.
        assert_eq!(store.scan_table(&term).expect("Term").len(), terms_before);
        assert_eq!(store.scan_table(&f).expect("F"), f_before);
    }
}

mod commits {
    use crate::{
        store::frag::FragmentSync,
        test_utils::{commit_int, commit_int_store, single_int_store},
    };

    use super::*;

    /// Replayed commits mint their own row ids, so these tests can only
    /// compare the values a scan reports.
    fn row_values(store: &Store, table: &Path) -> Vec<Vec<StoreScalarValue>> {
        store
            .scan_table(table)
            .expect("table")
            .iter()
            .map(|row| row.values().to_vec())
            .collect()
    }

    #[rstest]
    fn heads_and_commit_by_hash_track_current_frontier(#[from(single_int_store)] mut store: Store) {
        let root = store.heads();
        assert_eq!(root.len(), 1);
        assert_eq!(
            store.commit_by_hash(&root[0]).expect("root").hash(),
            root[0]
        );

        let commit = commit_int(&mut store, 42);

        assert_eq!(store.heads(), vec![commit]);
        assert_eq!(
            store.commit_by_hash(&commit).expect("data commit").hash(),
            commit
        );
    }

    #[rstest]
    fn commits_after_returns_descendants_in_topological_order(
        #[from(single_int_store)] mut store: Store,
    ) {
        let root = store.heads();
        let first = commit_int(&mut store, 1);
        let second = commit_int(&mut store, 2);

        let commits = store.commits_after(&root);
        let hashes = commits.iter().map(Commit::hash).collect::<Vec<_>>();

        assert_eq!(hashes, vec![first, second]);
        assert!(store.commits_after(&store.heads()).is_empty());
    }

    #[rstest]
    fn commits_added_returns_commits_in_other_store(
        #[from(single_int_store)] base: Store,
        #[from(commit_int_store)]
        #[with(7)]
        other: (Store, CommitHash),
    ) {
        let (other, commit) = other;

        let commits = base.commits_added(&other);
        let hashes = commits.iter().map(Commit::hash).collect::<Vec<_>>();

        assert_eq!(hashes, vec![commit]);
    }

    #[test]
    fn commit_chunks_create_empty_store() {
        let source = Store::new();
        let chunks = source
            .commit_chunks_after(&[])
            .into_iter()
            .map(|chunk| chunk.bytes)
            .collect::<Vec<_>>();

        let restored = Store::try_from_commit_bytes(chunks).expect("store from chunks");

        assert_eq!(restored.pending_commits_len(), 0);
        assert_eq!(restored.table_count(), 0);
        assert_eq!(restored.heads(), source.heads());
    }

    #[rstest]
    fn commit_chunks_create_store_from_out_of_order_data(
        #[from(commit_int_store)]
        #[with(99)]
        source: (Store, CommitHash),
    ) {
        let (source, commit) = source;
        let mut chunks = source
            .commit_chunks_after(&[])
            .into_iter()
            .map(|chunk| chunk.bytes)
            .collect::<Vec<_>>();
        chunks.reverse();

        let restored = Store::try_from_commit_bytes(chunks).expect("store from chunks");

        assert_eq!(restored.pending_commits_len(), 0);
        assert_eq!(
            row_values(&restored, &Path::from("T")),
            vec![vec![StoreScalarValue::I32(99)]]
        );
        assert_eq!(restored.heads(), vec![commit]);
    }

    #[rstest]
    fn apply_commits_applies_rows_and_updates_heads(
        #[from(commit_int_store)]
        #[with(99)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (source, _) = source;

        let commits = source.commits_after(&target.heads());
        target.apply_commits(commits).expect("apply commits");

        assert_eq!(
            row_values(&target, &Path::from("T")),
            vec![vec![StoreScalarValue::I32(99)]]
        );
        assert_eq!(target.heads(), source.heads());
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

        let mut commits = source.commits_after(&target.heads());
        commits.reverse();
        target.apply_commits(commits).expect("apply commits");

        assert_eq!(
            row_values(&target, &Path::from("T")),
            vec![
                vec![StoreScalarValue::I32(1)],
                vec![StoreScalarValue::I32(2)]
            ]
        );
        assert_eq!(target.heads(), source.heads());
    }

    #[rstest]
    fn apply_commits_ignores_known_commits(
        #[from(commit_int_store)]
        #[with(5)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (source, _) = source;

        let commits = source.commits_after(&target.heads());
        target
            .apply_commits(commits.clone())
            .expect("first apply commits");
        target.apply_commits(commits).expect("second apply commits");

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
            .commit_by_hash(&second)
            .expect("second commit")
            .clone();

        let leftover = target
            .apply_commits([second_commit])
            .expect("skip missing dependency");
        let leftover_hashes: Vec<_> = leftover.iter().map(Commit::hash).collect();

        assert_eq!(leftover_hashes, vec![second]);
        assert_eq!(target.scan_table(&Path::from("T")).expect("table"), vec![]);
    }

    #[rstest]
    fn apply_commits_applies_ready_commits_and_returns_blocked_ones(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, first) = source;
        commit_int(&mut source, 2);
        let third = commit_int(&mut source, 3);
        let first_commit = source.commit_by_hash(&first).expect("first commit").clone();
        let third_commit = source.commit_by_hash(&third).expect("third commit").clone();

        let leftover = target
            .apply_commits([first_commit, third_commit])
            .expect("apply ready commits");
        let leftover_hashes: Vec<_> = leftover.iter().map(Commit::hash).collect();

        assert_eq!(leftover_hashes, vec![third]);
        // The blocked commit's row never landed, so only the first row is stored.
        assert_eq!(
            row_values(&target, &Path::from("T")),
            vec![vec![StoreScalarValue::I32(1)]]
        );
        assert_eq!(target.heads(), vec![first]);
    }

    #[rstest]
    fn apply_chunk_bytes_retries_leftover_when_missing_parent_arrives(
        #[from(commit_int_store)]
        #[with(1)]
        source: (Store, CommitHash),
        #[from(single_int_store)] mut target: Store,
    ) {
        let (mut source, _) = source;
        let second = commit_int(&mut source, 2);

        let chunks: Vec<Vec<u8>> = source
            .commit_chunks_after(&target.heads())
            .into_iter()
            .map(|chunk| chunk.bytes)
            .collect();
        assert_eq!(chunks.len(), 2);

        target
            .apply_chunk_bytes([chunks[1].clone()])
            .expect("skip child without parent");
        assert_eq!(target.pending_commits.len(), 1);
        assert_eq!(target.scan_table(&Path::from("T")).expect("table"), vec![]);

        target
            .apply_chunk_bytes(std::iter::once(chunks[0].clone()))
            .expect("retry leftover with parent");
        assert!(target.pending_commits.is_empty());

        assert_eq!(
            row_values(&target, &Path::from("T")),
            vec![
                vec![StoreScalarValue::I32(1)],
                vec![StoreScalarValue::I32(2)]
            ]
        );
        assert_eq!(target.heads(), vec![second]);
    }
}
