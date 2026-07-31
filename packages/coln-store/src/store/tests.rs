use super::*;
use crate::{
    ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, RuleVariant, Schema},
    store::tests::test_support::link_foreign_key_theory,
};

/// Shared theory fixtures for unit tests (`store`, `transaction`, etc.).
#[cfg(test)]
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

fn commit_int(store: &mut Store, value: i64) -> CommitHash {
    let path = Path::from("T");
    let mut tx = store.transaction();
    tx.add(&path, vec![value.into()]).expect("add row");
    tx.commit().expect("commit row")
}

#[test]
fn test_store_create_table() {
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
fn test_store_resolve_table_oid() {
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

#[test]
fn transaction_validates_then_applies() {
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
    txn.add(&path, vec![CellValue::Int(1).into()])
        .expect("first add");
    txn.add(&path, vec![CellValue::Int(2).into()])
        .expect("second add");

    txn.commit().expect("commit");

    assert_eq!(store.table_at(&path).expect("T").row_count(), 2);
}

/// Covers the same rollback guarantee as the old `transact` test: if validation fails,
/// no rows from the batch are committed (here the second op references an unregistered table).
#[test]
fn transaction_unknown_table_leaves_store_unchanged() {
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
        txn.add(&path, vec![CellValue::Int(1).into()])
            .expect("first add");
        txn.add(&Path::from("missing"), vec![CellValue::Int(2).into()])
            .unwrap_err()
    };

    assert!(matches!(
        err,
        StoreIntError::Validation(ValidationError::UnknownTable { .. })
    ));
    assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
}

#[test]
fn transaction_duplicate_primary_key_within_batch() {
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
    txn.add(&path, vec![CellValue::Int(1).into()])
        .expect("first add");
    txn.add(&path, vec![CellValue::Int(1).into()])
        .expect("second add");
    let err = txn.commit().unwrap_err();

    assert!(matches!(
        err,
        StoreIntError::Validation(ValidationError::DuplicatePrimaryKey)
    ));
    assert_eq!(store.table_at(&path).expect("T").row_count(), 0);
}

#[test]
fn transaction_single_insert_commits() {
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
    txn.add(&path, vec![CellValue::Int(42).into()])
        .expect("add");
    txn.commit().expect("commit");

    let t = store.table_at(&path).expect("T");
    assert_eq!(t.row_count(), 1);
    assert_eq!(t.cell_at(0, 0), Some(CellValue::Int(42)));
}

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
            row_id: RowId { commit, counter: 0 },
            values: vec![CellValue::Int(42)],
        }]
    );
}

#[test]
fn row_by_id_finds_committed_row() {
    let path = Path::from("T");
    let mut store = single_int_store();
    let commit = commit_int(&mut store, 42);
    let row_id = RowId { commit, counter: 0 };

    assert_eq!(
        store.row_by_id(&path, row_id),
        Some(RowView {
            row_id,
            values: vec![CellValue::Int(42)],
        })
    );
    assert_eq!(store.row_by_id(&path, RowId { commit, counter: 1 }), None);
    assert_eq!(store.row_by_id(&Path::from("missing"), row_id), None);
}

fn row_id_from(commit_byte: u8, counter: u32) -> RowId {
    RowId {
        commit: CommitHash([commit_byte; 32]),
        counter,
    }
}

/// Store with a hashcons `Term` table (one int column), a hashcons
/// `Plus` table (two id columns), and a non-hashcons `Note` table (one
/// id column).
fn hashcons_store() -> Store {
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
    for (path, table_schema, hashcons) in [
        ("Term", schema(vec![int_col("value")]), true),
        (
            "Plus",
            schema(vec![id_col("left", "Term"), id_col("right", "Term")]),
            true,
        ),
        ("Note", schema(vec![id_col("term", "Term")]), false),
    ] {
        store
            .create_table(Path::from(path), table_schema)
            .expect("create table");
        store.set_hashcons_for_test(&Path::from(path), hashcons);
    }
    store
}

fn add_op(table: &str, rid: RowId, values: Vec<CellValue>) -> Op {
    Op::Add {
        row_id: rid,
        table: Path::from(table),
        values,
    }
}

/// When a smaller structurally equal row swaps a class's canonical id,
/// `apply_batch` renames the row in its own table and rewrites the id
/// cells of every table that references it.
#[test]
#[ignore = "fails until rowing"]
fn swap_rewrites_referencing_table_cells() {
    let mut store = hashcons_store();

    let t_high = row_id_from(2, 0);
    store.apply_commit_ops(vec![add_op("Term", t_high, vec![CellValue::Int(7)])]);

    let plus = row_id_from(3, 0);
    let note = row_id_from(4, 0);
    store.apply_commit_ops(vec![
        add_op(
            "Plus",
            plus,
            vec![CellValue::Id(t_high), CellValue::Id(t_high)],
        ),
        add_op("Note", note, vec![CellValue::Id(t_high)]),
    ]);

    // A smaller equal term swaps the class canonical from t_high to t_low.
    let t_low = row_id_from(1, 0);
    store.apply_commit_ops(vec![add_op("Term", t_low, vec![CellValue::Int(7)])]);

    // The stored row is now t_low; the stale id t_high resolves to it.
    let term_path = Path::from("Term");
    let term_view = Some(RowView {
        row_id: t_low,
        values: vec![CellValue::Int(7)],
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
            values: vec![CellValue::Id(t_low), CellValue::Id(t_low)],
        })
    );
    assert_eq!(
        store.row_by_id(&Path::from("Note"), note),
        Some(RowView {
            row_id: note,
            values: vec![CellValue::Id(t_low)],
        })
    );
}

