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

impl Tuple {
    pub fn empty() -> Self {
        Self { inner: Rc::new([]) }
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn get(&self, at: usize) -> &Value {
        &self.inner[at]
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Value> {
        self.inner.iter()
    }
}

impl<'a> IntoIterator for &'a Tuple {
    type Item = &'a Value;
    type IntoIter = std::slice::Iter<'a, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl IntoIterator for Tuple {
    type Item = Value;
    type IntoIter = TupleIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        let back = self.inner.len();
        TupleIntoIter {
            inner: self.inner,
            front: 0,
            back,
        }
    }
}

/// A consuming iterator over the [`Value`]s of a [`Tuple`].
///
/// The backing `Rc<[Value]>` may be shared, so the values cannot be moved out;
/// they are cloned lazily as the iterator is advanced. This avoids allocating
/// an intermediate buffer up front.
pub struct TupleIntoIter {
    inner: Rc<[Value]>,
    /// Index of the next front element.
    front: usize,
    /// Index one past the next back element.
    back: usize,
}

impl Iterator for TupleIntoIter {
    type Item = Value;

    fn next(&mut self) -> Option<Value> {
        if self.front == self.back {
            return None;
        }
        let value = self.inner[self.front].clone();
        self.front += 1;
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.back - self.front;
        (remaining, Some(remaining))
    }
}

impl DoubleEndedIterator for TupleIntoIter {
    fn next_back(&mut self) -> Option<Value> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.inner[self.back].clone())
    }
}

impl ExactSizeIterator for TupleIntoIter {}

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

    #[test]
    fn tuple_iter() {
        let tuple = Tuple::from([Value::Uint(1), Value::Uint(2), Value::Uint(3)]);
        let expected = [Value::Uint(1), Value::Uint(2), Value::Uint(3)];

        // via the inherent `iter` method
        assert!(tuple.iter().eq(expected.iter()));

        // via `IntoIterator` for `&Tuple` (e.g. in a `for` loop)
        let collected: Vec<&Value> = (&tuple).into_iter().collect();
        assert_eq!(collected, expected.iter().collect::<Vec<_>>());

        let mut count = 0;
        for value in &tuple {
            assert_eq!(*value, expected[count]);
            count += 1;
        }
        assert_eq!(count, tuple.len());

        // an empty tuple yields nothing
        assert_eq!(Tuple::empty().iter().next(), None);
    }

    #[test]
    fn tuple_into_iter() {
        let tuple = Tuple::from([Value::Uint(1), Value::Uint(2), Value::Uint(3)]);
        let expected = vec![Value::Uint(1), Value::Uint(2), Value::Uint(3)];

        // consuming iteration yields owned `Value`s
        let collected: Vec<Value> = tuple.clone().into_iter().collect();
        assert_eq!(collected, expected);

        // `ExactSizeIterator` reports the correct length
        let mut iter = tuple.clone().into_iter();
        assert_eq!(iter.len(), 3);
        iter.next();
        assert_eq!(iter.len(), 2);

        // `DoubleEndedIterator` yields from the back
        let reversed: Vec<Value> = tuple.clone().into_iter().rev().collect();
        assert_eq!(
            reversed,
            expected.iter().rev().cloned().collect::<Vec<Value>>(),
        );

        // front and back cursors meet without overlap
        let mut iter = tuple.clone().into_iter();
        assert_eq!(iter.next(), Some(Value::Uint(1)));
        assert_eq!(iter.next_back(), Some(Value::Uint(3)));
        assert_eq!(iter.next(), Some(Value::Uint(2)));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);

        // usable directly in a `for` loop
        let mut count = 0;
        for value in tuple {
            assert_eq!(value, expected[count]);
            count += 1;
        }
        assert_eq!(count, expected.len());

        // an empty tuple yields nothing
        assert_eq!(Tuple::empty().into_iter().next(), None);
    }
}
