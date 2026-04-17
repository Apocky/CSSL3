//! Byte-offset `Span` type tying a region of text to its owning `SourceId`.
//!
//! § Spans are the authoritative position-carrier throughout the frontend.
//!   `SourceLocation` (line + column) is derived for rendering only.
//! § Two spans with the same `SourceId` may be `join`ed into a covering span.
//!   Joining across different `SourceId`s is not supported (returns `None`).

use core::fmt;

use crate::source::SourceId;

/// Byte-offset span `[start, end)` within a specific source file.
///
/// Construction via `new` requires `start <= end`. Empty spans (`start == end`) are
/// permitted and used for "point" diagnostics (e.g., "missing semicolon here").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Source file the span refers to.
    pub source: SourceId,
    /// Byte-offset of the first covered byte.
    pub start: u32,
    /// Byte-offset one past the last covered byte.
    pub end: u32,
}

impl Span {
    /// Build a new span, asserting `start <= end`.
    ///
    /// # Panics
    /// Panics if `start > end`. Callers with un-trusted input should validate first.
    #[must_use]
    pub const fn new(source: SourceId, start: u32, end: u32) -> Self {
        assert!(start <= end, "Span::new : start must not exceed end");
        Self { source, start, end }
    }

    /// Zero-length span used as a placeholder for compiler-generated items.
    pub const DUMMY: Self = Self {
        source: SourceId::SYNTHETIC,
        start: 0,
        end: 0,
    };

    /// Width of the span in bytes.
    #[must_use]
    pub const fn len(&self) -> u32 {
        self.end - self.start
    }

    /// `true` iff the span covers zero bytes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// `true` iff both spans refer to the same source file.
    #[must_use]
    pub const fn same_source(&self, other: &Self) -> bool {
        self.source.0 == other.source.0
    }

    /// Smallest span covering both inputs; `None` if sources differ.
    #[must_use]
    pub const fn join(&self, other: &Self) -> Option<Self> {
        if !self.same_source(other) {
            return None;
        }
        let start = if self.start < other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end > other.end {
            self.end
        } else {
            other.end
        };
        Some(Self {
            source: self.source,
            start,
            end,
        })
    }

    /// `true` iff `offset` falls within `[start, end)`.
    #[must_use]
    pub const fn contains_offset(&self, offset: u32) -> bool {
        self.start <= offset && offset < self.end
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}..{}", self.source, self.start, self.end)
    }
}

#[cfg(test)]
mod tests {
    use super::Span;
    use crate::source::SourceId;

    const SRC: SourceId = SourceId(1);

    #[test]
    fn new_sets_fields() {
        let s = Span::new(SRC, 10, 20);
        assert_eq!(s.source, SRC);
        assert_eq!(s.start, 10);
        assert_eq!(s.end, 20);
        assert_eq!(s.len(), 10);
    }

    #[test]
    fn empty_is_detected() {
        let s = Span::new(SRC, 5, 5);
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn dummy_is_synthetic_and_empty() {
        assert!(Span::DUMMY.source.is_synthetic());
        assert!(Span::DUMMY.is_empty());
    }

    #[test]
    #[should_panic(expected = "start must not exceed end")]
    fn new_rejects_inverted() {
        let _ = Span::new(SRC, 10, 5);
    }

    #[test]
    fn join_same_source() {
        let a = Span::new(SRC, 5, 10);
        let b = Span::new(SRC, 20, 25);
        let j = a.join(&b).unwrap();
        assert_eq!(j.start, 5);
        assert_eq!(j.end, 25);
    }

    #[test]
    fn join_overlapping() {
        let a = Span::new(SRC, 5, 15);
        let b = Span::new(SRC, 10, 20);
        let j = a.join(&b).unwrap();
        assert_eq!(j.start, 5);
        assert_eq!(j.end, 20);
    }

    #[test]
    fn join_different_source_returns_none() {
        let a = Span::new(SourceId(1), 0, 5);
        let b = Span::new(SourceId(2), 0, 5);
        assert!(a.join(&b).is_none());
    }

    #[test]
    fn contains_offset_is_half_open() {
        let s = Span::new(SRC, 10, 20);
        assert!(!s.contains_offset(9));
        assert!(s.contains_offset(10));
        assert!(s.contains_offset(19));
        assert!(!s.contains_offset(20));
    }
}