/// Rows referencing a deduplicated (kept-old) member store the canonical
/// id, not the member id, which names no stored row.
#[test]
#[ignore = "fails because rowing not implemented"]
fn insert_stores_canonical_child_ids() {
    let mut store = hashcons_store();

    let t_a = row_id_from(1, 0);
    let t_b = row_id_from(2, 0);
    store.apply_commit_ops(vec![
        add_op("Term", t_a, vec![CellValue::Int(7)]),
        add_op("Term", t_b, vec![CellValue::Int(7)]),
    ]);

    // t_b deduplicated into t_a's class and is not stored; looking it up
    // resolves to the canonical row.
    let term_path = Path::from("Term");
    assert_eq!(store.table_at(&term_path).expect("Term").row_count(), 1);
    assert_eq!(
        store.row_by_id(&term_path, t_b),
        Some(RowView {
            row_id: t_a,
            values: vec![CellValue::Int(7)],
        })
    );

    let plus = row_id_from(3, 0);
    store.apply_commit_ops(vec![add_op(
        "Plus",
        plus,
        vec![CellValue::Id(t_b), CellValue::Id(t_b)],
    )]);

    assert_eq!(
        store.row_by_id(&Path::from("Plus"), plus),
        Some(RowView {
            row_id: plus,
            values: vec![CellValue::Id(t_a), CellValue::Id(t_a)],
        })
    );
}

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
fn apply_commits_applies_rows_and_updates_heads() {
    let mut source = single_int_store();
    let mut target = single_int_store();
    let commit = commit_int(&mut source, 99);

    let commits = source.commits_after(&target.heads());
    target.apply_commits(commits).expect("apply commits");

    let table = target.table_at(&Path::from("T")).expect("table");
    assert_eq!(table.row_count(), 1);
    assert_eq!(table.cell_at(0, 0), Some(CellValue::Int(99)));
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
    assert_eq!(table.cell_at(0, 0), Some(CellValue::Int(1)));
    assert_eq!(table.cell_at(1, 0), Some(CellValue::Int(2)));
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
fn apply_commits_rejects_missing_dependency_without_changing_store() {
    let mut source = single_int_store();
    let mut target = single_int_store();
    commit_int(&mut source, 1);
    let second = commit_int(&mut source, 2);
    let second_commit = source
        .commit_by_hash(&second)
        .expect("second commit")
        .clone();

    let err = target.apply_commits([second_commit]).unwrap_err();

    assert!(matches!(
        err,
        StoreIntError::Commit(CommitApplyError::MissingDep)
    ));
    assert_eq!(
        target
            .table_at(&Path::from("T"))
            .expect("table")
            .row_count(),
        0
    );
}

#[test]
fn transaction_leaves_store_unchanged_when_rules_fail() {
    let theory = link_foreign_key_theory();
    let link = Path::from("Link");
    let mut store = Store::try_from_theory(theory).expect("theory");
    let packed_id_count = store.id_packer.len();

    let mut txn = store.transaction();
    txn.add(
        &link,
        vec![CellValue::Int(10).into(), CellValue::Int(20).into()],
    )
    .expect("add");
    let err = txn.commit().unwrap_err();

    assert!(matches!(err, StoreIntError::Rule(_)));
    assert_eq!(store.table_at(&link).expect("Link").row_count(), 0);
    assert_eq!(store.id_packer.len(), packed_id_count);
}

#[test]
fn owned_transaction_commit_err_returns_original_store() {
    let theory = link_foreign_key_theory();
    let link = Path::from("Link");
    let store = Store::try_from_theory(theory).expect("theory");

    let mut tx = OwnedTransaction::new(store);
    tx.add(&link, vec![10_i64.into(), 20_i64.into()])
        .expect("add");

    let (err, recovered) = tx.commit().unwrap_err();
    assert!(matches!(err, StoreIntError::Rule(_)));
    assert_eq!(recovered.table_at(&link).expect("Link").row_count(), 0);
}

#[test]
fn apply_error_from_inner_errors() {
    let validation = StoreIntError::from(ValidationError::DuplicatePrimaryKey);
    assert!(matches!(
        validation,
        StoreIntError::Validation(ValidationError::DuplicatePrimaryKey)
    ));

    let compile = StoreIntError::from(CompileError::UnsupportedTerm);
    assert!(matches!(
        compile,
        StoreIntError::Compile(CompileError::UnsupportedTerm)
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
    let rule = StoreIntError::from(Box::new(violation));
    assert!(matches!(rule, StoreIntError::Rule(_)));
}
