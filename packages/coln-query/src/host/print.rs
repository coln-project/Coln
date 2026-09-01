// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Rendering a [`QueryIr`](super::QueryIr) program as an indented node tree.
//!
//! One line per node: how its parent addresses it, its kind, and the payloads
//! that are *not* children — an operator, a schema, whether a variable has been
//! resolved. A `Vec<Stmt>` is a forest, so the top-level statements are rendered
//! flush at column zero, one after the other.
//!
//! # Why this is a visitor and not a walk
//!
//! [`walk`](mod@super::walk) exists so that a **scan** need not restate the
//! AST's shape. This is not a scan: it has to address each child *exactly* — by
//! the field it came from, the name that field associates with it, or the
//! relation it is evaluated against — and none of that is a property of the
//! child. It lives in the parent, next to it.
//!
//! A generic traversal can only hand a consumer a positional tag and hope it
//! reconstructs the rest; that is how `Vec<(String, Expr)>` and
//! `Vec<(RelationIdx, Expr)>` fields end up rendered as bare repeated labels
//! whose meaning the reader has to recover by counting. Visiting instead puts
//! the parent's payload and its children in the same scope, so
//! [`ProjectionExpr`] labels an attribute with its *name* and
//! [`MultiWayEquiJoinExpr`] labels an occurrence with the *relation* it belongs
//! to. The price is that this module states the AST's shape a second time — the
//! cost of fidelity, paid here rather than pushed into the walk's shared types.
//!
//! # Addressing vocabulary
//!
//! A label is a field name, optionally followed by which occurrence in
//! parentheses:
//!
//! | Label           | Means                                              |
//! | --------------- | -------------------------------------------------- |
//! | `relation`      | the field holds a single child                     |
//! | `relation(2)`   | element 2 of a sequence                            |
//! | `select(out)`   | the entry the field keys as `out`                  |
//! | `on(0 in left)` | equality 0, as evaluated against the left relation |
//! | `on(y in 2)`    | join variable `y`, as evaluated against relation 2 |
//!
//! The last two are the same statement — *which equality*, in *which relation* —
//! for a binary and an N-ary join respectively.

use super::{
    expr::{
        AssignExpr, BinaryExpr, CallExpr, Expr, ExprVisitor, FunctionExpr, GetIndexExpr,
        GroupingExpr, Literal, LiteralExpr, TupleExpr, UnaryExpr, VarExpr,
    },
    stmt::{BlockStmt, ExprStmt, Stmt, StmtVisitor, VarStmt},
    variable::VariableSlot,
    walk::Node,
};
use crate::relational::catalog::Catalog;
use crate::relational::expr::{
    AliasExpr, AntiJoinExpr, CartesianProductExpr, DifferenceExpr, DistinctExpr, EquiJoinExpr,
    FixedPointIterExpr, MultiWayEquiJoinExpr, OutputExpr, OutputKind, ProjectionExpr, RelExpr,
    RelExprVisitor, SelectionExpr, SourceExpr, UnionExpr,
};
use std::fmt::Write;

/// Writing into a [`String`] cannot fail, which is why every method of the
/// printer below returns `()` instead of a [`std::fmt::Result`].
macro_rules! emit {
    ($printer:ident, $($arg:tt)*) => {
        write!($printer.out, $($arg)*).expect("writing into a String cannot fail")
    };
}

/// `code` rendered as an indented node tree.
///
/// A whole program is better reached through
/// [`QueryIr::to_tree`](super::QueryIr::to_tree), which needs no import.
/// This takes a bare `[Stmt]` because a *sub*-forest (a fixed point's step body,
/// a function's body) is a plain `Vec<Stmt>` rather than a
/// [`QueryIr`](super::QueryIr), and inspecting one of those in isolation is
/// exactly when a tree rendering earns its keep.
///
/// Named after [`ToString`]: it says what it *returns*, not what it does.
/// There is deliberately no [`Display`](std::fmt::Display) wrapper — the
/// printer assembles a subtree's text before it knows the guide lines that
/// prefix it, so it buffers regardless, and a wrapper could only copy that
/// buffer into a formatter: a second allocation, and a second way to say the
/// same thing.
pub fn to_tree(code: &[Stmt]) -> String {
    render(code, None)
}

