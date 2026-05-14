//! Render `Diagnostic` records to a `String` (or stderr) in rustc-style format.
//!
//! § STATUS : T3 — first cut · A89.1 (file:line:col + caret) + A89.3 (ANSI color)
//!            from `specs/70_PHASE_A_CORRECTNESS.csl § item-89`.
//!            Multi-line spans, Unicode width counting, and macro-expansion-aware
//!            spans are deferred (per §RZ.open FM.5).
//! § POLICY : preserve every existing `Diagnostic` API and call-site. This module
//!            only adds a renderer; it does not change emitter shape.
//!
//! Output shape (single-span error with code + suggestion) :
//!
//! ```text
//! error[T0001]: expected `i64`, got `u32`
//!   --> examples/diag_test.cssl:3:23
//!    |
//!  3 | fn f(x : u32) -> i64 { x }
//!    |                       ^ this is `u32`
//!    |
//!    = help: did you mean : `x as i64`?
//! ```
//!
//! When `Renderer::with_color(true)` (or auto-detected via `IsTerminal`), the severity
//! tag, code, and caret are colorized using minimal ANSI escape sequences. The
//! `NO_COLOR` environment variable (per <https://no-color.org>) overrides any auto/explicit
//! color enable.

use core::fmt::Write as _;
use std::env;
use std::io::IsTerminal;

use crate::diagnostic::{Diagnostic, Note, Severity};
use crate::source::{SourceFile, SourceLocation};
use crate::span::Span;

/// ANSI escape sequences used by the colorizer. Kept to a tiny set so we don't pull
/// in a color crate (per §RZ.open · zero-deps for diagnostic rendering).
mod ansi {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const RED: &str = "\x1b[31m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const CYAN: &str = "\x1b[36m";
    pub const BLUE: &str = "\x1b[34m";
    pub const GREEN: &str = "\x1b[32m";
}

/// Renders `Diagnostic` into rustc-style human-readable text.
#[derive(Debug, Clone, Copy)]
pub struct Renderer {
    color: bool,
}

impl Renderer {
    /// Plain (no-color) renderer. Always safe for log files and CI capture.
    #[must_use]
    pub const fn plain() -> Self {
        Self { color: false }
    }

    /// Renderer with explicit color setting. Caller-controlled; ignores
    /// `NO_COLOR`. Use `auto_for_stderr` for the env-aware variant.
    #[must_use]
    pub const fn with_color(color: bool) -> Self {
        Self { color }
    }

    /// Auto-detect whether to emit color based on stderr being a tty AND
    /// `NO_COLOR` being unset. `CLICOLOR_FORCE=1` forces color on regardless.
    /// See spec-70 § A89.3.
    #[must_use]
    pub fn auto_for_stderr() -> Self {
        let force = matches!(env::var("CLICOLOR_FORCE").as_deref(), Ok("1"));
        if force {
            return Self { color: true };
        }
        let no_color = env::var_os("NO_COLOR").is_some();
        if no_color {
            return Self { color: false };
        }
        Self {
            color: std::io::stderr().is_terminal(),
        }
    }

    /// Render a diagnostic to a `String`. `source` is consulted only when the
    /// diagnostic carries a `Span`; otherwise the renderer produces a header-only
    /// form (severity + message + code).
    #[must_use]
    pub fn render(&self, diag: &Diagnostic, source: Option<&SourceFile>) -> String {
        let mut out = String::with_capacity(256);
        self.render_into(&mut out, diag, source);
        out
    }

    /// Render into an existing buffer (useful when batching).
    pub fn render_into(&self, out: &mut String, diag: &Diagnostic, source: Option<&SourceFile>) {
        self.write_header(out, diag);
        if let (Some(span), Some(src)) = (diag.span, source) {
            if span.source == src.id {
                self.write_snippet(out, src, span, /* label */ &severity_caret_label(diag));
            } else {
                let _ = writeln!(
                    out,
                    "  --> <span source mismatch: span.source={} != src.id={}>",
                    span.source, src.id
                );
            }
        }
        for note in &diag.notes {
            self.write_note(out, note, source);
        }
        if let Some(sug) = &diag.suggestion {
            self.write_suggestion(out, sug);
        }
    }

    fn write_header(&self, out: &mut String, diag: &Diagnostic) {
        let sev_color = self.color_for(diag.severity);
        let bold_open = self.bold();
        let reset = self.reset();
        if let Some(code) = &diag.code {
            let _ = writeln!(
                out,
                "{sev_color}{bold_open}{label}[{code}]{reset}: {bold_open}{msg}{reset}",
                label = diag.severity.label(),
                msg = diag.message,
            );
        } else {
            let _ = writeln!(
                out,
                "{sev_color}{bold_open}{label}{reset}: {bold_open}{msg}{reset}",
                label = diag.severity.label(),
                msg = diag.message,
            );
        }
    }

