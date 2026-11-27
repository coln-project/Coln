// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::diagnostic::Diagnostic;
use super::lexer::{Atom, Lexer, Token, TokenKind};
use super::span::Span;

/// The default maximum nesting depth of lists accepted by the parser.
pub const DEFAULT_MAX_DEPTH: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SExpr {
    pub kind: SExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SExprKind {
    Atom(Atom),
    List(Vec<SExpr>),
    /// A list the parser closed itself at the end of input. Its children are
    /// kept (e.g., for tooling), but a diagnostic has already been emitted, so
    /// later stages should skip it silently like an [`Error`](Self::Error).
    UnclosedList(Vec<SExpr>),
    /// A malformed expression. A diagnostic has already been emitted for it,
    /// so later stages should skip it silently.
    Error,
}

/// A recursive descent parser with a bound on the nesting depth of lists.
///
/// The bound keeps the recursion here, in every later recursive pass over the
/// tree and in the (recursive) drop of the tree from overflowing the stack.
pub(super) struct Parser<'src> {
    lexer: Lexer<'src>,
    diagnostics: Vec<Diagnostic>,
    depth: usize,
    max_depth: usize,
    /// Whether the end of input inside an unclosed list was reported already.
    /// Only the innermost unclosed list is reported to avoid a cascade.
    unclosed_reported: bool,
}

impl<'src> Parser<'src> {
    pub fn new(src: &'src str, max_depth: usize) -> Self {
        Self {
            lexer: Lexer::new(src),
            diagnostics: Vec::new(),
            depth: 0,
            max_depth,
            unclosed_reported: false,
        }
    }

    pub fn parse(mut self) -> (Vec<SExpr>, Vec<Diagnostic>) {
        let mut exprs = Vec::new();
        while let Some(token) = self.next_token() {
            if token.kind == TokenKind::RParen {
                self.diagnostics.push(
                    Diagnostic::error("unexpected `)`", token.span)
                        .with_primary_message("no matching `(`"),
                );
                continue;
            }
            exprs.push(self.parse_expr(token));
        }
        (exprs, self.diagnostics)
    }

    fn next_token(&mut self) -> Option<Token> {
        self.lexer.next_token(&mut self.diagnostics)
    }

    /// Parses the expression starting with `token`, which must not be a `)`.
    fn parse_expr(&mut self, token: Token) -> SExpr {
        let kind = match token.kind {
            TokenKind::LParen => return self.parse_list(token.span),
            TokenKind::RParen => unreachable!("callers handle `)`"),
            TokenKind::Atom(atom) => SExprKind::Atom(atom),
            TokenKind::Error => SExprKind::Error,
        };
        SExpr {
            kind,
            span: token.span,
        }
    }

    /// Parses the rest of a list whose `(` spans `open`.
    fn parse_list(&mut self, open: Span) -> SExpr {
        if self.depth == self.max_depth {
            return self.skip_list(open);
        }
        self.depth += 1;
        let mut children = Vec::new();
        let (close, closed) = loop {
            match self.next_token() {
                Some(Token {
                    kind: TokenKind::RParen,
                    span,
                }) => break (span, true),
                Some(token) => children.push(self.parse_expr(token)),
                None => break (self.report_unclosed(open), false),
            }
        };
        self.depth -= 1;
        SExpr {
            kind: if closed {
                SExprKind::List(children)
            } else {
                SExprKind::UnclosedList(children)
            },
            span: open.to(close),
        }
    }

    /// Reports a list nested too deeply and skips it without recursing.
    fn skip_list(&mut self, open: Span) -> SExpr {
        self.diagnostics.push(
            Diagnostic::error(
                format!("nesting exceeds the maximum depth of {}", self.max_depth),
                open,
            )
            .with_primary_message("this list is nested too deeply"),
        );
        let mut open_lists = 1usize;
        let close = loop {
            match self.next_token() {
                Some(Token {
                    kind: TokenKind::LParen,
                    ..
                }) => open_lists += 1,
                Some(Token {
                    kind: TokenKind::RParen,
                    span,
                }) => {
                    open_lists -= 1;
                    if open_lists == 0 {
                        break span;
                    }
                }
                Some(_) => {}
                // Let an enclosing list report the missing `)`, if any;
                // otherwise report it here.
                None if self.depth > 0 => break Span::empty_at(self.lexer.end()),
                None => break self.report_unclosed(open),
            }
        };
        SExpr {
            kind: SExprKind::Error,
            span: open.to(close),
        }
    }

