// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::{
    Atom, BuiltinTy, ColType, ColumnEntry, EntityVariant, FlatRealm, Path, Prop, Rule, RuleEntry,
    RuleVariant, Schema, TableEntry, Term, ValueEntry,
};
use rstest::fixture;

use crate::{commit::hash::CommitHash, store::Store, table::WireRowId};

mod rowid {
    use super::*;

    #[fixture]
    pub(crate) fn row_id_from(
        #[default(0)] commit_byte: u8,
        #[default(0)] counter: u32,
    ) -> WireRowId {
        WireRowId {
            commit: CommitHash([commit_byte; 32]),
            counter,
        }
    }

    #[fixture]
    pub(crate) fn zerocounter_row_id(#[default(0)] byte: u8) -> WireRowId {
        row_id_from(byte, 0)
    }

    #[fixture]
    pub(crate) fn zerohash_row_id(#[default(0)] counter: u32) -> WireRowId {
        row_id_from(0, counter)
    }
}

mod schema {
    use super::*;

    #[fixture]
    pub(crate) fn int_col_type() -> ColType {
        ColType::BuiltinTy {
            builtin_ty: BuiltinTy::BuiltinInt,
        }
    }

    #[fixture]
    pub(crate) fn id_col_type(#[default(Path::from("T"))] table: Path) -> ColType {
        ColType::RowId { path: table }
    }

    fn table_schema(col_names: Vec<&'static str>, col_type: ColType) -> Schema {
        Schema {
            entity_variant: EntityVariant::Table,
            columns: col_names
                .into_iter()
                .map(|name| ColumnEntry {
                    path: Path::from(name),
                    col_type: col_type.clone(),
                })
                .collect(),
            primary_key: None,
        }
    }

    #[fixture]
    pub(crate) fn int_schema(
        #[default(vec!["x"])] col_names: Vec<&'static str>,
        int_col_type: ColType,
    ) -> Schema {
        table_schema(col_names, int_col_type)
    }

    #[fixture]
    pub(crate) fn id_schema(
        #[default(vec!["x"])] col_names: Vec<&'static str>,
        id_col_type: ColType,
    ) -> Schema {
        table_schema(col_names, id_col_type)
    }
}

mod store {
    use super::schema::{int_col_type, int_schema};
    use super::*;

    #[fixture]
    pub(crate) fn single_int_store(
        #[from(int_schema)]
        #[with(vec!["c0"])]
        schema: Schema,
    ) -> Store {
        let path = Path::from("T");
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

    #[fixture]
    pub(crate) fn link_foreign_key_theory(
        int_col_type: ColType,
        #[from(int_schema)]
        #[with(vec!["x"])]
        left_table: Schema,
        #[from(int_schema)]
        #[with(vec!["x"])]
        right_table: Schema,
        #[from(int_schema)]
        #[with(vec!["a", "b"])]
        link_table: Schema,
    ) -> FlatRealm {
        let left = Path::from("Left");
        let right = Path::from("Right");
        let link = Path::from("Link");
        FlatRealm {
            tables: vec![
                TableEntry {
                    path: left.clone(),
                    table: left_table,
                },
                TableEntry {
                    path: right.clone(),
                    table: right_table,
                },
                TableEntry {
                    path: link.clone(),
                    table: link_table,
                },
            ],
            rules: vec![RuleEntry {
                path: Path::from("Link.foreignKeys"),
                rule: Rule {
                    rule_variant: RuleVariant::Enforced,
                    var_names: vec![Path::from("a"), Path::from("b")],
                    var_types: vec![int_col_type.clone(), int_col_type],
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

    pub(crate) fn commit_int(store: &mut Store, value: i32) -> CommitHash {
        let path = Path::from("T");

        let mut tx = store.transaction();
        tx.add(&path, vec![value]).expect("add row");
        tx.commit().expect("commit row")
    }
}

pub(crate) use rowid::*;
pub(crate) use schema::*;
pub(crate) use store::*;
