// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Generic depth-first traversal of the AST, for consumers that only need to
//! *scan* it.
//!
//! # Why this exists next to the visitor traits
//!
//! The AST has two kinds of consumer, and they need opposite things:
//!
//! - A **fold** computes a value per node out of its children's values, and the
//!   parent decides whether and in which order children are visited at all
//!   (`TypeResolver::visit_unary_expr` answers `Not` without ever looking at the
//!   operand; `DbspInterpreter::visit_equi_join_expr` must consume the left
//!   alias *between* its two operands). No traversal can be factored out of
//!   those — they stay on the [`ExprVisitor`](super::expr::ExprVisitor) family.
//! - A **scan** reads nodes, produces no per-node result, and does not care
//!   about order. Everything a scan needs from the AST's shape is "what are this
//!   node's children", which is exactly what this module states *once*.
//!
//! So this is not a replacement for the visitors, it is the other half: scans
//! stop restating the tree's shape, and a potential future flatter
//! representation only has to re-implement [`Node::push_children`] to keep
//! every scan working.
//!
//! # Traversal orders
//!
//! [`Walk`] yields [`Enter`](Event::Enter)/[`Leave`](Event::Leave) events, from
//! which every order follows: [`pre_order`] keeps the enters, [`post_order`]
//! keeps the leaves. A static order is not the real execution order either way:
//! A [`CallExpr`](super::expr::CallExpr) jumps into a function body and a
//! [`FixedPointIterExpr`](crate::relational::expr::FixedPointIterExpr) repeats
//! its step.
//!
//! There is deliberately no `&mut` counterpart. A `&mut` to a parent and to its
//! children cannot be held at once, so a mutable walk could only ever be
//! pre-order and could never emit [`Leave`](Event::Leave) — which is precisely
//! the half `Resolver` needs, to pop the scopes and tuple contexts it pushes on
//! the way down. Rewriting passes stay visitors.

use crate::{
    host::{expr::Expr, stmt::Stmt},
    relational::expr::{EquiJoinExpr, RelExpr, SourceExpr},
};

/// A borrowed pointer to a node of any of the three mutually recursive node
/// kinds. The kinds interleave in both directions — a
/// [`FunctionExpr`](super::expr::FunctionExpr) body and a
/// [`FixedPointIterExpr`](crate::relational::expr::FixedPointIterExpr) step are
/// statements — so a traversal has to be able to hold any of them.
///
/// [`Expr::Relational`] never appears as a [`Node::Expr`]: it is a pure wrapper
/// carrying no data of its own, so the walk yields the [`RelExpr`] inside it
/// directly rather than both.
#[derive(Clone, Copy, Debug)]
pub enum Node<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
    Rel(&'a RelExpr),
}

impl<'a> From<&'a Stmt> for Node<'a> {
    fn from(stmt: &'a Stmt) -> Self {
        Node::Stmt(stmt)
    }
}

impl<'a> From<&'a Expr> for Node<'a> {
    fn from(expr: &'a Expr) -> Self {
        match expr {
            // Unwrap the bridge into the relational layer, see [`Node`].
            Expr::Relational(rel) => Node::Rel(rel),
            expr => Node::Expr(expr),
        }
    }
}

impl<'a> From<&'a RelExpr> for Node<'a> {
    fn from(rel: &'a RelExpr) -> Self {
        Node::Rel(rel)
    }
}

/// A child of a node, tagged with the position it occupies in its parent.
///
/// A scan ignores all of it; a rendering needs it, because a bare node cannot
/// say *which* operand it is: an [`EquiJoinExpr`]'s left relation and one of its
/// join keys are both plain [`Expr`]s. The three parts spell out one position
/// exactly — see [`label`](Self::label) for how they read together — so a
/// consumer never has to recover it by counting children and knowing which
/// field is a `Vec` of what.
#[derive(Clone, Copy, Debug)]
pub struct Child<'a> {
    /// The field this child came from.
    pub role: &'static str,
    /// Which occurrence within [`role`](Self::role), when that field holds a
    /// collection. `None` for a field holding a single child.
    pub index: Option<usize>,
    /// Which part of that occurrence, when the collection holds tuples — the
    /// two sides of a join's `on` pair. `None` otherwise.
    pub part: Option<&'static str>,
    pub node: Node<'a>,
}

