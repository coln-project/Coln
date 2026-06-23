// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    commit::{
        error::CodecError,
        leb128 as commit_leb128,
        utils::{read_u8, write_u8},
    },
    ir::{
        Atom, BuiltinTy, ColType, ColumnEntry, EntityVariant, IndexMethod, Lit, Materialization,
        Path, Prop, QName, Rule, RuleEntry, RuleVariant, Schema, Term, ValueEntry,
    },
    table::TableOid,
};

/// Store metadata captured by the root commit.
///
/// This is the commit-layer equivalent of the old store header: it describes
/// the empty store that normal data commits replay into.
#[derive(Debug, Clone)]
pub(crate) struct RootCommitData {
    pub(crate) tables: Vec<RootTableEntry>,
    pub(crate) laws: Vec<RuleEntry>,
}

#[derive(Debug, Clone)]
pub(crate) struct RootTableEntry {
    pub(crate) path: String,
    pub(crate) oid: TableOid,
    pub(crate) schema: Schema,
}

/// Encode root store metadata as a deterministic binary payload.
///
/// Layout, scalar integers use LEB128:
///
/// `[table_count]`
/// `repeat table_count: [path_len][path_utf8][oid][schema_len][schema_bytes]`
/// `[rule_count]`
/// `repeat rule_count: [rule_len][rule_bytes]`
pub(crate) fn serialize_root(root: &RootCommitData) -> Result<Vec<u8>, CodecError> {
    let mut buf = Vec::new();

    let mut tables = root.tables.iter().collect::<Vec<_>>();
    tables.sort_by_key(|entry| entry.oid);

    write_count(&mut buf, tables.len());
    for table in tables {
        commit_leb128::write_len_prefixed_bytes(&mut buf, table.path.as_bytes());
        commit_leb128::write_u64(&mut buf, table.oid);

        let schema_bytes = encode_schema(&table.schema)?;
        commit_leb128::write_len_prefixed_bytes(&mut buf, &schema_bytes);
    }

    write_count(&mut buf, root.laws.len());
    for rule in &root.laws {
        let rule_bytes = encode_rule_entry(rule)?;
        commit_leb128::write_len_prefixed_bytes(&mut buf, &rule_bytes);
    }

    Ok(buf)
}

pub(crate) fn deserialize_root(data: &[u8]) -> Result<RootCommitData, CodecError> {
    let mut pos = 0usize;

    let table_count = commit_leb128::read_len(data, &mut pos, "root table count")?;
    let mut tables = Vec::with_capacity(table_count);
    for _ in 0..table_count {
        let path_bytes = commit_leb128::read_len_prefixed_bytes(data, &mut pos, "root table path")?;
        let path = std::str::from_utf8(path_bytes)
            .map_err(|_| CodecError::DataFormatError("root table path: invalid utf-8".into()))?
            .to_owned();

        let oid = commit_leb128::read_u64(data, &mut pos, "root table oid")?;

        let schema_bytes =
            commit_leb128::read_len_prefixed_bytes(data, &mut pos, "root table schema")?;
        let schema = decode_schema(schema_bytes)?;

        tables.push(RootTableEntry { path, oid, schema });
    }

    let rule_count = commit_leb128::read_len(data, &mut pos, "root rule count")?;
    let mut laws = Vec::with_capacity(rule_count);
    for _ in 0..rule_count {
        let rule_bytes = commit_leb128::read_len_prefixed_bytes(data, &mut pos, "root rule")?;
        laws.push(decode_rule_entry(rule_bytes)?);
    }

    if pos != data.len() {
        return Err(CodecError::DataFormatError(format!(
            "trailing bytes after root payload: {} bytes",
            data.len() - pos
        )));
    }

    Ok(RootCommitData { tables, laws })
}

fn write_count(buf: &mut Vec<u8>, count: usize) {
    commit_leb128::write_len(buf, count)
}

fn encode_schema(schema: &Schema) -> Result<Vec<u8>, CodecError> {
    let mut buf = Vec::new();
    write_entity_variant(&mut buf, &schema.entity_variant)?;
    write_count(&mut buf, schema.columns.len());
    for column in &schema.columns {
        write_column_entry(&mut buf, column)?;
    }

    match &schema.primary_key {
        Some(primary_key) => {
            write_u8(&mut buf, 1)?;
            write_count(&mut buf, primary_key.len());
            for column in primary_key {
                write_path(&mut buf, column)?;
            }
        }
        None => write_u8(&mut buf, 0)?,
    }

    Ok(buf)
}

