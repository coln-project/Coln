//! Lexer for Geolog. Mirrors geolog-lang Lexer.hs and Token.hs.
//! Tokenizes source for the LSP (syntax highlighting). Geolog is experimental; syntax may change.

use std::ops::Range;

/// Token kinds aligned with geolog-lang Token.Kind and Lexer specialTable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    // Decl/block keywords (Lexer.hs specialTable)
    Theory,  // theory -> Decl
    Def,     // def -> Decl
    Let,     // let -> Decl
    Open,    // open -> Decl
    Import,  // import -> Decl
    Sig,     // sig -> Block
    End,     // end -> End
    Query,   // Query -> AKeyword (type-like)

    // Identifiers (alphanumeric vs symbolic; keyword vs ident)
    AIdent,   // alphanumerical identifier
    #[allow(dead_code)] // reserved for future alphanumeric keywords
    AKeyword, // alphanumerical keyword (only Query for now)
    SIdent,   // symbolic identifier (e.g. +, *, ->)
    SKeyword, // symbolic keyword (:, :=, =, ->)

    // Special
    Tag,   // 'qname
    Field, // .qname
    Int,   // integer literal

    // Brackets and punctuation
    LParen,
    RParen,
    LBrack,
    RBrack,
    LCurly,
    RCurly,
    Comma,
    Semicolon,
    Nl,

    Comment,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub range: Range<usize>,
}

/// Lex the entire source. Best-effort; never fails.
/// Comments are included (kind == Comment); omit from semantic tokens if desired.
pub fn lex(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut i = 0;
    let bytes = source.as_bytes();

    while i < bytes.len() {
        // Skip spaces and tabs (Haskell: skip)
        if bytes[i] == b' ' || bytes[i] == b'\t' {
            i += 1;
            continue;
        }

        // Newline
        if bytes[i] == b'\n' {
            tokens.push(Token {
                kind: TokenKind::Nl,
                range: i..i + 1,
            });
            i += 1;
            continue;
        }

        // Comment: # to EOL (geolog uses #, not //)
        if bytes[i] == b'#' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Comment,
                range: start..i,
            });
            continue;
        }

        // Single-char punctuation
        let single = match bytes[i] {
            b'(' => TokenKind::LParen,
            b')' => TokenKind::RParen,
            b'[' => TokenKind::LBrack,
            b']' => TokenKind::RBrack,
            b'{' => TokenKind::LCurly,
            b'}' => TokenKind::RCurly,
            b',' => TokenKind::Comma,
            b';' => TokenKind::Semicolon,
            _ => TokenKind::Nl, // sentinel
        };
        if single != TokenKind::Nl {
            tokens.push(Token {
                kind: single,
                range: i..i + 1,
            });
            i += 1;
            continue;
        }

        // Field: .qname
        if bytes[i] == b'.' && i + 1 < bytes.len() && (is_alpha_num_start(bytes[i + 1]) || is_symbol_start(bytes[i + 1])) {
            let start = i;
            i += 1;
            i = eat_qname(source, bytes, &mut i);
            tokens.push(Token {
                kind: TokenKind::Field,
                range: start..i,
            });
            continue;
        }

        // Tag: 'qname
        if bytes[i] == b'\'' && i + 1 < bytes.len() && (is_alpha_num_start(bytes[i + 1]) || is_symbol_start(bytes[i + 1])) {
            let start = i;
            i += 1;
            i = eat_qname(source, bytes, &mut i);
            tokens.push(Token {
                kind: TokenKind::Tag,
                range: start..i,
            });
            continue;
        }

        // Integer literal
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Int,
                range: start..i,
            });
            continue;
        }

        // Identifier or keyword: alphaNum or symbol, then optional / segs (qname)
        if is_alpha_num_start(bytes[i]) || is_symbol_start(bytes[i]) {
            let start = i;
            i = eat_qname(source, bytes, &mut i);
            let slice = &source[start..i];
            let kind = classify_name(slice);
            tokens.push(Token {
                kind,
                range: start..i,
            });
            continue;
        }

        // Unknown: skip one byte (recover)
        i += 1;
    }

    tokens
}

