// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

/// Shared theory fixtures for unit tests (`store`, `transaction`, etc.).
pub(crate) mod test_support {
    use crate::ir::{
        Atom, BuiltinTy, ColType, ColumnEntry, EntityVariant, FlatRealm, Path, Prop, Rule,
        RuleEntry, RuleVariant, Schema, TableEntry, Term, ValueEntry,
    };

    fn int_col_type() -> ColType {
        ColType::BuiltinTy {
            builtin_ty: BuiltinTy::BuiltinInt,
        }
    }

    fn int_entity(col_names: &[&str]) -> Schema {
        Schema {
            entity_variant: EntityVariant::Table,
            columns: col_names
                .iter()
                .map(|name| ColumnEntry {
                    path: Path::from(*name),
                    col_type: int_col_type(),
                })
                .collect(),
            primary_key: None,
        }
    }

    pub fn link_foreign_key_theory() -> FlatRealm {
        let left = Path::from("Left");
        let right = Path::from("Right");
        let link = Path::from("Link");
        FlatRealm {
            tables: vec![
                TableEntry {
                    path: left.clone(),
                    table: int_entity(&["x"]),
                },
                TableEntry {
                    path: right.clone(),
                    table: int_entity(&["x"]),
                },
                TableEntry {
                    path: link.clone(),
                    table: int_entity(&["a", "b"]),
                },
            ],
            rules: vec![RuleEntry {
                path: Path::from("Link.foreignKeys"),
                rule: Rule {
                    rule_variant: RuleVariant::Enforced,
                    var_names: vec![Path::from("a"), Path::from("b")],
                    var_types: vec![int_col_type(), int_col_type()],
                    antecedents: vec![Prop::Atom {
                        atom: Atom {
                            entity: link.clone(),
                            row_id: None,
                            values: vec![
                                ValueEntry {
                                    column: 0,
                                    term: Term::Var { index: 0 },
                                },
                                ValueEntry {
                                    column: 1,
                                    term: Term::Var { index: 1 },
                                },
                            ],
                        },
                    }],
                    consequents: vec![
                        Prop::Atom {
                            atom: Atom {
                                entity: left.clone(),
                                row_id: None,
                                values: vec![ValueEntry {
                                    column: 0,
                                    term: Term::Var { index: 0 },
                                }],
                            },
                        },
                        Prop::Atom {
                            atom: Atom {
                                entity: right.clone(),
                                row_id: None,
                                values: vec![ValueEntry {
                                    column: 0,
                                    term: Term::Var { index: 1 },
                                }],
                            },
                        },
                    ],
                },
            }],
        }
    }
}

use super::*;
use crate::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, RuleVariant, Schema};

fn single_int_store() -> Store {
    let path = Path::from("T");
    let schema = Schema {
        entity_variant: EntityVariant::Table,
        columns: vec![ColumnEntry {
            path: Path::from("c0"),
            col_type: ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinInt,
            },
        }],
        primary_key: None,
    };
    let mut store = Store::new();
    store.create_table(path, schema).expect("create test table");
    store
}

fn commit_int(store: &mut Store, value: i32) -> CommitHash {
    let path = Path::from("T");
    let mut tx = store.transaction();
    tx.add(&path, vec![value.into()]).expect("add row");
    tx.commit().expect("commit row")
}

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

mod transactions {
    use super::test_support::link_foreign_key_theory;
    use super::*;

    #[test]
    fn validates_then_applies() {
        let path = Path::from("T");
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");

        let mut txn = store.transaction();
        txn.add(&path, vec![1i32.into()]).expect("first add");
        txn.add(&path, vec![2i32.into()]).expect("second add");

        txn.commit().expect("commit");

        assert_eq!(store.table_at(&path).expect("T").row_count(), 2);
    }

