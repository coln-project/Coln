// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Rendering a [`Code`](super::Code) program back into text, in two flavors.
//!
//! - [`AsSource`] reads like a program: infix host expressions, relational
//!   operators as named calls. What you want in order to *review* a plan, e.g.
//!   to check what a `coln-flir` lowering produced.
//! - [`AsTree`] reads like the data structure: one line per node, tagged with
//!   the role it plays in its parent, plus the payloads [`AsSource`] leaves out
//!   (schemas, resolved variable slots). What you want in order to *debug* which
//!   nodes a pass actually built.
//!
//! The split mirrors the one in [`walk`](mod@super::walk): the tree rendering is a
//! scan and rides on [`Walk`](super::walk::Walk), while the source rendering is
//! a fold — parenthesization flows *down* from the enclosing operator and text
//! is assembled *up* from the operands — so it is a visitor, like the
//! interpreter and the type resolver. A flat node stream cannot express it.

use super::{
    expr::{
        AssignExpr, BinaryExpr, CallExpr, Expr, ExprVisitor, FunctionExpr, GetIndexExpr,
        GroupingExpr, Literal, LiteralExpr, TupleExpr, UnaryExpr, VarExpr,
    },
    operator::{PRIMARY_PRECEDENCE, UNARY_PRECEDENCE},
    stmt::{BlockStmt, ExprStmt, Stmt, StmtVisitor, VarStmt},
    walk::{Child, Event, Node, ROOT, walk},
};
use crate::relational::expr::{
    AliasExpr, AntiJoinExpr, CartesianProductExpr, DifferenceExpr, DistinctExpr, EquiJoinExpr,
    FixedPointIterExpr, JoinVariable, MultiWayEquiJoinExpr, OutputExpr, OutputKind, ProjectionExpr,
    RelExpr, RelExprVisitor, SelectionExpr, SourceExpr, UnionExpr,
};
use std::fmt::{self, Display, Write};

/// One level of indentation.
const INDENT: &str = "  ";

/// The width a rendered line aims to stay within.
const MAX_WIDTH: usize = 80;

/// The precedence of an expression in a position where nothing can need
/// parentheses (a statement, an argument, inside brackets).
const OUTERMOST: u8 = 0;

/// Writing into a [`String`] cannot fail, which is why every method of the
/// printers below returns `()` instead of a [`fmt::Result`].
macro_rules! emit {
    ($printer:ident, $($arg:tt)*) => {
        write!($printer.out, $($arg)*).expect("writing into a String cannot fail")
    };
}

/// `code` rendered as host-language source text. See [`AsSource`].
pub fn to_source(code: &[Stmt]) -> String {
    AsSource(code).to_string()
}

/// `code` rendered as an indented node tree. See [`AsTree`].
pub fn to_tree(code: &[Stmt]) -> String {
    AsTree(code).to_string()
}

/// A program rendered as source text, e.g. `println!("{}", AsSource(&code))`.
///
/// The rendering is meant to be *read*, not parsed back: there is no parser for
/// the host language, so nothing round-trips and no attempt is made to keep the
/// output unambiguous under re-parsing. It does keep the output *faithful*,
/// which for a tree built programmatically (as every `coln-flir` lowering builds
/// it) means re-deriving parentheses from operator precedence, since such a tree
/// carries no [`GroupingExpr`].
pub struct AsSource<'a>(pub &'a [Stmt]);

impl Display for AsSource<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut printer = SourcePrinter {
            out: String::new(),
            indent: 0,
        };
        printer.stmts(self.0);
        f.write_str(&printer.out)
    }
}

struct SourcePrinter {
    out: String,
    /// Indentation of the line currently being written, in levels.
    indent: usize,
}

impl SourcePrinter {
    /// The statements of one block, one per line, without a trailing newline.
    /// Every `visit_*_stmt` starts at the cursor the caller left and ends
    /// without a newline, so nesting composes.
    fn stmts(&mut self, stmts: &[Stmt]) {
        for (index, stmt) in stmts.iter().enumerate() {
            if index > 0 {
                self.newline();
            }
            self.visit_stmt(stmt, ());
        }
    }

    /// `{ … }` with the body one level deeper. Shared by blocks, function bodies
    /// and fixed-point steps.
    fn block(&mut self, stmts: &[Stmt]) {
        if stmts.is_empty() {
            self.out.push_str("{}");
            return;
        }
        self.out.push('{');
        self.indent += 1;
        self.newline();
        self.stmts(stmts);
        self.indent -= 1;
        self.newline();
        self.out.push('}');
    }

    fn newline(&mut self) {
        self.out.push('\n');
        for _ in 0..self.indent {
            self.out.push_str(INDENT);
        }
    }

