// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::expr::Expr;
use crate::{impl_from_auto_box, util::MemAddr};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Stmt {
    // TODO: control flow: IfStmt, WhileStmt, Return?, Print?
    Var(Box<VarStmt>),
    Expr(Box<ExprStmt>),
    Block(Box<BlockStmt>),
}

impl_from_auto_box! {
    Stmt,
    (Stmt::Var, VarStmt),
    (Stmt::Expr, ExprStmt),
    (Stmt::Block, BlockStmt)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VarStmt {
    pub name: String,
    pub initializer: Option<Expr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExprStmt {
    pub expr: Expr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockStmt {
    pub stmts: Vec<Stmt>,
}

pub trait StmtVisitor<T, C> {
    fn visit_stmt(&mut self, stmt: &Stmt, ctx: C) -> T {
        match stmt {
            Stmt::Var(stmt) => self.visit_var_stmt(stmt, ctx),
            Stmt::Expr(stmt) => self.visit_expr_stmt(stmt, ctx),
            Stmt::Block(stmt) => self.visit_block_stmt(stmt, ctx),
        }
    }
    fn visit_var_stmt(&mut self, stmt: &VarStmt, ctx: C) -> T;
    fn visit_expr_stmt(&mut self, stmt: &ExprStmt, ctx: C) -> T;
    fn visit_block_stmt(&mut self, stmt: &BlockStmt, ctx: C) -> T;
}

pub trait StmtVisitorMut<T, C> {
    fn visit_stmt(&mut self, stmt: &mut Stmt, ctx: C) -> T {
        match stmt {
            Stmt::Var(stmt) => self.visit_var_stmt(stmt, ctx),
            Stmt::Expr(stmt) => self.visit_expr_stmt(stmt, ctx),
            Stmt::Block(stmt) => self.visit_block_stmt(stmt, ctx),
        }
    }
    fn visit_var_stmt(&mut self, stmt: &mut VarStmt, ctx: C) -> T;
    fn visit_expr_stmt(&mut self, stmt: &mut ExprStmt, ctx: C) -> T;
    fn visit_block_stmt(&mut self, stmt: &mut BlockStmt, ctx: C) -> T;
}

pub trait StmtVisitorOwn<T, C> {
    fn visit_stmt(&mut self, stmt: Stmt, ctx: C) -> T {
        match stmt {
            Stmt::Var(stmt) => self.visit_var_stmt(*stmt, ctx),
            Stmt::Expr(stmt) => self.visit_expr_stmt(*stmt, ctx),
            Stmt::Block(stmt) => self.visit_block_stmt(*stmt, ctx),
        }
    }
    fn visit_var_stmt(&mut self, stmt: VarStmt, ctx: C) -> T;
    fn visit_expr_stmt(&mut self, stmt: ExprStmt, ctx: C) -> T;
    fn visit_block_stmt(&mut self, stmt: BlockStmt, ctx: C) -> T;
}

impl MemAddr for Stmt {}
impl MemAddr for VarStmt {}
impl MemAddr for ExprStmt {}
impl MemAddr for BlockStmt {}
