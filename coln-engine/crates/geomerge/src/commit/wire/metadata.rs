use std::io::Write;

use crate::{
    commit::{
        error::PersistError,
        utils::{
            read_i64, read_len_prefixed_bytes, read_u8, read_u32, read_u64, write_i64,
            write_len_prefixed_bytes, write_u8,
        },
    },
    ir::{
        Atom, ColType, ConsField, Law, LawEntry, Lit, Path, PrimType, Prop, QName, Schema, Term,
        TupleField, ValueEntry,
    },
    table::TableOid,
};

pub(crate) const ROOT_FORMAT_VERSION: u32 = 1;

/// Store metadata captured by the root commit.
///
/// This is the commit-layer equivalent of the old store header: it describes
/// the empty store that normal data commits replay into.
#[derive(Debug, Clone)]
pub(crate) struct RootCommitData {
    pub(crate) format_version: u32,
    pub(crate) next_oid: TableOid,
    pub(crate) tables: Vec<RootTableEntry>,
    pub(crate) laws: Vec<LawEntry>,
}

#[derive(Debug, Clone)]
pub(crate) struct RootTableEntry {
    pub(crate) path: String,
    pub(crate) oid: TableOid,
    pub(crate) schema: Schema,
}

/// Encode root store metadata as a deterministic binary payload.
///
/// Layout, little-endian:
///
/// `[root_format_version:u32]`
/// `[next_oid:u64]`
/// `[table_count:u32]`
/// `repeat table_count: [path_len:u64][path_utf8][oid:u64][schema_len:u64][schema_bytes]`
/// `[law_count:u32]`
/// `repeat law_count: [law_len:u64][law_bytes]`
pub(crate) fn serialise_root(root: &RootCommitData) -> Result<Vec<u8>, PersistError> {
    let mut buf = Vec::new();
    buf.write_all(&root.format_version.to_le_bytes())?;
    buf.write_all(&root.next_oid.to_le_bytes())?;

    let mut tables = root.tables.iter().collect::<Vec<_>>();
    tables.sort_by_key(|entry| entry.oid);

    write_count(&mut buf, tables.len(), "too many root tables")?;
    for table in tables {
        write_len_prefixed_bytes(&mut buf, table.path.as_bytes(), "root table path too large")?;
        buf.write_all(&table.oid.to_le_bytes())?;

        let schema_bytes = encode_schema(&table.schema)?;
        write_len_prefixed_bytes(&mut buf, &schema_bytes, "root table schema too large")?;
    }

    write_count(&mut buf, root.laws.len(), "too many root laws")?;
    for law in &root.laws {
        let law_bytes = encode_law_entry(law)?;
        write_len_prefixed_bytes(&mut buf, &law_bytes, "root law too large")?;
    }

    Ok(buf)
}

pub(crate) fn deserialise_root(data: &[u8]) -> Result<RootCommitData, PersistError> {
    let mut pos = 0usize;
    let format_version = read_u32(data, &mut pos, "root format version")?;
    if format_version != ROOT_FORMAT_VERSION {
        return Err(PersistError::DataFormatError(format!(
            "unsupported root format version: {format_version}"
        )));
    }

    let next_oid = read_u64(data, &mut pos, "root next oid")?;

    let table_count = read_u32(data, &mut pos, "root table count")? as usize;
    let mut tables = Vec::with_capacity(table_count);
    for _ in 0..table_count {
        let path_bytes = read_len_prefixed_bytes(data, &mut pos, "root table path")?;
        let path = std::str::from_utf8(path_bytes)
            .map_err(|_| PersistError::DataFormatError("root table path: invalid utf-8".into()))?
            .to_owned();

        let oid = read_u64(data, &mut pos, "root table oid")?;

        let schema_bytes = read_len_prefixed_bytes(data, &mut pos, "root table schema")?;
        let schema = decode_schema(schema_bytes)?;

        tables.push(RootTableEntry { path, oid, schema });
    }

    let law_count = read_u32(data, &mut pos, "root law count")? as usize;
    let mut laws = Vec::with_capacity(law_count);
    for _ in 0..law_count {
        let law_bytes = read_len_prefixed_bytes(data, &mut pos, "root law")?;
        laws.push(decode_law_entry(law_bytes)?);
    }

    if pos != data.len() {
        return Err(PersistError::DataFormatError(format!(
            "trailing bytes after root payload: {} bytes",
            data.len() - pos
        )));
    }

    Ok(RootCommitData {
        format_version,
        next_oid,
        tables,
        laws,
    })
}