    /// Render whatever `render` writes into a detached buffer, so a caller can
    /// lay the pieces out only once they exist — which is how an operator
    /// decides between one line and one argument per line.
    fn captured(&mut self, render: impl FnOnce(&mut Self)) -> String {
        let enclosing = std::mem::take(&mut self.out);
        render(self);
        std::mem::replace(&mut self.out, enclosing)
    }

    /// Wrap what `render` writes in parentheses if `own` binds looser than the
    /// enclosing position `ctx` demands.
    fn parenthesized(&mut self, own: u8, ctx: u8, render: impl FnOnce(&mut Self)) {
        let parens = own < ctx;
        if parens {
            self.out.push('(');
        }
        render(self);
        if parens {
            self.out.push(')');
        }
    }

    /// Emit `name(…)`: on one line if that fits within [`MAX_WIDTH`], otherwise
    /// one argument per line.
    ///
    /// The arguments are rendered *before* the decision, so it is made on their
    /// real width rather than on a structural guess. This is a greedy layout
    /// with no backtracking: enough to keep an atom lowering
    /// (`project(select(source("edge"), …), …)`) on one line while a plan that
    /// genuinely does not fit still nests readably, and far short of a real
    /// layout algorithm.
    fn operator(&mut self, name: &str, arguments: impl FnOnce(&mut Self) -> Vec<String>) {
        // Rendered one level deeper, so the newlines *inside* a multi-line
        // argument are already indented for the broken layout below. A single-
        // line argument holds no newline, so the deeper level cannot leak into
        // the compact layout.
        self.indent += 1;
        let arguments = arguments(self);
        self.indent -= 1;
        let compact = arguments.join(", ");
        if !compact.contains('\n') && self.start() + name.len() + compact.len() + 2 <= MAX_WIDTH {
            emit!(self, "{name}({compact})");
            return;
        }
        emit!(self, "{name}(");
        self.indent += 1;
        for argument in arguments {
            self.newline();
            self.out.push_str(&argument);
            self.out.push(',');
        }
        self.indent -= 1;
        self.newline();
        self.out.push(')');
    }

    /// The column the text written next will start at — approximately.
    ///
    /// Exact for text appended to the buffer being emitted, but a piece being
    /// [`captured`](Self::captured) has no line to measure yet, so its
    /// indentation stands in: that *is* where its first line will be placed.
    /// Both under-estimate a keyword prefix a caller adds afterwards
    /// (`where: …`), which only ever makes the layout more compact than asked.
    fn start(&self) -> usize {
        let column = self.out.len() - self.out.rfind('\n').map_or(0, |index| index + 1);
        column.max(INDENT.len() * self.indent)
    }

    /// A rendered operand, for [`Self::operator`].
    fn operand(&mut self, expr: &Expr) -> String {
        self.captured(|printer| printer.visit_expr(expr, OUTERMOST))
    }

    /// `{ a, b: <expr> }`.
    ///
    /// An attribute whose expression is just the variable of the same name is a
    /// plain column pick, so it prints as the bare name — which is the shape
    /// every atom lowered from coln's FLIR has, and spelling it `a: a` would
    /// bury the interesting attributes among the trivial ones.
    fn attributes(&mut self, attributes: &[(String, Expr)]) -> String {
        if attributes.is_empty() {
            return "{}".to_string();
        }
        let rendered: Vec<String> = attributes
            .iter()
            .map(|(name, expr)| match expr {
                Expr::Var(var) if var.name == *name => name.clone(),
                expr => format!("{name}: {}", self.operand(expr)),
            })
            .collect();
        format!("{{ {} }}", rendered.join(", "))
    }

    /// `[a == b, …]`, one entry per attribute pair to match on. The two sides
    /// are evaluated against different relations, so equal-looking sides are
    /// normal rather than redundant.
    fn on_pairs(&mut self, on: &[(Expr, Expr)]) -> String {
        let rendered: Vec<String> = on
            .iter()
            .map(|(left, right)| format!("{} == {}", self.operand(left), self.operand(right)))
            .collect();
        format!("[{}]", rendered.join(", "))
    }

    /// `[x: 0.a == 2.b, …]`, one entry per equality class, each occurrence
    /// prefixed with the index of the relation it is evaluated against.
    fn join_variables(&mut self, on: &[JoinVariable]) -> String {
        let rendered: Vec<String> = on
            .iter()
            .map(|variable| {
                let occurrences: Vec<String> = variable
                    .occurrences
                    .iter()
                    .map(|(relation, expr)| format!("{relation}.{}", self.operand(expr)))
                    .collect();
                format!("{}: {}", variable.name, occurrences.join(" == "))
            })
            .collect();
        format!("[{}]", rendered.join(", "))
    }