/// Geolog identifier: letters, digits, _, - (Lexer.hs isAlphaNum).
fn is_alpha_num_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_alpha_num_rest(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// Symbol characters: < > - + / * : = (Lexer.hs isSymbol).
fn is_symbol_start(b: u8) -> bool {
    matches!(b, b'<' | b'>' | b'-' | b'+' | b'/' | b'*' | b':' | b'=')
}

fn is_symbol_rest(b: u8) -> bool {
    is_symbol_start(b)
}

/// Consume a qualified name: one segment (alphaNum or symbol), then (/ segment)*.
/// Returns the byte index after the last segment.
fn eat_qname(_source: &str, bytes: &[u8], i: &mut usize) -> usize {
    if *i >= bytes.len() {
        return *i;
    }
    if is_alpha_num_start(bytes[*i]) {
        while *i < bytes.len() && is_alpha_num_rest(bytes[*i]) {
            *i += 1;
        }
    } else if is_symbol_start(bytes[*i]) {
        while *i < bytes.len() && is_symbol_rest(bytes[*i]) {
            *i += 1;
        }
    }
    while *i + 1 < bytes.len() && bytes[*i] == b'/' {
        *i += 1;
        if is_alpha_num_start(bytes[*i]) {
            while *i < bytes.len() && is_alpha_num_rest(bytes[*i]) {
                *i += 1;
            }
        } else if is_symbol_start(bytes[*i]) {
            while *i < bytes.len() && is_symbol_rest(bytes[*i]) {
                *i += 1;
            }
        } else {
            break;
        }
    }
    *i
}

/// Map lexed name to token kind (specialTable + AIdent/SIdent vs AKeyword/SKeyword).
fn classify_name(name: &str) -> TokenKind {
    match name {
        "theory" => TokenKind::Theory,
        "def" => TokenKind::Def,
        "let" => TokenKind::Let,
        "open" => TokenKind::Open,
        "import" => TokenKind::Import,
        "sig" => TokenKind::Sig,
        "end" => TokenKind::End,
        "Query" => TokenKind::Query,
        "=" => TokenKind::SKeyword,
        ":=" => TokenKind::SKeyword,
        ":" => TokenKind::SKeyword,
        "->" => TokenKind::SKeyword,
        _ => {
            if name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/')
            {
                TokenKind::AIdent
            } else {
                TokenKind::SIdent
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_comment_hash() {
        let t = lex("# comment\nx");
        let kinds: Vec<_> = t.iter().map(|x| x.kind).collect();
        assert!(kinds.contains(&TokenKind::Comment));
        assert!(kinds.contains(&TokenKind::AIdent));
    }

    #[test]
    fn lex_theory_and_brackets() {
        let t = lex("theory Graph := [");
        let kinds: Vec<_> = t.iter().map(|x| x.kind).collect();
        assert!(kinds.contains(&TokenKind::Theory));
        assert!(kinds.contains(&TokenKind::SKeyword)); // :=
        assert!(kinds.contains(&TokenKind::LBrack));
    }

    #[test]
    fn lex_field_and_tag() {
        let t = lex(".foo 'bar");
        assert!(t.len() >= 2);
        assert_eq!(t[0].kind, TokenKind::Field);
        assert_eq!(t[1].kind, TokenKind::Tag);
    }

    #[test]
    fn lex_qualified_name() {
        let t = lex("G0.V");
        assert!(t.len() >= 2);
        assert_eq!(t[0].kind, TokenKind::AIdent);
        assert_eq!(t[1].kind, TokenKind::Field);
    }

    #[test]
    fn lex_arrow_and_int() {
        let t = lex("-> 42");
        assert!(t.len() >= 2);
        assert_eq!(t[0].kind, TokenKind::SKeyword);
        assert_eq!(t[1].kind, TokenKind::Int);
    }
}
