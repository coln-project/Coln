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
//!   So does anything that has to *address* a child rather than just reach it,
//!   which is why [`print`](super::print) is a visitor too.
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
    relational::expr::{EquiJoinExpr, OutputExpr, RelExpr, SourceExpr},
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

impl<'a> Node<'a> {
    /// Append this node's node-typed children to `out`, in source order.
    ///
    /// **This is the only place the AST's shape is spelled out for traversal
    /// purposes.** Non-node payloads (an operator, an attribute's name, a
    /// [`RelationIdx`](crate::relational::expr::RelationIdx)) are not children,
    /// and neither is the *position* a child occupies here: a scan reaches every
    /// child either way, and a consumer that needs to tell one `on` key from
    /// another is addressing rather than scanning, so it visits instead.
    pub fn push_children(self, out: &mut Vec<Node<'a>>) {
        match self {
            Node::Stmt(stmt) => Self::push_stmt_children(stmt, out),
            Node::Expr(expr) => Self::push_expr_children(expr, out),
            Node::Rel(rel) => Self::push_rel_children(rel, out),
        }
    }

    fn push_stmt_children(stmt: &'a Stmt, out: &mut Vec<Node<'a>>) {
        match stmt {
            Stmt::Var(stmt) => out.extend(stmt.initializer.iter().map(Node::from)),
            Stmt::Expr(stmt) => out.push(Node::from(&stmt.expr)),
            Stmt::Block(stmt) => out.extend(stmt.stmts.iter().map(Node::from)),
        }
    }

    fn push_expr_children(expr: &'a Expr, out: &mut Vec<Node<'a>>) {
        match expr {
            Expr::Literal(_) | Expr::Var(_) => {}
            Expr::Tuple(expr) => out.extend(expr.elements.iter().map(Node::from)),
            Expr::GetIndex(expr) => {
                out.extend([Node::from(&expr.target), Node::from(&expr.index)]);
            }
            Expr::Grouping(expr) => out.push(Node::from(&expr.expr)),
            Expr::Binary(expr) => out.extend([Node::from(&expr.left), Node::from(&expr.right)]),
            Expr::Unary(expr) => out.push(Node::from(&expr.operand)),
            Expr::Assign(expr) => out.push(Node::from(&expr.value)),
            Expr::Call(expr) => {
                out.push(Node::from(&expr.callee));
                out.extend(expr.arguments.iter().map(Node::from));
            }
            // The parameters are plain names, so only the body holds nodes.
            Expr::Function(expr) => out.extend(expr.body.stmts.iter().map(Node::from)),
            // Normalized away by `Node::from`; reachable only through a
            // hand-built `Node::Expr`, which is handled rather than pruned.
            Expr::Relational(rel) => out.push(Node::Rel(rel)),
        }
    }

    fn push_rel_children(rel: &'a RelExpr, out: &mut Vec<Node<'a>>) {
        match rel {
            // A plan leaf: it only *names* an extensional relation.
            RelExpr::Source(_) => {}
            RelExpr::Output(expr) => out.push(Node::from(&expr.relation)),
            RelExpr::Alias(expr) => out.push(Node::from(&expr.relation)),
            RelExpr::Distinct(expr) => out.push(Node::from(&expr.relation)),
            RelExpr::Union(expr) => out.extend(expr.relations.iter().map(Node::from)),
            RelExpr::Difference(expr) => {
                out.extend([Node::from(&expr.left), Node::from(&expr.right)]);
            }
            RelExpr::Selection(expr) => {
                out.extend([Node::from(&expr.relation), Node::from(&expr.condition)]);
            }
            RelExpr::Projection(expr) => {
                out.push(Node::from(&expr.relation));
                out.extend(expr.attributes.iter().map(|(_, expr)| Node::from(expr)));
            }
            // A cartesian product is an equi join with an empty `on`, so it has
            // no children beyond that join's.
            RelExpr::CartesianProduct(expr) => Self::push_equi_join_children(&expr.inner, out),
            RelExpr::EquiJoin(expr) => Self::push_equi_join_children(expr, out),
            RelExpr::MultiWayEquiJoin(expr) => {
                out.extend(expr.relations.iter().map(Node::from));
                out.extend(expr.on_exprs().map(Node::from));
                out.extend(
                    expr.attributes
                        .iter()
                        .flatten()
                        .map(|(_, expr)| Node::from(expr)),
                );
            }
            RelExpr::AntiJoin(expr) => {
                out.extend([Node::from(&expr.left), Node::from(&expr.right)]);
                out.extend(
                    expr.on
                        .iter()
                        .flat_map(|(left, right)| [Node::from(left), Node::from(right)]),
                );
            }
            RelExpr::FixedPointIter(expr) => {
                out.push(Node::from(&expr.accumulator.1));
                out.extend(expr.step.stmts.iter().map(Node::from));
            }
        }
    }