    /// Covers the same rollback guarantee as the old `transact` test: if validation fails,
    /// no rows from the batch are committed (here the second op references an unregistered table).
    #[test]
    fn unknown_table_leaves_store_unchanged() {
        let path = Path::from("T");
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");

        let err = {
            let mut txn = store.transaction();
            txn.add(&path, vec![1i32.into()]).expect("first add");
            txn.add(&Path::from("missing"), vec![2i32.into()])
                .unwrap_err()
        };

        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::UnknownTable { .. })
        ));
        assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
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
            primary_key: Some(vec![Path::from("c0")]),
        };
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");

        let mut txn = store.transaction();
        txn.add(&path, vec![1i32.into()]).expect("first add");
        txn.add(&path, vec![1i32.into()]).expect("second add");
        let err = txn.commit().unwrap_err();

        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::DuplicatePrimaryKey)
        ));
        assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
    }

    #[test]
    fn single_insert_commits() {
        let path = Path::from("T");
        let schema = Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: None,
        };
        let mut store = Store::new();
        store
            .create_table(path.clone(), schema)
            .expect("create table");

        let mut txn = store.transaction();
        txn.add(&path, vec![42i32.into()]).expect("add");
        txn.commit().expect("commit");

        let t = store.table_at(&path).expect("T");
        assert_eq!(t.row_count(), 1);
        assert_eq!(t.cell_at(0, 0), Some(42i32.into()));
    }

    #[test]
    fn leaves_store_unchanged_when_rules_fail() {
        let theory = link_foreign_key_theory();
        let link = Path::from("Link");
        let mut store = Store::try_from_ir(theory).expect("theory");
        let packed_id_count = store.id_packer.len();

        let mut txn = store.transaction();
        txn.add(&link, vec![10i32.into(), 20i32.into()])
            .expect("add");
        let err = txn.commit().unwrap_err();

        assert!(matches!(err, StoreError::Rule(_)));
        assert_eq!(store.table_at(&link).expect("Link").row_count(), 0);
        assert_eq!(store.id_packer.len(), packed_id_count);
    }

    #[test]
    fn owned_transaction_commit_err_returns_original_store() {
        let theory = link_foreign_key_theory();
        let link = Path::from("Link");
        let store = Store::try_from_ir(theory).expect("theory");

        let mut tx = OwnedTransaction::new(store);
        tx.add(&link, vec![10_i32.into(), 20_i32.into()])
            .expect("add");

        let (err, recovered) = tx.commit().unwrap_err();
        assert!(matches!(err, StoreError::Rule(_)));
        assert_eq!(recovered.table_at(&link).expect("Link").row_count(), 0);
    }
}

mod query {
    use super::*;

    #[test]
    fn scan_table_returns_rows_for_known_table() {
        let path = Path::from("T");
        let mut store = single_int_store();

        assert_eq!(
            store
                .scan_table(&path)
                .expect("known table")
                .collect::<Vec<_>>(),
            vec![]
        );
        assert!(store.scan_table(&Path::from("missing")).is_none());

        let commit = commit_int(&mut store, 42);

        assert_eq!(
            store
                .scan_table(&path)
                .expect("known table")
                .collect::<Vec<_>>(),
            vec![RowView {
                row_id: WireRowId { commit, counter: 0 },
                values: vec![42i32.into()],
            }]
        );
    }

    #[test]
    fn row_by_id_finds_committed_row() {
        let path = Path::from("T");
        let mut store = single_int_store();
        let commit = commit_int(&mut store, 42);
        let row_id = WireRowId { commit, counter: 0 };

        assert_eq!(
            store.row_by_id(&path, row_id),
            Some(RowView {
                row_id,
                values: vec![42i32.into()],
            })
        );
        assert_eq!(
            store.row_by_id(&path, WireRowId { commit, counter: 1 }),
            None
        );
        assert_eq!(store.row_by_id(&Path::from("missing"), row_id), None);
    }
}

mod rowing {
    use super::*;
    use crate::txn::TxnLiveValue;

    fn row_id_from(commit_byte: u8, counter: u32) -> WireRowId {
        WireRowId {
            commit: CommitHash([commit_byte; 32]),
            counter,
        }
    }

    /// Store with a structural `Term` table (one int column), a structural
    /// `Plus` table (two id columns), and a non-structural `Note` table (one
    /// id column).
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
        let schema = |columns: Vec<ColumnEntry>| Schema {
            entity_variant: EntityVariant::Table,
            columns,
            primary_key: None,
        };

