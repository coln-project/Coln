// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{engine::schema::ColnDef, ir, public::PublicScalarValue};
use coln_store::{
    store::{Store, commit::CommitChunk, error::StoreError},
    table::ValidationError,
    txn::rw::StoreWrite,
};
use rstest::{fixture, rstest};

use crate::{
    bouncer::Bouncer,
    error::{BouncerError, RuleViolation},
    rw::{DbRead, DbWrite},
};

#[fixture]
fn only_ones_realm() -> ir::FlatRealm {
    let int = ir::ColType::BuiltinTy {
        builtin_ty: ir::BuiltinTy::BuiltinInt,
    };
    let row = ir::Prop::Atom {
        atom: ir::Atom {
            entity: "t".into(),
            row_id: None,
            values: vec![ir::ValueEntry {
                column: 0,
                term: ir::El::Var { index: 0 },
            }],
        },
    };
    // t(value = x) => t(value = x) and x == 1.
    ir::FlatRealm {
        tables: vec![ir::TableEntry {
            path: "t".into(),
            table: ir::Schema {
                entity_variant: ir::EntityVariant::Table,
                columns: vec![ir::ColumnEntry {
                    path: "value".into(),
                    col_type: int.clone(),
                }],
                primary_key: None,
            },
        }],
        definitions: vec![],
        rules: vec![ir::RuleEntry {
            path: "only_ones".into(),
            rule: ir::Rule {
                rule_variant: ir::RuleVariant::Enforced,
                vars: vec![("x".into(), int)],
                antecedents: vec![row.clone()],
                consequents: vec![
                    row,
                    ir::Prop::Eq {
                        equality: ir::Equality {
                            left: ir::El::Var { index: 0 },
                            right: ir::El::Lit {
                                lit: ir::Lit::Int { value: 1 },
                            },
                        },
                    },
                ],
            },
        }],
    }
}

#[fixture]
fn coln_def() -> ColnDef {
    ColnDef {
        theory: String::new(),
        realm: "OnlyOnes".into(),
    }
}

#[fixture]
fn only_ones(only_ones_realm: ir::FlatRealm, coln_def: ColnDef) -> Bouncer {
    Bouncer::try_new(only_ones_realm, coln_def).expect("create bouncer with an enforced rule")
}

// Plain store transactions let incoming commits violate query rules while
// retaining the same realm and root as the receiving bouncer.
fn plain_commits(realm: &ir::FlatRealm, coln_def: ColnDef, values: &[i32]) -> Vec<CommitChunk> {
    let mut source = Store::try_from_ir(realm, coln_def).expect("source store");
    let heads = source.heads();
    for value in values {
        let mut txn = source.transaction();
        txn.add(&ir::Path::from("t"), vec![*value])
            .expect("add row");
        txn.commit().expect("commit without query rules");
    }
    source.commit_chunks_after(&heads)
}

#[rstest]
fn rejected_commit_rolls_back_store_and_allows_next_transaction(mut only_ones: Bouncer) {
    let table = ir::Path::from("t");
    only_ones.transaction();
    only_ones.add(&table, vec![1i32]).expect("add initial row");
    let initial = only_ones.commit().expect("commit initial row");
    let before = only_ones.scan_table(&table).expect("table exists");
    assert_eq!(before.len(), 1);

    only_ones.transaction();
    only_ones.add(&table, vec![1i32]).expect("add valid row");
    only_ones
        .add(&table, vec![2i32])
        .expect("add violating row");
    let violations = match only_ones.commit() {
        Err(BouncerError::Rule(RuleViolation::HardViolation(violations))) => violations,
        Err(error) => panic!("expected a hard constraint violation, got {error}"),
        Ok(_) => panic!("the enforced rule should reject value 2"),
    };
    assert!(!violations.is_empty());
    assert_eq!(
        violations
            .iter()
            .next()
            .expect("violated rule")
            .for_entity(),
        &ir::Path::from("only_ones"),
    );
    assert_eq!(only_ones.scan_table(&table).expect("table exists"), before);
    // This also requires AutoStore to have returned to Idle after rejection.
    assert!(
        only_ones
            .commit_chunks_after(&[initial.cmt_hash])
            .is_empty()
    );

    only_ones.transaction();
    only_ones
        .add(&table, vec![1i32])
        .expect("add next valid row");
    let next = only_ones.commit().expect("commit after rejection");
    let after = only_ones.scan_table(&table).expect("table exists");
    assert_eq!(after.len(), 2);
    assert!(after.contains(&before[0]));
    let added = only_ones.commit_chunks_after(&[initial.cmt_hash]);
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].hash, next.cmt_hash);
    assert_eq!(added[0].parents, vec![initial.cmt_hash]);
}