fn write_count(buf: &mut Vec<u8>, count: usize, ctx: &'static str) -> Result<(), PersistError> {
    let count: u32 = count
        .try_into()
        .map_err(|_| PersistError::Other(ctx.into()))?;
    buf.write_all(&count.to_le_bytes())?;
    Ok(())
}

fn encode_schema(schema: &Schema) -> Result<Vec<u8>, PersistError> {
    let mut buf = Vec::new();
    write_count(&mut buf, schema.columns.len(), "too many schema columns")?;
    for col_type in &schema.columns {
        write_col_type(&mut buf, col_type)?;
    }

    match &schema.primary_key {
        Some(primary_key) => {
            write_u8(&mut buf, 1)?;
            write_count(&mut buf, primary_key.len(), "too many primary key columns")?;
            for column in primary_key {
                write_i64(&mut buf, *column)?;
            }
        }
        None => write_u8(&mut buf, 0)?,
    }

    Ok(buf)
}

fn decode_schema(data: &[u8]) -> Result<Schema, PersistError> {
    let mut pos = 0usize;
    let column_count = read_u32(data, &mut pos, "schema column count")? as usize;
    let mut columns = Vec::with_capacity(column_count);
    for _ in 0..column_count {
        columns.push(read_col_type(data, &mut pos)?);
    }

    let primary_key = match read_u8(data, &mut pos, "schema primary key tag")? {
        0 => None,
        1 => {
            let key_count = read_u32(data, &mut pos, "schema primary key count")? as usize;
            let mut columns = Vec::with_capacity(key_count);
            for _ in 0..key_count {
                columns.push(read_i64(data, &mut pos, "schema primary key column")?);
            }
            Some(columns)
        }
        tag => {
            return Err(PersistError::DataFormatError(format!(
                "unknown schema primary key tag {tag}"
            )));
        }
    };

    reject_trailing(data, pos, "schema")?;
    Ok(Schema {
        columns,
        primary_key,
    })
}

fn encode_law_entry(entry: &LawEntry) -> Result<Vec<u8>, PersistError> {
    let mut buf = Vec::new();
    write_path(&mut buf, &entry.path)?;
    write_law(&mut buf, &entry.law)?;
    Ok(buf)
}

fn decode_law_entry(data: &[u8]) -> Result<LawEntry, PersistError> {
    let mut pos = 0usize;
    let path = read_path(data, &mut pos)?;
    let law = read_law(data, &mut pos)?;
    reject_trailing(data, pos, "law entry")?;
    Ok(LawEntry { path, law })
}

fn write_law(buf: &mut Vec<u8>, law: &Law) -> Result<(), PersistError> {
    write_count(buf, law.variables.len(), "too many law variables")?;
    for variable in &law.variables {
        write_col_type(buf, variable)?;
    }
    write_prop(buf, &law.antecedent)?;
    write_prop(buf, &law.consequent)
}

fn read_law(data: &[u8], pos: &mut usize) -> Result<Law, PersistError> {
    let variable_count = read_u32(data, pos, "law variable count")? as usize;
    let mut variables = Vec::with_capacity(variable_count);
    for _ in 0..variable_count {
        variables.push(read_col_type(data, pos)?);
    }
    let antecedent = read_prop(data, pos)?;
    let consequent = read_prop(data, pos)?;
    Ok(Law {
        variables,
        antecedent,
        consequent,
    })
}

