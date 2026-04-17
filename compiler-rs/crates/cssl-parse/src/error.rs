//! Parser-specific diagnostic helpers.
//!
//! § DESIGN
//!   Parser rules never return `Result<_, _>` — they always produce a CST node (possibly
//!   a synthetic / `Error` variant) and accumulate diagnostics into a shared bag. This
//!   keeps rule-signatures uniform and lets LSP partial-documents still return a walkable
//!   CST after a parse error.
//!
//! § HELPERS
//!   The factory functions here centralize the diagnostic messages so that :
//!     - CI-assertion on diagnostic substrings stays stable
//!     - IDE quick-fix detection can match on message prefixes

use cssl_ast::{Diagnostic, Span};
use cssl_lex::TokenKind;
use thiserror::Error;

/// Structured parser-error variants. These are converted into `cssl_ast::Diagnostic`
/// records via [`ParseError::to_diagnostic`] and pushed onto the shared `DiagnosticBag`.
#[derive(Debug, Clone, Error)]
pub enum ParseError {
    /// Expected one of the listed token kinds; got something else.
    #[error("expected one of {expected:?}, found {found:?}")]
    Expected {
        /// Token kinds legal at this position.
        expected: Vec<TokenKind>,
        /// The token kind actually present.
        found: TokenKind,
        /// Span of the offending token (or the EOF marker).
        span: Span,
        /// Human-readable name of the construct being parsed (e.g., `"item"`, `"type"`).
        context: &'static str,
    },
    /// Saw end-of-file while parsing a construct that requires more tokens.
    #[error("unexpected end of input while parsing {context}")]
    UnexpectedEof {
        /// EOF span.
        span: Span,
        /// Construct being parsed when EOF was reached.
        context: &'static str,
    },
    /// Reserved-for-future syntactic form encountered (spec says this is syntactically valid
    /// but not yet implemented by stage0).
    #[error("syntactic form not yet supported at stage0 : {form}")]
    NotYetSupported {
        /// Short name of the form.
        form: &'static str,
        /// Span of the encountered form.
        span: Span,
    },
    /// Free-form parse error with a custom message (used sparingly).
    #[error("{message}")]
    Custom {
        /// Error text.
        message: String,
        /// Span to label.
        span: Span,
    },
}

impl ParseError {
    /// The span this error is tied to.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Expected { span, .. }
            | Self::UnexpectedEof { span, .. }
            | Self::NotYetSupported { span, .. }
            | Self::Custom { span, .. } => *span,
        }
    }

    /// Convert to a `cssl_ast::Diagnostic` suitable for pushing into a `DiagnosticBag`.
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic {
        let msg = self.to_string();
        Diagnostic::error(msg).with_span(self.span())
    }
}

/// Convenience : build an `Expected` diagnostic with a single alternative.
#[must_use]
pub fn expected_one(
    expected: TokenKind,
    found: TokenKind,
    span: Span,
    context: &'static str,
) -> Diagnostic {
    ParseError::Expected {
        expected: vec![expected],
        found,
        span,
        context,
    }
    .to_diagnostic()
}

/// Convenience : build an `Expected` diagnostic with multiple alternatives.
#[must_use]
pub fn expected_any(
    expected: Vec<TokenKind>,
    found: TokenKind,
    span: Span,
    context: &'static str,
) -> Diagnostic {
    ParseError::Expected {
        expected,
        found,
        span,
        context,
    }
    .to_diagnostic()
}

/// Convenience : build a custom-message diagnostic.
#[must_use]
pub fn custom(message: impl Into<String>, span: Span) -> Diagnostic {
    ParseError::Custom {
        message: message.into(),
        span,
    }
    .to_diagnostic()
}

/// Convenience : build a `NotYetSupported` diagnostic.
#[must_use]
pub fn nyi(form: &'static str, span: Span) -> Diagnostic {
    ParseError::NotYetSupported { form, span }.to_diagnostic()
}

#[cfg(test)]
mod tests {
    use super::{custom, expected_any, expected_one, nyi, ParseError};
    use cssl_ast::{Severity, SourceId, Span};
    use cssl_lex::TokenKind;

    #[test]
    fn parse_error_span_accessor() {
        let sp = Span::new(SourceId::first(), 5, 10);
        let e = ParseError::Custom {
            message: "oops".into(),
            span: sp,
        };
        assert_eq!(e.span(), sp);
    }

    #[test]
    fn expected_one_builds_error_diagnostic() {
        let sp = Span::new(SourceId::first(), 0, 3);
        let d = expected_one(TokenKind::Semi, TokenKind::Eof, sp, "let binding");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.span, Some(sp));
        assert!(d.message.contains("Semi"));
    }

    #[test]
    fn expected_any_lists_alternatives() {
        let sp = Span::new(SourceId::first(), 0, 1);
        let d = expected_any(
            vec![TokenKind::Ident, TokenKind::Keyword(cssl_lex::Keyword::Fn)],
            TokenKind::Eof,
            sp,
            "item",
        );
        assert!(d.message.contains("Ident"));
    }

    #[test]
    fn custom_carries_message() {
        let sp = Span::new(SourceId::first(), 0, 0);
        let d = custom("boom", sp);
        assert_eq!(d.message, "boom");
    }

    #[test]
    fn nyi_mentions_form_name() {
        let sp = Span::new(SourceId::first(), 0, 5);
        let d = nyi("effect-row-polymorphism", sp);
        assert!(d.message.contains("effect-row-polymorphism"));
    }
}
