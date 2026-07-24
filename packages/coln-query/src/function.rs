// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{expr::FunctionExpr, type_resolver::ExprType, variable::Environment};
use std::{
    cell::RefCell,
    fmt::{self, Debug, Display},
    rc::Rc,
};

/// Convenience type alias for a reference to a [`Function`].
pub type FunctionRef = Rc<RefCell<Function>>;

/// A helper function to avoid writing the interior mutability boilerplate.
pub fn new_function(
    name: Option<String>,
    declaration: FunctionExpr,
    environment: Environment,
) -> FunctionRef {
    Rc::new(RefCell::new(Function::new(name, declaration, environment)))
}

#[derive(Clone)]
pub struct Function {
    pub name: Option<String>,
    pub declaration: FunctionExpr,
    pub environment: Environment,
}

impl Function {
    pub fn new(name: Option<String>, declaration: FunctionExpr, environment: Environment) -> Self {
        Self {
            name,
            declaration,
            environment,
        }
    }
    pub fn declaration(&self) -> &FunctionExpr {
        &self.declaration
    }
    pub fn arity(&self) -> usize {
        self.declaration().parameters.len()
    }
    fn to_string_helper(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params = format!("({})", self.declaration().parameters.join(", "));
        if let Some(name) = &self.name {
            write!(f, "<fn {name}{params}>")
        } else {
            write!(f, "<anonymous fn{params}>")
        }
    }
}

impl Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_string_helper(f)
    }
}

impl Debug for Function {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_string_helper(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionType {
    // TODO: parameters' type
    pub return_type: Box<ExprType>,
}

impl Display for FunctionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<fn -> {}>", self.return_type)
    }
}