fn write_col_type(buf: &mut Vec<u8>, col_type: &ColType) -> Result<(), PersistError> {
    match col_type {
        ColType::EntityType { path } => {
            write_u8(buf, 0)?;
            write_path(buf, path)
        }
        ColType::PrimType { prim } => {
            write_u8(buf, 1)?;
            write_prim_type(buf, *prim)
        }
        ColType::Tuple { fields } => {
            write_u8(buf, 2)?;
            write_count(buf, fields.len(), "too many tuple fields")?;
            for field in fields {
                write_qname(buf, &field.name)?;
                write_col_type(buf, &field.col_type)?;
            }
            Ok(())
        }
    }
}

fn read_col_type(data: &[u8], pos: &mut usize) -> Result<ColType, PersistError> {
    match read_u8(data, pos, "column type tag")? {
        0 => Ok(ColType::EntityType {
            path: read_path(data, pos)?,
        }),
        1 => Ok(ColType::PrimType {
            prim: read_prim_type(data, pos)?,
        }),
        2 => {
            let field_count = read_u32(data, pos, "tuple field count")? as usize;
            let mut fields = Vec::with_capacity(field_count);
            for _ in 0..field_count {
                fields.push(TupleField {
                    name: read_qname(data, pos)?,
                    col_type: Box::new(read_col_type(data, pos)?),
                });
            }
            Ok(ColType::Tuple { fields })
        }
        tag => Err(PersistError::DataFormatError(format!(
            "unknown column type tag {tag}"
        ))),
    }
}

fn write_prim_type(buf: &mut Vec<u8>, prim: PrimType) -> Result<(), PersistError> {
    let tag = match prim {
        PrimType::PrimInt => 0,
        PrimType::PrimString => 1,
    };
    write_u8(buf, tag)
}

fn read_prim_type(data: &[u8], pos: &mut usize) -> Result<PrimType, PersistError> {
    match read_u8(data, pos, "primitive type tag")? {
        0 => Ok(PrimType::PrimInt),
        1 => Ok(PrimType::PrimString),
        tag => Err(PersistError::DataFormatError(format!(
            "unknown primitive type tag {tag}"
        ))),
    }
}

fn write_prop(buf: &mut Vec<u8>, prop: &Prop) -> Result<(), PersistError> {
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
        Prop::And { props } => {
            write_u8(buf, 2)?;
            write_count(buf, props.len(), "too many and props")?;
            for prop in props {
                write_prop(buf, prop)?;
            }
            Ok(())
        }
        Prop::Or { props } => {
            write_u8(buf, 3)?;
            write_count(buf, props.len(), "too many or props")?;
            for prop in props {
                write_prop(buf, prop)?;
            }
            Ok(())
        }
    }
}

fn read_prop(data: &[u8], pos: &mut usize) -> Result<Prop, PersistError> {
    match read_u8(data, pos, "prop tag")? {
        0 => Ok(Prop::Atom {
            atom: read_atom(data, pos)?,
        }),
        1 => Ok(Prop::Eq {
            left: read_term(data, pos)?,
            right: read_term(data, pos)?,
        }),
        2 => {
            let prop_count = read_u32(data, pos, "and prop count")? as usize;
            let mut props = Vec::with_capacity(prop_count);
            for _ in 0..prop_count {
                props.push(read_prop(data, pos)?);
            }
            Ok(Prop::And { props })
        }
        3 => {
            let prop_count = read_u32(data, pos, "or prop count")? as usize;
            let mut props = Vec::with_capacity(prop_count);
            for _ in 0..prop_count {
                props.push(read_prop(data, pos)?);
            }
            Ok(Prop::Or { props })
        }
        tag => Err(PersistError::DataFormatError(format!(
            "unknown prop tag {tag}"
        ))),
    }
}

fn write_atom(buf: &mut Vec<u8>, atom: &Atom) -> Result<(), PersistError> {
    write_path(buf, &atom.table)?;
    match &atom.row_id {
        Some(term) => {
            write_u8(buf, 1)?;
            write_term(buf, term)?;
        }
        None => write_u8(buf, 0)?,
    }
    write_count(buf, atom.values.len(), "too many atom values")?;
    for value in &atom.values {
        write_i64(buf, value.column)?;
        write_term(buf, &value.term)?;
    }
    Ok(())
}

