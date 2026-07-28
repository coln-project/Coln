// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The host language is a small scripting language which allows to capture
//! expressions of scalar computations and to express a collection of queries.
//! The latter is important because `coln-flir` rarely emits just a single
//! query but an entire program of queries.

pub mod expr;
pub mod function;
pub mod interpreter;
pub mod operator;
pub mod resolver;
pub mod stmt;
pub mod tuple;
pub mod variable;

pub use interpreter::{HostInterpreter, InterpreterContext, ScalarHost};

pub type Code = Vec<stmt::Stmt>;
