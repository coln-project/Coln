// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A realm's program cut down to its derived views.
//!
//! The FLIR frontend translates a whole realm: every derived view, and every
//! rule as a query for its violations. The batch backend computes the views
//! only, so [`ViewProgram`] keeps just the statements the views' outputs
//! depend on. The rules, and with them the anti joins that find their
//! violations, never reach the lowering.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::api::query::FlirProgram;
use crate::host::QueryIr;
use crate::host::expr::Expr;
use crate::host::stmt::Stmt;
use crate::host::walk::{self, Node};
use crate::program::QueryProgram;
use crate::relational::catalog::Catalog;
use crate::relational::expr::{RelExpr, SinkId, SourceId};
use crate::relational::schema::TableSchema;

/// The derived views of a [`FlirProgram`], as a program the batch pipeline
/// runs: `Pipeline::batch().runtime(&mut ViewProgram::new(program))`.
#[derive(Debug)]
pub struct ViewProgram {
    program: FlirProgram,
    code: QueryIr,
}

impl ViewProgram {
    pub fn new(mut program: FlirProgram) -> Self {
        let code = program.take_code();
        let code = retain_outputs(code, |sink| {
            program.logical().derived_view_meta(sink).is_some()
        });
        Self { program, code }
    }
}

impl Catalog for ViewProgram {
    fn source_schema(&self, id: &SourceId) -> Option<Cow<'_, TableSchema>> {
        self.program.source_schema(id)
    }
}

impl QueryProgram for ViewProgram {
    fn code(&self) -> &QueryIr {
        &self.code
    }

    fn take_code(&mut self) -> QueryIr {
        std::mem::take(&mut self.code)
    }
}

/// Keep the top-level statements that the outputs selected by `keep` depend
/// on, in their order, and drop every other statement, including the other
/// outputs.
fn retain_outputs(code: QueryIr, keep: impl Fn(&SinkId) -> bool) -> QueryIr {
    let stmts = code.into_stmts();
    let needed = needed_statements(&stmts, keep);
    stmts
        .into_iter()
        .enumerate()
        .filter(|(at, _)| needed.contains(at))
        .map(|(_, stmt)| stmt)
        .collect::<Vec<_>>()
        .into()
}

/// The positions of the outputs selected by `keep` and of every statement
/// they read from, directly or indirectly.
fn needed_statements(stmts: &[Stmt], keep: impl Fn(&SinkId) -> bool) -> HashSet<usize> {
    let defined_at: HashMap<&str, usize> = stmts
        .iter()
        .enumerate()
        .filter_map(|(at, stmt)| match stmt {
            Stmt::Var(var) => Some((var.name.as_str(), at)),
            _ => None,
        })
        .collect();

    let mut needed: HashSet<usize> = (0..stmts.len())
        .filter(|&at| output_of(&stmts[at]).is_some_and(&keep))
        .collect();
    let mut pending: Vec<usize> = needed.iter().copied().collect();
    while let Some(at) = pending.pop() {
        // Every variable the statement reads. Column references are variables
        // too, but none is named like a statement, so they select nothing.
        for node in walk::pre_order(std::slice::from_ref(&stmts[at])) {
            if let Node::Expr(Expr::Var(var)) = node
                && let Some(&dependency) = defined_at.get(var.name.as_str())
                && needed.insert(dependency)
            {
                pending.push(dependency);
            }
        }
    }
    needed
}

/// The sink a top-level statement outputs to, either as `Output(..)` on its
/// own or bound to a variable, `var x = Output(..)`.
fn output_of(stmt: &Stmt) -> Option<&SinkId> {
    let expr = match stmt {
        Stmt::Var(var) => var.initializer.as_ref()?,
        Stmt::Expr(expr_stmt) => &expr_stmt.expr,
        Stmt::Block(_) => return None,
    };
    match expr {
        Expr::Relational(RelExpr::Output(output)) => Some(&output.id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::expr::VarExpr;
    use crate::host::stmt::VarStmt;
    use crate::relational::expr::{OutputExpr, OutputKind, ProjectionExpr, SourceExpr};

    fn bind(name: &str, initializer: impl Into<Expr>) -> Stmt {
        Stmt::from(VarStmt {
            name: name.to_string(),
            initializer: Some(initializer.into()),
        })
    }

    fn read(name: &str) -> Expr {
        Expr::from(VarExpr::new(name))
    }

    fn output(name: &str, relation: Expr) -> OutputExpr {
        OutputExpr {
            relation,
            id: SinkId::from(name),
            kind: OutputKind::Channel,
        }
    }

    fn names(code: &QueryIr) -> Vec<&str> {
        code.iter()
            .map(|stmt| match stmt {
                Stmt::Var(var) => var.name.as_str(),
                _ => "<not a binding>",
            })
            .collect()
    }

    /// The kept output keeps what it reads, also indirectly and through
    /// column references; the other output and what only it reads go.
    #[test]
    fn keeps_exactly_what_the_kept_outputs_read() {
        let code = QueryIr::new(vec![
            bind("edges", SourceExpr::new(SourceId::from("edge"))),
            bind("unused", SourceExpr::new(SourceId::from("edge"))),
            bind(
                "targets",
                ProjectionExpr {
                    relation: read("edges"),
                    attributes: vec![("node".to_string(), read("to"))],
                },
            ),
            bind("rule_input", SourceExpr::new(SourceId::from("other"))),
            bind("rule", output("rule", read("rule_input"))),
            bind("view", output("view", read("targets"))),
        ]);

        let kept = retain_outputs(code, |sink| sink.as_str() == "view");
        assert_eq!(names(&kept), ["edges", "targets", "view"]);
    }

    #[test]
    fn keeping_no_output_keeps_nothing() {
        let code = QueryIr::new(vec![
            bind("edges", SourceExpr::new(SourceId::from("edge"))),
            bind("view", output("view", read("edges"))),
        ]);
        assert!(retain_outputs(code, |_| false).is_empty());
    }
}