    /// The optional projection an equi join carries, as a trailing argument.
    fn select(&mut self, attributes: Option<&Vec<(String, Expr)>>) -> Option<String> {
        attributes.map(|attributes| format!("select: {}", self.attributes(attributes)))
    }
}

/// A string literal's text, with the escapes it needs to stay one token.
fn escaped(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

impl StmtVisitor<(), ()> for SourcePrinter {
    fn visit_var_stmt(&mut self, stmt: &VarStmt, ctx: ()) {
        emit!(self, "var {}", stmt.name);
        if let Some(initializer) = &stmt.initializer {
            self.out.push_str(" = ");
            self.visit_expr(initializer, OUTERMOST);
        }
        self.out.push(';');
    }

    fn visit_expr_stmt(&mut self, stmt: &ExprStmt, ctx: ()) {
        self.visit_expr(&stmt.expr, OUTERMOST);
        self.out.push(';');
    }

    fn visit_block_stmt(&mut self, stmt: &BlockStmt, ctx: ()) {
        self.block(&stmt.stmts);
    }
}

/// The context of a host expression is the precedence its position demands:
/// render parentheses if the expression itself binds looser than that.
impl ExprVisitor<(), u8> for SourcePrinter {
    fn visit_literal_expr(&mut self, expr: &LiteralExpr, ctx: u8) {
        match &expr.value {
            // `Literal`'s own `Display` prints a string bare, which would make
            // it indistinguishable from a variable here.
            Literal::String(value) => emit!(self, "\"{}\"", escaped(value)),
            value => emit!(self, "{value}"),
        }
    }

    fn visit_tuple_expr(&mut self, expr: &TupleExpr, ctx: u8) {
        let elements: Vec<String> = expr
            .elements
            .iter()
            .map(|element| self.operand(element))
            .collect();
        // A trailing comma is what distinguishes a one-element tuple from a
        // grouping.
        let trailing = if elements.len() == 1 { "," } else { "" };
        emit!(self, "({}{trailing})", elements.join(", "));
    }

    fn visit_get_index_expr(&mut self, expr: &GetIndexExpr, ctx: u8) {
        self.parenthesized(PRIMARY_PRECEDENCE, ctx, |printer| {
            printer.visit_expr(&expr.target, PRIMARY_PRECEDENCE);
            printer.out.push('[');
            printer.visit_expr(&expr.index, OUTERMOST);
            printer.out.push(']');
        });
    }

    fn visit_grouping_expr(&mut self, expr: &GroupingExpr, ctx: u8) {
        // An explicit grouping is kept even where precedence makes it
        // redundant: it is a node the tree actually contains.
        self.out.push('(');
        self.visit_expr(&expr.expr, OUTERMOST);
        self.out.push(')');
    }

    fn visit_binary_expr(&mut self, expr: &BinaryExpr, ctx: u8) {
        let own = expr.operator.precedence();
        self.parenthesized(own, ctx, |printer| {
            printer.visit_expr(&expr.left, own);
            emit!(printer, " {} ", expr.operator);
            // Left-associative, so an equally tight right operand needs
            // parentheses to keep its shape.
            printer.visit_expr(&expr.right, own + 1);
        });
    }

    fn visit_unary_expr(&mut self, expr: &UnaryExpr, ctx: u8) {
        self.parenthesized(UNARY_PRECEDENCE, ctx, |printer| {
            emit!(printer, "{}", expr.operator);
            printer.visit_expr(&expr.operand, UNARY_PRECEDENCE);
        });
    }

    fn visit_var_expr(&mut self, expr: &VarExpr, ctx: u8) {
        self.out.push_str(&expr.name);
    }

    fn visit_assign_expr(&mut self, expr: &AssignExpr, ctx: u8) {
        self.parenthesized(OUTERMOST, ctx, |printer| {
            emit!(printer, "{} = ", expr.name);
            printer.visit_expr(&expr.value, OUTERMOST);
        });
    }

    fn visit_function_expr(&mut self, expr: &FunctionExpr, ctx: u8) {
        emit!(self, "fn({}) ", expr.parameters.join(", "));
        self.block(&expr.body.stmts);
    }

    fn visit_call_expr(&mut self, expr: &CallExpr, ctx: u8) {
        self.parenthesized(PRIMARY_PRECEDENCE, ctx, |printer| {
            printer.visit_expr(&expr.callee, PRIMARY_PRECEDENCE);
            let arguments: Vec<String> = expr
                .arguments
                .iter()
                .map(|argument| printer.operand(argument))
                .collect();
            emit!(printer, "({})", arguments.join(", "));
        });
    }

    fn visit_relational_expr(&mut self, expr: &RelExpr, ctx: u8) {
        self.visit_rel(expr, ctx);
    }
}

impl RelExprVisitor<(), u8> for SourcePrinter {
    fn visit_source_expr(&mut self, expr: &SourceExpr, ctx: u8) {
        // The name *is* the source's identity (see `SourceExpr::to_id`); the
        // rest of the schema is derived, and shown by `AsTree` instead.
        emit!(self, "source(\"{}\")", escaped(expr.as_id()));
    }

    fn visit_output_expr(&mut self, expr: &OutputExpr, ctx: u8) {
        let kind = match expr.kind {
            OutputKind::Cli => "cli",
            OutputKind::Channel => "channel",
        };
        self.operator("output", |printer| {
            vec![
                printer.operand(&expr.relation),
                format!("as: \"{}\"", escaped(expr.id.as_str())),
                format!("to: {kind}"),
            ]
        });
    }

    fn visit_alias_expr(&mut self, expr: &AliasExpr, ctx: u8) {
        self.operator("alias", |printer| {
            vec![
                printer.operand(&expr.relation),
                format!("as: {}", expr.alias),
            ]
        });
    }

    fn visit_distinct_expr(&mut self, expr: &DistinctExpr, ctx: u8) {
        self.operator("distinct", |printer| vec![printer.operand(&expr.relation)]);
    }

    fn visit_union_expr(&mut self, expr: &UnionExpr, ctx: u8) {
        self.operator("union", |printer| {
            expr.relations
                .iter()
                .map(|relation| printer.operand(relation))
                .collect()
        });
    }

    fn visit_difference_expr(&mut self, expr: &DifferenceExpr, ctx: u8) {
        self.operator("difference", |printer| {
            vec![printer.operand(&expr.left), printer.operand(&expr.right)]
        });
    }

    fn visit_selection_expr(&mut self, expr: &SelectionExpr, ctx: u8) {
        self.operator("select", |printer| {
            vec![
                printer.operand(&expr.relation),
                format!("where: {}", printer.operand(&expr.condition)),
            ]
        });
    }

    fn visit_projection_expr(&mut self, expr: &ProjectionExpr, ctx: u8) {
        self.operator("project", |printer| {
            vec![
                printer.operand(&expr.relation),
                format!("select: {}", printer.attributes(&expr.attributes)),
            ]
        });
    }

    fn visit_cartesian_product_expr(&mut self, expr: &CartesianProductExpr, ctx: u8) {
        let inner = &expr.inner;
        // The `on` clause of the delegate is empty by construction, so printing
        // it would only add noise.
        self.operator("product", |printer| {
            [printer.operand(&inner.left), printer.operand(&inner.right)]
                .into_iter()
                .chain(printer.select(inner.attributes.as_ref()))
                .collect()
        });
    }

    fn visit_equi_join_expr(&mut self, expr: &EquiJoinExpr, ctx: u8) {
        self.operator("join", |printer| {
            [
                printer.operand(&expr.left),
                printer.operand(&expr.right),
                format!("on: {}", printer.on_pairs(&expr.on)),
            ]
            .into_iter()
            .chain(printer.select(expr.attributes.as_ref()))
            .collect()
        });
    }

    fn visit_multi_way_equi_join_expr(&mut self, expr: &MultiWayEquiJoinExpr, ctx: u8) {
        self.operator("multijoin", |printer| {
            let relations: Vec<String> = expr
                .relations
                .iter()
                .map(|relation| printer.operand(relation))
                .collect();
            // The relations are one bracketed argument rather than N, because
            // `on` addresses them by index.
            [
                format!("[{}]", relations.join(", ")),
                format!("on: {}", printer.join_variables(&expr.on)),
            ]
            .into_iter()
            .chain(printer.select(expr.attributes.as_ref()))
            .collect()
        });
    }

    fn visit_anti_join_expr(&mut self, expr: &AntiJoinExpr, ctx: u8) {
        self.operator("antijoin", |printer| {
            vec![
                printer.operand(&expr.left),
                printer.operand(&expr.right),
                format!("on: {}", printer.on_pairs(&expr.on)),
            ]
        });
    }

    fn visit_fixed_point_iter_expr(&mut self, expr: &FixedPointIterExpr, ctx: u8) {
        // A binding form rather than a call: the step body can only be read
        // with the accumulator's name in scope.
        emit!(self, "fix {} = ", expr.accumulator.0);
        self.visit_expr(&expr.accumulator.1, OUTERMOST);
        self.out.push(' ');
        self.block(&expr.step.stmts);
    }
}

/// A program rendered as an indented node tree, e.g.
/// `println!("{}", AsTree(&code))`.
///
/// One line per node: the role it plays in its parent, its kind, and the
/// payloads that are not children — an operator, a schema, whether a variable
/// has been resolved. Built on [`walk`], so it needs no knowledge of the tree's
/// shape beyond [`Node::push_children`].
pub struct AsTree<'a>(pub &'a [Stmt]);

