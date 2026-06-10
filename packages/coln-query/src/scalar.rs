// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::variable::Value;
use std::fmt::{self, Display, Formatter};

/// Stores a scalar value plus its type. These are the kinds of values that
/// can be assumed in a relation's column (or a tuple's field).
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    size_of::SizeOf,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
)]
#[archive_attr(derive(Ord, Eq, PartialEq, PartialOrd))]
pub enum ScalarTypedValue {
    /// String.
    String(String),
    /// Unsigned integer value of 64 bits.
    Uint(u64),
    /// Signed integer value of 64 bits.
    Iint(i64),
    /// Boolean.
    Bool(bool),
    /// A single character.
    Char(char),
    /// Null.
    // The `Null` variant carries the unit type to align its field-arity with
    // other variants. That eases the definition of macros operating on the enum.
    Null(()),
}

macro_rules! expect_data {
    ( $instance:expr, $variant:path ) => {{
        match $instance {
            $variant(data) => data,
            _ => panic!(
                "Expected a value of type `{}` but got `{}`",
                stringify!($variant),
                $instance
            ),
        }
    }};
}

impl ScalarTypedValue {
    pub fn unwrap_into_string(&self) -> String {
        expect_data!(self, ScalarTypedValue::String).clone()
    }
    pub fn unwrap_into_uint(&self) -> u64 {
        *expect_data!(self, ScalarTypedValue::Uint)
    }
    pub fn unwrap_into_iint(&self) -> i64 {
        *expect_data!(self, ScalarTypedValue::Iint)
    }
    pub fn unwrap_into_bool(&self) -> bool {
        *expect_data!(self, ScalarTypedValue::Bool)
    }
    pub fn unwrap_into_char(&self) -> char {
        *expect_data!(self, ScalarTypedValue::Char)
    }
    pub fn unwrap_into_null(&self) {
        *expect_data!(self, ScalarTypedValue::Null)
    }
}

impl Default for ScalarTypedValue {
    fn default() -> Self {
        ScalarTypedValue::Null(())
    }
}

impl TryFrom<Value> for ScalarTypedValue {
    type Error = ();

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::String(value) => Ok(ScalarTypedValue::String(value)),
            Value::Uint(value) => Ok(ScalarTypedValue::Uint(value)),
            Value::Iint(value) => Ok(ScalarTypedValue::Iint(value)),
            Value::Bool(value) => Ok(ScalarTypedValue::Bool(value)),
            Value::Char(value) => Ok(ScalarTypedValue::Char(value)),
            Value::Null(()) => Ok(ScalarTypedValue::Null(())),
            _ => Err(()),
        }
    }
}

impl From<String> for ScalarTypedValue {
    fn from(value: String) -> Self {
        ScalarTypedValue::String(value)
    }
}

impl From<&str> for ScalarTypedValue {
    fn from(value: &str) -> Self {
        ScalarTypedValue::String(value.to_owned())
    }
}

impl From<u64> for ScalarTypedValue {
    fn from(value: u64) -> Self {
        ScalarTypedValue::Uint(value)
    }
}

impl From<i64> for ScalarTypedValue {
    fn from(value: i64) -> Self {
        ScalarTypedValue::Iint(value)
    }
}

impl From<char> for ScalarTypedValue {
    fn from(value: char) -> Self {
        ScalarTypedValue::Char(value)
    }
}

impl From<bool> for ScalarTypedValue {
    fn from(value: bool) -> Self {
        ScalarTypedValue::Bool(value)
    }
}

impl From<()> for ScalarTypedValue {
    fn from(value: ()) -> Self {
        ScalarTypedValue::Null(value)
    }
}

impl Display for ScalarTypedValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ScalarTypedValue::String(value) => write!(f, "{value}"),
            ScalarTypedValue::Uint(value) => write!(f, "{value}"),
            ScalarTypedValue::Iint(value) => write!(f, "{value}"),
            ScalarTypedValue::Bool(value) => write!(f, "{value}"),
            ScalarTypedValue::Char(value) => write!(f, "{value}"),
            ScalarTypedValue::Null(()) => write!(f, "null"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    String,
    Uint,
    Iint,
    Bool,
    Char,
    Null,
}
