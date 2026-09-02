// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Value<I> {
    Id(I),
    Int(i32),
    Str(String),
}

impl<I> Value<I> {
    pub fn map<J, F: Fn(&I) -> J>(&self, f: F) -> Value<J> {
        match self {
            Value::Id(i) => Value::Id(f(i)),
            Value::Int(i) => Value::Int(*i),
            Value::Str(s) => Value::Str(s.clone()),
        }
    }

    pub fn map_owned<J, F: Fn(I) -> J>(self, f: F) -> Value<J> {
        match self {
            Value::Id(i) => Value::Id(f(i)),
            Value::Int(i) => Value::Int(i),
            Value::Str(s) => Value::Str(s),
        }
    }
}

impl<I> From<i32> for Value<I> {
    fn from(value: i32) -> Self {
        Value::Int(value)
    }
}

impl<I> From<String> for Value<I> {
    fn from(value: String) -> Self {
        Value::Str(value)
    }
}

impl<I> From<&str> for Value<I> {
    fn from(value: &str) -> Self {
        Value::Str(value.into())
    }
}