/// [`to_tree`], with each [`SourceExpr`] leaf described by the [`Catalog`] the
/// code is compiled against — the leaf itself only names its relation.
///
/// Reach for this over [`to_tree`] whenever a catalog is at hand, which for a
/// whole program it always is:
/// [`QueryProgram::to_tree`](crate::program::QueryProgram::to_tree) is this
/// function applied to a program and its own catalog. Besides showing the schema
/// at all, it is what makes a leaf the catalog does *not* describe visible, which
/// is the one failure mode a name-only leaf introduces.
pub fn to_tree_with(code: &[Stmt], catalog: &dyn Catalog) -> String {
    render(code, Some(catalog))
}

fn render(code: &[Stmt], catalog: Option<&dyn Catalog>) -> String {
    let mut printer = TreePrinter {
        out: String::new(),
        prefix: String::new(),
        catalog,
    };
    // A program is a forest, so every root starts a fresh tree at column zero
    // rather than hanging off a shared parent.
    if let Some((first, rest)) = code.split_first() {
        printer.visit_stmt(first, ());
        rest.iter().for_each(|stmt| {
            printer.out.push('\n');
            printer.visit_stmt(stmt, ());
        });
    }
    printer.out
}

/// The labelled children of one node, in source order. Built by the node's own
/// visit method, which is the only place that knows how to address them.
type Branches<'a> = Vec<(String, Node<'a>)>;

struct TreePrinter<'a> {
    out: String,
    /// The guide lines every line of the current subtree is prefixed with. Owned
    /// by the printer and pushed/popped around each child, so a node needs to
    /// know nothing about where it sits — hence the `()` visitor context.
    prefix: String,
    /// What the plan's [`SourceExpr`] leaves name, when the caller has it.
    /// [`None`] for a rendering of bare code — a sub-forest, or a plan under
    /// test that was never paired with a catalog — where a leaf can only be
    /// named, not described.
    catalog: Option<&'a dyn Catalog>,
}

impl TreePrinter<'_> {
    /// Emit `node` as a child on its own line, then its subtree one level in.
    ///
    /// `last` decides the elbow and whether the guide line continues past this
    /// child, which is the only reason a parent has to know how many children it
    /// has before emitting the first.
    fn branch(&mut self, label: &str, node: Node<'_>, last: bool) {
        self.out.push('\n');
        self.out.push_str(&self.prefix);
        self.out.push_str(if last { "└─ " } else { "├─ " });
        self.out.push_str(label);
        self.out.push_str(": ");
        let enclosing = self.prefix.len();
        self.prefix.push_str(if last { "   " } else { "│  " });
        match node {
            Node::Stmt(stmt) => self.visit_stmt(stmt, ()),
            Node::Expr(expr) => self.visit_expr(expr, ()),
            Node::Rel(rel) => self.visit_rel(rel, ()),
        }
        self.prefix.truncate(enclosing);
    }

    /// Emit every child of the node just described.
    fn branches(&mut self, branches: Branches<'_>) {
        let last = branches.len().saturating_sub(1);
        for (index, (label, node)) in branches.into_iter().enumerate() {
            self.branch(&label, node, index == last);
        }
    }
}

/// The children of an [`EquiJoinExpr`], shared with the
/// [`CartesianProductExpr`] that delegates to one.
///
/// `on(0 in left)` reads as "equality 0, as evaluated against the left
/// relation": the two sides of a pair are evaluated against *different*
/// relations, and those relations are the `left` and `right` children.
fn equi_join_branches(expr: &EquiJoinExpr) -> Branches<'_> {
    let mut branches = vec![
        ("left".to_string(), Node::from(&expr.left)),
        ("right".to_string(), Node::from(&expr.right)),
    ];
    for (index, (left, right)) in expr.on.iter().enumerate() {
        branches.push((format!("on({index} in left)"), Node::from(left)));
        branches.push((format!("on({index} in right)"), Node::from(right)));
    }
    branches.extend(select_branches(expr.attributes.as_deref()));
    branches
}

