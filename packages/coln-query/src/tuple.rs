// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::variable::Value;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tuple {
    inner: Rc<[Value]>,
}

// TODO: Write test case with tuple (multiple DBSP circuits as output)!

impl Tuple {
    pub fn empty() -> Self {
        Self { inner: Rc::new([]) }
    }
    pub fn get(&self, at: usize) -> &Value {
        &self.inner[at]
    }
}

impl<T: Into<Rc<[Value]>>> From<T> for Tuple {
    fn from(value: T) -> Self {
        Self {
            inner: value.into(),
        }
    }
}

impl<T: Into<Value>> FromIterator<T> for Tuple {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            inner: iter.into_iter().map(Into::<Value>::into).collect(),
        }
    }
}

impl fmt::Display for Tuple {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        let mut iter = self.inner.iter();
        if let Some(first) = iter.next() {
            write!(f, "{}", first)?;
            for v in iter {
                write!(f, ", {}", v)?;
            }
        }
        write!(f, ")")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn tuple_creation_and_get() {
        let check_tuple = |tuple: Tuple| {
            assert_eq!(*tuple.get(0), Value::Uint(1));
            assert_eq!(*tuple.get(1), Value::Uint(2));
        };

        let vec = vec![Value::Uint(1), Value::Uint(2)];
        let tuple = Tuple::from(vec);
        check_tuple(tuple);

        let array = [Value::Uint(1), Value::Uint(2)];
        let tuple = Tuple::from(array);
        check_tuple(tuple);

        let slice = &[Value::Uint(1), Value::Uint(2)][..];
        let tuple = Tuple::from(slice);
        check_tuple(tuple);

        let tuple = vec![1_u32, 2].into_iter().collect::<Tuple>();
        check_tuple(tuple);
    }

    #[test]
    fn tuple_display() {
        let tuple = Tuple::from([Value::from(1), Value::from(true), Value::from("Hi")]);
        let display = format!("{tuple}");
        assert_eq!(display, "(1, true, \"Hi\")");
    }

    #[test]
    fn tuple_eq() {
        let a = Tuple::from([Value::from(1), Value::from(true)]);
        let b = Tuple::from(vec![Value::from(1), Value::from(true)]);
        let c = Tuple::empty();

        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
