// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lowering of [`SExpr`]s into the IR.
//!
//! Forms are dispatched on their head symbol in [`Lowerer::lower_form`]. To
//! add a built-in form or operation, add one match arm there together with a
//! `lower_*` function, using the span-aware `expect_*` helpers below to report
//! malformed input.
//!
//! All helpers return `None` after reporting a diagnostic. They also return
//! `None`, but silently, on [`SExprKind::Error`] nodes, as those were reported
//! during parsing already.

use super::diagnostic::Diagnostic;
use super::lexer::{Atom, Literal};
use super::parser::{SExpr, SExprKind};
use super::span::Span;
use crate::ir::Lit;

/// The result of lowering a top-level form.
///
/// Placeholder until the surface syntax of the forms (`rule`, `table`, ...)
/// is settled; each form will contribute a variant.
#[derive(Debug)]
pub enum Form {}

/// A form `(head args...)` split into its parts.
#[derive(Debug, Clone, Copy)]
pub struct FormParts<'a> {
    pub head: &'a str,
    pub head_span: Span,
    pub args: &'a [SExpr],
    /// The span of the whole form.
    pub span: Span,
}

#[derive(Debug, Default)]
pub struct Lowerer {
    pub diagnostics: Vec<Diagnostic>,
}

impl Lowerer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lower_form(&mut self, expr: &SExpr) -> Option<Form> {
        let form = self.split_form(expr)?;
        #[allow(clippy::match_single_binding, reason = "forms are added as arms")]
        match form.head {
            // Built-in forms and operations go here, e.g.,
            // "rule" => self.lower_rule(form),
            _ => self.unknown_form(form),
        }
    }

    pub fn lower_literal(&mut self, expr: &SExpr) -> Option<Lit> {
        let literal = match &expr.kind {
            SExprKind::Atom(Atom::Literal(literal)) => literal,
            SExprKind::Error => return None,
            SExprKind::Atom(Atom::Symbol(_)) | SExprKind::List(_) => {
                return self.error(Diagnostic::error("expected a literal", expr.span));
            }
        };
        match literal {
            Literal::Int(value) => match i32::try_from(*value) {
                Ok(value) => Some(Lit::Int { value }),
                Err(_) => self.error(
                    Diagnostic::error("integer literal out of range", expr.span).with_note(
                        format!("integers must lie within [{}, {}]", i32::MIN, i32::MAX),
                    ),
                ),
            },
            Literal::Str(value) => Some(Lit::String {
                value: value.clone(),
            }),
        }
    }

    /// Splits a non-empty list with a symbol at its head into a [`FormParts`].
    pub fn split_form<'a>(&mut self, expr: &'a SExpr) -> Option<FormParts<'a>> {
        let items = self.expect_list(expr)?;
        let Some((head, args)) = items.split_first() else {
            return self.error(
                Diagnostic::error("empty form", expr.span)
                    .with_primary_message("expected `(head args...)`"),
            );
        };
        let head_name = self.expect_symbol(head)?;
        Some(FormParts {
            head: head_name,
            head_span: head.span,
            args,
            span: expr.span,
        })
    }

    pub fn expect_list<'a>(&mut self, expr: &'a SExpr) -> Option<&'a [SExpr]> {
        match &expr.kind {
            SExprKind::List(items) => Some(items),
            SExprKind::Error => None,
            _ => self.error(Diagnostic::error("expected a list", expr.span)),
        }
    }

    pub fn expect_symbol<'a>(&mut self, expr: &'a SExpr) -> Option<&'a str> {
        match &expr.kind {
            SExprKind::Atom(Atom::Symbol(name)) => Some(name),
            SExprKind::Error => None,
            _ => self.error(Diagnostic::error("expected a symbol", expr.span)),
        }
    }

    /// Checks that `form` has exactly `arity` arguments.
    pub fn expect_arity(&mut self, form: FormParts<'_>, arity: usize) -> Option<()> {
        let got = form.args.len();
        let message = format!(
            "`{}` takes {arity} argument{}, but {got} {} given",
            form.head,
            if arity == 1 { "" } else { "s" },
            if got == 1 { "was" } else { "were" },
        );
        if got > arity {
            let extra = form.args[arity].span.to(form.args[got - 1].span);
            self.error(
                Diagnostic::error(message, extra)
                    .with_primary_message("unexpected argument(s)")
                    .with_secondary(form.head_span, "form starts here"),
            )
        } else if got < arity {
            self.error(
                Diagnostic::error(message, form.span)
                    .with_primary_message(format!("missing {} argument(s)", arity - got)),
            )
        } else {
            Some(())
        }
    }

    fn unknown_form<T>(&mut self, form: FormParts<'_>) -> Option<T> {
        self.error(
            Diagnostic::error(format!("unknown form `{}`", form.head), form.head_span)
                .with_secondary(form.span, "in this form"),
        )
    }

    fn error<T>(&mut self, diagnostic: Diagnostic) -> Option<T> {
        self.diagnostics.push(diagnostic);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    fn one(src: &str) -> SExpr {
        let mut parsed = parse(src, None);
        assert_eq!(parsed.exprs.len(), 1);
        parsed.exprs.pop().unwrap()
    }

    #[test]
    fn lower_lits() {
        let mut lowerer = Lowerer::new();
        assert_eq!(
            lowerer.lower_literal(&one("-5")),
            Some(Lit::Int { value: -5 })
        );
        assert_eq!(
            lowerer.lower_literal(&one("\"s\"")),
            Some(Lit::String {
                value: "s".to_owned()
            })
        );
        assert_eq!(lowerer.diagnostics, vec![]);
    }

    #[test]
    fn lit_out_of_i32_range() {
        let mut lowerer = Lowerer::new();
        assert_eq!(lowerer.lower_literal(&one("2147483648")), None);
        assert_eq!(lowerer.diagnostics.len(), 1);
        assert_eq!(lowerer.diagnostics[0].primary.span, Span::new(0, 10));
    }

    #[test]
    fn errors_are_not_reported_twice() {
        let mut lowerer = Lowerer::new();
        assert_eq!(lowerer.lower_literal(&one("\"\\q\"")), None);
        assert!(lowerer.lower_form(&one("(\"\\q\" a)")).is_none());
        assert_eq!(lowerer.diagnostics, vec![]);
    }

    #[test]
    fn unknown_form_points_at_head() {
        let mut lowerer = Lowerer::new();
        assert!(lowerer.lower_form(&one("( frob 1 2)")).is_none());
        let d = &lowerer.diagnostics[0];
        assert_eq!(d.message, "unknown form `frob`");
        assert_eq!(d.primary.span, Span::new(2, 6));
        assert_eq!(d.secondary[0].span, Span::new(0, 11));
    }

    #[test]
    fn malformed_forms() {
        let mut lowerer = Lowerer::new();
        assert!(lowerer.lower_form(&one("()")).is_none());
        assert!(lowerer.lower_form(&one("(1 a)")).is_none());
        assert!(lowerer.lower_form(&one("a")).is_none());
        let messages: Vec<_> = lowerer
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec!["empty form", "expected a symbol", "expected a list"]
        );
    }

    #[test]
    fn arity() {
        let mut lowerer = Lowerer::new();
        let expr = one("(f a b c)");
        let form = lowerer.split_form(&expr).unwrap();
        assert_eq!(lowerer.expect_arity(form, 3), Some(()));
        assert_eq!(lowerer.expect_arity(form, 1), None);
        assert_eq!(lowerer.expect_arity(form, 4), None);
        let d = &lowerer.diagnostics;
        assert_eq!(d[0].message, "`f` takes 1 argument, but 3 were given");
        assert_eq!(d[0].primary.span, Span::new(5, 8));
        assert_eq!(d[0].secondary[0].span, Span::new(1, 2));
        assert_eq!(d[1].message, "`f` takes 4 arguments, but 3 were given");
        assert_eq!(d[1].primary.span, Span::new(0, 9));
    }
}
