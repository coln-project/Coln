// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use coln_flir_rs::ir::{
    Atom, BuiltinTy, ColType, ColumnEntry, El, EntityVariant, FlatRealm, Path, Prop, Rule,
    RuleEntry, RuleVariant, Schema, TableEntry, ValueEntry,
};
use rstest::fixture;

use crate::{
    commit::{hash::CommitHash, wire::root::RootCommitData},
    store::{ColnDef, Store},
    table::WireRowId,
    txn::rw::StoreWrite,
};

mod rowid {
    use super::*;

    pub(crate) fn row_id_from(commit_byte: u8, counter: u32) -> WireRowId {
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

    fn table_schema(
        col_names: Vec<&'static str>,
        col_type: ColType,
        primary_key: Option<Vec<u64>>,
    ) -> Schema {
        Schema {
            entity_variant: EntityVariant::Table,
            columns: col_names
                .into_iter()
                .map(|name| ColumnEntry {
                    path: Path::from(name),
                    col_type: col_type.clone(),
                })
                .collect(),
            primary_key,
        }
    }

    #[fixture]
    pub(crate) fn idonly_schema(id_col_type: ColType) -> Schema {
        table_schema(vec![], id_col_type, None)
    }

    #[fixture]
    pub(crate) fn int_schema(
        #[default(vec!["x"])] col_names: Vec<&'static str>,
        #[default(None)] primary_key: Option<Vec<u64>>,
    ) -> Schema {
        table_schema(col_names, int_col_type(), primary_key)
    }

    #[fixture]
    pub(crate) fn string_schema(
        #[default(vec!["x"])] col_names: Vec<&'static str>,
        #[default(None)] primary_key: Option<Vec<u64>>,
    ) -> Schema {
        table_schema(
            col_names,
            ColType::BuiltinTy {
                builtin_ty: BuiltinTy::BuiltinStr,
            },
            primary_key,
        )
    }

    #[fixture]
    pub(crate) fn id_schema(
        #[default(vec!["x"])] col_names: Vec<&'static str>,
        #[default(None)] primary_key: Option<Vec<u64>>,
        id_col_type: ColType,
    ) -> Schema {
        table_schema(col_names, id_col_type, primary_key)
    }
}

mod root {
    use super::*;

    #[fixture]
    pub(crate) fn empty_colndef() -> ColnDef {
        ColnDef {
            theory: String::new(),
            realm: String::new(),
        }
    }

    #[fixture]
    pub(crate) fn non_empty_colndef() -> ColnDef {
        ColnDef {
            theory: "theory T".into(),
            realm: "realm R".into(),
        }
    }

    #[fixture]
    pub(crate) fn simple_rule() -> RuleEntry {
        let table = Path::from("T");
        RuleEntry {
            path: Path::from("T.non_negative"),
            rule: Rule {
                rule_variant: RuleVariant::Enforced,
                vars: vec![(
                    Path::from("x"),
                    ColType::BuiltinTy {
                        builtin_ty: BuiltinTy::BuiltinInt,
                    },
                )],
                antecedents: vec![Prop::Atom {
                    atom: Atom {
                        entity: table.clone(),
                        row_id: None,
                        values: vec![ValueEntry {
                            column: 0,
                            term: El::Var { index: 0 },
                        }],
                    },
                }],
                consequents: vec![Prop::Atom {
                    atom: Atom {
                        entity: table,
                        row_id: None,
                        values: vec![ValueEntry {
                            column: 0,
                            term: El::Var { index: 0 },
                        }],
                    },
                }],
            },
        }
    }

    #[fixture]
    pub(crate) fn root_commit_data(empty_colndef: ColnDef) -> RootCommitData {
        RootCommitData::new(
            FlatRealm {
                tables: vec![],
                definitions: vec![],
                rules: vec![],
            },
            empty_colndef,
        )
    }

    #[fixture]
    pub(crate) fn non_empty_root_commit_data(
        non_empty_colndef: ColnDef,
        simple_rule: RuleEntry,
        #[from(int_schema)]
        #[with(vec!["c0"], Some(vec![0]))]
        schema: Schema,
    ) -> RootCommitData {
        RootCommitData::new(
            FlatRealm {
                tables: vec![TableEntry {
                    path: Path::from("T"),
                    table: schema,
                }],
                definitions: vec![],
                rules: vec![simple_rule],
            },
            non_empty_colndef,
        )
    }
}

mod store {
    use crate::store::auto::AutoStore;

    use super::root::empty_colndef;
    use super::schema::{id_col_type, id_schema, idonly_schema, int_col_type, int_schema};
    use super::*;

    #[fixture]
    pub(crate) fn nodes_edges_store(
        idonly_schema: Schema,
        #[from(id_schema)]
        #[with(vec!["node"], None, id_col_type(Path::from("Nodes")))]
        edges_schema: Schema,
    ) -> Store {
        let mut store = Store::new();
        store
            .create_table(Path::from("Nodes"), idonly_schema)
            .expect("create nodes table");
        store
            .create_table(Path::from("Edges"), edges_schema)
            .expect("create edges table");
        store
    }

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

    #[fixture]
    pub(crate) fn single_int_autostore(single_int_store: Store) -> AutoStore {
        AutoStore::new(single_int_store)
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
            definitions: vec![],
            rules: vec![RuleEntry {
                path: Path::from("Link.foreignKeys"),
                rule: Rule {
                    rule_variant: RuleVariant::Enforced,
                    vars: vec![
                        (Path::from("a"), int_col_type.clone()),
                        (Path::from("b"), int_col_type),
                    ],
                    antecedents: vec![Prop::Atom {
                        atom: Atom {
                            entity: link.clone(),
                            row_id: None,
                            values: vec![
                                ValueEntry {
                                    column: 0,
                                    term: El::Var { index: 0 },
                                },
                                ValueEntry {
                                    column: 1,
                                    term: El::Var { index: 1 },
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
                                    term: El::Var { index: 0 },
                                }],
                            },
                        },
                        Prop::Atom {
                            atom: Atom {
                                entity: right.clone(),
                                row_id: None,
                                values: vec![ValueEntry {
                                    column: 0,
                                    term: El::Var { index: 1 },
                                }],
                            },
                        },
                    ],
                },
            }],
        }
    }

    #[fixture]
    pub(crate) fn link_foreign_key_root_commit_data(
        link_foreign_key_theory: FlatRealm,
        empty_colndef: ColnDef,
    ) -> RootCommitData {
        RootCommitData::new(link_foreign_key_theory, empty_colndef)
    }

    pub(crate) fn commit_int(store: &mut Store, value: i32) -> CommitHash {
        let path = Path::from("T");

        let mut tx = store.transaction();
        tx.add(&path, vec![value]).expect("add row");
        tx.commit().expect("commit row")
    }
}

pub(crate) use root::*;
pub(crate) use rowid::*;
pub(crate) use schema::*;
pub(crate) use store::*;
