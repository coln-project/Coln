// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::{engine::schema::ColnDef, ir};
use rstest::{fixture, rstest};

use crate::{
    bouncer::Bouncer,
    error::{BouncerError, RuleViolation},
    rw::{DbRead, DbWrite},
};

#[fixture]
fn only_ones() -> Bouncer {
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
    let realm = ir::FlatRealm {
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
    };
    Bouncer::try_new(
        realm,
        ColnDef {
            theory: String::new(),
            realm: "OnlyOnes".into(),
        },
    )
    .expect("create bouncer with an enforced rule")
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