    fn push_equi_join_children(expr: &'a EquiJoinExpr, out: &mut Vec<Node<'a>>) {
        out.extend([Node::from(&expr.left), Node::from(&expr.right)]);
        out.extend(
            expr.on
                .iter()
                .flat_map(|(left, right)| [Node::from(left), Node::from(right)]),
        );
        out.extend(
            expr.attributes
                .iter()
                .flatten()
                .map(|(_, expr)| Node::from(expr)),
        );
    }

    /// Every event of the subtree rooted at this node.
    pub fn walk(self) -> Walk<'a> {
        Walk::new([self])
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
    /// discovery filters a [walk](Walk) on.
    pub fn as_source(self) -> Option<&'a SourceExpr> {
        self.as_rel().and_then(|expr| match expr {
            RelExpr::Source(source) => Some(source.as_ref()),
            _ => None,
        })
    }

    /// The [`SourceExpr`] leaf this node is, if any. What a plan-wide output
    /// discovery filters a [walk](Walk) on.
    pub fn as_output(self) -> Option<&'a OutputExpr> {
        self.as_rel().and_then(|expr| match expr {
            RelExpr::Output(output) => Some(output.as_ref()),
            _ => None,
        })
    }
}

/// One step of a [`Walk`]. Every node is reported twice, so a consumer can pick
/// its order ([`pre_order`], [`post_order`]) or track depth, without the walk
/// having to offer one iterator per traversal.
#[derive(Clone, Copy, Debug)]
pub enum Event<'a> {
    Enter(Node<'a>),
    Leave(Node<'a>),
}

impl<'a> Event<'a> {
    /// The node, whichever half of its visit this is.
    pub fn node(self) -> Node<'a> {
        match self {
            Event::Enter(node) | Event::Leave(node) => node,
        }
    }

    pub fn entered(self) -> Option<Node<'a>> {
        match self {
            Event::Enter(node) => Some(node),
            Event::Leave(_) => None,
        }
    }

    pub fn left(self) -> Option<Node<'a>> {
        match self {
            Event::Leave(node) => Some(node),
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
    children: Vec<Node<'a>>,
}

impl<'a> Walk<'a> {
    fn new(roots: impl IntoIterator<Item = Node<'a>>) -> Self {
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
        if let Event::Enter(node) = event {
            // The node's own leave sits below its children, so it is reported
            // once the whole subtree is done.
            self.pending.push(Event::Leave(node));
            node.push_children(&mut self.children);
            self.pending
                .extend(self.children.drain(..).rev().map(Event::Enter));
        }
        Some(event)
    }
}

/// Every event of `code`, depth-first, statements in order.
pub fn walk(code: &[Stmt]) -> Walk<'_> {
    Walk::new(code.iter().map(Node::from))
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
    use crate::relational::expr::{DistinctExpr, EquiJoinExpr, SelectionExpr, UnionExpr};

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
        // The operands are reduced before the operator combining them, which is
        // why post-order — not pre-order — is what an interpreter follows.
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
                relation: Expr::from(SourceExpr::new("edge")),
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
                    Expr::from(SourceExpr::new("left")),
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
                        Expr::from(SourceExpr::new("left")),
                        Expr::from(SourceExpr::new("right")),
                    ],
                }),
                condition: Expr::from(VarExpr::new("a")),
            }),
        })];
        let sources: Vec<&str> = pre_order(&code)
            .filter_map(Node::as_source)
            .map(|source| source.as_id().as_str())
            .collect();
        assert_eq!(sources, ["left", "right"]);
    }

    #[test]
    fn every_operand_of_a_join_is_reached_even_though_none_is_addressable() {
        // A scan has to reach the relations, the keys *and* the projected
        // attributes; telling them apart is not its job (see `push_children`),
        // so this pins reachability and order, nothing more.
        let code = vec![Stmt::from(ExprStmt {
            expr: Expr::from(EquiJoinExpr {
                left: Expr::from(VarExpr::new("l")),
                right: Expr::from(VarExpr::new("r")),
                on: vec![(Expr::from(VarExpr::new("a")), Expr::from(VarExpr::new("b")))],
                attributes: Some(vec![("out".to_string(), Expr::from(VarExpr::new("c")))]),
            }),
        })];
        assert_eq!(
            labels(pre_order(&code)),
            ["stmt", "join", "l", "r", "a", "b", "c"]
        );
    }
}
