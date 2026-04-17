//! Source file representation + location tracking.
//!
//! § `SourceFile` owns a single compilation unit's text; the compiler maintains a map
//!   from `SourceId` → `SourceFile` in a `SourceMap` (added when multi-file compilation
//!   lands at T3).
//! § `SourceLocation` is a human-facing (line, column) pair used only for diagnostic
//!   rendering — the authoritative position is always the byte-offset in `Span`.
//! § SPEC : `specs/09_SYNTAX.csl` § FILE-LAYOUT + `specs/16_DUAL_SURFACE.csl` § MODE-DETECTION.

use core::fmt;
use core::num::NonZeroU32;

/// Unique identifier for a source file within a compilation unit.
///
/// `SourceId(0)` is reserved for the synthetic "no-source" sentinel (used for compiler-
/// generated items with no originating text). Real files start at `SourceId(1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Ord, PartialOrd)]
pub struct SourceId(pub u32);

impl SourceId {
    /// The synthetic "no source" marker — compiler-generated items use this.
    pub const SYNTHETIC: Self = Self(0);

    /// Create the first real source id (1).
    #[must_use]
    pub const fn first() -> Self {
        Self(1)
    }

    /// Return the next-higher source id.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// `true` iff this is the synthetic marker.
    #[must_use]
    pub const fn is_synthetic(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_synthetic() {
            f.write_str("<synthetic>")
        } else {
            write!(f, "src#{}", self.0)
        }
    }
}

/// Which surface grammar the file is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Surface {
    /// Rust-hybrid surface — logos + chumsky-based lexer/parser (default).
    #[default]
    RustHybrid,
    /// CSLv3-native surface — hand-rolled Rust port of `CSLv3/specs/12_TOKENIZER` + `13_GRAMMAR_SELF`.
    CslNative,
    /// Surface not yet determined; callers should invoke mode-detection.
    Auto,
}

impl Surface {
    /// Human-readable surface label for diagnostics.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::RustHybrid => "rust-hybrid",
            Self::CslNative => "csl-native",
            Self::Auto => "auto",
        }
    }
}

impl fmt::Display for Surface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A source file's text + metadata.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Assigned by the source-map owner on registration.
    pub id: SourceId,
    /// Filesystem path or synthetic label (e.g., `"<test-input>"`).
    pub path: String,
    /// Full UTF-8 content. Byte offsets in `Span` index into this.
    pub contents: String,
    /// Which surface grammar this file uses.
    pub surface: Surface,
    /// Precomputed byte-offsets of line starts (including 0 for line 1).
    /// Used by `position_of` to map byte-offset → `SourceLocation` in O(log n).
    line_offsets: Vec<u32>,
}

impl SourceFile {
    /// Build a new `SourceFile` and precompute line-offset index.
    #[must_use]
    pub fn new(
        id: SourceId,
        path: impl Into<String>,
        contents: impl Into<String>,
        surface: Surface,
    ) -> Self {
        let contents = contents.into();
        let line_offsets = Self::compute_line_offsets(&contents);
        Self {
            id,
            path: path.into(),
            contents,
            surface,
            line_offsets,
        }
    }

    fn compute_line_offsets(text: &str) -> Vec<u32> {
        let mut offsets = Vec::with_capacity(text.len() / 40 + 1);
        offsets.push(0);
        for (idx, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                let line_start = u32::try_from(idx + 1).unwrap_or(u32::MAX);
                offsets.push(line_start);
            }
        }
        offsets
    }

    /// Total byte length of the source.
    #[must_use]
    pub fn len_bytes(&self) -> u32 {
        u32::try_from(self.contents.len()).unwrap_or(u32::MAX)
    }

    /// Slice the source by byte-offsets `[start, end)`.
    ///
    /// Returns `None` if the span lies outside the source, or if the endpoints are
    /// not on a UTF-8 char boundary.
    #[must_use]
    pub fn slice(&self, start: u32, end: u32) -> Option<&str> {
        let start = start as usize;
        let end = end as usize;
        if end < start || end > self.contents.len() {
            return None;
        }
        if !self.contents.is_char_boundary(start) || !self.contents.is_char_boundary(end) {
            return None;
        }
        Some(&self.contents[start..end])
    }

    /// Map a byte-offset to a `SourceLocation` (line + column, both 1-indexed).
    ///
    /// Returns the position of the last byte (EOF marker) for out-of-range offsets.
    /// Column is counted in UTF-8 *code units*, not grapheme clusters — sufficient for
    /// diagnostic arrow alignment in monospace renderers.
    #[must_use]
    pub fn position_of(&self, offset: u32) -> SourceLocation {
        let offsets = &self.line_offsets;
        // binary search for the largest line-offset ≤ `offset`
        let line_idx = match offsets.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = offsets.get(line_idx).copied().unwrap_or(0);
        let column = offset.saturating_sub(line_start) + 1;
        let line = NonZeroU32::new(u32::try_from(line_idx + 1).unwrap_or(1))
            .unwrap_or(NonZeroU32::new(1).expect("1 is non-zero"));
        let col = NonZeroU32::new(column).unwrap_or(NonZeroU32::new(1).expect("1 is non-zero"));
        SourceLocation { line, column: col }
    }
}

