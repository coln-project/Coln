// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::collections::HashMap;

use crate::{
    relation::{SchemaTuple, Tuple, TupleSchema},
    resolver::{ScopeStack, VariableMeta},
    scalar::ScalarTypedValue,
    stmt::Program,
    variable::Environment,
};

pub struct ProgramContext {
    /// Stores the program code executed so far by the interpreter.
    /// Read-only during resolution and interpretation.
    pub program: Program,
    /// The environment stores the variables and their values.
    /// Writable and _changing_ during interpretation.
    pub environment: Environment,
    /// Scope stack for the resolver.
    /// Writable during resolution and not accessed during interpretation.
    pub scopes: ScopeStack<VariableMeta>,
}

impl Default for ProgramContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgramContext {
    pub fn new() -> Self {
        Self {
            program: Program::empty(),
            environment: Environment::default(),
            scopes: ScopeStack::new(),
        }
    }
}

#[derive(Debug)]
pub struct InterpreterContext<'a> {
    pub environment: &'a mut Environment,
    /// If the interpreter runs within a DBSP context, we store the currently
    /// processing tuple here for making each of its fields accessible
    /// as a variable.
    // No need to wrap it in an Option because HashMap::new() does not allocate!
    pub tuple_vars: HashMap<String, ScalarTypedValue>,
    /// Stores the most recent alias for a relation.
    alias: Option<String>,
}

impl InterpreterContext<'_> {
    pub fn new(environment: &mut Environment) -> InterpreterContext<'_> {
        InterpreterContext {
            environment,
            tuple_vars: HashMap::new(),
            alias: None,
        }
    }
    pub fn set_alias(&mut self, alias: String) {
        self.alias = Some(alias);
    }
    pub fn consume_alias(&mut self) -> Option<String> {
        self.alias.take()
    }
    pub fn extend_tuple_ctx<T: Tuple>(
        &mut self,
        alias: &Option<String>,
        schema: &TupleSchema,
        tuple: &T,
    ) {
        self.tuple_vars
            .extend(SchemaTuple::new(schema, tuple).named_fields(alias));
    }
    pub fn clear_tuple_ctx(&mut self) {
        self.tuple_vars.clear();
    }
}

pub struct ResolverContext<'a> {
    pub scopes: &'a mut ScopeStack<VariableMeta>,
    pub is_tuple_context: bool,
}

impl ResolverContext<'_> {
    pub fn new(scopes: &mut ScopeStack<VariableMeta>) -> ResolverContext<'_> {
        ResolverContext {
            scopes,
            is_tuple_context: false,
        }
    }
    pub fn begin_tuple_context(&mut self) {
        self.is_tuple_context = true;
    }
    pub fn end_tuple_context(&mut self) {
        self.is_tuple_context = false;
    }
}
