//! Geolog Language Server (prototype).
//! Mirrors geolog-lang Lexer.hs / Token.hs for syntax highlighting. Geolog is experimental; syntax may change.

mod lexer;

use std::collections::HashMap;
use std::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use url::Url;

use lexer::{lex, TokenKind};

/// Document storage: URI -> full text (we do full sync on did_change for simplicity).
struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, String>>,
}

// Semantic token type indices for our legend (must match token_types in capabilities).
const ST_KEYWORD: u32 = 0;
const ST_TYPE: u32 = 1;
const ST_OPERATOR: u32 = 2;
const ST_COMMENT: u32 = 3;
const ST_VARIABLE: u32 = 4;
const ST_PROPERTY: u32 = 5;
const ST_NUMBER: u32 = 6;

fn token_kind_to_type(kind: TokenKind) -> Option<u32> {
    Some(match kind {
        TokenKind::Theory
        | TokenKind::Def
        | TokenKind::Let
        | TokenKind::Open
        | TokenKind::Import
        | TokenKind::Sig
        | TokenKind::End
        | TokenKind::Tag => ST_KEYWORD,
        TokenKind::Query => ST_TYPE,
        TokenKind::SIdent
        | TokenKind::SKeyword
        | TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::LBrack
        | TokenKind::RBrack
        | TokenKind::LCurly
        | TokenKind::RCurly
        | TokenKind::Comma
        | TokenKind::Semicolon => ST_OPERATOR,
        TokenKind::Comment => ST_COMMENT,
        TokenKind::AIdent | TokenKind::AKeyword => ST_VARIABLE,
        TokenKind::Field => ST_PROPERTY,
        TokenKind::Int => ST_NUMBER,
        TokenKind::Nl => return None, // skip newlines in semantic tokens
    })
}

/// Build line start offsets (byte index of first char of each line).
fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Count UTF-16 code units in the substring source[start_byte..end_byte].
fn utf16_len(source: &str, start_byte: usize, end_byte: usize) -> u32 {
    source[start_byte..end_byte]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum()
}

/// Byte offset -> (line, utf16 character offset in line). LSP uses UTF-16 code units.
fn offset_to_line_utf16(source: &str, line_starts: &[usize], offset: usize) -> (u32, u32) {
    let line = line_starts
        .iter()
        .position(|&s| s > offset)
        .unwrap_or(line_starts.len())
        .saturating_sub(1);
    let line_start = line_starts.get(line).copied().unwrap_or(0);
    let character = utf16_len(source, line_start, offset);
    (line as u32, character)
}

/// LSP (line, character) 0-based -> byte offset for applying incremental edits.
fn line_char_to_offset(source: &str, line: u32, character: u32) -> usize {
    let starts = line_starts(source);
    let line_start = starts.get(line as usize).copied().unwrap_or(source.len());
    let rest = source.get(line_start..).unwrap_or("");
    let line_len = rest.find('\n').unwrap_or(rest.len());
    let line_slice = &rest[..line_len];
    let mut chars_left = character as usize;
    for (i, _) in line_slice.char_indices() {
        if chars_left == 0 {
            return line_start + i;
        }
        chars_left -= 1;
    }
    line_start + line_slice.len()
}

/// Produce LSP semantic tokens (delta-encoded) from lexed tokens. Offsets in UTF-16 code units.
fn tokens_to_semantic_data(source: &str, tokens: &[lexer::Token]) -> Vec<SemanticToken> {
    let line_starts = line_starts(source);
    let mut data = Vec::with_capacity(tokens.len());
    let mut prev_line = 0u32;
    let mut prev_char = 0u32;

    for t in tokens {
        let Some(token_type) = token_kind_to_type(t.kind) else {
            continue;
        };
        let (line, char_start) = offset_to_line_utf16(source, &line_starts, t.range.start);
        let length = utf16_len(source, t.range.start, t.range.end);

        let delta_line = line - prev_line;
        let delta_start = if line == prev_line {
            char_start - prev_char
        } else {
            char_start
        };

        data.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });

        prev_line = line;
        prev_char = char_start;
    }

    data
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        let capabilities = ServerCapabilities {
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    legend: SemanticTokensLegend {
                        token_types: vec![
                            SemanticTokenType::KEYWORD,
                            SemanticTokenType::TYPE,
                            SemanticTokenType::OPERATOR,
                            SemanticTokenType::COMMENT,
                            SemanticTokenType::VARIABLE,
                            SemanticTokenType::PROPERTY,
                            SemanticTokenType::NUMBER,
                        ],
                        token_modifiers: vec![],
                    },
                    range: Some(true),
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                }),
            ),
            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::INCREMENTAL,
            )),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            ..ServerCapabilities::default()
        };

        Ok(InitializeResult {
            capabilities,
            server_info: Some(ServerInfo {
                name: "geolog-lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            ..InitializeResult::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "geolog-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.documents.lock().unwrap().insert(uri, text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let mut docs = self.documents.lock().unwrap();
        let text = docs.get_mut(&uri).map(|s| std::mem::take(s));
        if let Some(mut current) = text {
            for change in params.content_changes {
                match &change.range {
                    None => current = change.text.clone(),
                    Some(r) => {
                        let start = line_char_to_offset(&current, r.start.line, r.start.character);
                        let end = line_char_to_offset(&current, r.end.line, r.end.character);
                        if start <= end && end <= current.len() {
                            current =
                                format!("{}{}{}", &current[..start], &change.text, &current[end..]);
                        }
                    }
                }
            }
            docs.insert(uri, current);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .remove(&params.text_document.uri);
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let text = self
            .documents
            .lock()
            .unwrap()
            .get(&uri)
            .cloned()
            .unwrap_or_default();
        let tokens = lex(&text);
        let data = tokens_to_semantic_data(&text, &tokens);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri;
        let text = self
            .documents
            .lock()
            .unwrap()
            .get(&uri)
            .cloned()
            .unwrap_or_default();
        let tokens = lex(&text);
        // For range we could filter tokens by params.range; for prototype return full.
        let data = tokens_to_semantic_data(&text, &tokens);
        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn hover(&self, _: HoverParams) -> Result<Option<Hover>> {
        Ok(Some(Hover {
            contents: HoverContents::Scalar(
                MarkedString::String("You're hovering!".to_string())
            ),
            range: None
        }))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Mutex::new(HashMap::new()),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