/// Human-facing (line, column) pair, both 1-indexed.
///
/// Used only for diagnostic rendering. Authoritative offsets live in `Span`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    /// Line number, 1-indexed.
    pub line: NonZeroU32,
    /// Column (code-unit count), 1-indexed.
    pub column: NonZeroU32,
}

impl SourceLocation {
    /// Build a location from raw `(line, column)`; returns `None` if either is zero.
    #[must_use]
    pub const fn new(line: u32, column: u32) -> Option<Self> {
        match (NonZeroU32::new(line), NonZeroU32::new(column)) {
            (Some(l), Some(c)) => Some(Self { line: l, column: c }),
            _ => None,
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceFile, SourceId, SourceLocation, Surface};

    fn mk(contents: &str) -> SourceFile {
        SourceFile::new(SourceId::first(), "<test>", contents, Surface::RustHybrid)
    }

    #[test]
    fn source_id_synthetic_sentinel() {
        assert!(SourceId::SYNTHETIC.is_synthetic());
        assert!(!SourceId::first().is_synthetic());
        assert_eq!(SourceId::first().next(), SourceId(2));
    }

    #[test]
    fn surface_default_is_rust_hybrid() {
        assert_eq!(Surface::default(), Surface::RustHybrid);
    }

    #[test]
    fn surface_labels_unique() {
        let labels = [Surface::RustHybrid, Surface::CslNative, Surface::Auto].map(Surface::label);
        let mut sorted = labels;
        sorted.sort_unstable();
        assert_eq!(sorted, ["auto", "csl-native", "rust-hybrid"]);
    }

    #[test]
    fn slice_returns_substring() {
        let f = mk("hello world");
        assert_eq!(f.slice(0, 5), Some("hello"));
        assert_eq!(f.slice(6, 11), Some("world"));
        assert_eq!(f.slice(0, 11), Some("hello world"));
    }

    #[test]
    fn slice_rejects_out_of_bounds() {
        let f = mk("abc");
        assert_eq!(f.slice(0, 99), None);
        assert_eq!(f.slice(2, 1), None);
    }

    #[test]
    fn slice_rejects_non_char_boundary() {
        // 'é' = 0xC3 0xA9 ; byte-offset 1 is mid-char
        let f = mk("é");
        assert_eq!(f.slice(0, 1), None);
        assert_eq!(f.slice(0, 2), Some("é"));
    }

    #[test]
    fn position_of_single_line() {
        let f = mk("hello");
        let pos = f.position_of(2);
        assert_eq!(pos, SourceLocation::new(1, 3).unwrap());
    }

    #[test]
    fn position_of_multi_line() {
        let f = mk("ab\ncde\nfgh");
        //          0123 4567 89A
        assert_eq!(f.position_of(0), SourceLocation::new(1, 1).unwrap());
        assert_eq!(f.position_of(3), SourceLocation::new(2, 1).unwrap());
        assert_eq!(f.position_of(5), SourceLocation::new(2, 3).unwrap());
        assert_eq!(f.position_of(7), SourceLocation::new(3, 1).unwrap());
    }

    #[test]
    fn position_of_past_eof_clamps() {
        let f = mk("abc");
        let pos = f.position_of(999);
        assert_eq!(pos.line.get(), 1);
    }

    #[test]
    fn len_bytes_matches_content() {
        let f = mk("αβγ");
        // each Greek letter is 2 UTF-8 bytes
        assert_eq!(f.len_bytes(), 6);
    }
}
