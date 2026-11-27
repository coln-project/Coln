// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::diagnostic::Diagnostic;
use super::span::Span;

/// The atoms of the language, i.e., the leaves of an s-expression. They are
/// shared by tokens and s-expressions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atom {
    /// A name whose meaning is determined during lowering.
    Symbol(String),
    Literal(Literal),
}

/// A self-denoting value. A new kind of literal only needs to be added here,
/// in the lexer and in [`Lowerer::lower_literal`](super::Lowerer::lower_literal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    Int(i64),
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TokenKind {
    LParen,
    RParen,
    Atom(Atom),
    /// A malformed token. A diagnostic has already been emitted for it.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

const COMMENT_START: &str = "//";

/// A streaming lexer. It never aborts: malformed input yields
/// [`TokenKind::Error`] tokens and pushes diagnostics into the sink passed to
/// [`next_token`](Self::next_token).
pub(super) struct Lexer<'src> {
    src: &'src str,
    pos: usize,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Self { src, pos: 0 }
    }

    /// The byte length of the input, i.e., the offset of the end of input.
    pub fn end(&self) -> usize {
        self.src.len()
    }

    pub fn next_token(&mut self, diagnostics: &mut Vec<Diagnostic>) -> Option<Token> {
        self.skip_trivia();
        let start = self.pos;
        let c = self.rest().chars().next()?;
        let kind = match c {
            '(' => {
                self.pos += 1;
                TokenKind::LParen
            }
            ')' => {
                self.pos += 1;
                TokenKind::RParen
            }
            '"' => self.lex_string(diagnostics),
            _ => self.lex_atom(diagnostics),
        };
        Some(Token {
            kind,
            span: Span::new(start, self.pos),
        })
    }

    fn rest(&self) -> &'src str {
        &self.src[self.pos..]
    }

    /// Skips whitespace and `//` line comments.
    fn skip_trivia(&mut self) {
        loop {
            let rest = self.rest();
            let trimmed = rest.trim_start();
            self.pos += rest.len() - trimmed.len();
            if !trimmed.starts_with(COMMENT_START) {
                return;
            }
            self.pos += trimmed.find('\n').unwrap_or(trimmed.len());
        }
    }

    /// Lexes a symbol or an integer: a maximal run of characters that are not
    /// whitespace, `(`, `)` or `"`, stopping in front of a `//` comment.
    fn lex_atom(&mut self, diagnostics: &mut Vec<Diagnostic>) -> TokenKind {
        let start = self.pos;
        let rest = self.rest();
        let len = rest
            .char_indices()
            .find(|&(i, c)| {
                c.is_whitespace()
                    || matches!(c, '(' | ')' | '"')
                    || rest[i..].starts_with(COMMENT_START)
            })
            .map_or(rest.len(), |(i, _)| i);
        self.pos += len;
        let text = &rest[..len];

        if !is_int_literal(text) {
            return TokenKind::Atom(Atom::Symbol(text.to_owned()));
        }
        match text.parse::<i64>() {
            Ok(value) => TokenKind::Atom(Atom::Literal(Literal::Int(value))),
            Err(_) => {
                diagnostics.push(
                    Diagnostic::error("integer literal out of range", Span::new(start, self.pos))
                        .with_note(format!(
                            "integers must lie within [{}, {}]",
                            i64::MIN,
                            i64::MAX
                        )),
                );
                TokenKind::Error
            }
        }
    }

    /// Lexes a double-quoted string with the escapes `\"`, `\\`, `\n`, `\t`.
    fn lex_string(&mut self, diagnostics: &mut Vec<Diagnostic>) -> TokenKind {
        let open = Span::new(self.pos, self.pos + 1);
        self.pos += 1;
        let mut value = String::new();
        let mut malformed = false;
        let mut chars = self.rest().char_indices();
        let base = self.pos;
        while let Some((i, c)) = chars.next() {
            match c {
                '"' => {
                    self.pos = base + i + 1;
                    return if malformed {
                        TokenKind::Error
                    } else {
                        TokenKind::Atom(Atom::Literal(Literal::Str(value)))
                    };
                }
                '\\' => match chars.next() {
                    Some((_, '"')) => value.push('"'),
                    Some((_, '\\')) => value.push('\\'),
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 't')) => value.push('\t'),
                    Some((j, other)) => {
                        malformed = true;
                        let span = Span::new(base + i, base + j + other.len_utf8());
                        diagnostics.push(
                            Diagnostic::error(format!("unknown escape sequence `\\{other}`"), span)
                                .with_note(r#"supported escapes are `\"`, `\\`, `\n` and `\t`"#),
                        );
                    }
                    // Unterminated; reported below.
                    None => break,
                },
                _ => value.push(c),
            }
        }
        self.pos = self.src.len();
        diagnostics.push(
            Diagnostic::error("unterminated string literal", Span::empty_at(self.pos))
                .with_primary_message("expected `\"`")
                .with_secondary(open, "string starts here"),
        );
        TokenKind::Error
    }
}

