// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An s-expression surface syntax for the IR.
//!
//! Processing happens in two stages:
//!
//! 1. [`parse`] reads the source into generic [`SExpr`]s. It knows nothing
//!    about the forms of the language, bounds the nesting depth since the
//!    input is untrusted, and recovers from errors to report several
//!    [`Diagnostic`]s at once.
//! 2. [`Lowerer`] turns [`SExpr`]s into the IR by dispatching on the head
//!    symbol of each form.
//!
//! [`lower()`] runs both stages.
//!
//! Syntax: lists `( ... )`, symbols, integers (`-?[0-9]+`), double-quoted
//! strings with the escapes `\"`, `\\`, `\n`, `\t` and `//` line comments.
//!
//! Every node and diagnostic carries [`Span`]s (byte offsets into the source),
//! which are independent of any diagnostic rendering backend.

mod diagnostic;
mod lexer;
mod lower;
mod parser;
mod span;

pub use diagnostic::{Diagnostic, Label, Severity};
pub use lexer::{Atom, Literal};
pub use lower::{Form, FormParts, Lowerer};
pub use parser::{DEFAULT_MAX_DEPTH, SExpr, SExprKind};
pub use span::Span;

/// The result of [`parse`]. The expressions are returned even if there are
/// diagnostics, with malformed parts replaced by [`SExprKind::Error`].
#[derive(Debug)]
pub struct Parsed {
    pub exprs: Vec<SExpr>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses `src` into a sequence of s-expressions.
///
/// Lists nested deeper than `max_depth` (default: [`DEFAULT_MAX_DEPTH`]) are
/// rejected with a diagnostic and replaced by [`SExprKind::Error`].
pub fn parse(src: &str, max_depth: Option<usize>) -> Parsed {
    let (exprs, diagnostics) =
        parser::Parser::new(src, max_depth.unwrap_or(DEFAULT_MAX_DEPTH)).parse();
    Parsed { exprs, diagnostics }
}

/// The result of [`lower()`]. Only well-formed forms are returned.
#[derive(Debug)]
pub struct Lowered {
    pub forms: Vec<Form>,
    /// The diagnostics of both stages, ordered by their position in the source.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses `src` (see [`parse`]) and lowers each top-level expression as a form.
pub fn lower(src: &str, max_depth: Option<usize>) -> Lowered {
    let Parsed {
        exprs,
        mut diagnostics,
    } = parse(src, max_depth);
    let mut lowerer = Lowerer::new();
    let forms = exprs
        .iter()
        .filter_map(|expr| lowerer.lower_form(expr))
        .collect();
    diagnostics.append(&mut lowerer.diagnostics);
    // Stable, so diagnostics at the same position keep their stage order.
    diagnostics.sort_by_key(|d| d.primary.span.start);
    Lowered { forms, diagnostics }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_merges_diagnostics_of_both_stages() {
        let lowered = lower(r#"(frob 1) (a "\q") ) ()"#, None);
        assert!(lowered.forms.is_empty());
        let reported: Vec<_> = lowered
            .diagnostics
            .iter()
            .map(|d| (d.message.as_str(), d.primary.span))
            .collect();
        assert_eq!(
            reported,
            vec![
                ("unknown form `frob`", Span::new(1, 5)),
                ("unknown form `a`", Span::new(10, 11)),
                ("unknown escape sequence `\\q`", Span::new(13, 15)),
                ("unexpected `)`", Span::new(18, 19)),
                ("empty form", Span::new(20, 22)),
            ]
        );
    }

    #[test]
    fn lower_does_not_report_parse_errors_twice() {
        // The trailing unclosed list would be an "empty form" otherwise.
        let lowered = lower("(\"\\q\" x) 99999999999999999999 (", None);
        let messages: Vec<_> = lowered
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec![
                "unknown escape sequence `\\q`",
                "integer literal out of range",
                "unclosed `(`",
            ]
        );
    }

    #[test]
    fn lower_skips_unclosed_lists() {
        // Without skipping, `frob` would be an unknown form and `1` not a
        // symbol.
        let lowered = lower("(frob (1 a)", None);
        let messages: Vec<_> = lowered
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(messages, vec!["unclosed `(`"]);
    }
}