/// The role reported for the node a walk was started from.
pub const ROOT: &str = "root";

impl<'a> Child<'a> {
    /// The only child of a field: `relation`, `condition`, `callee`.
    fn new(role: &'static str, node: impl Into<Node<'a>>) -> Self {
        Self {
            role,
            index: None,
            part: None,
            node: node.into(),
        }
    }

    /// One element of a collection field: `argument[1]`, `select[0]`.
    fn at(role: &'static str, index: usize, node: impl Into<Node<'a>>) -> Self {
        Self {
            index: Some(index),
            ..Self::new(role, node)
        }
    }

    /// One part of a tuple element of a collection field: `on[0].left`.
    fn part(
        role: &'static str,
        index: usize,
        part: &'static str,
        node: impl Into<Node<'a>>,
    ) -> Self {
        Self {
            part: Some(part),
            ..Self::at(role, index, node)
        }
    }

    fn root(node: impl Into<Node<'a>>) -> Self {
        Self::new(ROOT, node)
    }

    /// This child's position as a path — `relation`, `select[1]`,
    /// `on[0].left` — which is what a rendering puts in front of the node.
    pub fn label(&self) -> String {
        let index = self
            .index
            .map(|index| format!("[{index}]"))
            .unwrap_or_default();
        let part = self.part.map(|part| format!(".{part}")).unwrap_or_default();
        format!("{}{index}{part}", self.role)
    }
}

impl<'a> Node<'a> {
    /// Append this node's node-typed children to `out`, in source order, each
    /// tagged with the position it occupies here.
    ///
    /// **This is the only place the AST's shape is spelled out for traversal
    /// purposes.** Non-node payloads (an operator, an attribute's name, a
    /// [`SinkId`](crate::relational::expr::SinkId)) are not children and are
    /// reached by matching on the node itself — which is also why a
    /// [`Child`]'s position is worth stating: it is what lets a consumer tie a
    /// child back to the payload that names it, without re-deriving the field
    /// layout it is trying not to depend on.
    pub fn push_children(self, out: &mut Vec<Child<'a>>) {
        match self {
            Node::Stmt(stmt) => Self::push_stmt_children(stmt, out),
            Node::Expr(expr) => Self::push_expr_children(expr, out),
            Node::Rel(rel) => Self::push_rel_children(rel, out),
        }
    }

    fn push_stmt_children(stmt: &'a Stmt, out: &mut Vec<Child<'a>>) {
        match stmt {
            // At most one initializer, so there is nothing to index.
            Stmt::Var(stmt) => {
                out.extend(stmt.initializer.iter().map(|expr| Child::new("init", expr)))
            }
            Stmt::Expr(stmt) => out.push(Child::new("expr", &stmt.expr)),
            Stmt::Block(stmt) => out.extend(
                stmt.stmts
                    .iter()
                    .enumerate()
                    .map(|(index, stmt)| Child::at("stmt", index, stmt)),
            ),
        }
    }

    fn push_expr_children(expr: &'a Expr, out: &mut Vec<Child<'a>>) {
        match expr {
            Expr::Literal(_) | Expr::Var(_) => {}
            Expr::Tuple(expr) => out.extend(
                expr.elements
                    .iter()
                    .enumerate()
                    .map(|(index, expr)| Child::at("element", index, expr)),
            ),
            Expr::GetIndex(expr) => out.extend([
                Child::new("target", &expr.target),
                Child::new("index", &expr.index),
            ]),
            Expr::Grouping(expr) => out.push(Child::new("expr", &expr.expr)),
            Expr::Binary(expr) => out.extend([
                Child::new("left", &expr.left),
                Child::new("right", &expr.right),
            ]),
            Expr::Unary(expr) => out.push(Child::new("operand", &expr.operand)),
            Expr::Assign(expr) => out.push(Child::new("value", &expr.value)),
            Expr::Call(expr) => {
                out.push(Child::new("callee", &expr.callee));
                out.extend(
                    expr.arguments
                        .iter()
                        .enumerate()
                        .map(|(index, expr)| Child::at("argument", index, expr)),
                );
            }
            // The parameters are plain names, so only the body holds nodes.
            Expr::Function(expr) => out.extend(
                expr.body
                    .stmts
                    .iter()
                    .enumerate()
                    .map(|(index, stmt)| Child::at("body", index, stmt)),
            ),
            // Normalized away by `Node::from`; reachable only through a
            // hand-built `Node::Expr`, which is handled rather than pruned.
            Expr::Relational(rel) => out.push(Child::new("rel", &**rel)),
        }
    }