impl Display for AsTree<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // How many children each currently open ancestor still has to enter.
        // A node is its parent's last child exactly when that count is 1 as the
        // node is entered, which is what the elbow and the guide lines need.
        let mut remaining: Vec<usize> = Vec::new();
        let mut children = Vec::new();
        let mut first = true;
        for event in walk(self.0) {
            let child = match event {
                Event::Enter(child) => child,
                Event::Leave(_) => {
                    remaining.pop();
                    continue;
                }
            };
            if !first {
                f.write_char('\n')?;
            }
            first = false;
            if let Some((parent, ancestors)) = remaining.split_last_mut() {
                for ancestor in ancestors {
                    f.write_str(if *ancestor > 0 { "│  " } else { "   " })?;
                }
                f.write_str(if *parent > 1 { "├─ " } else { "└─ " })?;
                *parent -= 1;
            }
            child.node.push_children(&mut children);
            remaining.push(children.len());
            children.clear();
            if child.role != ROOT {
                write!(f, "{}: ", child.label())?;
            }
            f.write_str(&describe(child))?;
        }
        Ok(())
    }
}

/// One node as a single line: its kind, plus every payload that is not a child
/// and would therefore be invisible in the tree.
fn describe(child: Child<'_>) -> String {
    match child.node {
        Node::Stmt(stmt) => match stmt {
            Stmt::Var(stmt) => format!("VarStmt {}", stmt.name),
            Stmt::Expr(_) => "ExprStmt".to_string(),
            Stmt::Block(stmt) => format!("Block ({} stmts)", stmt.stmts.len()),
        },
        Node::Expr(expr) => match expr {
            Expr::Literal(expr) => match &expr.value {
                Literal::String(value) => format!("Literal \"{}\"", escaped(value)),
                value => format!("Literal {value}"),
            },
            Expr::Tuple(expr) => format!("Tuple ({} elements)", expr.elements.len()),
            Expr::GetIndex(_) => "GetIndex".to_string(),
            Expr::Grouping(_) => "Grouping".to_string(),
            Expr::Binary(expr) => format!("Binary {}", expr.operator),
            Expr::Unary(expr) => format!("Unary {}", expr.operator),
            // Whether the resolver has been here is invisible in the source
            // rendering but is exactly what one debugs a resolution with.
            Expr::Var(expr) => format!("Var {}{}", expr.name, slot(expr.resolved)),
            Expr::Assign(expr) => format!("Assign {}{}", expr.name, slot(expr.resolved)),
            Expr::Function(expr) => format!("Function ({})", expr.parameters.join(", ")),
            Expr::Call(_) => "Call".to_string(),
            // Normalized away by the walk, see `Node`.
            Expr::Relational(_) => "Relational".to_string(),
        },
        Node::Rel(rel) => match rel {
            RelExpr::Source(expr) => format!(
                "Source \"{}\" tuple={} key={}",
                expr.as_id(),
                expr.schema.tuple,
                expr.schema.key
            ),
            RelExpr::Output(expr) => format!(
                "Output \"{}\" {}",
                expr.id.as_str(),
                match expr.kind {
                    OutputKind::Cli => "cli",
                    OutputKind::Channel => "channel",
                }
            ),
            RelExpr::Alias(expr) => format!("Alias {}", expr.alias),
            RelExpr::Distinct(_) => "Distinct".to_string(),
            RelExpr::Union(_) => "Union".to_string(),
            RelExpr::Difference(_) => "Difference".to_string(),
            RelExpr::Selection(_) => "Selection".to_string(),
            RelExpr::Projection(expr) => {
                format!("Projection {}", attribute_names(&expr.attributes))
            }
            // A cartesian product delegates to an equi join, so its projection
            // lives on that delegate — and its `select` children come from
            // there too, which is why the names have to be read off it as well.
            RelExpr::CartesianProduct(expr) => {
                format!("CartesianProduct{}", select_tag(&expr.inner.attributes))
            }
            RelExpr::EquiJoin(expr) => format!("EquiJoin{}", select_tag(&expr.attributes)),
            RelExpr::MultiWayEquiJoin(expr) => format!(
                "MultiWayEquiJoin on={}{}",
                join_variable_classes(&expr.on),
                select_tag(&expr.attributes)
            ),
            RelExpr::AntiJoin(_) => "AntiJoin".to_string(),
            RelExpr::FixedPointIter(expr) => format!("FixedPointIter {}", expr.accumulator.0),
        },
    }
}