    fn write_snippet(&self, out: &mut String, src: &SourceFile, span: Span, label: &str) {
        let loc = src.position_of(span.start);
        let blue = self.color_or_empty(ansi::BLUE);
        let bold = self.bold();
        let reset = self.reset();
        let path = src.path.as_str();

        // header line: "  --> path:line:col"
        let _ = writeln!(
            out,
            "{blue}{bold}  -->{reset} {path}:{line}:{col}",
            line = loc.line,
            col = loc.column,
        );

        // line of source. We render exactly one line (the start-line of the span).
        let line_text = line_for(src, loc);
        let line_no_str = format!("{:>3}", loc.line.get());
        let gutter = " ".repeat(line_no_str.len());
        let bar = format!("{blue}{bold}|{reset}");

        // empty gutter line above
        let _ = writeln!(out, "{gutter} {bar}");

        // source line
        let _ = writeln!(out, "{blue}{bold}{line_no_str}{reset} {bar} {line_text}");

        // caret line — count carets across the visible span on this line
        let line_start = src.position_of(span.start).column.get() as usize - 1;
        let span_len_on_line = caret_width(line_text, line_start, span);
        let pad = " ".repeat(line_start);
        let carets = "^".repeat(span_len_on_line.max(1));
        let caret_color = self.color_for_severity_caret(label);
        let _ = writeln!(
            out,
            "{gutter} {bar} {pad}{caret_color}{bold}{carets}{maybe_space}{label}{reset}",
            maybe_space = if label.is_empty() { "" } else { " " },
        );

        // trailing empty bar line
        let _ = writeln!(out, "{gutter} {bar}");
    }

    fn write_note(&self, out: &mut String, note: &Note, source: Option<&SourceFile>) {
        let cyan = self.color_or_empty(ansi::CYAN);
        let bold = self.bold();
        let reset = self.reset();
        let _ = writeln!(
            out,
            "   = {cyan}{bold}{label}{reset}: {msg}",
            label = note.severity.label(),
            msg = note.message,
        );
        if let (Some(span), Some(src)) = (note.span, source) {
            if span.source == src.id {
                let loc = src.position_of(span.start);
                let _ = writeln!(out, "     at {}:{}:{}", src.path, loc.line, loc.column);
            }
        }
    }

    fn write_suggestion(&self, out: &mut String, sug: &str) {
        let green = self.color_or_empty(ansi::GREEN);
        let bold = self.bold();
        let reset = self.reset();
        let _ = writeln!(
            out,
            "   = {green}{bold}help{reset}: did you mean : `{sug}`?"
        );
    }

    fn color_for(&self, sev: Severity) -> &'static str {
        if !self.color {
            return "";
        }
        match sev {
            Severity::Error => ansi::RED,
            Severity::Warning => ansi::YELLOW,
            Severity::Note => ansi::CYAN,
            Severity::Help => ansi::GREEN,
        }
    }

    fn color_for_severity_caret(&self, _label: &str) -> &'static str {
        // Caret takes the primary diagnostic's severity color. We don't have the
        // severity here directly (label is a freeform string); default to red for
        // the first cut. A follow-up can thread severity through.
        if self.color { ansi::RED } else { "" }
    }

    fn color_or_empty(&self, code: &'static str) -> &'static str {
        if self.color { code } else { "" }
    }

    fn bold(&self) -> &'static str {
        if self.color { ansi::BOLD } else { "" }
    }

    fn reset(&self) -> &'static str {
        if self.color { ansi::RESET } else { "" }
    }
}

/// Choose a short caret-label from the diagnostic. Currently empty; future work
/// can derive a label from the primary message (e.g. the type the span carries).
fn severity_caret_label(_diag: &Diagnostic) -> String {
    String::new()
}

/// Extract the source line containing `loc` from `src`. Strips trailing CR/LF.
fn line_for(src: &SourceFile, loc: SourceLocation) -> &str {
    // We need byte offsets for line N. position_of doesn't directly expose them,
    // but slice + scan suffices: walk back to the start of line, forward to '\n'.
    let line_idx_zero_based = loc.line.get().saturating_sub(1) as usize;
    let mut start = 0usize;
    let mut current_line = 0usize;
    let bytes = src.contents.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if current_line == line_idx_zero_based {
            start = i;
            break;
        }
        if *b == b'\n' {
            current_line += 1;
        }
    }
    let mut end = bytes.len();
    for (i, b) in bytes[start..].iter().enumerate() {
        if *b == b'\n' {
            end = start + i;
            break;
        }
    }
    let slice = &src.contents[start..end];
    slice.trim_end_matches('\r')
}

