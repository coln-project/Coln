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