#[rstest]
fn failed_local_validation_does_not_poison_sync(
    mut only_ones_realm: ir::FlatRealm,
    coln_def: ColnDef,
) {
    only_ones_realm.tables[0].table.primary_key = Some(vec![0]);
    only_ones_realm.rules.clear();
    let mut target = Bouncer::try_new(only_ones_realm.clone(), coln_def.clone())
        .expect("bouncer with a primary key");
    let table = ir::Path::from("t");

    target.transaction();
    target.add(&table, vec![1i32]).expect("initial row");
    target.commit().expect("initial commit");
    let before = target.scan_table(&table).expect("table");

    target.transaction();
    target.add(&table, vec![1i32]).expect("duplicate row");
    assert!(matches!(
        target.commit(),
        Err(BouncerError::StoreError(StoreError::Validation(
            ValidationError::DuplicatePrimaryKey
        )))
    ));
    target.abort();
    assert_eq!(target.scan_table(&table).expect("table"), before);

    // Even an empty delivery must not retry the aborted local transaction.
    assert!(target.apply_chunk_bytes([]).expect("empty sync").is_empty());
    let incoming = plain_commits(&only_ones_realm, coln_def, &[2]);
    target
        .apply_chunk_bytes(incoming.into_iter().map(|chunk| chunk.bytes))
        .expect("valid sync after local failure");
    let rows = target.scan_table(&table).expect("table");
    assert_eq!(rows.len(), 2);
    assert!(rows.contains(&before[0]));
    assert!(rows.iter().any(|row| row[1] == PublicScalarValue::I32(2)));
}

#[rstest]
fn sync_hard_rejection_preserves_earlier_commits_and_blocks_descendants(
    only_ones_realm: ir::FlatRealm,
    coln_def: ColnDef,
    mut only_ones: Bouncer,
) {
    let chunks = plain_commits(&only_ones_realm, coln_def, &[1, 2, 1]);
    let root = only_ones.commit_chunks_after(&[])[0].hash;
    let table = ir::Path::from("t");

    assert!(matches!(
        only_ones.apply_chunk_bytes(chunks.iter().map(|chunk| chunk.bytes.clone())),
        Err(BouncerError::Rule(RuleViolation::HardViolation(violations)))
            if !violations.is_empty()
    ));
    let rows = only_ones.scan_table(&table).expect("table");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][1], PublicScalarValue::I32(1));
    let applied = only_ones.commit_chunks_after(&[root]);
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].hash, chunks[0].hash);

    // The rejected commit is discarded, and its descendant remains blocked.
    assert!(
        only_ones
            .apply_chunk_bytes([])
            .expect("retry pending")
            .is_empty()
    );
    assert_eq!(only_ones.scan_table(&table).expect("table"), rows);

    only_ones.transaction();
    only_ones.add(&table, vec![1i32]).expect("valid local row");
    let local = only_ones
        .commit()
        .expect("both engines recovered after rejection");
    assert_eq!(only_ones.scan_table(&table).expect("table").len(), 2);
    let added = only_ones.commit_chunks_after(&[chunks[0].hash]);
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].hash, local.cmt_hash);
    assert_eq!(added[0].parents, vec![chunks[0].hash]);
}

#[rstest]
fn sync_soft_violation_reports_the_violation_and_applies_later_commits(
    mut only_ones_realm: ir::FlatRealm,
    coln_def: ColnDef,
) {
    only_ones_realm.rules[0].rule.rule_variant = ir::RuleVariant::Monitored;
    let chunks = plain_commits(&only_ones_realm, coln_def.clone(), &[2, 1]);
    let mut target = Bouncer::try_new(only_ones_realm, coln_def).expect("monitored realm");
    let root = target.commit_chunks_after(&[])[0].hash;

    let reports = target
        .apply_chunk_bytes(chunks.iter().map(|chunk| chunk.bytes.clone()))
        .expect("soft violation permits both commits");
    assert_eq!(reports.len(), 2);
    let violations = reports[0].iter().collect::<Vec<_>>();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].for_entity(), &ir::Path::from("only_ones"));
    assert_eq!(violations[0].delta().len(), 1);
    assert_eq!(violations[0].delta()[0].zweight(), 1);
    assert!(reports[1].is_empty());
    let mut values = target
        .scan_table(&ir::Path::from("t"))
        .expect("table")
        .into_iter()
        .map(|row| match row[1] {
            PublicScalarValue::I32(value) => value,
            ref other => panic!("expected integer, got {other:?}"),
        })
        .collect::<Vec<_>>();
    values.sort();
    assert_eq!(values, vec![1, 2]);
    let hashes = target
        .commit_chunks_after(&[root])
        .into_iter()
        .map(|chunk| chunk.hash)
        .collect::<Vec<_>>();
    assert_eq!(hashes, vec![chunks[0].hash, chunks[1].hash]);

    assert!(
        target
            .apply_chunk_bytes(chunks.into_iter().map(|chunk| chunk.bytes))
            .expect("repeated delivery")
            .is_empty()
    );
}
