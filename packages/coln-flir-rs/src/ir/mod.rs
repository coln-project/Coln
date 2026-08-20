// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

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

/// A column name is given by a [`Path`].
pub type ColName = Path;

/// An index into the [`varNames`](Rule::var_names) and
/// [`varTypes`](Rule::var_types) arrays of a [`Rule`].
///
/// Note: An `FId` in `coln-compiler`.
pub type VarIdx = u64;

/// An index into a relation's physical [`columns`](Schema::columns).
pub type ColumnIdx = u64;

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
            BuiltinTy::BuiltinInt => serializer.serialize_str("builtinInt"),
            BuiltinTy::BuiltinStr => serializer.serialize_str("builtinString"),
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
            "builtinInt" => Ok(BuiltinTy::BuiltinInt),
            "builtinString" => Ok(BuiltinTy::BuiltinStr),
            _ => Err(DeError::unknown_variant(
                &s,
                &["builtinInt", "builtinString"],
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tag", rename_all = "camelCase")]
pub enum ColType {
    /// A foreign key into another table by referencing its _row id_ through
    /// the provided path.
    RowId { path: Path },
    /// A data column with the scalar type [`BuiltinTy`].
    #[serde(rename = "builtin")]
    BuiltinTy {
        #[serde(rename = "type")]
        builtin_ty: BuiltinTy,
    },
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
    /// A base table of the extensional database (EDB).
    Table,
    /// A derived view of the intensional database (IDB).
    View(Materialization),
    /// Tell `coln-store` to create an index and possibly hint to `coln-query`.
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

/// Describes a schema of a relation.
///
/// Note: An `Entity` in `coln-compiler`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub entity_variant: EntityVariant,
    /// The columns of the table in their physical order.
    pub columns: Vec<ColumnEntry>,
    /// A `None` indicates that there is no primary key. `Some(vec![])` means
    /// that there is at most one row in the table. `Some(vec![ColA, ColB])`
    /// encodes a compound primary key consisting of the columns `ColA` and
    ///  `ColB`.
    ///
    /// At the moment there is only support for a single (compound) primary key.
    pub primary_key: Option<Vec<ColName>>,
}

/// A literal expression.
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
    Var { index: VarIdx },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueEntry {
    pub column: ColumnIdx,
    pub term: Term,
}

/// An [`Atom`] references an entity (a relation or a table) to bring some of
/// its fields into the scope of a [`Rule`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Atom {
    /// The "name" of the entity being referenced by this [`Atom`].
    pub entity: Path,
    /// To bring the `row_id` of the [`Entity`](Self::entity) into scope.
    ///
    /// Note: A [`Some(Term::Lit)`](Term::Lit) does not make sense in this
    /// context, as we do not support a row id literal at the moment, I suppose.
    pub row_id: Option<Term>,
    /// To bring some columns of the [`Entity`](Self::entity) into scope.
    pub values: Vec<ValueEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tag", rename_all = "lowercase")]
pub enum Prop {
    Atom {
        atom: Atom,
    },
    Eq {
        #[serde(flatten)]
        equality: Equality,
    },
}

/// An equality condition between the left and the right term, that is,
/// we assert `left == right`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Equality {
    pub left: Term,
    pub right: Term,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleVariant {
    /// _Chased_ rules are not yet fully alive but become relevant once initial
    /// models land.
    Chased,
    /// Violations of _enforced_ rules cause a transaction to abort.
    Enforced,
    /// Violations of _monitored_ rules are just reported back to the user but
    /// still allow a transaction to commit.
    Monitored,
}

/// A `Rule` is an implication and must be true in all valid states of
/// `coln-store` and `coln-query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub rule_variant: RuleVariant,
    /// Assigns some names to the variables the rule binds.
    ///
    /// Note: Must be of the same arity as [`Self::var_types`].
    pub var_names: Vec<ColName>,
    /// Tells the types of the variables the rule binds.
    ///
    /// Note: Must be of the same arity as [`Self::var_names`].
    pub var_types: Vec<ColType>,
    /// The left-hand side of the implication.
    pub antecedents: Vec<Prop>,
    /// The right-hand side of the implication.
    pub consequents: Vec<Prop>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableEntry {
    /// The "name" of the table.
    pub path: Path,
    #[serde(rename = "value")]
    pub table: Schema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEntry {
    /// The "name" of the rule.
    pub path: Path,
    #[serde(rename = "value")]
    pub rule: Rule,
}

/// The top-level type of a flattened realm and the starting point of the FLIR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatRealm {
    /// The tables of the flattened realm.
    #[serde(rename = "entities")]
    pub tables: Vec<TableEntry>,
    /// The rules (laws) of the flattened realm.
    pub rules: Vec<RuleEntry>,
}
