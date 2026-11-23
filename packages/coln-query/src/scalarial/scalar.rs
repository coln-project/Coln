// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A scalar of the host language.

/// Generated as the scalar subset of the host's
/// [`Value`](crate::host::variable::Value), see there.
pub use crate::host::variable::ScalarTypedValue;
use std::fmt::{self, Display, Formatter};

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
    /// Which [`ScalarType`] this value is of. The value side of the same
    /// distinction the type side spells out, so a consumer holding data and a
    /// schema can check one against the other.
    pub fn scalar_type(&self) -> ScalarType {
        match self {
            ScalarTypedValue::String(_) => ScalarType::String,
            ScalarTypedValue::Uint(_) => ScalarType::Uint,
            ScalarTypedValue::Iint(_) => ScalarType::Iint,
            ScalarTypedValue::Bool(_) => ScalarType::Bool,
            ScalarTypedValue::Char(_) => ScalarType::Char,
            ScalarTypedValue::Null(()) => ScalarType::Null,
        }
    }
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
            // Quoted and escaped, as a bare string would be indistinguishable
            // from a variable of the same name when printed as a literal.
            ScalarTypedValue::String(value) => write!(f, "{value:?}"),
            ScalarTypedValue::Uint(value) => write!(f, "{value}"),
            ScalarTypedValue::Iint(value) => write!(f, "{value}"),
            ScalarTypedValue::Bool(value) => write!(f, "{value}"),
            ScalarTypedValue::Char(value) => write!(f, "{value:?}"),
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

impl Display for ScalarType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let name = match self {
            ScalarType::String => "string",
            ScalarType::Uint => "uint",
            ScalarType::Iint => "iint",
            ScalarType::Bool => "bool",
            ScalarType::Char => "char",
            ScalarType::Null => "null",
        };
        write!(f, "{name}")
    }
}