/// The children of an optional projection, keyed by the name each attribute
/// produces — the payload that would otherwise be reachable only by counting.
fn select_branches(attributes: Option<&[(String, Expr)]>) -> Branches<'_> {
    attributes
        .unwrap_or_default()
        .iter()
        .map(|(name, expr)| (format!("select({name})"), Node::from(expr)))
        .collect()
}

/// One `on` pair of a binary join, for the operators that carry no projection.
fn on_branches(on: &[(Expr, Expr)]) -> Branches<'_> {
    on.iter()
        .enumerate()
        .flat_map(|(index, (left, right))| {
            [
                (format!("on({index} in left)"), Node::from(left)),
                (format!("on({index} in right)"), Node::from(right)),
            ]
        })
        .collect()
}

/// A string payload, with the escapes it needs to stay one token.
fn escaped(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The resolved slot of a variable reference, as `@scope:index`, or a marker
/// that the resolver has not reached it. Invisible in the plan itself, and
/// exactly what one debugs a resolution with.
fn slot(resolved: Option<VariableSlot>) -> String {
    match resolved {
        Some((scope, index)) => format!(" @{scope}:{index}"),
        None => " (unresolved)".to_string(),
    }
}

impl StmtVisitor<(), ()> for TreePrinter<'_> {
    fn visit_var_stmt(&mut self, stmt: &VarStmt, ctx: ()) {
        emit!(self, "VarStmt {}", stmt.name);
        // At most one initializer, so there is no occurrence to name.
        self.branches(
            stmt.initializer
                .iter()
                .map(|expr| ("init".to_string(), Node::from(expr)))
                .collect(),
        );
    }

    fn visit_expr_stmt(&mut self, stmt: &ExprStmt, ctx: ()) {
        self.out.push_str("ExprStmt");
        self.branches(vec![("expr".to_string(), Node::from(&stmt.expr))]);
    }

    fn visit_block_stmt(&mut self, stmt: &BlockStmt, ctx: ()) {
        self.out.push_str("Block");
        self.branches(sequence("stmt", &stmt.stmts));
    }
}

impl ExprVisitor<(), ()> for TreePrinter<'_> {
    fn visit_literal_expr(&mut self, expr: &LiteralExpr, ctx: ()) {
        match &expr.value {
            // `Literal`'s own `Display` prints a string bare, which would make
            // it indistinguishable from a variable.
            Literal::String(value) => emit!(self, "Literal \"{}\"", escaped(value)),
            value => emit!(self, "Literal {value}"),
        }
    }

    fn visit_tuple_expr(&mut self, expr: &TupleExpr, ctx: ()) {
        self.out.push_str("Tuple");
        self.branches(sequence("element", &expr.elements));
    }

    fn visit_get_index_expr(&mut self, expr: &GetIndexExpr, ctx: ()) {
        self.out.push_str("GetIndex");
        self.branches(vec![
            ("target".to_string(), Node::from(&expr.target)),
            ("index".to_string(), Node::from(&expr.index)),
        ]);
    }

    fn visit_grouping_expr(&mut self, expr: &GroupingExpr, ctx: ()) {
        self.out.push_str("Grouping");
        self.branches(vec![("expr".to_string(), Node::from(&expr.expr))]);
    }

    fn visit_binary_expr(&mut self, expr: &BinaryExpr, ctx: ()) {
        emit!(self, "Binary {}", expr.operator);
        self.branches(vec![
            ("left".to_string(), Node::from(&expr.left)),
            ("right".to_string(), Node::from(&expr.right)),
        ]);
    }

    fn visit_unary_expr(&mut self, expr: &UnaryExpr, ctx: ()) {
        emit!(self, "Unary {}", expr.operator);
        self.branches(vec![("operand".to_string(), Node::from(&expr.operand))]);
    }

    fn visit_var_expr(&mut self, expr: &VarExpr, ctx: ()) {
        emit!(self, "Var {}{}", expr.name, slot(expr.resolved));
    }

    fn visit_assign_expr(&mut self, expr: &AssignExpr, ctx: ()) {
        emit!(self, "Assign {}{}", expr.name, slot(expr.resolved));
        self.branches(vec![("value".to_string(), Node::from(&expr.value))]);
    }

    fn visit_function_expr(&mut self, expr: &FunctionExpr, ctx: ()) {
        // The parameters are plain names, so they belong on this line rather
        // than among the children.
        emit!(self, "Function ({})", expr.parameters.join(", "));
        self.branches(sequence("body", &expr.body.stmts));
    }

    fn visit_call_expr(&mut self, expr: &CallExpr, ctx: ()) {
        self.out.push_str("Call");
        let mut branches = vec![("callee".to_string(), Node::from(&expr.callee))];
        branches.extend(sequence("argument", &expr.arguments));
        self.branches(branches);
    }

    fn visit_relational_expr(&mut self, expr: &RelExpr, ctx: ()) {
        self.visit_rel(expr, ctx);
    }
}

