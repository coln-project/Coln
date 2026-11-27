// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::ops::Range;

/// A half-open range of byte offsets `[start, end)` into the source text.
///
/// This is deliberately backend-agnostic: it converts to and from
/// [`Range<usize>`], which is what most diagnostic rendering libraries accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "span start {start} > end {end}");
        Self { start, end }
    }

    /// An empty span at `pos`, e.g., to point at the end of the input.
    pub fn empty_at(pos: usize) -> Self {
        Self::new(pos, pos)
    }

    /// The smallest span covering both `self` and `other`.
    pub fn to(self, other: Span) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }

    pub fn len(self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

impl From<Span> for Range<usize> {
    fn from(span: Span) -> Self {
        span.start..span.end
    }
}

impl From<Range<usize>> for Span {
    fn from(range: Range<usize>) -> Self {
        Self::new(range.start, range.end)
    }
}
