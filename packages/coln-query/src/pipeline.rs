// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The main entrypoint to the query engine. This wires up all stages into one
//! piece. The input to the [`Pipeline`] is a [(logical) query program](`Code`)
//! which is technically abstract syntax _forest_ (ASF). A _single_ query would
//! be a tree but a query _program_ can contain multiple queries, hence the name.
//! Importantly, said query program must be in a valid execution order already,
//! that is, any variable must have been declared first. It is up to the producer
//! of a query program to generate a valid ordering of statements, e.g., mapping
//! a Datalog program to a query program would require a topological sorting
//! of the rules, given that each rule turns into its own query.
//!
//! Here's an overview of the stages:
//!
//! 1. Typechecker: Takes an [ASF](Code) and type checks it.
//!    Currently skipped, as `coln-compiler` already emits type checked FLIR.
//! 2. Optimizer: Takes the [type-checked ASF](Code) and optimizes it
//!    _logically_. It can thereby rewrite parts of the queries. As of now,
//!    there is no logical optimization implemented. Returns a type-checked
//!    and optimized ASF.
//! 3. Resolver: Takes a [type-checked and optimized ASF](`Code`).
//!    It resolves all variables (of the host language) to slots in an
//!    interpretation [`Environment`].
//!    in a static pass over the ASF, speeding up variable lookup and checking
//!    for invalid variable access. Returns a [resolved ASF](ResolvedCode).
//! 4. Build: Takes a [resolved ASF](ResolvedCode) (and maybe type-checked and
//!    optimized) and hands it off to the supplied [`Backend`] to prepare
//!    for execution. A backend can work incrementally or batchwise.
//!    Returns a [`Runtime`](crate::relational::Runtime).
//! 5. Run: [`Runtime`](crate::relational::Runtime) is the runnable artifact:
//!    Feed input changes, advance, and output results. This is where
//!    incremental vs batch actually differ: DBSP's `commit` runs one
//!    incremental transaction and yields per-commit
//!    [`Delta`](crate::relational::Delta)s; the batch engine recomputes from
//!    the accumulated inputs and yields [`Snapshot`](crate::relational::Snapshot)s.
use std::num::NonZeroUsize;

use crate::{
    error::QueryEngineError,
    host::{
        Code, HostInterpreter, InterpreterContext, ScalarHost,
        resolver::ResolvedCode,
        variable::{Environment, Value},
    },
    optimizer::{NoOptimizer, Optimizer},
    relational::{Backend, batch::BatchBackend, incremental::DbspBackend},
};

pub struct Pipeline<O, B> {
    optimizer: O,
    threads: NonZeroUsize,
    backend: B,
}

impl Pipeline<NoOptimizer, DbspBackend> {
    /// The default incremental pipeline uses the [`DbspBackend`] and does not
    /// optimize logically ([`NoOptimizer`]) at the moment.
    pub fn incremental() -> Self {
        Self::new(DbspBackend::default())
    }
}

impl Pipeline<NoOptimizer, BatchBackend> {
    /// The default incremental pipeline uses the stub [`BatchBackend`] and does not
    /// optimize logically ([`NoOptimizer`]) at the moment.
    pub fn batch() -> Self {
        Self::new(BatchBackend::default())
    }
}

impl<B: Backend> Pipeline<NoOptimizer, B> {
    fn new(backend: B) -> Self {
        const FALLBACK: NonZeroUsize = NonZeroUsize::new(8).unwrap();
        let threads = std::thread::available_parallelism().unwrap_or(FALLBACK);
        Self {
            optimizer: NoOptimizer::default(),
            threads,
            backend,
        }
    }
}

impl<O: Optimizer, B: Backend> Pipeline<O, B> {
    pub fn with_optimizer<ONew: Optimizer>(self, optimizer: ONew) -> Pipeline<ONew, B> {
        Pipeline::<ONew, B> {
            optimizer,
            threads: self.threads,
            backend: self.backend,
        }
    }
    pub fn with_threads(mut self, threads: NonZeroUsize) -> Self {
        self.threads = threads;
        self
    }
    /// Optimize, resolve and evaluate a self-container **query** program
    /// (with relational operators) on the [`Backend`](`Self::backend`) and
    /// with the [`Optimizer`](`Self::optimizer`).
    pub fn runtime(self, plan: impl Into<Code>) -> Result<B::Runtime, QueryEngineError> {
        let type_checked = plan.into(); // Not for now.
        let optimized = self.optimizer.optimize(type_checked)?;
        let resolved = ResolvedCode::from(optimized)?;
        self.backend
            .build(self.threads, resolved)
            .map_err(|e| e.into().into())
    }
}

impl Pipeline<(), ()> {
    /// Resolve and evaluate a self-contained **host-language** program once (no
    /// relational operators), returning the value of its last statement. For pure
    /// scalar/host tests. Relational programs must go through [`Self::runtime`].
    pub fn run(host_code: impl Into<Code>) -> Result<Option<Value>, QueryEngineError> {
        let type_checked = host_code.into(); // Not for now.
        let resolved = ResolvedCode::from(type_checked)?;

        let mut environment = Environment::default();
        let mut interpreter_ctx = InterpreterContext::new(&mut environment);

        Ok(ScalarHost.interpret(resolved.as_code(), &mut interpreter_ctx)?)
    }
}