    fn push_rel_children(rel: &'a RelExpr, out: &mut Vec<Child<'a>>) {
        match rel {
            // A plan leaf: it only *names* an extensional relation.
            RelExpr::Source(_) => {}
            RelExpr::Output(expr) => out.push(Child::new("relation", &expr.relation)),
            RelExpr::Alias(expr) => out.push(Child::new("relation", &expr.relation)),
            RelExpr::Distinct(expr) => out.push(Child::new("relation", &expr.relation)),
            RelExpr::Union(expr) => out.extend(
                expr.relations
                    .iter()
                    .enumerate()
                    .map(|(index, expr)| Child::at("relation", index, expr)),
            ),
            RelExpr::Difference(expr) => out.extend([
                Child::new("left", &expr.left),
                Child::new("right", &expr.right),
            ]),
            RelExpr::Selection(expr) => out.extend([
                Child::new("relation", &expr.relation),
                Child::new("condition", &expr.condition),
            ]),
            RelExpr::Projection(expr) => {
                out.push(Child::new("relation", &expr.relation));
                // The attribute *name* stays a payload of the projection, so the
                // index is what ties this expression back to it.
                out.extend(
                    expr.attributes
                        .iter()
                        .enumerate()
                        .map(|(index, (_, expr))| Child::at("select", index, expr)),
                );
            }
            // A cartesian product is an equi join with an empty `on`, so it has
            // no children beyond that join's.
            RelExpr::CartesianProduct(expr) => Self::push_equi_join_children(&expr.inner, out),
            RelExpr::EquiJoin(expr) => Self::push_equi_join_children(expr, out),
            RelExpr::MultiWayEquiJoin(expr) => {
                out.extend(
                    expr.relations
                        .iter()
                        .enumerate()
                        .map(|(index, expr)| Child::at("relation", index, expr)),
                );
                // Indexed by equality class rather than by flattened position,
                // so `on[0]` names the same class the node's own rendering
                // reports the relation indices for. This is why `on_exprs` is
                // not used here: it drops the class boundaries.
                out.extend(expr.on.iter().enumerate().flat_map(|(index, variable)| {
                    variable
                        .occurrences
                        .iter()
                        .map(move |(_, expr)| Child::at("on", index, expr))
                }));
                out.extend(
                    expr.attributes
                        .iter()
                        .flatten()
                        .enumerate()
                        .map(|(index, (_, expr))| Child::at("select", index, expr)),
                );
            }
            RelExpr::AntiJoin(expr) => {
                out.extend([
                    Child::new("left", &expr.left),
                    Child::new("right", &expr.right),
                ]);
                out.extend(
                    expr.on
                        .iter()
                        .enumerate()
                        .flat_map(|(index, (left, right))| {
                            [
                                Child::part("on", index, "left", left),
                                Child::part("on", index, "right", right),
                            ]
                        }),
                );
            }
            RelExpr::FixedPointIter(expr) => {
                out.push(Child::new("init", &expr.accumulator.1));
                out.extend(
                    expr.step
                        .stmts
                        .iter()
                        .enumerate()
                        .map(|(index, stmt)| Child::at("step", index, stmt)),
                );
            }
        }
    }

    fn push_equi_join_children(expr: &'a EquiJoinExpr, out: &mut Vec<Child<'a>>) {
        out.extend([
            Child::new("left", &expr.left),
            Child::new("right", &expr.right),
        ]);
        // Each pair is one equality to match on, and its two sides are evaluated
        // against *different* relations, so which side a child is cannot be left
        // to the reader's arithmetic.
        out.extend(
            expr.on
                .iter()
                .enumerate()
                .flat_map(|(index, (left, right))| {
                    [
                        Child::part("on", index, "left", left),
                        Child::part("on", index, "right", right),
                    ]
                }),
        );
        out.extend(
            expr.attributes
                .iter()
                .flatten()
                .enumerate()
                .map(|(index, (_, expr))| Child::at("select", index, expr)),
        );
    }

