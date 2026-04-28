//! § diag — diagnostic rendering for `csslc`.
//!
//! Stage-0 keeps this minimal : map every internal diagnostic-bag entry to
//! a single `<file>:<line>:<col>: <severity>: <code> <message>` line on
//! stderr. miette-style fancy rendering is deferred ; the stable diagnostic
//! codes (`AD0001..0003` / `IFC0001..0004` / `STG0001..0003` / `MAC0001..0003`
//! / `SMT-*` / `TEL-*`) are preserved verbatim so downstream tools can match
//! them.

use std::path::Path;

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

/// Emit a `DiagnosticBag`'s contents to stderr. Returns the count of
/// fatal entries (errors).
///
/// Each diagnostic renders to one line in the canonical
/// `<file>:<line>:<col>: <sev>: <msg>` form. Stage-0 uses placeholder
/// `0:0` coordinates because `DiagnosticBag` Diagnostic spans are not
/// yet linked to source-line resolution ; the tooling-stable diagnostic
/// codes (when present) are preserved.
pub fn emit_diagnostics(file_path: &Path, bag: &cssl_ast::DiagnosticBag) -> u32 {
    let mut fatal = 0u32;
    let file_display = file_path.display().to_string();
    for diag in bag.iter() {
        let severity = match diag.severity {
            cssl_ast::Severity::Error => Severity::Error,
            cssl_ast::Severity::Warning => Severity::Warning,
            cssl_ast::Severity::Note | cssl_ast::Severity::Help => Severity::Note,
        };
        if severity.is_fatal() {
            fatal = fatal.saturating_add(1);
        }
        let line = DiagLine {
            severity,
            code: None,
            file: file_display.clone(),
            line: 0,
            col: 0,
            message: diag.message.clone(),
        };
        eprintln!("{}", line.render());
        for note in &diag.notes {
            let note_line = DiagLine {
                severity: match note.severity {
                    cssl_ast::Severity::Error => Severity::Error,
                    cssl_ast::Severity::Warning => Severity::Warning,
                    cssl_ast::Severity::Note | cssl_ast::Severity::Help => Severity::Note,
                },
                code: None,
                file: file_display.clone(),
                line: 0,
                col: 0,
                message: note.message.clone(),
            };
            eprintln!("    {}", note_line.render());
        }
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
}
