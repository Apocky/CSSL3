//! § diag — diagnostic rendering for `csslc`.
//!
//! As of spec-70 § item-89 PR-2, csslc routes every `DiagnosticBag` entry through
//! `cssl_ast::Renderer`, giving us : rustc-style `severity[code]: message` headers,
//! `--> file:line:col` source-locators, caret underlines (when a `SourceFile` is
//! threaded), `did you mean : X?` suggestions, and ANSI color when stderr is a tty
//! (honoring `NO_COLOR` and `CLICOLOR_FORCE` per <https://no-color.org>).
//!
//! Two emit-paths are exposed :
//!   - [`emit_diagnostics`]              : path-only ; renders headers without carets.
//!                                          Kept for callers that don't have the
//!                                          source text in hand.
//!   - [`emit_diagnostics_with_source`] : full-fidelity ; uses `SourceFile` to draw
//!                                          the source line + caret + column.
//!
//! Both return the count of `Severity::Error`-level entries.
//!
//! The legacy `Severity` + `DiagLine` types remain as public API for downstream
//! callers that wanted manual line-shaping ; new code should prefer the renderer.

use std::path::Path;

use cssl_ast::{DiagnosticBag, Renderer, SourceFile};

/// Severity classes recognized by `csslc`. Maps from internal diagnostic
/// kinds to a printable label and an exit-code influence flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    /// Printable label for this severity.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }

    /// Whether this severity represents a non-zero exit-code condition.
    #[must_use]
    pub const fn is_fatal(self) -> bool {
        matches!(self, Self::Error)
    }
}

/// One rendered diagnostic line.
#[derive(Debug, Clone)]
pub struct DiagLine {
    pub severity: Severity,
    pub code: Option<String>,
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub message: String,
}

impl DiagLine {
    /// Render the diagnostic in the canonical `<file>:<line>:<col>: <sev>: <code> <msg>`
    /// form. The `<code>` segment is omitted if unset.
    #[must_use]
    pub fn render(&self) -> String {
        let code = self
            .code
            .as_ref()
            .map(|c| format!("[{c}] "))
            .unwrap_or_default();
        format!(
            "{}:{}:{}: {}: {}{}",
            self.file,
            self.line,
            self.col,
            self.severity.label(),
            code,
            self.message,
        )
    }
}

/// Emit a `DiagnosticBag`'s contents to stderr without source-line carets.
/// Returns the count of `Error`-severity entries.
///
/// Use this when the caller has only a `Path` (no loaded `SourceFile`). For
/// caret-quality rendering, prefer [`emit_diagnostics_with_source`].
///
/// `file_path` is currently advisory — header lines are produced by
/// `cssl_ast::Renderer` and don't include the path when no span is present;
/// it is retained in the signature so the legacy call-sites do not need to
/// change and so that future spec-70 follow-ups (e.g. emitting a synthetic
/// `<file>:0:0` prefix for span-less diagnostics) can use it without another
/// signature churn.
pub fn emit_diagnostics(file_path: &Path, bag: &cssl_ast::DiagnosticBag) -> u32 {
    let _ = file_path; // reserved for future synthetic-prefix wiring
    emit_via_renderer(bag, None)
}

/// Emit a `DiagnosticBag`'s contents to stderr with full caret rendering.
/// Returns the count of `Error`-severity entries. See module docs.
pub fn emit_diagnostics_with_source(source: &SourceFile, bag: &DiagnosticBag) -> u32 {
    emit_via_renderer(bag, Some(source))
}

/// Render a single diagnostic to a `String` for tests / capture. Plain-text
/// (no ANSI) regardless of tty state ; suitable for golden-snapshot asserts.
#[must_use]
pub fn render_diagnostic_plain(
    source: Option<&SourceFile>,
    diag: &cssl_ast::Diagnostic,
) -> String {
    Renderer::plain().render(diag, source)
}

fn emit_via_renderer(bag: &DiagnosticBag, source: Option<&SourceFile>) -> u32 {
    let renderer = Renderer::auto_for_stderr();
    let mut fatal: u32 = 0;
    for diag in bag.iter() {
        if diag.severity.is_error() {
            fatal = fatal.saturating_add(1);
        }
        // Renderer always terminates each diagnostic with a trailing newline
        // for span-bearing diagnostics ; for header-only it emits one line +
        // newline. `eprint!` (no extra newline) keeps output clean.
        eprint!("{}", renderer.render(diag, source));
    }
    fatal
}

