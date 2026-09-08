// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The engine's value types.
//!
//! Every column has a [`ScalarType`] and every cell holds a [`Value`].
//! Inside the engine a cell is stored as a **normalized key**: a `u64`
//! whose order agrees with the natural order of the value's type. The
//! join machinery only ever compares and sorts, so it runs on plain
//! integers for every type alike; types matter at the boundary, where
//! values are encoded on the way in and decoded on the way out. Decoding
//! a key needs the column's type and, for strings, the [`Dictionary`]: a
//! [`Relation`](crate::relation::Relation) carries the former, a
//! [`Catalog`](crate::query::Catalog) owns the latter.
//!
//! Key encodings:
//!
//! | type     | key                                   | order preserved       |
//! |----------|---------------------------------------|-----------------------|
//! | `Uint`   | the value                             | yes                   |
//! | `Iint`   | the value with its sign bit flipped   | yes                   |
//! | `Bool`   | 0 or 1                                | yes                   |
//! | `Char`   | the Unicode scalar value              | yes                   |
//! | `String` | dictionary code, assigned on first use | no (insertion order) |
//!
//! String keys follow insertion order, not lexicographic order: a join
//! needs equality only, and equality is what the dictionary preserves.
//! Range predicates over strings would need an order-preserving
//! dictionary; none are supported yet.

use std::collections::HashMap;
use std::fmt;

use anyhow::{Result, bail};

/// A cell as the engine stores it. See the module docs for the encoding.
pub type Key = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScalarType {
    /// Unsigned 64-bit integer.
    Uint,
    /// Signed 64-bit integer.
    Iint,
    Bool,
    /// One Unicode scalar value.
    Char,
    String,
}

impl fmt::Display for ScalarType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ScalarType::Uint => "uint",
            ScalarType::Iint => "iint",
            ScalarType::Bool => "bool",
            ScalarType::Char => "char",
            ScalarType::String => "string",
        })
    }
}

/// A typed cell value at the engine's boundary.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Value {
    Uint(u64),
    Iint(i64),
    Bool(bool),
    Char(char),
    String(String),
}

const SIGN_BIT: u64 = 1 << 63;

impl Value {
    pub fn scalar_type(&self) -> ScalarType {
        match self {
            Value::Uint(_) => ScalarType::Uint,
            Value::Iint(_) => ScalarType::Iint,
            Value::Bool(_) => ScalarType::Bool,
            Value::Char(_) => ScalarType::Char,
            Value::String(_) => ScalarType::String,
        }
    }

    /// Encode into the key space, interning a string on first use.
    pub fn to_key(&self, dict: &mut Dictionary) -> Key {
        match self {
            Value::String(s) => dict.intern(s),
            other => other
                .key_if_known(dict)
                .expect("only strings depend on the dictionary"),
        }
    }

    /// Encode without touching the dictionary. `None` for a string the
    /// dictionary does not know: no stored cell can be equal to it.
    pub fn key_if_known(&self, dict: &Dictionary) -> Option<Key> {
        Some(match self {
            Value::Uint(u) => *u,
            Value::Iint(i) => (*i as u64) ^ SIGN_BIT,
            Value::Bool(b) => u64::from(*b),
            Value::Char(c) => u64::from(*c),
            Value::String(s) => return dict.code(s),
        })
    }

    /// Decode a key of type `ty`. Fails on keys no value of that type
    /// encodes to, which indicates a mismatch between key and schema.
    pub fn from_key(ty: ScalarType, key: Key, dict: &Dictionary) -> Result<Value> {
        Ok(match ty {
            ScalarType::Uint => Value::Uint(key),
            ScalarType::Iint => Value::Iint((key ^ SIGN_BIT) as i64),
            ScalarType::Bool => match key {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                other => bail!("key {other} is not a boolean"),
            },
            ScalarType::Char => match u32::try_from(key).ok().and_then(char::from_u32) {
                Some(c) => Value::Char(c),
                None => bail!("key {key} is not a character"),
            },
            ScalarType::String => match dict.string(key) {
                Some(s) => Value::String(s.to_owned()),
                None => bail!("key {key} is not a dictionary code"),
            },
        })
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Uint(u) => write!(f, "{u}"),
            Value::Iint(i) => write!(f, "{i}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Char(c) => write!(f, "{c:?}"),
            Value::String(s) => write!(f, "{s:?}"),
        }
    }
}

impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Value::Uint(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Iint(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<char> for Value {
    fn from(v: char) -> Self {
        Value::Char(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_owned())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

/// One column of a relation: its name and type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub ty: ScalarType,
}

impl Column {
    pub fn new(name: impl Into<String>, ty: ScalarType) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

/// The columns of a relation, in tuple order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schema {
    columns: Vec<Column>,
}

impl Schema {
    pub fn new(columns: impl IntoIterator<Item = Column>) -> Self {
        Self {
            columns: columns.into_iter().collect(),
        }
    }

    /// A schema of unsigned integer columns with the given names.
    pub fn uint(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::new(names.into_iter().map(|n| Column::new(n, ScalarType::Uint)))
    }

    /// Number of columns.
    pub fn arity(&self) -> usize {
        self.columns.len()
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn name(&self, col: usize) -> &str {
        &self.columns[col].name
    }

    pub fn column_type(&self, col: usize) -> ScalarType {
        self.columns[col].ty
    }

    pub fn names(&self) -> Vec<String> {
        self.columns.iter().map(|c| c.name.clone()).collect()
    }

    pub fn types(&self) -> Vec<ScalarType> {
        self.columns.iter().map(|c| c.ty).collect()
    }

    /// The same columns with new names (types unchanged).
    pub fn renamed(&self, names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let columns: Vec<Column> = names
            .into_iter()
            .zip(&self.columns)
            .map(|(name, c)| Column::new(name, c.ty))
            .collect();
        assert_eq!(columns.len(), self.arity(), "one name per column");
        Self { columns }
    }
}

impl FromIterator<Column> for Schema {
    fn from_iter<I: IntoIterator<Item = Column>>(iter: I) -> Self {
        Self::new(iter)
    }
}

/// Stable string codes: every distinct string gets the next free code on
/// first use and keeps it. Shared by all relations of a catalog, so a
/// string column joins with any other string column by key equality.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dictionary {
    strings: Vec<String>,
    codes: HashMap<String, Key>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self::default()
    }

    /// The code of `s`, assigning a new one if the string is unknown.
    pub fn intern(&mut self, s: &str) -> Key {
        if let Some(&code) = self.codes.get(s) {
            return code;
        }
        let code = self.strings.len() as Key;
        self.strings.push(s.to_owned());
        self.codes.insert(s.to_owned(), code);
        code
    }

    /// The code of `s`, if it has one.
    pub fn code(&self, s: &str) -> Option<Key> {
        self.codes.get(s).copied()
    }

    /// The string behind `code`, if the code exists.
    pub fn string(&self, code: Key) -> Option<&str> {
        usize::try_from(code)
            .ok()
            .and_then(|i| self.strings.get(i))
            .map(String::as_str)
    }

    /// Number of distinct strings.
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_round_trips() {
        let mut dict = Dictionary::new();
        let values = [
            Value::Uint(0),
            Value::Uint(u64::MAX),
            Value::Iint(i64::MIN),
            Value::Iint(-1),
            Value::Iint(0),
            Value::Iint(i64::MAX),
            Value::Bool(false),
            Value::Bool(true),
            Value::Char('a'),
            Value::Char('\u{10FFFF}'),
            Value::from("alpha"),
            Value::from(""),
        ];
        for v in &values {
            let key = v.to_key(&mut dict);
            let back = Value::from_key(v.scalar_type(), key, &dict).unwrap();
            assert_eq!(&back, v);
        }
    }

    #[test]
    fn integer_keys_preserve_order() {
        let dict = Dictionary::new();
        let ints = [i64::MIN, -1_000, -1, 0, 1, 1_000, i64::MAX];
        let keys: Vec<Key> = ints
            .iter()
            .map(|&i| Value::Iint(i).key_if_known(&dict).unwrap())
            .collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]), "{keys:?}");
        assert!(Value::Bool(false).key_if_known(&dict) < Value::Bool(true).key_if_known(&dict));
        assert!(Value::Char('a').key_if_known(&dict) < Value::Char('b').key_if_known(&dict));
    }

    #[test]
    fn dictionary_codes_are_stable_and_shared() {
        let mut dict = Dictionary::new();
        let a = dict.intern("a");
        let b = dict.intern("b");
        assert_ne!(a, b);
        assert_eq!(dict.intern("a"), a, "re-interning keeps the code");
        assert_eq!(dict.code("b"), Some(b));
        assert_eq!(dict.code("c"), None);
        assert_eq!(dict.string(a), Some("a"));
        assert_eq!(dict.string(99), None);
        assert_eq!(Value::from("c").key_if_known(&dict), None);
        assert_eq!(dict.len(), 2);
    }

    #[test]
    fn decoding_rejects_keys_outside_the_type() {
        let dict = Dictionary::new();
        assert!(Value::from_key(ScalarType::Bool, 2, &dict).is_err());
        assert!(Value::from_key(ScalarType::Char, 0xD800, &dict).is_err());
        assert!(Value::from_key(ScalarType::String, 0, &dict).is_err());
    }

    #[test]
    fn schema_helpers() {
        let s = Schema::new([
            Column::new("id", ScalarType::Uint),
            Column::new("name", ScalarType::String),
        ]);
        assert_eq!(s.arity(), 2);
        assert_eq!(s.names(), vec!["id", "name"]);
        assert_eq!(s.column_type(1), ScalarType::String);
        let r = s.renamed(["a", "b"]);
        assert_eq!(r.name(0), "a");
        assert_eq!(r.types(), s.types());
        assert_eq!(Schema::uint(["x"]).column_type(0), ScalarType::Uint);
    }
}