fn decode_schema(data: &[u8]) -> Result<Schema, CodecError> {
    let mut pos = 0usize;
    let entity_variant = read_entity_variant(data, &mut pos)?;
    let column_count = commit_leb128::read_len(data, &mut pos, "schema column count")?;
    let mut columns = Vec::with_capacity(column_count);
    for _ in 0..column_count {
        columns.push(read_column_entry(data, &mut pos)?);
    }

    let primary_key = match read_u8(data, &mut pos, "schema primary key tag")? {
        0 => None,
        1 => {
            let key_count = commit_leb128::read_len(data, &mut pos, "schema primary key count")?;
            let mut key_columns = Vec::with_capacity(key_count);
            for _ in 0..key_count {
                key_columns.push(read_path(data, &mut pos)?);
            }
            Some(key_columns)
        }
        tag => {
            return Err(CodecError::DataFormatError(format!(
                "unknown schema primary key tag {tag}"
            )));
        }
    };

    reject_trailing(data, pos, "schema")?;
    Ok(Schema {
        entity_variant,
        columns,
        primary_key,
    })
}

fn write_column_entry(buf: &mut Vec<u8>, column: &ColumnEntry) -> Result<(), CodecError> {
    write_path(buf, &column.path)?;
    write_col_type(buf, &column.col_type)
}

fn read_column_entry(data: &[u8], pos: &mut usize) -> Result<ColumnEntry, CodecError> {
    let path = read_path(data, pos)?;
    let col_type = read_col_type(data, pos)?;
    Ok(ColumnEntry { path, col_type })
}

fn write_entity_variant(buf: &mut Vec<u8>, variant: &EntityVariant) -> Result<(), CodecError> {
    match variant {
        EntityVariant::Table => write_u8(buf, 0),
        EntityVariant::View(materialization) => {
            write_u8(buf, 1)?;
            write_materialization(buf, materialization)
        }
        EntityVariant::Index { method, columns } => {
            write_u8(buf, 2)?;
            write_index_method(buf, method)?;
            write_count(buf, columns.len());
            for column in columns {
                write_path(buf, column)?;
            }
            Ok(())
        }
    }
}

fn read_entity_variant(data: &[u8], pos: &mut usize) -> Result<EntityVariant, CodecError> {
    match read_u8(data, pos, "entity variant tag")? {
        0 => Ok(EntityVariant::Table),
        1 => Ok(EntityVariant::View(read_materialization(data, pos)?)),
        2 => {
            let method = read_index_method(data, pos)?;
            let column_count = commit_leb128::read_len(data, pos, "index column count")?;
            let mut columns = Vec::with_capacity(column_count);
            for _ in 0..column_count {
                columns.push(read_path(data, pos)?);
            }
            Ok(EntityVariant::Index { method, columns })
        }
        tag => Err(CodecError::DataFormatError(format!(
            "unknown entity variant tag {tag}"
        ))),
    }
}

fn write_materialization(
    buf: &mut Vec<u8>,
    materialization: &Materialization,
) -> Result<(), CodecError> {
    let tag = match materialization {
        Materialization::Recomputed => 0,
        Materialization::Memoized => 1,
        Materialization::Materialized => 2,
    };
    write_u8(buf, tag)
}

fn read_materialization(data: &[u8], pos: &mut usize) -> Result<Materialization, CodecError> {
    match read_u8(data, pos, "materialization tag")? {
        0 => Ok(Materialization::Recomputed),
        1 => Ok(Materialization::Memoized),
        2 => Ok(Materialization::Materialized),
        tag => Err(CodecError::DataFormatError(format!(
            "unknown materialization tag {tag}"
        ))),
    }
}

fn write_index_method(buf: &mut Vec<u8>, method: &IndexMethod) -> Result<(), CodecError> {
    let tag = match method {
        IndexMethod::BTree => 0,
    };
    write_u8(buf, tag)
}

