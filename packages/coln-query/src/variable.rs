// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{
    expr::Literal, function::FunctionRef, relation::RelationRef, scalar::ScalarTypedValue,
    tuple::Tuple,
};
use std::{
    cell::{Ref, RefCell},
    fmt,
    rc::Rc,
};

/// The value of a variable of the [`Interpreter`](crate::interpreter::Interpreter)
/// at runtime.
/// Compared to [`ScalarTypedValue`], this type allows
/// [functions](`FunctionRef`) and [relations](`RelationRef`), too.
#[derive(Clone, Debug)]
pub enum Value {
    /// String.
    // TODO: maybe make strings reference counted to avoid cloning costs?
    String(String),
    /// Unsigned integer value of 64 bits.
    Uint(u64),
    /// Signed integer value of 64 bits.
    Iint(i64),
    /// Boolean.
    Bool(bool),
    /// A single character as defined by Rust's [`char`].
    Char(char),
    /// Null.
    // The `Null` variant carries the unit type to align its field-arity with
    // other variants. That eases the definition of macros operating on the enum.
    Null(()),
    /// Function.
    Function(FunctionRef),
    /// Relation.
    Relation(RelationRef),
    /// Tuple.
    Tuple(Tuple),
}

impl Eq for Value {}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Uint(a), Value::Uint(b)) => a == b,
            (Value::Iint(a), Value::Iint(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null(()), Value::Null(())) => true,
            (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
            (Value::Relation(a), Value::Relation(b)) => Rc::ptr_eq(a, b),
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            _ => false,
        }
    }
}

impl Default for Value {
    fn default() -> Self {
        Value::Null(())
    }
}

impl From<RelationRef> for Value {
    fn from(value: RelationRef) -> Self {
        Value::Relation(value)
    }
}

impl From<ScalarTypedValue> for Value {
    fn from(value: ScalarTypedValue) -> Self {
        match value {
            ScalarTypedValue::String(value) => Value::String(value),
            ScalarTypedValue::Uint(value) => Value::Uint(value),
            ScalarTypedValue::Iint(value) => Value::Iint(value),
            ScalarTypedValue::Bool(value) => Value::Bool(value),
            ScalarTypedValue::Char(value) => Value::Char(value),
            ScalarTypedValue::Null(()) => Value::Null(()),
        }
    }
}

impl From<Literal> for Value {
    fn from(literal: Literal) -> Self {
        match literal {
            Literal::String(value) => Value::String(value),
            Literal::Uint(value) => Value::Uint(value),
            Literal::Iint(value) => Value::Iint(value),
            Literal::Bool(value) => Value::Bool(value),
            Literal::Null(()) => Value::Null(()),
            Literal::Relation(value) => Value::Relation(Rc::new(RefCell::new(value))),
        }
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Self::Uint(value)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Self::Uint(value as u64)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Self::Iint(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Self::Iint(value as i64)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(value) => write!(f, "\"{value}\""),
            Value::Uint(value) => write!(f, "{value}"),
            Value::Iint(value) => write!(f, "{value}"),
            Value::Bool(value) => write!(f, "{value}"),
            Value::Char(value) => write!(f, "{value}"),
            Value::Null(()) => write!(f, "null"),
            Value::Function(function) => write!(f, "{}", function.borrow()),
            Value::Relation(relation) => write!(f, "{}", relation.borrow()),
            Value::Tuple(tuple) => write!(f, "{}", tuple),
        }
    }
}

/// First entry is the scope, second entry is the variable within that scope.
pub type VariableSlot = (usize, usize);

/// A scope in the environment. Roughly, the space between two curly braces `{}`.
#[derive(Clone, Debug)]
struct Scope {
    /// Variable slots of an environment.
    inner: Rc<RefCell<Vec<Value>>>,
}

impl Scope {
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(Vec::new())),
        }
    }
    fn define_var(&mut self, val: Value) {
        self.inner.borrow_mut().push(val);
    }
    fn assign_var(&mut self, slot_idx: usize, val: Value) {
        self.inner.borrow_mut()[slot_idx] = val;
    }
    fn lookup_var(&self, slot_idx: usize) -> Ref<'_, Value> {
        let vec = self.inner.borrow();
        Ref::map(vec, |vec| &vec[slot_idx])
    }
}

pub const SCOPES_CAPACITY: usize = 8;

/// The environment of the interpreter. It holds the variables.
#[derive(Clone, Debug)]
pub struct Environment {
    /// The vector models a stack of scopes with the root environment at
    /// the bottom and the innermost scope at the top.
    scopes: Vec<Scope>,
}

impl Default for Environment {
    fn default() -> Self {
        let mut environment = Self {
            scopes: Vec::with_capacity(SCOPES_CAPACITY),
        };
        // Create the global scope.
        environment.begin_scope();
        environment
    }
}

impl Environment {
    pub fn just_global(&self) -> bool {
        self.scopes.len() == 1
    }
    pub fn begin_scope(&mut self) {
        self.scopes.push(Scope::new());
    }
    pub fn end_scope(&mut self) {
        self.scopes.pop();
    }
    pub fn define_var<T: Into<Value>>(&mut self, val: T) {
        self.scopes
            .last_mut()
            .expect("no root env")
            .define_var(val.into());
    }
    pub fn assign_var(&mut self, at: &VariableSlot, val: Value) {
        let (scope_idx, slot_idx) = *at;
        self.scopes[scope_idx].assign_var(slot_idx, val);
    }
    pub fn lookup_var(&self, at: &VariableSlot) -> Ref<'_, Value> {
        let (scope_idx, slot_idx) = *at;
        self.scopes[scope_idx].lookup_var(slot_idx)
    }
}