/// Format a "file not found" / "unreadable" error for the user.
#[must_use]
pub fn fs_error(path: &Path, err: &std::io::Error) -> String {
    format!(
        "{}: error: cannot read source file ({})",
        path.display(),
        err.kind(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_labels_canonical() {
        assert_eq!(Severity::Error.label(), "error");
        assert_eq!(Severity::Warning.label(), "warning");
        assert_eq!(Severity::Note.label(), "note");
    }

    #[test]
    fn severity_fatality_only_errors() {
        assert!(Severity::Error.is_fatal());
        assert!(!Severity::Warning.is_fatal());
        assert!(!Severity::Note.is_fatal());
    }

    #[test]
    fn diag_line_renders_with_code() {
        let d = DiagLine {
            severity: Severity::Error,
            code: Some("AD0001".to_string()),
            file: "foo.cssl".to_string(),
            line: 12,
            col: 7,
            message: "expected differentiable expression".to_string(),
        };
        assert_eq!(
            d.render(),
            "foo.cssl:12:7: error: [AD0001] expected differentiable expression"
        );
    }

    #[test]
    fn diag_line_renders_without_code() {
        let d = DiagLine {
            severity: Severity::Warning,
            code: None,
            file: "bar.cssl".to_string(),
            line: 1,
            col: 1,
            message: "deprecated syntax".to_string(),
        };
        assert_eq!(d.render(), "bar.cssl:1:1: warning: deprecated syntax");
    }

    #[test]
    fn fs_error_includes_path_and_kind() {
        let p = std::path::PathBuf::from("/tmp/missing.cssl");
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let s = fs_error(&p, &err);
        assert!(s.contains("missing.cssl"));
        assert!(s.contains("error"));
    }

    #[test]
    fn emit_diagnostics_returns_zero_for_empty_bag() {
        use cssl_ast::DiagnosticBag;
        let bag = DiagnosticBag::new();
        let n = emit_diagnostics(std::path::Path::new("foo.cssl"), &bag);
        assert_eq!(n, 0);
    }

    #[test]
    fn emit_diagnostics_counts_only_errors_as_fatal() {
        use cssl_ast::{Diagnostic, DiagnosticBag};
        let mut bag = DiagnosticBag::new();
        bag.push(Diagnostic::error("e1"));
        bag.push(Diagnostic::warning("w1"));
        bag.push(Diagnostic::error("e2"));
        let n = emit_diagnostics(std::path::Path::new("foo.cssl"), &bag);
        assert_eq!(n, 2);
    }

    #[test]
    fn render_diagnostic_plain_emits_rustc_style_header() {
        use cssl_ast::Diagnostic;
        let d = Diagnostic::error("type mismatch").with_code("T0001");
        let s = render_diagnostic_plain(None, &d);
        assert!(s.starts_with("error[T0001]: type mismatch\n"), "got: {s:?}");
        // no ANSI escapes in plain mode
        assert!(!s.contains('\x1b'));
    }

    #[test]
    fn render_diagnostic_plain_with_source_includes_caret() {
        use cssl_ast::{Diagnostic, SourceFile, SourceId, Span, Surface};
        let src = SourceFile::new(
            SourceId::first(),
            "fixture.cssl",
            "let x : u32 = 0;\n",
            Surface::RustHybrid,
        );
        // span over the literal `0`
        let zero_off = src.contents.find('0').expect("fixture has 0") as u32;
        let span = Span::new(src.id, zero_off, zero_off + 1);
        let d = Diagnostic::error("expected i32, got u32")
            .with_code("T0001")
            .with_span(span)
            .with_suggestion("0i32");
        let s = render_diagnostic_plain(Some(&src), &d);
        assert!(s.contains("error[T0001]: expected i32, got u32"), "got: {s}");
        assert!(s.contains("fixture.cssl:1:"), "got: {s}");
        assert!(s.contains("let x : u32 = 0;"), "got: {s}");
        assert!(s.contains("^"), "got: {s}");
        assert!(s.contains("help: did you mean : `0i32`?"), "got: {s}");
    }

    #[test]
    fn emit_diagnostics_with_source_returns_fatal_count() {
        use cssl_ast::{Diagnostic, DiagnosticBag, SourceFile, SourceId, Surface};
        let src = SourceFile::new(SourceId::first(), "x.cssl", "abc\n", Surface::RustHybrid);
        let mut bag = DiagnosticBag::new();
        bag.push(Diagnostic::error("a"));
        bag.push(Diagnostic::warning("b"));
        bag.push(Diagnostic::error("c"));
        let n = emit_diagnostics_with_source(&src, &bag);
        assert_eq!(n, 2);
    }
}