fn read_index_method(data: &[u8], pos: &mut usize) -> Result<IndexMethod, CodecError> {
    match read_u8(data, pos, "index method tag")? {
        0 => Ok(IndexMethod::BTree),
        tag => Err(CodecError::DataFormatError(format!(
            "unknown index method tag {tag}"
        ))),
    }
}

fn encode_rule_entry(entry: &RuleEntry) -> Result<Vec<u8>, CodecError> {
    let mut buf = Vec::new();
    write_path(&mut buf, &entry.path)?;
    write_rule(&mut buf, &entry.rule)?;
    Ok(buf)
}

fn decode_rule_entry(data: &[u8]) -> Result<RuleEntry, CodecError> {
    let mut pos = 0usize;
    let path = read_path(data, &mut pos)?;
    let rule = read_rule(data, &mut pos)?;
    reject_trailing(data, pos, "rule entry")?;
    Ok(RuleEntry { path, rule })
}

fn write_rule(buf: &mut Vec<u8>, rule: &Rule) -> Result<(), CodecError> {
    write_rule_variant(buf, &rule.rule_variant)?;
    write_count(buf, rule.var_names.len());
    for name in &rule.var_names {
        write_path(buf, name)?;
    }
    write_count(buf, rule.var_types.len());
    for ty in &rule.var_types {
        write_col_type(buf, ty)?;
    }
    write_count(buf, rule.antecedents.len());
    for prop in &rule.antecedents {
        write_prop(buf, prop)?;
    }
    write_count(buf, rule.consequents.len());
    for prop in &rule.consequents {
        write_prop(buf, prop)?;
    }
    Ok(())
}

fn read_rule(data: &[u8], pos: &mut usize) -> Result<Rule, CodecError> {
    let rule_variant = read_rule_variant(data, pos)?;

    let name_count = commit_leb128::read_len(data, pos, "rule var name count")?;
    let mut var_names = Vec::with_capacity(name_count);
    for _ in 0..name_count {
        var_names.push(read_path(data, pos)?);
    }

    let type_count = commit_leb128::read_len(data, pos, "rule var type count")?;
    let mut var_types = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        var_types.push(read_col_type(data, pos)?);
    }

    let antecedent_count = commit_leb128::read_len(data, pos, "rule antecedent count")?;
    let mut antecedents = Vec::with_capacity(antecedent_count);
    for _ in 0..antecedent_count {
        antecedents.push(read_prop(data, pos)?);
    }

    let consequent_count = commit_leb128::read_len(data, pos, "rule consequent count")?;
    let mut consequents = Vec::with_capacity(consequent_count);
    for _ in 0..consequent_count {
        consequents.push(read_prop(data, pos)?);
    }

    Ok(Rule {
        rule_variant,
        var_names,
        var_types,
        antecedents,
        consequents,
    })
}

fn write_rule_variant(buf: &mut Vec<u8>, variant: &RuleVariant) -> Result<(), CodecError> {
    let tag = match variant {
        RuleVariant::Chased => 0,
        RuleVariant::Enforced => 1,
        RuleVariant::Monitored => 2,
    };
    write_u8(buf, tag)
}

fn read_rule_variant(data: &[u8], pos: &mut usize) -> Result<RuleVariant, CodecError> {
    match read_u8(data, pos, "rule variant tag")? {
        0 => Ok(RuleVariant::Chased),
        1 => Ok(RuleVariant::Enforced),
        2 => Ok(RuleVariant::Monitored),
        tag => Err(CodecError::DataFormatError(format!(
            "unknown rule variant tag {tag}"
        ))),
    }
}

fn write_col_type(buf: &mut Vec<u8>, col_type: &ColType) -> Result<(), CodecError> {
    match col_type {
        ColType::RowId { path } => {
            write_u8(buf, 0)?;
            write_path(buf, path)
        }
        ColType::BuiltinTy { builtin_ty } => {
            write_u8(buf, 1)?;
            write_builtin_ty(buf, *builtin_ty)
        }
    }
}

fn read_col_type(data: &[u8], pos: &mut usize) -> Result<ColType, CodecError> {
    match read_u8(data, pos, "column type tag")? {
        0 => Ok(ColType::RowId {
            path: read_path(data, pos)?,
        }),
        1 => Ok(ColType::BuiltinTy {
            builtin_ty: read_builtin_ty(data, pos)?,
        }),
        tag => Err(CodecError::DataFormatError(format!(
            "unknown column type tag {tag}"
        ))),
    }
}

