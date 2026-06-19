pub mod path;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// A QName is a vec of string, potentially separated by a forward slash /
pub type QName = Vec<String>;

// For example a G.V would become [["G"], ["V"]], this is at a higher level than
// QName because V would be a query inside a theory G
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Path(pub Vec<QName>);

type ColName = Path;
pub type FId = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinTy {
    BuiltinInt,
    BuiltinStr,
    // TODO add floating point number primitives
    // arbitrary precision integers (store as two cols)
    // arbitrary precision rationals (fractions)
    // IEEE 754 floats 16, 32, 64 bits
    // bfloat
}

impl Serialize for BuiltinTy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            BuiltinTy::BuiltinInt => serializer.serialize_str("int"),
            BuiltinTy::BuiltinStr => serializer.serialize_str("string"),
        }
    }
}

impl<'de> Deserialize<'de> for BuiltinTy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "int" => Ok(BuiltinTy::BuiltinInt),
            "string" => Ok(BuiltinTy::BuiltinStr),
            _ => Err(DeError::unknown_variant(&s, &["int", "string"])),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tag", rename_all = "camelCase")]
pub enum ColType {
    RowId { path: Path },
    BuiltinTy { builtin_ty: BuiltinTy },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tag", rename_all = "camelCase")]
pub enum Materialization {
    Recomputed,
    Memoized,
    Materialized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexMethod {
    BTree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tag", rename_all = "camelCase")]
pub enum EntityVariant {
    Table,
    View(Materialization),
    Index {
        method: IndexMethod,
        columns: Vec<ColName>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnEntry {
    pub path: ColName,
    #[serde(rename = "type")]
    pub col_type: ColType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub entity_variant: EntityVariant,
    pub columns: Vec<ColumnEntry>,
    pub primary_key: Option<Vec<ColName>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tag", rename_all = "lowercase")]
pub enum Lit {
    #[serde(rename = "int")]
    Int { value: i64 },
    #[serde(rename = "string")]
    String { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tag", rename_all = "lowercase")]
pub enum Term {
    Lit { lit: Lit },
    Var { index: FId },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueEntry {
    pub column: i64,
    pub term: Term,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Atom {
    pub entity: Path,
    pub row_id: Option<Term>,
    pub values: Vec<ValueEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tag", rename_all = "lowercase")]
pub enum Prop {
    Atom { atom: Atom },
    Eq { left: Term, right: Term },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleVariant {
    Chased,
    Enforced,
    Monitored,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub rule_variant: RuleVariant,
    pub var_names: Vec<ColName>,
    pub var_types: Vec<ColType>,
    pub antecedents: Vec<Prop>,
    pub consequents: Vec<Prop>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableEntry {
    pub path: Path,
    #[serde(rename = "value")]
    pub entity: Entity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEntry {
    pub path: Path,
    #[serde(rename="value")]
    pub rule: Rule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatRealm {
    #[serde(rename = "entities")]
    pub tables: Vec<TableEntry>,
    pub rules: Vec<RuleEntry>,
}