    /// Every event of the subtree rooted at this node.
    pub fn walk(self) -> Walk<'a> {
        Walk::new([Child::root(self)])
    }

    /// This node and its descendants, parents before children.
    pub fn pre_order(self) -> impl Iterator<Item = Node<'a>> {
        self.walk().filter_map(|event| event.entered())
    }

    /// This node and its descendants, children before parents.
    pub fn post_order(self) -> impl Iterator<Item = Node<'a>> {
        self.walk().filter_map(|event| event.left())
    }

    pub fn as_stmt(self) -> Option<&'a Stmt> {
        match self {
            Node::Stmt(stmt) => Some(stmt),
            _ => None,
        }
    }

    pub fn as_expr(self) -> Option<&'a Expr> {
        match self {
            Node::Expr(expr) => Some(expr),
            _ => None,
        }
    }

    pub fn as_rel(self) -> Option<&'a RelExpr> {
        match self {
            Node::Rel(rel) => Some(rel),
            _ => None,
        }
    }

    /// The [`SourceExpr`] leaf this node is, if any. What a plan-wide source
    /// discovery filters a walk on.
    pub fn as_source(self) -> Option<&'a SourceExpr> {
        match self {
            Node::Rel(RelExpr::Source(source)) => Some(source),
            _ => None,
        }
    }
}

/// One step of a [`Walk`]. Every node is reported twice, so a consumer can pick
/// its order ([`pre_order`], [`post_order`]) or track depth, without the walk
/// having to offer one iterator per traversal.
#[derive(Clone, Copy, Debug)]
pub enum Event<'a> {
    Enter(Child<'a>),
    Leave(Child<'a>),
}

impl<'a> Event<'a> {
    /// The child, whichever half of its visit this is.
    pub fn child(self) -> Child<'a> {
        match self {
            Event::Enter(child) | Event::Leave(child) => child,
        }
    }

    pub fn entered(self) -> Option<Node<'a>> {
        match self {
            Event::Enter(child) => Some(child.node),
            Event::Leave(_) => None,
        }
    }

    pub fn left(self) -> Option<Node<'a>> {
        match self {
            Event::Leave(child) => Some(child.node),
            Event::Enter(_) => None,
        }
    }
}

/// A depth-first walk over one or more subtrees, as a stream of [`Event`]s.
/// Iterative rather than recursive, so a deeply nested plan cannot exhaust the
/// stack.
pub struct Walk<'a> {
    /// Pending work, innermost last. A node's `Leave` is pushed underneath its
    /// children when the node is entered.
    pending: Vec<Event<'a>>,
    /// Scratch space for [`Node::push_children`], reused across nodes so the
    /// walk allocates amortized nothing per node.
    children: Vec<Child<'a>>,
}

impl<'a> Walk<'a> {
    fn new(roots: impl IntoIterator<Item = Child<'a>>) -> Self {
        let mut pending: Vec<Event<'a>> = roots.into_iter().map(Event::Enter).collect();
        // The stack is popped from the back, so the first root has to end up
        // last.
        pending.reverse();
        Self {
            pending,
            children: Vec::new(),
        }
    }
}

impl<'a> Iterator for Walk<'a> {
    type Item = Event<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let event = self.pending.pop()?;
        if let Event::Enter(child) = event {
            // The node's own leave sits below its children, so it is reported
            // once the whole subtree is done.
            self.pending.push(Event::Leave(child));
            child.node.push_children(&mut self.children);
            self.pending
                .extend(self.children.drain(..).rev().map(Event::Enter));
        }
        Some(event)
    }
}

/// Every event of `code`, depth-first, statements in order.
pub fn walk(code: &[Stmt]) -> Walk<'_> {
    Walk::new(code.iter().map(Child::root))
}

/// Every node of `code`, parents before children.
pub fn pre_order(code: &[Stmt]) -> impl Iterator<Item = Node<'_>> {
    walk(code).filter_map(|event| event.entered())
}