fn write_builtin_ty(buf: &mut Vec<u8>, builtin_ty: BuiltinTy) -> Result<(), CodecError> {
    let tag = match builtin_ty {
        BuiltinTy::BuiltinInt => 0,
        BuiltinTy::BuiltinStr => 1,
    };
    write_u8(buf, tag)
}

fn read_builtin_ty(data: &[u8], pos: &mut usize) -> Result<BuiltinTy, CodecError> {
    match read_u8(data, pos, "builtin type tag")? {
        0 => Ok(BuiltinTy::BuiltinInt),
        1 => Ok(BuiltinTy::BuiltinStr),
        tag => Err(CodecError::DataFormatError(format!(
            "unknown builtin type tag {tag}"
        ))),
    }
}

fn write_prop(buf: &mut Vec<u8>, prop: &Prop) -> Result<(), CodecError> {
    match prop {
        Prop::Atom { atom } => {
            write_u8(buf, 0)?;
            write_atom(buf, atom)
        }
        Prop::Eq { left, right } => {
            write_u8(buf, 1)?;
            write_term(buf, left)?;
            write_term(buf, right)
        }
    }
}

fn read_prop(data: &[u8], pos: &mut usize) -> Result<Prop, CodecError> {
    match read_u8(data, pos, "prop tag")? {
        0 => Ok(Prop::Atom {
            atom: read_atom(data, pos)?,
        }),
        1 => Ok(Prop::Eq {
            left: read_term(data, pos)?,
            right: read_term(data, pos)?,
        }),
        tag => Err(CodecError::DataFormatError(format!(
            "unknown prop tag {tag}"
        ))),
    }
}

fn write_atom(buf: &mut Vec<u8>, atom: &Atom) -> Result<(), CodecError> {
    write_path(buf, &atom.entity)?;
    match &atom.row_id {
        Some(term) => {
            write_u8(buf, 1)?;
            write_term(buf, term)?;
        }
        None => write_u8(buf, 0)?,
    }
    write_count(buf, atom.values.len());
    for value in &atom.values {
        commit_leb128::write_i64(buf, value.column);
        write_term(buf, &value.term)?;
    }
    Ok(())
}

fn read_atom(data: &[u8], pos: &mut usize) -> Result<Atom, CodecError> {
    let entity = read_path(data, pos)?;
    let row_id = match read_u8(data, pos, "atom row id tag")? {
        0 => None,
        1 => Some(read_term(data, pos)?),
        tag => {
            return Err(CodecError::DataFormatError(format!(
                "unknown atom row id tag {tag}"
            )));
        }
    };
    let value_count = commit_leb128::read_len(data, pos, "atom value count")?;
    let mut values = Vec::with_capacity(value_count);
    for _ in 0..value_count {
        values.push(ValueEntry {
            column: commit_leb128::read_i64(data, pos, "atom value column")?,
            term: read_term(data, pos)?,
        });
    }
    Ok(Atom {
        entity,
        row_id,
        values,
    })
}

fn write_term(buf: &mut Vec<u8>, term: &Term) -> Result<(), CodecError> {
    match term {
        Term::Lit { lit } => {
            write_u8(buf, 0)?;
            write_lit(buf, lit)
        }
        Term::Var { index } => {
            write_u8(buf, 1)?;
            commit_leb128::write_i64(buf, *index);
            Ok(())
        }
    }
}

fn read_term(data: &[u8], pos: &mut usize) -> Result<Term, CodecError> {
    match read_u8(data, pos, "term tag")? {
        0 => Ok(Term::Lit {
            lit: read_lit(data, pos)?,
        }),
        1 => Ok(Term::Var {
            index: commit_leb128::read_i64(data, pos, "term var index")?,
        }),
        tag => Err(CodecError::DataFormatError(format!(
            "unknown term tag {tag}"
        ))),
    }
}

fn write_lit(buf: &mut Vec<u8>, lit: &Lit) -> Result<(), CodecError> {
    match lit {
        Lit::Int { value } => {
            write_u8(buf, 0)?;
            commit_leb128::write_i64(buf, *value);
            Ok(())
        }
        Lit::String { value } => {
            write_u8(buf, 1)?;
            commit_leb128::write_len_prefixed_bytes(buf, value.as_bytes());
            Ok(())
        }
    }
}