        let mut store = Store::new();
        for (path, table_schema, structural) in [
            ("Term", schema(vec![int_col("value")]), true),
            (
                "Plus",
                schema(vec![id_col("left", "Term"), id_col("right", "Term")]),
                true,
            ),
            (
                "Mult",
                schema(vec![id_col("left", "Term"), id_col("right", "Term")]),
                true,
            ),
            ("Note", schema(vec![id_col("term", "Term")]), false),
        ] {
            store
                .create_table(Path::from(path), table_schema)
                .expect("create table");
            store.set_structural_index_for_test(&Path::from(path), structural);
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
                    entity_variant: EntityVariant::Table,
                    columns: vec![int_col("value")],
                    primary_key: None,
                },
            ),
            (
                "F",
                Schema {
                    entity_variant: EntityVariant::Table,
                    columns: vec![id_col("x", "Term"), id_col("y", "Term")],
                    primary_key: Some(vec![Path::from("x")]),
                },
            ),
        ] {
            store
                .create_table(Path::from(path), table_schema)
                .expect("create table");
            store.set_structural_index_for_test(&Path::from(path), true);
        }
        store
    }

    fn add_op(store: &Store, table: &str, rid: WireRowId, values: Vec<WireValue>) -> Op {
        Op::Add {
            row_id: rid,
            table: store
                .resolve_table(&Path::from(table))
                .expect("test table exists"),
            values,
        }
    }

    /// When a smaller structurally equal row swaps a class's canonical id, the
    /// rebuild renames the row in its own table and rewrites the id cells of
    /// every table that references it.
    #[test]
    fn swap_rewrites_referencing_table_cells() {
        let mut store = structural_store();

        let t_high = row_id_from(2, 0);
        store
            .apply_ops_and_rebuild(vec![add_op(
                &store,
                "Term",
                t_high,
                vec![WireValue::Int(7)],
            )])
            .unwrap();

        let plus = row_id_from(3, 0);
        let note = row_id_from(4, 0);
        store
            .apply_ops_and_rebuild(vec![
                add_op(
                    &store,
                    "Plus",
                    plus,
                    vec![WireValue::Id(t_high), WireValue::Id(t_high)],
                ),
                add_op(&store, "Note", note, vec![WireValue::Id(t_high)]),
            ])
            .unwrap();

        // A smaller equal term swaps the class canonical from t_high to t_low.
        let t_low = row_id_from(1, 0);
        store
            .apply_ops_and_rebuild(vec![add_op(&store, "Term", t_low, vec![WireValue::Int(7)])])
            .unwrap();

        // The stored row is now t_low; the stale id t_high resolves to it.
        let term_path = Path::from("Term");
        let term_view = Some(RowView {
            row_id: t_low,
            values: vec![WireValue::Int(7)],
        });
        assert_eq!(store.row_by_id(&term_path, t_low), term_view);
        assert_eq!(store.row_by_id(&term_path, t_high), term_view);
        // An id that was never observed still misses.
        assert_eq!(store.row_by_id(&term_path, row_id_from(9, 0)), None);

        // Both referencing tables now name the new canonical id.
        assert_eq!(
            store.row_by_id(&Path::from("Plus"), plus),
            Some(RowView {
                row_id: plus,
                values: vec![WireValue::Id(t_low), WireValue::Id(t_low)],
            })
        );
        assert_eq!(
            store.row_by_id(&Path::from("Note"), note),
            Some(RowView {
                row_id: note,
                values: vec![WireValue::Id(t_low)],
            })
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
        store
            .apply_ops_and_rebuild(vec![
                add_op(&store, "Term", t_low, vec![WireValue::Int(7)]),
                add_op(&store, "Term", t_high, vec![WireValue::Int(7)]),
                add_op(
                    &store,
                    "Plus",
                    keep,
                    vec![WireValue::Id(t_high), WireValue::Id(t_high)],
                ),
                add_op(
                    &store,
                    "Plus",
                    dup,
                    vec![WireValue::Id(t_high), WireValue::Id(t_high)],
                ),
            ])
            .expect("duplicates merge rather than failing the commit");

        let terms: Vec<RowView> = store.scan_table(&Path::from("Term")).unwrap().collect();
        let plus: Vec<RowView> = store.scan_table(&Path::from("Plus")).unwrap().collect();
        assert_eq!(terms.len(), 1);
        assert_eq!(plus.len(), 1);

        // The surviving row keeps the canonical id and names canonical children.
        assert_eq!(plus[0].row_id, keep);
        assert_eq!(plus[0].values, [WireValue::Id(t_low), WireValue::Id(t_low)]);
        // Both stale ids still resolve to what replaced them.
        let plus_path = Path::from("Plus");
        assert_eq!(
            store.row_by_id(&plus_path, dup),
            store.row_by_id(&plus_path, keep)
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
        store
            .apply_ops_and_rebuild(vec![
                add_op(&store, "Term", t_low, vec![WireValue::Int(7)]),
                add_op(&store, "Term", u_low, vec![WireValue::Int(8)]),
                add_op(&store, "Term", t_high, vec![WireValue::Int(7)]),
                add_op(&store, "Term", u_high, vec![WireValue::Int(8)]),
                add_op(
                    &store,
                    "Plus",
                    plus,
                    vec![WireValue::Id(t_high), WireValue::Id(u_high)],
                ),
            ])
            .expect("duplicates merge rather than failing the commit");

        let terms: Vec<RowView> = store.scan_table(&Path::from("Term")).unwrap().collect();
        assert_eq!(terms.len(), 2);
        assert_eq!(
            store.row_by_id(&Path::from("Plus"), plus),
            Some(RowView {
                row_id: plus,
                values: vec![WireValue::Id(t_low), WireValue::Id(u_low)],
            })
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
        let t7 = txn.add(&term_path, vec![TxnLiveValue::Int(7)]).unwrap();
        let t8 = txn.add(&term_path, vec![TxnLiveValue::Int(8)]).unwrap();
        let tp = txn
            .add(&plus_path, vec![TxnLiveValue::Id(t7), TxnLiveValue::Id(t8)])
            .unwrap();
        txn.add(
            &mult_path,
            vec![TxnLiveValue::Id(tp.clone()), TxnLiveValue::Id(tp)],
        )
        .unwrap();
        txn.commit().unwrap();

        let mut txn2 = store.transaction();
        let t7 = txn2.add(&term_path, vec![TxnLiveValue::Int(7)]).unwrap();
        let t8 = txn2.add(&term_path, vec![TxnLiveValue::Int(8)]).unwrap();
        let tp = txn2
            .add(&plus_path, vec![TxnLiveValue::Id(t7), TxnLiveValue::Id(t8)])
            .unwrap();
        txn2.add(
            &mult_path,
            vec![TxnLiveValue::Id(tp.clone()), TxnLiveValue::Id(tp)],
        )
        .unwrap();
        txn2.commit().unwrap();

        let terms: Vec<RowView> = store.scan_table(&term_path).unwrap().collect();
        let plus: Vec<RowView> = store.scan_table(&plus_path).unwrap().collect();
        let mult: Vec<RowView> = store.scan_table(&mult_path).unwrap().collect();

        // The second commit adds no rows: every row it names is structurally
        // identical to one the first commit already stored.
        assert_eq!(terms.len(), 2);
        assert_eq!(plus.len(), 1);
        assert_eq!(mult.len(), 1);

        let term_id = |value: i32| {
            let matching: Vec<&RowView> = terms
                .iter()
                .filter(|row| row.values == [value.into()])
                .collect();
            assert_eq!(matching.len(), 1, "exactly one Term({value})");
            matching[0].row_id
        };
        let t7 = term_id(7);
        let t8 = term_id(8);

        // Each surviving row references the canonical id of its children, not the
        // duplicate the second commit allocated for them.
        assert_eq!(plus[0].values, [WireValue::Id(t7), WireValue::Id(t8)]);
        assert_eq!(
            mult[0].values,
            [WireValue::Id(plus[0].row_id), WireValue::Id(plus[0].row_id)]
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
        let t1 = first
            .add(&term, vec![TxnLiveValue::Int(1)])
            .expect("Term(1)");
        let t2 = first
            .add(&term, vec![TxnLiveValue::Int(2)])
            .expect("Term(2)");
        first
            .add(&term, vec![TxnLiveValue::Int(3)])
            .expect("Term(3)");
        first
            .add(&f, vec![TxnLiveValue::Id(t1), TxnLiveValue::Id(t2)])
            .expect("F(Term1, Term2)");
        first.commit().expect("x is mapped only once");

        let terms_before = store.scan_table(&term).expect("Term").count();
        let f_before = store.scan_table(&f).expect("F").collect::<Vec<RowView>>();
        assert_eq!(terms_before, 3);
        assert_eq!(f_before.len(), 1);

        // Term(1) is added again under a fresh row id, so F(Term1, Term4) still
        // looks unique on x when it is inserted. The conflict only appears once
        // the two Term(1) rows canonicalise onto one id and F's x cell is
        // rewritten, which is why the check cannot live in the pre-apply pass.
        let mut second = store.transaction();
        let t1_again = second
            .add(&term, vec![TxnLiveValue::Int(1)])
            .expect("Term(1)");
        let t4 = second
            .add(&term, vec![TxnLiveValue::Int(4)])
            .expect("Term(4)");
        second
            .add(&f, vec![TxnLiveValue::Id(t1_again), TxnLiveValue::Id(t4)])
            .expect("F(Term1, Term4)");

        let err = second.commit().unwrap_err();
        assert!(matches!(
            err,
            StoreError::Validation(ValidationError::DuplicatePrimaryKey)
        ));

        // The rejected commit rolls back whole, including Term(4), which was
        // legal on its own.
        assert_eq!(store.scan_table(&term).expect("Term").count(), terms_before);
        assert_eq!(
            store.scan_table(&f).expect("F").collect::<Vec<RowView>>(),
            f_before
        );
    }
}

mod commits {
    use super::*;

    #[test]
    fn heads_and_commit_by_hash_track_current_frontier() {
        let mut store = single_int_store();
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

    #[test]
    fn commits_after_returns_descendants_in_topological_order() {
        let mut store = single_int_store();
        let root = store.heads();
        let first = commit_int(&mut store, 1);
        let second = commit_int(&mut store, 2);

        let commits = store.commits_after(&root);
        let hashes = commits.iter().map(Commit::hash).collect::<Vec<_>>();

        assert_eq!(hashes, vec![first, second]);
        assert!(store.commits_after(&store.heads()).is_empty());
    }

    #[test]
    fn commits_added_returns_commits_in_other_store() {
        let base = single_int_store();
        let mut other = single_int_store();
        let commit = commit_int(&mut other, 7);

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

        let (restored, pending) = Store::try_from_commit_bytes(chunks).expect("store from chunks");

        assert!(pending.is_empty());
        assert_eq!(restored.table_count(), 0);
        assert_eq!(restored.heads(), source.heads());
    }

    #[test]
    fn commit_chunks_create_store_from_out_of_order_data() {
        let mut source = single_int_store();
        let commit = commit_int(&mut source, 99);
        let mut chunks = source
            .commit_chunks_after(&[])
            .into_iter()
            .map(|chunk| chunk.bytes)
            .collect::<Vec<_>>();
        chunks.reverse();

        let (restored, pending) = Store::try_from_commit_bytes(chunks).expect("store from chunks");

        assert!(pending.is_empty());
        let table = restored.table_at(&Path::from("T")).expect("table");
        assert_eq!(table.cell_at(0, 0), Some(WireValue::Int(99)));
        assert_eq!(restored.heads(), vec![commit]);
    }

    #[test]
    fn apply_commits_applies_rows_and_updates_heads() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        let commit = commit_int(&mut source, 99);

        let commits = source.commits_after(&target.heads());
        target.apply_commits(commits).expect("apply commits");

        let table = target.table_at(&Path::from("T")).expect("table");
        assert_eq!(table.row_count(), 1);
        assert_eq!(table.cell_at(0, 0), Some(WireValue::Int(99)));
        assert_eq!(table.row_id_at(0).expect("row id").commit, commit);
        assert_eq!(target.heads(), source.heads());
    }

    #[test]
    fn apply_commits_accepts_out_of_order_input() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        commit_int(&mut source, 1);
        commit_int(&mut source, 2);

        let mut commits = source.commits_after(&target.heads());
        commits.reverse();
        target.apply_commits(commits).expect("apply commits");

        let table = target.table_at(&Path::from("T")).expect("table");
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.cell_at(0, 0), Some(WireValue::Int(1)));
        assert_eq!(table.cell_at(1, 0), Some(WireValue::Int(2)));
        assert_eq!(target.heads(), source.heads());
    }

    #[test]
    fn apply_commits_ignores_known_commits() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        commit_int(&mut source, 5);

        let commits = source.commits_after(&target.heads());
        target
            .apply_commits(commits.clone())
            .expect("first apply commits");
        target.apply_commits(commits).expect("second apply commits");

        assert_eq!(
            target
                .table_at(&Path::from("T"))
                .expect("table")
                .row_count(),
            1
        );
    }

    #[test]
    fn apply_commits_skips_missing_dependency_without_changing_store() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        commit_int(&mut source, 1);
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
        assert_eq!(
            target
                .table_at(&Path::from("T"))
                .expect("table")
                .row_count(),
            0
        );
    }

    #[test]
    fn apply_commits_applies_ready_commits_and_returns_blocked_ones() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        let first = commit_int(&mut source, 1);
        commit_int(&mut source, 2);
        let third = commit_int(&mut source, 3);
        let first_commit = source.commit_by_hash(&first).expect("first commit").clone();
        let third_commit = source.commit_by_hash(&third).expect("third commit").clone();

        let leftover = target
            .apply_commits([first_commit, third_commit])
            .expect("apply ready commits");
        let leftover_hashes: Vec<_> = leftover.iter().map(Commit::hash).collect();

        assert_eq!(leftover_hashes, vec![third]);
        assert_eq!(
            target
                .table_at(&Path::from("T"))
                .expect("table")
                .row_count(),
            1
        );
        assert_eq!(target.heads(), vec![first]);
    }

    #[test]
    fn apply_chunk_bytes_retries_leftover_when_missing_parent_arrives() {
        let mut source = single_int_store();
        let mut target = single_int_store();
        commit_int(&mut source, 1);
        let second = commit_int(&mut source, 2);

        let chunks: Vec<Vec<u8>> = source
            .commit_chunks_after(&target.heads())
            .into_iter()
            .map(|chunk| chunk.bytes)
            .collect();
        assert_eq!(chunks.len(), 2);

        let leftover = target
            .apply_chunk_bytes([chunks[1].clone()])
            .expect("skip child without parent");
        assert_eq!(leftover.len(), 1);
        assert_eq!(
            target
                .table_at(&Path::from("T"))
                .expect("table")
                .row_count(),
            0
        );

        let leftover = target
            .apply_chunk_bytes(
                leftover
                    .into_iter()
                    .chain(std::iter::once(chunks[0].clone())),
            )
            .expect("retry leftover with parent");
        assert!(leftover.is_empty());

        let table = target.table_at(&Path::from("T")).expect("table");
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.cell_at(0, 0), Some(1i32.into()));
        assert_eq!(table.cell_at(1, 0), Some(2i32.into()));
        assert_eq!(target.heads(), vec![second]);
    }
}

mod errors {
    use super::*;

    #[test]
    fn apply_error_from_inner_errors() {
        let validation = StoreError::from(ValidationError::DuplicatePrimaryKey);
        assert!(matches!(
            validation,
            StoreError::Validation(ValidationError::DuplicatePrimaryKey)
        ));

        let compile = StoreError::from(CompileError::UnsupportedTerm);
        assert!(matches!(
            compile,
            StoreError::Compile(CompileError::UnsupportedTerm)
        ));

        let compiled_rule = solver::compile::CompRule {
            path: Path::from("T.total"),
            rule_variant: RuleVariant::Enforced,
            vars: vec![],
            antecedent: solver::compile::CompProp::And(vec![]),
            consequent: solver::compile::CompProp::And(vec![]),
            tables: vec![Path::from("T")],
        };
        let violation = RuleViolation {
            rule: compiled_rule,
            cause: solver::validate::ViolationCause::MissingAtom(solver::compile::CompAtom {
                table: Path::from("T"),
                row_id: None,
                values: vec![],
            }),
            binding: vec![],
        };
        let rule = StoreError::from(Box::new(violation));
        assert!(matches!(rule, StoreError::Rule(_)));
    }
}