/// The names a projection produces. The expressions behind them are children,
/// so only the names belong on the node's own line.
fn attribute_names(attributes: &[(String, Expr)]) -> String {
    let names: Vec<&str> = attributes.iter().map(|(name, _)| name.as_str()).collect();
    format!("[{}]", names.join(", "))
}

/// The ` select=[…]` tag an *optional* projection contributes to a node's line,
/// or nothing when the operator carries none.
///
/// Every join-shaped operator holds its projection as an `Option`, so they all
/// need the same conditional tag; a [`ProjectionExpr`] always has one and prints
/// it through [`attribute_names`] directly.
fn select_tag(attributes: &Option<Vec<(String, Expr)>>) -> String {
    attributes
        .as_ref()
        .map(|attributes| format!(" select={}", attribute_names(attributes)))
        .unwrap_or_default()
}

/// The equality classes of a multi-way join, as `[y: 0=2, z: 1=2]`: which
/// relations each join variable equates.
///
/// The occurrence expressions are children, so only the relation indices belong
/// here — and they *have* to be here, because a
/// [`RelationIdx`](crate::relational::expr::RelationIdx) is a payload rather
/// than a node. Without them the drawing could not say whether `y` is bound by
/// relations 0 and 2 or by 1 and 2, which is the whole content of the join.
fn join_variable_classes(on: &[JoinVariable]) -> String {
    let classes: Vec<String> = on
        .iter()
        .map(|variable| {
            let relations: Vec<String> = variable
                .occurrences
                .iter()
                .map(|(relation, _)| relation.to_string())
                .collect();
            format!("{}: {}", variable.name, relations.join("="))
        })
        .collect();
    format!("[{}]", classes.join(", "))
}

