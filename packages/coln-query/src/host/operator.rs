// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Operator {
    // Comparison operations.
    /// Eagerly-evaluated binary operation.
    Equal,
    /// Eagerly-evaluated binary operation.
    NotEqual,
    /// Eagerly-evaluated binary operation.
    Less,
    /// Eagerly-evaluated binary operation.
    LessEqual,
    /// Eagerly-evaluated binary operation.
    Greater,
    /// Eagerly-evaluated binary operation.
    GreaterEqual,

    // Logical operations on booleans (and values coerced into booleans).
    /// Lazily-evaluated binary operation.
    And,
    /// Lazily-evaluated binary operation.
    Or,
    /// Eagerly-evaluated unary operation.
    Not,

    // Arithmetic operations on numbers.
    /// Eagerly-evaluated binary operation.
    Addition,
    /// Eagerly-evaluated binary operation or eagerly-evaluated unary operation.
    Subtraction,
    /// Eagerly-evaluated binary operation.
    Multiplication,
    /// Eagerly-evaluated binary operation.
    Division,
}

/// The binding power of a prefix operator ([`Operator::Not`],
/// [`Operator::Subtraction`] applied to one operand). Above every binary
/// operator, so `-a + b` needs no parentheses.
pub const UNARY_PRECEDENCE: u8 = 7;

/// The binding power of a postfix form (a call, an index) and of any atom that
/// can never need parentheses.
pub const PRIMARY_PRECEDENCE: u8 = 8;

impl Operator {
    /// This operator's binding power as a *binary* operator: higher binds
    /// tighter. Only a printer needs it — evaluation order is already fixed by
    /// the tree's shape — but a tree built by hand (or by a lowering) carries no
    /// [`GroupingExpr`](super::expr::GroupingExpr), so rendering it back to
    /// readable text has to re-derive where parentheses belong.
    pub fn precedence(self) -> u8 {
        match self {
            Operator::Or => 1,
            Operator::And => 2,
            Operator::Equal | Operator::NotEqual => 3,
            Operator::Less | Operator::LessEqual | Operator::Greater | Operator::GreaterEqual => 4,
            Operator::Addition | Operator::Subtraction => 5,
            Operator::Multiplication | Operator::Division => 6,
            Operator::Not => UNARY_PRECEDENCE,
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            Operator::Equal => "==",
            Operator::NotEqual => "!=",
            Operator::Less => "<",
            Operator::LessEqual => "<=",
            Operator::Greater => ">",
            Operator::GreaterEqual => ">=",
            Operator::And => "&&",
            Operator::Or => "||",
            Operator::Not => "!",
            Operator::Addition => "+",
            Operator::Subtraction => "-",
            Operator::Multiplication => "*",
            Operator::Division => "/",
        }
    }
}

impl std::fmt::Display for Operator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.symbol())
    }
}