fn read_lit(data: &[u8], pos: &mut usize) -> Result<Lit, CodecError> {
    match read_u8(data, pos, "literal tag")? {
        0 => Ok(Lit::Int {
            value: commit_leb128::read_i64(data, pos, "integer literal")?,
        }),
        1 => Ok(Lit::String {
            value: read_string(data, pos, "string literal")?,
        }),
        tag => Err(CodecError::DataFormatError(format!(
            "unknown literal tag {tag}"
        ))),
    }
}

fn write_path(buf: &mut Vec<u8>, path: &Path) -> Result<(), CodecError> {
    write_count(buf, path.0.len());
    for qname in &path.0 {
        write_qname(buf, qname)?;
    }
    Ok(())
}

fn read_path(data: &[u8], pos: &mut usize) -> Result<Path, CodecError> {
    let len = commit_leb128::read_len(data, pos, "path length")?;
    let mut parts = Vec::with_capacity(len);
    for _ in 0..len {
        parts.push(read_qname(data, pos)?);
    }
    Ok(Path(parts))
}

fn write_qname(buf: &mut Vec<u8>, qname: &QName) -> Result<(), CodecError> {
    write_count(buf, qname.len());
    for part in qname {
        commit_leb128::write_len_prefixed_bytes(buf, part.as_bytes());
    }
    Ok(())
}

fn read_qname(data: &[u8], pos: &mut usize) -> Result<QName, CodecError> {
    let len = commit_leb128::read_len(data, pos, "qname length")?;
    let mut parts = Vec::with_capacity(len);
    for _ in 0..len {
        parts.push(read_string(data, pos, "qname part")?);
    }
    Ok(parts)
}

fn read_string(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<String, CodecError> {
    let bytes = commit_leb128::read_len_prefixed_bytes(data, pos, ctx)?;
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| CodecError::DataFormatError(format!("{ctx}: invalid utf-8")))
}

fn reject_trailing(data: &[u8], pos: usize, ctx: &'static str) -> Result<(), CodecError> {
    if pos != data.len() {
        return Err(CodecError::DataFormatError(format!(
            "trailing bytes after {ctx}: {} bytes",
            data.len() - pos
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Term, ValueEntry};

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
        let root = RootCommitData {
            tables: vec![RootTableEntry {
                path: "T".to_owned(),
                oid: 1,
                schema: int_schema(),
            }],
            laws: vec![simple_rule()],
        };

        let bytes = serialize_root(&root).expect("encode root");
        let decoded = deserialize_root(&bytes).expect("decode root");

        assert_eq!(decoded.tables.len(), 1);
        assert_eq!(decoded.tables[0].path, "T");
        assert_eq!(decoded.tables[0].oid, 1);
        assert_eq!(decoded.tables[0].schema.columns, int_schema().columns);
        assert_eq!(
            decoded.tables[0].schema.primary_key,
            Some(vec![Path::from("c0")])
        );
        assert_eq!(decoded.laws.len(), 1);
        assert_eq!(decoded.laws[0].path, Path::from("T.non_negative"));
    }

    #[test]
    fn root_payload_encoding_sorts_tables_by_oid() {
        let low = RootTableEntry {
            path: "A".to_owned(),
            oid: 1,
            schema: int_schema(),
        };
        let high = RootTableEntry {
            path: "B".to_owned(),
            oid: 2,
            schema: string_schema(),
        };

        let left = RootCommitData {
            tables: vec![high.clone(), low.clone()],
            laws: vec![],
        };
        let right = RootCommitData {
            tables: vec![low, high],
            laws: vec![],
        };

        assert_eq!(
            serialize_root(&left).expect("encode left"),
            serialize_root(&right).expect("encode right")
        );
    }

    #[test]
    fn root_payload_rejects_trailing_bytes() {
        let root = RootCommitData {
            tables: vec![],
            laws: vec![],
        };

        let mut bytes = serialize_root(&root).expect("encode root");
        bytes.push(0);

        assert!(matches!(
            deserialize_root(&bytes),
            Err(CodecError::DataFormatError(_))
        ));
    }
}