/// The resolved slot of a variable reference, as `@scope:index`, or a marker
/// that the resolver has not reached it.
fn slot(resolved: Option<(usize, usize)>) -> String {
    match resolved {
        Some((scope, index)) => format!(" @{scope}:{index}"),
        None => " (unresolved)".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relational::{RelationSchema, expr::SinkId};

    fn schema(name: &str) -> RelationSchema {
        RelationSchema::new(name, ["x", "y"], ["x"]).expect("Correct schema definition")
    }

    fn var(name: &str) -> Expr {
        Expr::from(VarExpr::new(name))
    }

    fn expr_stmt(expr: Expr) -> Stmt {
        Stmt::from(ExprStmt { expr })
    }

    /// The plan a transitive-closure lowering produces: a fixed point whose step
    /// joins the accumulator against a source and unions the result back in.
    /// Parameterized by the names, which is what decides whether the source
    /// rendering fits on one line — the tree rendering is unaffected by them.
    fn transitive_closure(edge: &str, acc: &str, from: &str, to: &str) -> Vec<Stmt> {
        vec![
            Stmt::from(VarStmt {
                name: edge.to_string(),
                initializer: Some(Expr::from(SourceExpr::new(schema(edge)))),
            }),
            Stmt::from(VarStmt {
                name: "reach".to_string(),
                initializer: Some(Expr::from(FixedPointIterExpr {
                    accumulator: (acc.to_string(), var(edge)),
                    step: BlockStmt {
                        stmts: vec![expr_stmt(Expr::from(DistinctExpr {
                            relation: Expr::from(UnionExpr {
                                relations: vec![
                                    var(acc),
                                    Expr::from(EquiJoinExpr {
                                        left: var(acc),
                                        right: var(edge),
                                        on: vec![(var(to), var(from))],
                                        attributes: Some(vec![
                                            (from.to_string(), var(from)),
                                            (to.to_string(), var(to)),
                                        ]),
                                    }),
                                ],
                            }),
                        }))],
                    },
                })),
            }),
            expr_stmt(Expr::from(OutputExpr {
                relation: var("reach"),
                id: SinkId::from("reach"),
                kind: OutputKind::Channel,
            })),
        ]
    }

    /// [`AsSource`]: is the program readable, and does it stay faithful while
    /// getting there — parentheses, quoting, and where the lines break.
    mod source {
        use super::*;
        use crate::host::operator::Operator;

        fn binary(operator: Operator, left: Expr, right: Expr) -> Expr {
            Expr::from(BinaryExpr {
                operator,
                left,
                right,
            })
        }

        fn literal(value: u64) -> Expr {
            Expr::from(LiteralExpr::from(value))
        }

        #[test]
        fn renders_precedence_without_grouping_nodes() {
            // A programmatically built tree carries no `GroupingExpr`, so the
            // parentheses have to come from the operators themselves.
            let code = vec![expr_stmt(binary(
                Operator::Multiplication,
                binary(Operator::Addition, literal(1), literal(2)),
                literal(3),
            ))];
            assert_eq!(to_source(&code), "(1 + 2) * 3;");
        }

        #[test]
        fn omits_parentheses_where_precedence_already_agrees() {
            let code = vec![expr_stmt(binary(
                Operator::Addition,
                binary(Operator::Multiplication, literal(1), literal(2)),
                literal(3),
            ))];
            assert_eq!(to_source(&code), "1 * 2 + 3;");
        }

        #[test]
        fn keeps_a_right_nested_operand_of_equal_precedence_parenthesized() {
            // `-` is left-associative, so `1 - (2 - 3)` must not print as `1 - 2 - 3`.
            let code = vec![expr_stmt(binary(
                Operator::Subtraction,
                literal(1),
                binary(Operator::Subtraction, literal(2), literal(3)),
            ))];
            assert_eq!(to_source(&code), "1 - (2 - 3);");
        }

        #[test]
        fn quotes_string_literals_so_they_are_not_variables() {
            let code = vec![expr_stmt(binary(
                Operator::Equal,
                var("name"),
                Expr::from(LiteralExpr::from("a\"b")),
            ))];
            assert_eq!(to_source(&code), "name == \"a\\\"b\";");
        }

        #[test]
        fn renders_an_atom_lowering_on_one_line() {
            // `project(select(source))` is what one FLIR atom lowers to. Its only
            // nested operator is the source leaf, so it stays compact — and the
            // column picks print as bare names.
            let code = vec![expr_stmt(Expr::from(ProjectionExpr {
                relation: Expr::from(SelectionExpr {
                    relation: Expr::from(SourceExpr::new(schema("edge"))),
                    condition: binary(Operator::Greater, var("x"), literal(1)),
                }),
                attributes: vec![("x".to_string(), var("x")), ("z".to_string(), var("y"))],
            }))];
            assert_eq!(
                to_source(&code),
                "project(select(source(\"edge\"), where: x > 1), select: { x, z: y });"
            );
        }

        #[test]
        fn keeps_a_whole_fixed_point_step_on_one_line_when_it_fits() {
            // Three nested operators, still well inside the width budget: breaking
            // them would cost eight lines and buy nothing.
            assert_eq!(
                to_source(&transitive_closure("edge", "acc", "x", "y")),
                "\
var edge = source(\"edge\");
var reach = fix acc = edge {
  distinct(union(acc, join(acc, edge, on: [y == x], select: { x, y })));
};
output(reach, as: \"reach\", to: channel);"
            );
        }

        #[test]
        fn breaks_a_nested_plan_that_does_not_fit() {
            // The same plan with realistic names no longer fits, so each operator
            // takes one argument per line — and the indentation of a broken
            // argument's own inner lines has to follow.
            assert_eq!(
                to_source(&transitive_closure(
                    "transitive_edge",
                    "accumulated",
                    "source_node",
                    "target_node"
                )),
                "\
var transitive_edge = source(\"transitive_edge\");
var reach = fix accumulated = transitive_edge {
  distinct(
    union(
      accumulated,
      join(
        accumulated,
        transitive_edge,
        on: [target_node == source_node],
        select: { source_node, target_node },
      ),
    ),
  );
};
output(reach, as: \"reach\", to: channel);"
            );
        }

        #[test]
        fn renders_a_multi_way_join_by_relation_index() {
            let code = vec![expr_stmt(Expr::from(
                MultiWayEquiJoinExpr::new(
                    vec![var("r0"), var("r1"), var("r2")],
                    vec![JoinVariable {
                        name: "y".to_string(),
                        occurrences: vec![(0, var("y")), (2, var("y"))],
                    }],
                    None,
                )
                .expect("a variable bound by two relations is a join variable"),
            ))];
            assert_eq!(
                to_source(&code),
                "multijoin([r0, r1, r2], on: [y: 0.y == 2.y]);"
            );
        }
    }

    /// [`AsTree`]: does every node land at the right place in the drawing, and
    /// does it carry the payloads the source rendering has no room for.
    mod tree {
        use super::*;

        #[test]
        fn renders_a_whole_plan_the_source_rendering_compresses() {
            // The plan whose entire fixed-point step the source rendering puts
            // on one line. One node per line instead, which is what pins the
            // shape down: the join's six operands told apart only by their role,
            // the crossing back into statements at `step`, and the schema of the
            // source leaf. Every variable is still `(unresolved)` because this
            // plan has not been through the resolver.
            assert_eq!(
                to_tree(&transitive_closure("edge", "acc", "x", "y")),
                "\
VarStmt edge
└─ init: Source \"edge\" tuple=| x | y | key=| x |
VarStmt reach
└─ init: FixedPointIter acc
   ├─ init: Var edge (unresolved)
   └─ step[0]: ExprStmt
      └─ expr: Distinct
         └─ relation: Union
            ├─ relation[0]: Var acc (unresolved)
            └─ relation[1]: EquiJoin select=[x, y]
               ├─ left: Var acc (unresolved)
               ├─ right: Var edge (unresolved)
               ├─ on[0].left: Var y (unresolved)
               ├─ on[0].right: Var x (unresolved)
               ├─ select[0]: Var x (unresolved)
               └─ select[1]: Var y (unresolved)
ExprStmt
└─ expr: Output \"reach\" channel
   └─ relation: Var reach (unresolved)"
            );
        }

        #[test]
        fn names_the_projection_a_cartesian_product_carries_on_its_delegate() {
            // A `CartesianProductExpr` keeps its projection on the `EquiJoinExpr`
            // it delegates to, and the walk reports that delegate's `select`
            // children — so a node line that looked only at the product itself
            // would show the attribute expressions with no names to bind them to.
            let code = vec![expr_stmt(Expr::from(CartesianProductExpr::new(
                var("l"),
                var("r"),
                Some(vec![
                    ("out".to_string(), var("a")),
                    ("keep".to_string(), var("b")),
                ]),
            )))];
            assert_eq!(
                to_tree(&code),
                "\
ExprStmt
└─ expr: CartesianProduct select=[out, keep]
   ├─ left: Var l (unresolved)
   ├─ right: Var r (unresolved)
   ├─ select[0]: Var a (unresolved)
   └─ select[1]: Var b (unresolved)"
            );
        }

        #[test]
        fn spells_out_which_relations_each_join_variable_equates() {
            // The occurrence expressions arrive as flat `on` children, so the
            // relation indices are the one part of a multi-way join that only
            // the parent line can carry. `y` joins relations 0 and 2, `z` joins
            // 1 and 2 — and relation 1 is reachable *only* through `z`.
            let code = vec![expr_stmt(Expr::from(
                MultiWayEquiJoinExpr::new(
                    vec![var("r0"), var("r1"), var("r2")],
                    vec![
                        JoinVariable {
                            name: "y".to_string(),
                            occurrences: vec![(0, var("y")), (2, var("y"))],
                        },
                        JoinVariable {
                            name: "z".to_string(),
                            occurrences: vec![(1, var("z")), (2, var("z"))],
                        },
                    ],
                    None,
                )
                .expect("every variable is bound by two relations"),
            ))];
            assert_eq!(
                to_tree(&code),
                "\
ExprStmt
└─ expr: MultiWayEquiJoin on=[y: 0=2, z: 1=2]
   ├─ relation[0]: Var r0 (unresolved)
   ├─ relation[1]: Var r1 (unresolved)
   ├─ relation[2]: Var r2 (unresolved)
   ├─ on[0]: Var y (unresolved)
   ├─ on[0]: Var y (unresolved)
   ├─ on[1]: Var z (unresolved)
   └─ on[1]: Var z (unresolved)"
            );
        }

        #[test]
        fn tags_every_child_with_its_role() {
            // What the source rendering cannot show: which operand a child is, and
            // that the resolver has not run yet.
            let code = vec![expr_stmt(Expr::from(EquiJoinExpr {
                left: var("l"),
                right: var("r"),
                on: vec![(var("a"), var("b"))],
                attributes: None,
            }))];
            assert_eq!(
                to_tree(&code),
                "\
ExprStmt
└─ expr: EquiJoin
   ├─ left: Var l (unresolved)
   ├─ right: Var r (unresolved)
   ├─ on[0].left: Var a (unresolved)
   └─ on[0].right: Var b (unresolved)"
            );
        }

        #[test]
        fn shows_the_payloads_the_source_rendering_omits() {
            let code = vec![Stmt::from(VarStmt {
                name: "edge".to_string(),
                initializer: Some(Expr::from(DistinctExpr {
                    relation: Expr::from(SourceExpr::new(schema("edge"))),
                })),
            })];
            assert_eq!(
                to_tree(&code),
                "\
VarStmt edge
└─ init: Distinct
   └─ relation: Source \"edge\" tuple=| x | y | key=| x |"
            );
        }

        #[test]
        fn guides_lines_through_deeper_siblings() {
            // The `│` of the outer union has to continue past the whole first
            // branch, otherwise a deep tree cannot be read.
            let code = vec![expr_stmt(Expr::from(UnionExpr {
                relations: vec![Expr::from(DistinctExpr { relation: var("a") }), var("b")],
            }))];
            assert_eq!(
                to_tree(&code),
                "\
ExprStmt
└─ expr: Union
   ├─ relation[0]: Distinct
   │  └─ relation: Var a (unresolved)
   └─ relation[1]: Var b (unresolved)"
            );
        }
    }
}
