// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::commit::error::CodecError;
use coln_flir_rs::ir::FlatRealm;

/// Encode root store metadata as compact JSON of a [`FlatRealm`].
pub(crate) fn serialize_root(root: &FlatRealm) -> Result<Vec<u8>, CodecError> {
    Ok(serde_json::to_vec(root)?)
}

pub(crate) fn deserialize_root(data: &[u8]) -> Result<FlatRealm, CodecError> {
    Ok(serde_json::from_slice(data)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Atom, BuiltinTy, ColType, ColumnEntry, EntityVariant, Path, Prop, Rule, RuleEntry,
        RuleVariant, Schema, TableEntry, Term, ValueEntry,
    };

    fn int_schema() -> Schema {
        Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                },
            }],
            primary_key: Some(vec![Path::from("c0")]),
        }
    }

    fn string_schema() -> Schema {
        Schema {
            entity_variant: EntityVariant::Table,
            columns: vec![ColumnEntry {
                path: Path::from("c0"),
                col_type: ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinStr,
                },
            }],
            primary_key: None,
        }
    }

    fn table_entry(path: &str, schema: Schema) -> TableEntry {
        TableEntry {
            path: Path::from(path),
            table: schema,
        }
    }

    fn simple_rule() -> RuleEntry {
        let table = Path::from("T");
        RuleEntry {
            path: Path::from("T.non_negative"),
            rule: Rule {
                rule_variant: RuleVariant::Enforced,
                var_names: vec![Path::from("x")],
                var_types: vec![ColType::BuiltinTy {
                    builtin_ty: BuiltinTy::BuiltinInt,
                }],
                antecedents: vec![Prop::Atom {
                    atom: Atom {
                        entity: table.clone(),
                        row_id: None,
                        values: vec![ValueEntry {
                            column: 0,
                            term: Term::Var { index: 0 },
                        }],
                    },
                }],
                consequents: vec![Prop::Eq {
                    left: Term::Var { index: 0 },
                    right: Term::Var { index: 0 },
                }],
            },
        }
    }

    #[test]
    fn root_payload_round_trips() {
        let root = FlatRealm {
            tables: vec![table_entry("T", int_schema())],
            rules: vec![simple_rule()],
        };

        let bytes = serialize_root(&root).expect("encode root");
        let decoded = deserialize_root(&bytes).expect("decode root");

        assert_eq!(decoded.tables.len(), 1);
        assert_eq!(decoded.tables[0].path, Path::from("T"));
        assert_eq!(decoded.tables[0].table.columns, int_schema().columns);
        assert_eq!(
            decoded.tables[0].table.primary_key,
            Some(vec![Path::from("c0")])
        );
        assert_eq!(decoded.rules.len(), 1);
        assert_eq!(decoded.rules[0].path, Path::from("T.non_negative"));
    }

    #[test]
    fn root_payload_preserves_entity_order() {
        let a = table_entry("A", int_schema());
        let b = table_entry("B", string_schema());

        let left = FlatRealm {
            tables: vec![b.clone(), a.clone()],
            rules: vec![],
        };
        let right = FlatRealm {
            tables: vec![a, b],
            rules: vec![],
        };

        assert_ne!(
            serialize_root(&left).expect("encode left"),
            serialize_root(&right).expect("encode right")
        );
    }

    #[test]
    fn root_payload_rejects_trailing_bytes() {
        let root = FlatRealm {
            tables: vec![],
            rules: vec![],
        };

        let mut bytes = serialize_root(&root).expect("encode root");
        bytes.push(0);

        assert!(deserialize_root(&bytes).is_err());
    }
}
