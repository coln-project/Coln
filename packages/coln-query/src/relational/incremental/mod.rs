// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A [DBSP](`dbsp`) powered incremental [`Backend`], that is [`DbspBackend`],
//! and [`Runtime`], which is [`DbspRuntime`].

use super::relation::TupleValue;
use super::{Backend, Runtime};
use crate::error::{BuildError, LoweringError, RuntimeError};
use crate::{
    api::deltas::ZWeight,
    host::{
        Code, HostInterpreter, InterpreterContext, resolver::ResolvedCode, variable::Environment,
    },
    relational::{
        Delta,
        expr::{OutputKind, SinkId, SourceId},
    },
    scalarial::{RowScalarEngine, TreeWalk},
};
use dbsp::{DbspHandle, DbspInputs, DbspOutput, Runtime as CircuitRuntime};
use interpreter::DbspInterpreter;
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroUsize,
};

pub mod dbsp;
pub mod interpreter;
pub mod lowering;
pub mod operators;

/// The incremental backend: compiles the plan into a standing DBSP circuit.
///
/// Generic over the row scalar engine `E` it drives on the hot path — `TreeWalk`
/// by default, any other [`RowScalarEngine`] (e.g. a future bytecode VM) by
/// choosing it. `E: Send` because the circuit constructor runs on worker threads.
pub struct DbspBackend<E: RowScalarEngine + Send = TreeWalk> {
    scalar_engine: E,
}

impl Default for DbspBackend<TreeWalk> {
    fn default() -> Self {
        Self::new(TreeWalk)
    }
}

impl<E: RowScalarEngine + Send> DbspBackend<E> {
    pub fn new(engine: E) -> Self {
        Self {
            scalar_engine: engine,
        }
    }
}

impl<E: RowScalarEngine + Send> Backend for DbspBackend<E> {
    type Runtime = DbspRuntime;
    type Error = BuildError;

    /// A DBSP circuit joins two streams at a time, so every
    /// [`MultiWayEquiJoinExpr`](crate::relational::expr::MultiWayEquiJoinExpr)
    /// has to become a chain of binary ones before
    /// [`build`](Self::build) walks the plan. See [`lowering`].
    fn lower(&self, plan: Code) -> Result<Code, LoweringError> {
        lowering::fold_multi_way_joins(plan)
    }

    fn build(self, threads: NonZeroUsize, plan: ResolvedCode) -> Result<DbspRuntime, Self::Error> {
        let engine = self.scalar_engine;
        let (handle, (inputs, outputs)) =
            CircuitRuntime::init_circuit(threads, move |root_circuit| {
                // The plan is already resolved, so we interpret directly (no
                // resolver pass here) with a fresh environment.
                let mut environment = Environment::default();
                let mut ctx = InterpreterContext::new(&mut environment);
                let mut interpreter = DbspInterpreter::new(root_circuit.clone(), engine.clone());
                // Walk the plan for its side effects: each `SourceExpr` leaf
                // wires a fresh input stream (deduplicated by name) and each
                // `OutputExpr` tap wires an output read handle. The plan's final
                // value is no longer the output — inputs and outputs name
                // themselves.
                interpreter.interpret(plan.into_code().iter(), &mut ctx)?;
                let inputs = interpreter.take_inputs();
                let outputs = interpreter.take_outputs();
                Ok((inputs, outputs))
            })?;
        // Split the collected taps by kind: `Channel` taps are readable by name
        // via `output`; `Cli` taps are print-only (auto-printed after each
        // `commit`) and deliberately kept *out* of the readable map, so reading
        // one by name fails loudly instead of returning a drained, empty batch.
        // Output names must be unique across *both* kinds.
        let mut outputs_by_name: HashMap<SinkId, DbspOutput> = HashMap::new();
        let mut cli_outputs: Vec<(SinkId, DbspOutput)> = Vec::new();
        let mut seen: HashSet<SinkId> = HashSet::new();
        for (id, kind, output) in outputs {
            if !seen.insert(id.clone()) {
                return Err(BuildError::new(format!(
                    "duplicate output name '{}'",
                    id.as_str()
                )));
            }
            match kind {
                OutputKind::Cli => cli_outputs.push((id, output)),
                OutputKind::Channel => {
                    outputs_by_name.insert(id, output);
                }
            }
        }
        Ok(DbspRuntime {
            handle,
            inputs,
            outputs: outputs_by_name,
            cli_outputs,
        })
    }
}

/// A standing DBSP circuit plus its input feed handles (by [`SourceId`]) and
/// output read handles (by [`SinkId`]). Yields per-transaction [`Delta`]s.
pub struct DbspRuntime {
    handle: DbspHandle,
    inputs: DbspInputs,
    /// Read handles for the [`OutputKind::Channel`] taps, keyed by name. These
    /// are the outputs [`output`](Runtime::output) can read.
    outputs: HashMap<SinkId, DbspOutput>,
    /// The [`OutputKind::Cli`] taps (name + handle), printed after each commit in
    /// plan order. Kept separate from `outputs`: printing drains a handle, so
    /// these are print-only and never readable via [`output`](Runtime::output).
    cli_outputs: Vec<(SinkId, DbspOutput)>,
}

impl Runtime for DbspRuntime {
    type Output = Delta;
    type Error = RuntimeError;

    fn feed(
        &mut self,
        source: &SourceId,
        rows: impl IntoIterator<Item = (TupleValue, ZWeight)>,
    ) -> Result<(), Self::Error> {
        self.inputs.get(source.as_str()).map_or_else(
            || {
                Err(RuntimeError::new(format!(
                    "tried to feed unknown source '{}'",
                    source.as_str()
                )))
            },
            |input| {
                let _: () = input.feed(rows);
                Ok(())
            },
        )
    }

    fn commit(&mut self) -> Result<(), Self::Error> {
        self.handle.transaction()?;
        // Flush CLI taps: print each flagged output's current batch for
        // debugging. This drains the handle, which is why CLI taps are not
        // readable via `output`.
        for (id, output) in &self.cli_outputs {
            let batch = output.to_batch();
            println!("output '{}':\n{}", id.as_str(), batch.as_debug_table());
        }
        Ok(())
    }

    fn output(&self, out: &SinkId) -> Result<Delta, Self::Error> {
        match self.outputs.get(out) {
            Some(output) => Ok(Delta(output.to_batch().as_debug_zset())),
            // Distinguish an unknown name from a print-only CLI tap so the error
            // points at the actual mistake.
            None if self.cli_outputs.iter().any(|(id, _)| id == out) => {
                Err(RuntimeError::new(format!(
                    "output '{}' is a print-only CLI tap and cannot be read",
                    out.as_str()
                )))
            }
            None => Err(RuntimeError::new(format!(
                "no output named '{}'",
                out.as_str()
            ))),
        }
    }

    fn list_outputs(&self) -> impl Iterator<Item = &'_ SinkId> {
        self.outputs.keys()
    }
}