/// Number of caret characters to draw under `span` on the line that contains its
/// start. Capped at the remaining visible width of that line so multi-line spans
/// don't draw past the end of the rendered source line. Single-line case: the
/// span width directly.
fn caret_width(line_text: &str, col_zero_based: usize, span: Span) -> usize {
    let span_bytes = (span.end - span.start) as usize;
    let line_bytes = line_text.len();
    let max_on_line = line_bytes.saturating_sub(col_zero_based);
    span_bytes.min(max_on_line).max(1)
}

#[cfg(test)]
mod tests {
    use super::Renderer;
    use crate::diagnostic::Diagnostic;
    use crate::source::{SourceFile, SourceId, Surface};
    use crate::span::Span;

    fn fixture() -> SourceFile {
        SourceFile::new(
            SourceId::first(),
            "examples/diag_test.cssl",
            "fn add(a : i32, b : i32) -> i32 { a + b }\nfn f(x : u32) -> i64 { x }\n",
            Surface::RustHybrid,
        )
    }

    #[test]
    fn render_header_only_no_span() {
        let d = Diagnostic::error("module not found").with_code("T0010");
        let r = Renderer::plain();
        let out = r.render(&d, None);
        assert!(out.starts_with("error[T0010]: module not found\n"), "got: {out:?}");
        // no source-snippet block when there is no span
        assert!(!out.contains("-->"));
    }

    #[test]
    fn render_with_span_includes_caret() {
        let src = fixture();
        // "fn f(x : u32) -> i64 { x }" is line 2; the lone `x` on the right is at
        // column 24 (1-indexed) and is 1 byte wide.
        let line_two_start = src
            .contents
            .find("fn f")
            .expect("fixture has line 2") as u32;
        let x_offset = line_two_start
            + src.contents[line_two_start as usize..]
                .find("{ x }")
                .expect("expected `{ x }`") as u32
            + 2; // skip "{ "
        let span = Span::new(src.id, x_offset, x_offset + 1);

        let d = Diagnostic::error("expected `i64`, got `u32`")
            .with_code("T0001")
            .with_span(span)
            .with_suggestion("x as i64");

        let out = Renderer::plain().render(&d, Some(&src));
        // header
        assert!(out.contains("error[T0001]: expected `i64`, got `u32`"), "got: {out}");
        // path:line:col
        assert!(out.contains("examples/diag_test.cssl:2:"), "got: {out}");
        // source line and caret
        assert!(out.contains("fn f(x : u32) -> i64 { x }"), "got: {out}");
        assert!(out.contains("^"), "got: {out}");
        // suggestion
        assert!(out.contains("help: did you mean : `x as i64`?"), "got: {out}");
    }

    #[test]
    fn plain_renderer_emits_no_ansi_escapes() {
        let src = fixture();
        let span = Span::new(src.id, 0, 2);
        let d = Diagnostic::error("oops").with_span(span);
        let out = Renderer::plain().render(&d, Some(&src));
        assert!(!out.contains('\x1b'), "plain renderer must not emit ESC; got: {out:?}");
    }

    #[test]
    fn color_renderer_emits_ansi_escapes() {
        let src = fixture();
        let span = Span::new(src.id, 0, 2);
        let d = Diagnostic::error("oops").with_span(span);
        let out = Renderer::with_color(true).render(&d, Some(&src));
        assert!(out.contains('\x1b'), "color renderer must emit ESC; got: {out:?}");
        // reset must appear (we always close colored runs)
        assert!(out.contains("\x1b[0m"), "expected reset; got: {out:?}");
    }

    #[test]
    fn span_source_mismatch_renders_safe_marker() {
        let src = fixture();
        let other_src_span = Span::new(SourceId(99), 0, 1);
        let d = Diagnostic::error("oops").with_span(other_src_span);
        let out = Renderer::plain().render(&d, Some(&src));
        assert!(out.contains("source mismatch"), "got: {out}");
    }

    #[test]
    fn snippet_includes_gutter_and_bars() {
        let src = fixture();
        let span = Span::new(src.id, 0, 2);
        let d = Diagnostic::error("oops").with_span(span);
        let out = Renderer::plain().render(&d, Some(&src));
        // expect a "  --> ", at least three '|' chars (gutter top, source, caret line),
        // and a digit gutter "  1"
        assert!(out.contains("-->"));
        assert!(out.matches('|').count() >= 3, "got: {out}");
        assert!(out.contains("  1"), "got: {out}");
    }
}
