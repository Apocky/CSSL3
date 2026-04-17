//! Diagnostic records emitted by frontend passes.
//!
//! § STATUS : T2 scaffold — basic `Severity` + `Diagnostic` + `DiagnosticBag`.
//!            miette-integration (code + labels + help) lands at T3 when parser
//!            needs richer error-spans.
//! § POLICY : errors never silently swallowed — `DiagnosticBag` accumulates and the
//!            caller decides whether to bail or continue.

use core::fmt;

use crate::span::Span;

/// Severity of a diagnostic. Ordered `Error > Warning > Note > Help`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Severity {
    /// A help message suggesting a fix.
    Help,
    /// A supplementary note attached to another diagnostic.
    Note,
    /// A warning — compilation continues.
    Warning,
    /// An error — compilation must ultimately abort.
    Error,
}

impl Severity {
    /// Human-readable label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::Note => "note",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// `true` iff this severity blocks compilation.
    #[must_use]
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A single diagnostic record.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Severity of the primary message.
    pub severity: Severity,
    /// Primary message text.
    pub message: String,
    /// Primary span — where the diagnostic points.
    pub span: Option<Span>,
    /// Attached sub-diagnostics (notes, helps) for multi-part messages.
    pub notes: Vec<Note>,
}

/// A secondary note attached to a primary diagnostic.
#[derive(Debug, Clone)]
pub struct Note {
    /// Severity of the note (usually `Note` or `Help`).
    pub severity: Severity,
    /// Message text.
    pub message: String,
    /// Optional span the note refers to.
    pub span: Option<Span>,
}

impl Diagnostic {
    /// Construct a new `Error` diagnostic with the given message.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            span: None,
            notes: Vec::new(),
        }
    }

    /// Construct a new `Warning` diagnostic.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            span: None,
            notes: Vec::new(),
        }
    }

    /// Attach a primary span.
    #[must_use]
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach a `Note`-severity secondary message.
    #[must_use]
    pub fn with_note(mut self, message: impl Into<String>) -> Self {
        self.notes.push(Note {
            severity: Severity::Note,
            message: message.into(),
            span: None,
        });
        self
    }

    /// Attach a `Help`-severity secondary message.
    #[must_use]
    pub fn with_help(mut self, message: impl Into<String>) -> Self {
        self.notes.push(Note {
            severity: Severity::Help,
            message: message.into(),
            span: None,
        });
        self
    }

    /// Attach a span-carrying note.
    #[must_use]
    pub fn with_labeled_note(mut self, message: impl Into<String>, span: Span) -> Self {
        self.notes.push(Note {
            severity: Severity::Note,
            message: message.into(),
            span: Some(span),
        });
        self
    }
}

/// Accumulator for diagnostics produced by a single frontend pass.
///
/// Passes push diagnostics into a `DiagnosticBag` during execution; the bag can be
/// queried for error count before deciding whether to proceed to the next pass.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticBag {
    items: Vec<Diagnostic>,
    error_count: u32,
}

impl DiagnosticBag {
    /// Empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a diagnostic.
    pub fn push(&mut self, d: Diagnostic) {
        if d.severity.is_error() {
            self.error_count = self.error_count.saturating_add(1);
        }
        self.items.push(d);
    }

    /// Iterate over stored diagnostics in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    /// Number of items in the bag.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// `true` iff the bag has zero items.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Count of `Error`-severity items.
    #[must_use]
    pub const fn error_count(&self) -> u32 {
        self.error_count
    }

    /// `true` iff any error has been recorded.
    #[must_use]
    pub const fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// Consume the bag, returning the stored diagnostics.
    #[must_use]
    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, DiagnosticBag, Severity};
    use crate::source::SourceId;
    use crate::span::Span;

    #[test]
    fn severity_order() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Note);
        assert!(Severity::Note > Severity::Help);
    }

    #[test]
    fn severity_labels() {
        assert_eq!(Severity::Error.label(), "error");
        assert_eq!(Severity::Warning.label(), "warning");
        assert_eq!(Severity::Note.label(), "note");
        assert_eq!(Severity::Help.label(), "help");
    }

    #[test]
    fn diagnostic_builder_chain() {
        let span = Span::new(SourceId(1), 10, 20);
        let d = Diagnostic::error("unexpected token")
            .with_span(span)
            .with_note("inside function body")
            .with_help("did you mean `fn`?");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.span, Some(span));
        assert_eq!(d.notes.len(), 2);
        assert_eq!(d.notes[0].severity, Severity::Note);
        assert_eq!(d.notes[1].severity, Severity::Help);
    }

    #[test]
    fn bag_tracks_error_count() {
        let mut bag = DiagnosticBag::new();
        assert!(bag.is_empty());
        assert!(!bag.has_errors());

        bag.push(Diagnostic::warning("unused variable"));
        bag.push(Diagnostic::error("type mismatch"));
        bag.push(Diagnostic::error("missing semicolon"));
        bag.push(Diagnostic::warning("dead code"));

        assert_eq!(bag.len(), 4);
        assert_eq!(bag.error_count(), 2);
        assert!(bag.has_errors());
    }

    #[test]
    fn bag_into_vec_preserves_order() {
        let mut bag = DiagnosticBag::new();
        bag.push(Diagnostic::error("first"));
        bag.push(Diagnostic::warning("second"));
        let vec = bag.into_vec();
        assert_eq!(vec.len(), 2);
        assert_eq!(vec[0].message, "first");
        assert_eq!(vec[1].message, "second");
    }
}