impl RelExprVisitor<(), ()> for TreePrinter<'_> {
    fn visit_source_expr(&mut self, expr: &SourceExpr, ctx: ()) {
        // A plan leaf only *names* an extensional relation, so the schema comes
        // from the catalog. Copy the reference out of `self` first: the `Cow` it
        // hands back borrows from the catalog, not from `self`, which is what
        // lets `emit!` take `&mut self.out` while the schema is still in hand.
        let catalog = self.catalog;
        let name = escaped(expr.as_id().as_str());
        match catalog.map(|catalog| catalog.source_schema(&expr.id)) {
            Some(Some(schema)) => {
                emit!(self, "Source \"{name}\" {}", schema.shape());
            }
            // A catalog that does not describe this leaf is worth saying out
            // loud: the plan names a relation nothing will bind, and this is the
            // rendering that shows it.
            Some(None) => emit!(self, "Source \"{name}\" (not in catalog)"),
            // No catalog to consult — see [`TreePrinter::catalog`].
            None => emit!(self, "Source \"{name}\""),
        }
    }

    fn visit_output_expr(&mut self, expr: &OutputExpr, ctx: ()) {
        let kind = match expr.kind {
            OutputKind::Cli => "cli",
            OutputKind::Channel => "channel",
        };
        emit!(self, "Output \"{}\" {kind}", escaped(expr.id.as_str()));
        self.branches(vec![("relation".to_string(), Node::from(&expr.relation))]);
    }

    fn visit_alias_expr(&mut self, expr: &AliasExpr, ctx: ()) {
        emit!(self, "Alias {}", expr.alias);
        self.branches(vec![("relation".to_string(), Node::from(&expr.relation))]);
    }

    fn visit_distinct_expr(&mut self, expr: &DistinctExpr, ctx: ()) {
        self.out.push_str("Distinct");
        self.branches(vec![("relation".to_string(), Node::from(&expr.relation))]);
    }

    fn visit_union_expr(&mut self, expr: &UnionExpr, ctx: ()) {
        self.out.push_str("Union");
        self.branches(sequence("relation", &expr.relations));
    }

    fn visit_difference_expr(&mut self, expr: &DifferenceExpr, ctx: ()) {
        self.out.push_str("Difference");
        self.branches(vec![
            ("left".to_string(), Node::from(&expr.left)),
            ("right".to_string(), Node::from(&expr.right)),
        ]);
    }

    fn visit_selection_expr(&mut self, expr: &SelectionExpr, ctx: ()) {
        self.out.push_str("Selection");
        self.branches(vec![
            ("relation".to_string(), Node::from(&expr.relation)),
            ("condition".to_string(), Node::from(&expr.condition)),
        ]);
    }

    fn visit_projection_expr(&mut self, expr: &ProjectionExpr, ctx: ()) {
        self.out.push_str("Projection");
        let mut branches = vec![("relation".to_string(), Node::from(&expr.relation))];
        branches.extend(select_branches(Some(&expr.attributes)));
        self.branches(branches);
    }

    fn visit_cartesian_product_expr(&mut self, expr: &CartesianProductExpr, ctx: ()) {
        // The projection lives on the equi join this delegates to, so reading the
        // children off that delegate is what keeps their names attached.
        self.out.push_str("CartesianProduct");
        self.branches(equi_join_branches(&expr.inner));
    }

    fn visit_equi_join_expr(&mut self, expr: &EquiJoinExpr, ctx: ()) {
        self.out.push_str("EquiJoin");
        self.branches(equi_join_branches(expr));
    }

    fn visit_multi_way_equi_join_expr(&mut self, expr: &MultiWayEquiJoinExpr, ctx: ()) {
        self.out.push_str("MultiWayEquiJoin");
        let mut branches = sequence("relation", &expr.relations);
        // `on(y in 2)`: the equality class by name, and the relation the
        // occurrence is evaluated against. The relation index is a payload
        // rather than a node, so labelling the child with it is the only way it
        // appears at all — and it is the entire content of the join condition.
        branches.extend(expr.on.iter().flat_map(|variable| {
            variable.occurrences.iter().map(|(relation, expr)| {
                (
                    format!("on({} in {relation})", variable.name),
                    Node::from(expr),
                )
            })
        }));
        branches.extend(select_branches(expr.attributes.as_deref()));
        self.branches(branches);
    }

    fn visit_anti_join_expr(&mut self, expr: &AntiJoinExpr, ctx: ()) {
        self.out.push_str("AntiJoin");
        let mut branches = vec![
            ("left".to_string(), Node::from(&expr.left)),
            ("right".to_string(), Node::from(&expr.right)),
        ];
        branches.extend(on_branches(&expr.on));
        self.branches(branches);
    }

    fn visit_fixed_point_iter_expr(&mut self, expr: &FixedPointIterExpr, ctx: ()) {
        // The accumulator's name binds the step body, so it belongs on this line.
        emit!(self, "FixedPointIter {}", expr.accumulator.0);
        let mut branches = vec![("init".to_string(), Node::from(&expr.accumulator.1))];
        branches.extend(sequence("step", &expr.step.stmts));
        self.branches(branches);
    }
}