    /// Reports that the end of input was reached inside the list opened at
    /// `open` and returns the (empty) span at the end of input.
    fn report_unclosed(&mut self, open: Span) -> Span {
        let eof = Span::empty_at(self.lexer.end());
        if !self.unclosed_reported {
            self.unclosed_reported = true;
            self.diagnostics.push(
                Diagnostic::error("unclosed `(`", eof)
                    .with_primary_message("expected `)`")
                    .with_secondary(open, "list opened here"),
            );
        }
        eof
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;
    use crate::ir::sexpr::Literal;

    fn parse_ok(src: &str) -> Vec<SExpr> {
        let parsed = parse(src, None);
        assert_eq!(parsed.diagnostics, vec![], "unexpected diagnostics");
        parsed.exprs
    }

    /// Renders the tree back for compact assertions.
    fn show(expr: &SExpr) -> String {
        match &expr.kind {
            SExprKind::Atom(Atom::Symbol(s)) => s.clone(),
            SExprKind::Atom(Atom::Literal(Literal::Int(i))) => i.to_string(),
            SExprKind::Atom(Atom::Literal(Literal::Str(s))) => format!("{s:?}"),
            SExprKind::Error => "<error>".to_owned(),
            SExprKind::List(items) => format!("({})", show_items(items)),
            // Rendered without the `)` the source lacks.
            SExprKind::UnclosedList(items) => format!("({}", show_items(items)),
        }
    }

    fn show_items(items: &[SExpr]) -> String {
        items.iter().map(show).collect::<Vec<_>>().join(" ")
    }

    fn show_all(exprs: &[SExpr]) -> String {
        show_items(exprs)
    }

    #[test]
    fn nested_lists() {
        let exprs = parse_ok("(rule (a 1) // comment\n (b \"x\")) () top");
        assert_eq!(show_all(&exprs), r#"(rule (a 1) (b "x")) () top"#);
    }

    #[test]
    fn spans() {
        let exprs = parse_ok(" (a (b c)) d");
        assert_eq!(exprs[0].span, Span::new(1, 10));
        let SExprKind::List(items) = &exprs[0].kind else {
            panic!("expected a list")
        };
        assert_eq!(items[0].span, Span::new(2, 3));
        assert_eq!(items[1].span, Span::new(4, 9));
        assert_eq!(exprs[1].span, Span::new(11, 12));
    }

    #[test]
    fn unclosed_list() {
        let parsed = parse("(a (b c)", None);
        assert_eq!(show_all(&parsed.exprs), "(a (b c)");
        assert_eq!(parsed.exprs[0].span, Span::new(0, 8));
        assert_eq!(parsed.diagnostics.len(), 1);
        let d = &parsed.diagnostics[0];
        assert_eq!(d.message, "unclosed `(`");
        assert_eq!(d.primary.span, Span::empty_at(8));
        assert_eq!(d.secondary[0].span, Span::new(0, 1));
    }

    #[test]
    fn only_innermost_unclosed_list_is_reported() {
        let parsed = parse("(a (b (c", None);
        // All three lists are unclosed, but only the innermost is reported.
        assert_eq!(show_all(&parsed.exprs), "(a (b (c");
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].secondary[0].span, Span::new(6, 7));
    }

    #[test]
    fn stray_close_paren() {
        let parsed = parse("a ) (b))", None);
        assert_eq!(show_all(&parsed.exprs), "a (b)");
        let spans: Vec<_> = parsed.diagnostics.iter().map(|d| d.primary.span).collect();
        assert_eq!(spans, vec![Span::new(2, 3), Span::new(7, 8)]);
    }

    #[test]
    fn collects_several_errors() {
        let parsed = parse("(a \"\\q\") ) (99999999999999999999 \"open", None);
        assert_eq!(show_all(&parsed.exprs), "(a <error>) (<error> <error>");
        let messages: Vec<_> = parsed
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec![
                "unknown escape sequence `\\q`",
                "unexpected `)`",
                "integer literal out of range",
                "unterminated string literal",
                "unclosed `(`",
            ]
        );
    }

    fn nested(depth: usize) -> String {
        format!("{}x{}", "(".repeat(depth), ")".repeat(depth))
    }

    #[test]
    fn max_depth_is_accepted() {
        parse_ok(&nested(DEFAULT_MAX_DEPTH));
        let parsed = parse(&nested(3), Some(3));
        assert_eq!(parsed.diagnostics, vec![]);
    }

    #[test]
    fn too_deep_is_rejected() {
        let parsed = parse(&format!("{} after", nested(4)), Some(3));
        assert_eq!(show_all(&parsed.exprs), "(((<error>))) after");
        assert_eq!(parsed.diagnostics.len(), 1);
        let d = &parsed.diagnostics[0];
        assert_eq!(d.message, "nesting exceeds the maximum depth of 3");
        assert_eq!(d.primary.span, Span::new(3, 4));
    }

    #[test]
    fn huge_nesting_does_not_overflow() {
        let parsed = parse(&nested(1_000_000), None);
        assert_eq!(parsed.diagnostics.len(), 1);

        let parsed = parse(&"(".repeat(1_000_000), None);
        let messages: Vec<_> = parsed
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec!["nesting exceeds the maximum depth of 128", "unclosed `(`"]
        );
    }

    #[test]
    fn too_deep_and_unclosed_at_top_level() {
        let parsed = parse("((", Some(1));
        let messages: Vec<_> = parsed
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec!["nesting exceeds the maximum depth of 1", "unclosed `(`"]
        );

        let parsed = parse("(", Some(0));
        let messages: Vec<_> = parsed
            .diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            messages,
            vec!["nesting exceeds the maximum depth of 0", "unclosed `(`"]
        );
    }
}
