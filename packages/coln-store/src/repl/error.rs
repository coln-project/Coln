// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::fmt;

/// Parse failure for a single cell inside a `begin batch` block (before column index is known).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchCellParseError {
    UnknownBinding(String),
    InvalidValue(String),
}

impl fmt::Display for BatchCellParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BatchCellParseError::UnknownBinding(name) => write!(f, "unknown binding: {name}"),
            BatchCellParseError::InvalidValue(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for BatchCellParseError {}
