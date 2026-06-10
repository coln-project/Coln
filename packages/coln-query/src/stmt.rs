// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::{expr::Expr, impl_from_auto_box, util::MemAddr};

pub type Code = Vec<Stmt>;

#[derive(Clone, Debug)]
pub struct Program {
    stmts: Code,
    /// Index from which unexecuted code is stored in the program.
    partition_index: usize,
}

impl From<Code> for Program {
    fn from(stmts: Code) -> Self {
        Self {
            stmts,
            partition_index: 0,
        }
    }
}

impl Program {
    pub fn empty() -> Self {
        Self {
            stmts: Vec::new(),
            partition_index: 0,
        }
    }
    pub fn unexecuted_code(&self) -> impl Iterator<Item = &Stmt> {
        self.stmts.iter().skip(self.partition_index)
    }
    pub fn unexecuted_code_mut(&mut self) -> impl Iterator<Item = &mut Stmt> {
        self.stmts.iter_mut().skip(self.partition_index)
    }
    pub fn extend_program(&mut self, code: impl Iterator<Item = Stmt>) {
        self.partition_index = self.stmts.len();
        self.stmts.extend(code);
    }
}

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