/// Every node of `code`, children before parents — the order in which an
/// interpreter reduces them.
pub fn post_order(code: &[Stmt]) -> impl Iterator<Item = Node<'_>> {
    walk(code).filter_map(|event| event.left())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{
        expr::{BinaryExpr, LiteralExpr, VarExpr},
        operator::Operator,
        stmt::{ExprStmt, VarStmt},
    };
    use crate::relational::{
        RelationSchema,
        expr::{
            DistinctExpr, EquiJoinExpr, JoinVariable, MultiWayEquiJoinExpr, SelectionExpr,
            UnionExpr,
        },
    };

    fn schema(name: &str) -> RelationSchema {
        RelationSchema::new(name, ["a", "b"], ["a"]).expect("Correct schema definition")
    }

    /// `1 + 2` as a statement.
    fn arithmetic() -> Vec<Stmt> {
        vec![Stmt::from(ExprStmt {
            expr: Expr::from(BinaryExpr {
                operator: Operator::Addition,
                left: Expr::from(LiteralExpr::from(1_u64)),
                right: Expr::from(LiteralExpr::from(2_u64)),
            }),
        })]
    }

    /// A label per node kind, enough to pin an order down in a test.
    fn label(node: Node<'_>) -> String {
        match node {
            Node::Stmt(stmt) => match stmt {
                Stmt::Var(stmt) => format!("var {}", stmt.name),
                Stmt::Expr(_) => "stmt".to_string(),
                Stmt::Block(_) => "block".to_string(),
            },
            Node::Expr(expr) => match expr {
                Expr::Literal(expr) => expr.value.to_string(),
                Expr::Binary(expr) => expr.operator.to_string(),
                Expr::Var(expr) => expr.name.clone(),
                other => format!("{other:?}"),
            },
            Node::Rel(rel) => match rel {
                RelExpr::Source(source) => format!("source {}", source.as_id()),
                RelExpr::Distinct(_) => "distinct".to_string(),
                RelExpr::Union(_) => "union".to_string(),
                RelExpr::Selection(_) => "selection".to_string(),
                RelExpr::EquiJoin(_) => "join".to_string(),
                other => format!("{other:?}"),
            },
        }
    }

    fn labels<'a>(nodes: impl Iterator<Item = Node<'a>>) -> Vec<String> {
        nodes.map(label).collect()
    }

    #[test]
    fn pre_order_reports_parents_before_children_left_to_right() {
        let code = arithmetic();
        assert_eq!(labels(pre_order(&code)), ["stmt", "+", "1", "2"]);
    }

    #[test]
    fn post_order_is_evaluation_order() {
        let code = arithmetic();
        assert_eq!(labels(post_order(&code)), ["1", "2", "+", "stmt"]);
    }

    #[test]
    fn every_node_is_entered_and_left_exactly_once() {
        let code = arithmetic();
        let events: Vec<_> = walk(&code).collect();
        assert_eq!(events.len(), 2 * pre_order(&code).count());
        // A well-formed nesting: the depth returns to zero and never goes below.
        let mut depth = 0_isize;
        for event in events {
            depth += match event {
                Event::Enter(_) => 1,
                Event::Leave(_) => -1,
            };
            assert!(depth >= 0, "left a node that was never entered");
        }
        assert_eq!(depth, 0, "entered a node that was never left");
    }

    #[test]
    fn the_relational_wrapper_is_not_reported_as_a_node() {
        // `Expr::Relational` carries nothing of its own, so `distinct(source)`
        // must be two nodes below the statement, not four.
        let code = vec![Stmt::from(ExprStmt {
            expr: Expr::from(DistinctExpr {
                relation: Expr::from(SourceExpr::new(schema("edge"))),
            }),
        })];
        assert_eq!(
            labels(pre_order(&code)),
            ["stmt", "distinct", "source edge"]
        );
    }

    #[test]
    fn walking_crosses_between_statements_and_expressions() {
        // A source nested in a var initializer inside a union: the walk has to
        // change node kind twice to reach it.
        let code = vec![Stmt::from(VarStmt {
            name: "both".to_string(),
            initializer: Some(Expr::from(UnionExpr {
                relations: vec![
                    Expr::from(SourceExpr::new(schema("left"))),
                    Expr::from(VarExpr::new("right")),
                ],
            })),
        })];
        assert_eq!(
            labels(pre_order(&code)),
            ["var both", "union", "source left", "right"]
        );
    }

    #[test]
    fn a_scan_filters_the_walk_instead_of_restating_the_tree() {
        // The pattern that replaced the hand-written source collection in the
        // DBSP backend: no knowledge of the tree's shape at the call site.
        let code = vec![Stmt::from(ExprStmt {
            expr: Expr::from(SelectionExpr {
                relation: Expr::from(UnionExpr {
                    relations: vec![
                        Expr::from(SourceExpr::new(schema("left"))),
                        Expr::from(SourceExpr::new(schema("right"))),
                    ],
                }),
                condition: Expr::from(VarExpr::new("a")),
            }),
        })];
        let sources: Vec<&str> = pre_order(&code)
            .filter_map(Node::as_source)
            .map(SourceExpr::as_id)
            .collect();
        assert_eq!(sources, ["left", "right"]);
    }

    /// Every child of `code`'s first relational node, as `<label>: <node>`.
    fn children_of_first_rel(code: &[Stmt]) -> Vec<String> {
        let rel = pre_order(code)
            .find_map(Node::as_rel)
            .expect("the operator is reachable");
        let mut children = Vec::new();
        Node::from(rel).push_children(&mut children);
        children
            .into_iter()
            .map(|child| format!("{}: {}", child.label(), label(child.node)))
            .collect()
    }

    #[test]
    fn a_childs_label_pins_down_which_operand_it_is() {
        // A join's relations, its keys and its projected attributes are *all*
        // plain `Expr`s. Without a label a consumer would have to know the field
        // layout and count children to tell them apart; with one, each child
        // says where it came from — including which side of which `on` pair.
        let code = vec![Stmt::from(ExprStmt {
            expr: Expr::from(EquiJoinExpr {
                left: Expr::from(VarExpr::new("l")),
                right: Expr::from(VarExpr::new("r")),
                on: vec![
                    (Expr::from(VarExpr::new("a")), Expr::from(VarExpr::new("b"))),
                    (Expr::from(VarExpr::new("c")), Expr::from(VarExpr::new("d"))),
                ],
                attributes: Some(vec![
                    ("out".to_string(), Expr::from(VarExpr::new("a"))),
                    ("keep".to_string(), Expr::from(VarExpr::new("c"))),
                ]),
            }),
        })];
        assert_eq!(
            children_of_first_rel(&code),
            [
                "left: l",
                "right: r",
                "on[0].left: a",
                "on[0].right: b",
                "on[1].left: c",
                "on[1].right: d",
                "select[0]: a",
                "select[1]: c",
            ]
        );
    }

    #[test]
    fn a_multi_way_joins_on_children_are_indexed_by_equality_class() {
        // Not by flattened position: `on[1]` has to name the same class the
        // node's own rendering reports relation indices for, so that the two
        // `on[1]` children are the occurrences of *that* variable.
        let code = vec![Stmt::from(ExprStmt {
            expr: Expr::from(
                MultiWayEquiJoinExpr::new(
                    vec![
                        Expr::from(VarExpr::new("r0")),
                        Expr::from(VarExpr::new("r1")),
                        Expr::from(VarExpr::new("r2")),
                    ],
                    vec![
                        JoinVariable {
                            name: "y".to_string(),
                            occurrences: vec![
                                (0, Expr::from(VarExpr::new("y"))),
                                (2, Expr::from(VarExpr::new("y"))),
                            ],
                        },
                        JoinVariable {
                            name: "z".to_string(),
                            occurrences: vec![
                                (1, Expr::from(VarExpr::new("z"))),
                                (2, Expr::from(VarExpr::new("z"))),
                            ],
                        },
                    ],
                    None,
                )
                .expect("every variable is bound by two relations"),
            ),
        })];
        assert_eq!(
            children_of_first_rel(&code),
            [
                "relation[0]: r0",
                "relation[1]: r1",
                "relation[2]: r2",
                "on[0]: y",
                "on[0]: y",
                "on[1]: z",
                "on[1]: z",
            ]
        );
    }
}
