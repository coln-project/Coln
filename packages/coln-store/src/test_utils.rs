// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::{BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Schema};
use rstest::fixture;

use crate::{commit::hash::CommitHash, store::Store};

#[fixture]
pub(crate) fn single_int_store() -> Store {
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

// A single_int_store, but with a single commit added
#[fixture]
pub(crate) fn commit_int_store(
    #[default(42)] value: i32,
    mut single_int_store: Store,
) -> (Store, CommitHash) {
    let path = Path::from("T");

    let mut tx = single_int_store.transaction();
    tx.add(&path, vec![value]).expect("add row");
    let h = tx.commit().expect("commit row");

    (single_int_store, h)
}

pub(crate) fn commit_int(store: &mut Store, value: i32) -> CommitHash {
    let path = Path::from("T");

    let mut tx = store.transaction();
    tx.add(&path, vec![value]).expect("add row");
    tx.commit().expect("commit row")
}