/// Whether `text` matches `-?[0-9]+`.
fn is_int_literal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Range;

    fn lex(src: &str) -> (Vec<Token>, Vec<Diagnostic>) {
        let mut lexer = Lexer::new(src);
        let mut diagnostics = Vec::new();
        let tokens = std::iter::from_fn(|| lexer.next_token(&mut diagnostics)).collect();
        (tokens, diagnostics)
    }

    fn kinds(src: &str) -> Vec<TokenKind> {
        let (tokens, diagnostics) = lex(src);
        assert_eq!(diagnostics, vec![], "unexpected diagnostics for {src:?}");
        tokens.into_iter().map(|t| t.kind).collect()
    }

    fn sym(s: &str) -> TokenKind {
        TokenKind::Atom(Atom::Symbol(s.to_owned()))
    }

    fn int(i: i64) -> TokenKind {
        TokenKind::Atom(Atom::Literal(Literal::Int(i)))
    }

    fn str(s: &str) -> TokenKind {
        TokenKind::Atom(Atom::Literal(Literal::Str(s.to_owned())))
    }

    #[test]
    fn atoms_and_parens() {
        use TokenKind::*;
        assert_eq!(
            kinds("(foo 42 -7 - +1 \"hi\")"),
            vec![
                LParen,
                sym("foo"),
                int(42),
                int(-7),
                sym("-"),
                sym("+1"),
                str("hi"),
                RParen
            ]
        );
    }

    #[test]
    fn spans() {
        let (tokens, _) = lex("  (ab \"c\")");
        let spans: Vec<_> = tokens.iter().map(|t| Range::from(t.span)).collect();
        assert_eq!(spans, vec![2..3, 3..5, 6..9, 9..10]);
    }

    #[test]
    fn comments() {
        assert_eq!(kinds("// only a comment"), vec![]);
        assert_eq!(kinds("a // c1\n// c2\nb"), vec![sym("a"), sym("b")]);
        assert_eq!(kinds("foo// note"), vec![sym("foo")]);
        assert_eq!(kinds("/ a/b"), vec![sym("/"), sym("a/b")]);
    }

    #[test]
    fn string_escapes() {
        assert_eq!(kinds(r#""a\"b\\c\nd\te""#), vec![str("a\"b\\c\nd\te")]);
        // A comment marker inside a string is not a comment.
        assert_eq!(kinds(r#""// x""#), vec![str("// x")]);
    }

    #[test]
    fn unknown_escape() {
        let (tokens, diagnostics) = lex(r#""a\qb" c"#);
        assert_eq!(tokens[0].kind, TokenKind::Error);
        assert_eq!(tokens[0].span, Span::new(0, 6));
        assert_eq!(tokens[1].kind, sym("c"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].primary.span, Span::new(2, 4));
    }

    #[test]
    fn unterminated_string() {
        let (tokens, diagnostics) = lex("x \"abc");
        assert_eq!(tokens[1].kind, TokenKind::Error);
        assert_eq!(diagnostics.len(), 1);
        let d = &diagnostics[0];
        assert_eq!(d.message, "unterminated string literal");
        assert_eq!(d.primary.span, Span::empty_at(6));
        assert_eq!(d.secondary[0].span, Span::new(2, 3));
    }

    #[test]
    fn int_overflow() {
        let (tokens, diagnostics) = lex("99999999999999999999 ok");
        assert_eq!(tokens[0].kind, TokenKind::Error);
        assert_eq!(tokens[1].kind, sym("ok"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].primary.span, Span::new(0, 20));
    }
}