fn read_atom(data: &[u8], pos: &mut usize) -> Result<Atom, PersistError> {
    let table = read_path(data, pos)?;
    let row_id = match read_u8(data, pos, "atom row id tag")? {
        0 => None,
        1 => Some(read_term(data, pos)?),
        tag => {
            return Err(PersistError::DataFormatError(format!(
                "unknown atom row id tag {tag}"
            )));
        }
    };
    let value_count = read_u32(data, pos, "atom value count")? as usize;
    let mut values = Vec::with_capacity(value_count);
    for _ in 0..value_count {
        values.push(ValueEntry {
            column: read_i64(data, pos, "atom value column")?,
            term: read_term(data, pos)?,
        });
    }
    Ok(Atom {
        table,
        row_id,
        values,
    })
}

fn write_term(buf: &mut Vec<u8>, term: &Term) -> Result<(), PersistError> {
    match term {
        Term::Lit { lit } => {
            write_u8(buf, 0)?;
            write_lit(buf, lit)
        }
        Term::Var { index } => {
            write_u8(buf, 1)?;
            write_i64(buf, *index)
        }
        Term::Proj { term, field } => {
            write_u8(buf, 2)?;
            write_term(buf, term)?;
            write_qname(buf, field)
        }
        Term::Cons { fields } => {
            write_u8(buf, 3)?;
            write_count(buf, fields.len(), "too many cons fields")?;
            for field in fields {
                write_qname(buf, &field.name)?;
                write_term(buf, &field.term)?;
            }
            Ok(())
        }
    }
}

fn read_term(data: &[u8], pos: &mut usize) -> Result<Term, PersistError> {
    match read_u8(data, pos, "term tag")? {
        0 => Ok(Term::Lit {
            lit: read_lit(data, pos)?,
        }),
        1 => Ok(Term::Var {
            index: read_i64(data, pos, "term var index")?,
        }),
        2 => Ok(Term::Proj {
            term: Box::new(read_term(data, pos)?),
            field: read_qname(data, pos)?,
        }),
        3 => {
            let field_count = read_u32(data, pos, "cons field count")? as usize;
            let mut fields = Vec::with_capacity(field_count);
            for _ in 0..field_count {
                fields.push(ConsField {
                    name: read_qname(data, pos)?,
                    term: Box::new(read_term(data, pos)?),
                });
            }
            Ok(Term::Cons { fields })
        }
        tag => Err(PersistError::DataFormatError(format!(
            "unknown term tag {tag}"
        ))),
    }
}

fn write_lit(buf: &mut Vec<u8>, lit: &Lit) -> Result<(), PersistError> {
    match lit {
        Lit::Int { value } => {
            write_u8(buf, 0)?;
            write_i64(buf, *value)
        }
        Lit::String { value } => {
            write_u8(buf, 1)?;
            write_len_prefixed_bytes(buf, value.as_bytes(), "string literal too large")
        }
    }
}

fn read_lit(data: &[u8], pos: &mut usize) -> Result<Lit, PersistError> {
    match read_u8(data, pos, "literal tag")? {
        0 => Ok(Lit::Int {
            value: read_i64(data, pos, "integer literal")?,
        }),
        1 => Ok(Lit::String {
            value: read_string(data, pos, "string literal")?,
        }),
        tag => Err(PersistError::DataFormatError(format!(
            "unknown literal tag {tag}"
        ))),
    }
}

fn write_path(buf: &mut Vec<u8>, path: &Path) -> Result<(), PersistError> {
    write_count(buf, path.0.len(), "path too large")?;
    for qname in &path.0 {
        write_qname(buf, qname)?;
    }
    Ok(())
}

fn read_path(data: &[u8], pos: &mut usize) -> Result<Path, PersistError> {
    let len = read_u32(data, pos, "path length")? as usize;
    let mut parts = Vec::with_capacity(len);
    for _ in 0..len {
        parts.push(read_qname(data, pos)?);
    }
    Ok(Path(parts))
}