/// The children of a sequence field, labelled `role(index)`.
fn sequence<'a, T>(role: &str, items: &'a [T]) -> Branches<'a>
where
    &'a T: Into<Node<'a>>,
{
    items
        .iter()
        .enumerate()
        .map(|(index, item)| (format!("{role}({index})"), item.into()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::QueryIr;
    use crate::program::QueryProgram;
    use crate::relational::{
        TableSchema,
        expr::{JoinVariable, SinkId},
    };
    use crate::scalarial::ScalarType;
    use crate::test_utils::{TestProgram, table_schema};

    fn schema(name: &str) -> TableSchema {
        table_schema(
            name,
            [("x", ScalarType::Uint), ("y", ScalarType::Uint)],
            ["x"],
        )
    }

    fn var(name: &str) -> Expr {
        Expr::from(VarExpr::new(name))
    }

    fn expr_stmt(expr: Expr) -> Stmt {
        Stmt::from(ExprStmt { expr })
    }

    fn attributes(attributes: &[(&str, &str)]) -> Vec<(String, Expr)> {
        attributes
            .iter()
            .map(|(name, source)| (name.to_string(), var(source)))
            .collect()
    }

    /// The plan a transitive-closure lowering produces: a fixed point whose step
    /// joins the accumulator against a source and unions the result back in.
    fn transitive_closure() -> Vec<Stmt> {
        vec![
            Stmt::from(VarStmt {
                name: "edge".to_string(),
                initializer: Some(Expr::from(SourceExpr::new("edge"))),
            }),
            Stmt::from(VarStmt {
                name: "reach".to_string(),
                initializer: Some(Expr::from(FixedPointIterExpr {
                    accumulator: ("acc".to_string(), var("edge")),
                    step: BlockStmt {
                        stmts: vec![expr_stmt(Expr::from(DistinctExpr {
                            relation: Expr::from(UnionExpr {
                                relations: vec![
                                    var("acc"),
                                    Expr::from(EquiJoinExpr {
                                        left: var("acc"),
                                        right: var("edge"),
                                        on: vec![(var("y"), var("x"))],
                                        attributes: Some(attributes(&[("x", "x"), ("y", "y")])),
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

    #[test]
    fn a_source_is_described_by_the_catalog_the_program_carries() {
        // The leaf names `edge` and nothing more, so the schema on this line can
        // only have come from the program's catalog.
        let program = TestProgram::new(transitive_closure(), [schema("edge")]);
        assert!(
            program
                .to_tree()
                .contains("init: Source \"edge\" (x: uint, y: uint) key(x)"),
            "{}",
            program.to_tree()
        );
    }

    #[test]
    fn a_source_no_catalog_describes_is_rendered_as_such() {
        // The failure mode a name-only leaf introduces: the plan names a
        // relation nothing will bind. Rendering is where it becomes visible.
        let program = TestProgram::new(transitive_closure(), []);
        assert!(
            program
                .to_tree()
                .contains("init: Source \"edge\" (not in catalog)"),
            "{}",
            program.to_tree()
        );
    }

    #[test]
    fn renders_a_forest_of_statements_flush_at_column_zero() {
        assert_eq!(
            // Through the inherent method, which is how bare code is rendered:
            // no import, and no catalog to describe the leaves with — see
            // `a_source_is_described_by_the_catalog_the_program_carries` for the
            // same plan rendered against one.
            QueryIr::from(transitive_closure()).to_tree(),
            "\
VarStmt edge
└─ init: Source \"edge\"
VarStmt reach
└─ init: FixedPointIter acc
   ├─ init: Var edge (unresolved)
   └─ step(0): ExprStmt
      └─ expr: Distinct
         └─ relation: Union
            ├─ relation(0): Var acc (unresolved)
            └─ relation(1): EquiJoin
               ├─ left: Var acc (unresolved)
               ├─ right: Var edge (unresolved)
               ├─ on(0 in left): Var y (unresolved)
               ├─ on(0 in right): Var x (unresolved)
               ├─ select(x): Var x (unresolved)
               └─ select(y): Var y (unresolved)
ExprStmt
└─ expr: Output \"reach\" channel
   └─ relation: Var reach (unresolved)"
        );
    }

    #[test]
    fn prints_a_bare_slice_such_as_a_fixed_points_step_body() {
        // The trait is on `[Stmt]`, not on `Vec<Stmt>`, so a sub-forest reached
        // through a field prints without being collected first — which is how
        // one inspects a single fixed-point step in isolation.
        let code = transitive_closure();
        let Stmt::Var(stmt) = &code[1] else {
            unreachable!("the second statement binds `reach`")
        };
        let Some(Expr::Relational(rel)) = &stmt.initializer else {
            unreachable!("`reach` is bound to a relational expression")
        };
        let RelExpr::FixedPointIter(fixed_point) = rel else {
            unreachable!("`reach` is bound to a fixed point")
        };
        assert_eq!(
            to_tree(&fixed_point.step.stmts),
            "\
ExprStmt
└─ expr: Distinct
   └─ relation: Union
      ├─ relation(0): Var acc (unresolved)
      └─ relation(1): EquiJoin
         ├─ left: Var acc (unresolved)
         ├─ right: Var edge (unresolved)
         ├─ on(0 in left): Var y (unresolved)
         ├─ on(0 in right): Var x (unresolved)
         ├─ select(x): Var x (unresolved)
         └─ select(y): Var y (unresolved)"
        );
    }

    #[test]
    fn addresses_a_binary_joins_keys_by_equality_and_relation() {
        // Relations, keys and projected attributes are *all* plain `Expr`s. Each
        // label says which is which: `on(1 in right)` is the right-hand side of
        // the second equality, and `select(keep)` produces the `keep` column.
        let code = [expr_stmt(Expr::from(EquiJoinExpr {
            left: var("l"),
            right: var("r"),
            on: vec![(var("a"), var("b")), (var("c"), var("d"))],
            attributes: Some(attributes(&[("out", "a"), ("keep", "c")])),
        }))];
        assert_eq!(
            to_tree(&code),
            "\
ExprStmt
└─ expr: EquiJoin
   ├─ left: Var l (unresolved)
   ├─ right: Var r (unresolved)
   ├─ on(0 in left): Var a (unresolved)
   ├─ on(0 in right): Var b (unresolved)
   ├─ on(1 in left): Var c (unresolved)
   ├─ on(1 in right): Var d (unresolved)
   ├─ select(out): Var a (unresolved)
   └─ select(keep): Var c (unresolved)"
        );
    }

    #[test]
    fn addresses_a_multi_way_joins_occurrences_by_variable_and_relation() {
        // The same statement as `on(0 in left)`, for an N-ary join: which
        // equality, in which relation. A `RelationIdx` is a payload rather than a
        // node, so the label is the only place it can appear — note that
        // relation 1 is reachable only through `z`.
        let code = [expr_stmt(Expr::from(
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
└─ expr: MultiWayEquiJoin
   ├─ relation(0): Var r0 (unresolved)
   ├─ relation(1): Var r1 (unresolved)
   ├─ relation(2): Var r2 (unresolved)
   ├─ on(y in 0): Var y (unresolved)
   ├─ on(y in 2): Var y (unresolved)
   ├─ on(z in 1): Var z (unresolved)
   └─ on(z in 2): Var z (unresolved)"
        );
    }

    #[test]
    fn keeps_the_names_of_a_projection_a_cartesian_product_delegates() {
        // A `CartesianProductExpr` holds its projection on the `EquiJoinExpr` it
        // delegates to; reading the children off that delegate is what keeps
        // each attribute expression next to the name it produces.
        let code = [expr_stmt(Expr::from(CartesianProductExpr::new(
            var("l"),
            var("r"),
            Some(attributes(&[("out", "a"), ("keep", "b")])),
        )))];
        assert_eq!(
            to_tree(&code),
            "\
ExprStmt
└─ expr: CartesianProduct
   ├─ left: Var l (unresolved)
   ├─ right: Var r (unresolved)
   ├─ select(out): Var a (unresolved)
   └─ select(keep): Var b (unresolved)"
        );
    }

    #[test]
    fn guides_lines_through_deeper_siblings() {
        // The `│` of the outer union has to continue past the whole first
        // branch, otherwise a deep tree cannot be read.
        let code = [expr_stmt(Expr::from(UnionExpr {
            relations: vec![Expr::from(DistinctExpr { relation: var("a") }), var("b")],
        }))];
        assert_eq!(
            to_tree(&code),
            "\
ExprStmt
└─ expr: Union
   ├─ relation(0): Distinct
   │  └─ relation: Var a (unresolved)
   └─ relation(1): Var b (unresolved)"
        );
    }

    #[test]
    fn shows_a_resolved_variable_slot() {
        // The resolver writes into the tree in place, so this is how one sees
        // whether it has been here — and what it decided.
        let mut code = [expr_stmt(var("x"))];
        let Stmt::Expr(stmt) = &mut code[0] else {
            unreachable!("the statement is an expression statement")
        };
        let Expr::Var(expr) = &mut stmt.expr else {
            unreachable!("the expression is a variable")
        };
        expr.resolved = Some((1, 4));
        assert_eq!(
            to_tree(&code),
            "\
ExprStmt
└─ expr: Var x @1:4"
        );
    }

    #[test]
    fn quotes_and_escapes_string_payloads() {
        let code = [expr_stmt(Expr::from(LiteralExpr::from("a\"b")))];
        assert_eq!(
            to_tree(&code),
            "\
ExprStmt
└─ expr: Literal \"a\\\"b\""
        );
    }
}
