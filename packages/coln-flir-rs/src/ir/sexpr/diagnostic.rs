// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A span in the source with an optional message attached to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub message: Option<String>,
}

/// A diagnostic as plain data. Rendering is left to a backend.
///
/// The [`primary`](Self::primary) label points at the cause of the problem,
/// the [`secondary`](Self::secondary) labels at related locations (e.g., where
/// an unclosed parenthesis was opened).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The summary line of the diagnostic.
    pub message: String,
    pub primary: Label,
    pub secondary: Vec<Label>,
    /// Free-text notes or help.
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            primary: Label {
                span,
                message: None,
            },
            secondary: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn with_primary_message(mut self, message: impl Into<String>) -> Self {
        self.primary.message = Some(message.into());
        self
    }

    pub fn with_secondary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.secondary.push(Label {
            span,
            message: Some(message.into()),
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}