fn write_qname(buf: &mut Vec<u8>, qname: &QName) -> Result<(), PersistError> {
    write_count(buf, qname.len(), "qname too large")?;
    for part in qname {
        write_len_prefixed_bytes(buf, part.as_bytes(), "qname part too large")?;
    }
    Ok(())
}

fn read_qname(data: &[u8], pos: &mut usize) -> Result<QName, PersistError> {
    let len = read_u32(data, pos, "qname length")? as usize;
    let mut parts = Vec::with_capacity(len);
    for _ in 0..len {
        parts.push(read_string(data, pos, "qname part")?);
    }
    Ok(parts)
}

fn read_string(data: &[u8], pos: &mut usize, ctx: &'static str) -> Result<String, PersistError> {
    let bytes = read_len_prefixed_bytes(data, pos, ctx)?;
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| PersistError::DataFormatError(format!("{ctx}: invalid utf-8")))
}

fn reject_trailing(data: &[u8], pos: usize, ctx: &'static str) -> Result<(), PersistError> {
    if pos != data.len() {
        return Err(PersistError::DataFormatError(format!(
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
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimInt,
            }],
            primary_key: Some(vec![0]),
        }
    }

    fn string_schema() -> Schema {
        Schema {
            columns: vec![ColType::PrimType {
                prim: PrimType::PrimString,
            }],
            primary_key: None,
        }
    }

    fn simple_law() -> LawEntry {
        let table = Path::from("T");
        LawEntry {
            path: Path::from("T.non_negative"),
            law: Law {
                variables: vec![ColType::PrimType {
                    prim: PrimType::PrimInt,
                }],
                antecedent: Prop::Atom {
                    atom: Atom {
                        table: table.clone(),
                        row_id: None,
                        values: vec![ValueEntry {
                            column: 0,
                            term: Term::Var { index: 0 },
                        }],
                    },
                },
                consequent: Prop::Eq {
                    left: Term::Var { index: 0 },
                    right: Term::Var { index: 0 },
                },
            },
        }
    }

    #[test]
    fn root_payload_round_trips() {
        let root = RootCommitData {
            format_version: ROOT_FORMAT_VERSION,
            next_oid: 2,
            tables: vec![RootTableEntry {
                path: "T".to_owned(),
                oid: 1,
                schema: int_schema(),
            }],
            laws: vec![simple_law()],
        };

        let bytes = serialise_root(&root).expect("encode root");
        let decoded = deserialise_root(&bytes).expect("decode root");

        assert_eq!(decoded.format_version, ROOT_FORMAT_VERSION);
        assert_eq!(decoded.next_oid, 2);
        assert_eq!(decoded.tables.len(), 1);
        assert_eq!(decoded.tables[0].path, "T");
        assert_eq!(decoded.tables[0].oid, 1);
        assert_eq!(decoded.tables[0].schema.columns, int_schema().columns);
        assert_eq!(decoded.tables[0].schema.primary_key, Some(vec![0]));
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
            format_version: ROOT_FORMAT_VERSION,
            next_oid: 3,
            tables: vec![high.clone(), low.clone()],
            laws: vec![],
        };
        let right = RootCommitData {
            format_version: ROOT_FORMAT_VERSION,
            next_oid: 3,
            tables: vec![low, high],
            laws: vec![],
        };

        assert_eq!(
            serialise_root(&left).expect("encode left"),
            serialise_root(&right).expect("encode right")
        );
    }

    #[test]
    fn root_payload_rejects_trailing_bytes() {
        let root = RootCommitData {
            format_version: ROOT_FORMAT_VERSION,
            next_oid: 0,
            tables: vec![],
            laws: vec![],
        };

        let mut bytes = serialise_root(&root).expect("encode root");
        bytes.push(0);

        assert!(matches!(
            deserialise_root(&bytes),
            Err(PersistError::DataFormatError(_))
        ));
    }
}
