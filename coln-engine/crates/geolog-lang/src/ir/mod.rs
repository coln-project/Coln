pub mod path;

#[cfg(feature = "serde")]
use serde::de::Error as DeError;
#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// A QName is a vec of string, potentially separated by a forward slash /
pub type QName = Vec<String>;

// For example a G.V would become [["G"], ["V"]], this is at a higher level than
// QName because V would be a query inside a theory G
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Path(pub Vec<QName>);

pub type FId = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimType {
    PrimInt,
    PrimString,
}

#[cfg(feature = "serde")]
impl Serialize for PrimType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            PrimType::PrimInt => serializer.serialize_str("int"),
            PrimType::PrimString => serializer.serialize_str("string"),
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for PrimType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "int" => Ok(PrimType::PrimInt),
            "string" => Ok(PrimType::PrimString),
            _ => Err(DeError::unknown_variant(&s, &["int", "string"])),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TupleField {
    pub name: QName,
    pub col_type: Box<ColType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "tag", rename_all = "camelCase"))]
pub enum ColType {
    EntityType { path: Path },
    PrimType { prim: PrimType },
    Tuple { fields: Vec<TupleField> },
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Schema {
    pub columns: Vec<ColType>,
    pub primary_key: Option<Vec<i64>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "tag", rename_all = "lowercase"))]
pub enum Lit {
    #[cfg_attr(feature = "serde", serde(rename = "int"))]
    Int { value: i64 },
    #[cfg_attr(feature = "serde", serde(rename = "string"))]
    String { value: String },
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ConsField {
    pub name: QName,
    pub term: Box<Term>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "tag", rename_all = "lowercase"))]
pub enum Term {
    Lit { lit: Lit },
    Var { index: FId },
    Proj { term: Box<Term>, field: QName },
    Cons { fields: Vec<ConsField> },
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ValueEntry {
    pub column: i64,
    pub term: Term,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Atom {
    pub table: Path,
    pub row_id: Option<Term>,
    pub values: Vec<ValueEntry>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "tag", rename_all = "lowercase"))]
pub enum Prop {
    Atom { atom: Atom },
    Eq { left: Term, right: Term },
    And { props: Vec<Prop> },
    Or { props: Vec<Prop> },
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Law {
    pub variables: Vec<ColType>,
    pub antecedent: Prop,
    pub consequent: Prop,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TableEntry {
    pub path: Path,
    pub table: Schema,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LawEntry {
    pub path: Path,
    pub law: Law,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FlatTheory {
    pub tables: Vec<TableEntry>,
    pub laws: Vec<LawEntry>,
}
