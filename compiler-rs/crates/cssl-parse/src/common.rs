//! Shared parser helpers — identifiers, paths, literal recognition.
//!
//! § Both surfaces consume `Ident` tokens into `cssl_ast::Ident` nodes; both consume
//!   dotted / `::`-separated paths into `cssl_ast::ModulePath`. The helpers here are
//!   surface-independent : they work against a `TokenCursor` and push diagnostics when
//!   the stream doesn't match.

use cssl_ast::{DiagnosticBag, Ident, ModulePath, Span};
use cssl_lex::TokenKind;

use crate::cursor::TokenCursor;
use crate::error::{expected_any, expected_one};

/// Consume an identifier token; on mismatch, push a diagnostic and return an
/// `Ident` at a zero-width span pointing at the offending position.
pub fn parse_ident(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    context: &'static str,
) -> Ident {
    let t = cursor.peek();
    if t.kind == TokenKind::Ident {
        cursor.bump();
        Ident { span: t.span }
    } else {
        bag.push(expected_one(TokenKind::Ident, t.kind, t.span, context));
        Ident {
            span: Span::new(t.span.source, t.span.start, t.span.start),
        }
    }
}

/// Parse a `::`-separated path (`foo::bar::baz`) or a `.`-separated path
/// (`foo.bar.baz`) depending on the surface convention. The first segment is
/// required; subsequent segments are opt-in via the separator.
///
/// Both separators are accepted, so that Rust-hybrid `module com.apocky.loa`
/// module-declarations and CSLv3-native `foo.bar.baz` paths converge on the same
/// `ModulePath` node.
///
/// § DO NOT use in expression / pattern contexts : `.` is field-access there,
/// not a path separator. Use [`parse_colon_path`] instead.
pub fn parse_module_path(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    context: &'static str,
) -> ModulePath {
    parse_path_with_seps(cursor, bag, context, true)
}

/// Parse a `::`-only path — used in expression + pattern contexts where `.` means
/// field-access.
pub fn parse_colon_path(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    context: &'static str,
) -> ModulePath {
    parse_path_with_seps(cursor, bag, context, false)
}

fn parse_path_with_seps(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    context: &'static str,
    accept_dot: bool,
) -> ModulePath {
    let first = parse_ident(cursor, bag, context);
    let mut segments = vec![first];
    loop {
        let sep = cursor.peek();
        let is_sep =
            sep.kind == TokenKind::ColonColon || (accept_dot && sep.kind == TokenKind::Dot);
        if is_sep {
            // Only consume the separator if the *next* token is an identifier — otherwise
            // this `.` / `::` belongs to a higher-level construct (field-access, method call,
            // etc.) that must keep the separator.
            let peek2 = cursor.peek2();
            if peek2.kind == TokenKind::Ident {
                cursor.bump();
                let segment = parse_ident(cursor, bag, context);
                segments.push(segment);
                continue;
            }
        }
        break;
    }
    let span = match (segments.first(), segments.last()) {
        (Some(a), Some(b)) => Span::new(a.span.source, a.span.start, b.span.end),
        _ => Span::DUMMY,
    };
    ModulePath { span, segments }
}

/// Consume a specific expected token kind, pushing a diagnostic on mismatch.
/// Returns the consumed token's span (or a zero-width span at the error site).
pub fn expect(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    expected: TokenKind,
    context: &'static str,
) -> Span {
    let t = cursor.peek();
    if t.kind == expected {
        cursor.bump();
        t.span
    } else {
        bag.push(expected_one(expected, t.kind, t.span, context));
        Span::new(t.span.source, t.span.start, t.span.start)
    }
}

/// Consume any one of the listed kinds, pushing a diagnostic on mismatch.
pub fn expect_any(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    expected: &[TokenKind],
    context: &'static str,
) -> Option<TokenKind> {
    let t = cursor.peek();
    if expected.contains(&t.kind) {
        cursor.bump();
        Some(t.kind)
    } else {
        bag.push(expected_any(expected.to_vec(), t.kind, t.span, context));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{expect, parse_ident, parse_module_path};
    use cssl_ast::{DiagnosticBag, SourceFile, SourceId, Surface};
    use cssl_lex::TokenKind;

    use crate::cursor::TokenCursor;

    fn lex_rust(src: &str) -> (SourceFile, Vec<cssl_lex::Token>) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        (f, toks)
    }

    #[test]
    fn parse_ident_accepts_single_ident() {
        let (_f, toks) = lex_rust("foo");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let id = parse_ident(&mut c, &mut bag, "test");
        assert_eq!(bag.error_count(), 0);
        assert_eq!(id.span.len(), 3);
    }

    #[test]
    fn parse_ident_diagnoses_non_ident() {
        let (_f, toks) = lex_rust("42");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let _id = parse_ident(&mut c, &mut bag, "test");
        assert_eq!(bag.error_count(), 1);
    }

    #[test]
    fn parse_module_path_multi_segment() {
        let (_f, toks) = lex_rust("foo::bar::baz");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_module_path(&mut c, &mut bag, "use path");
        assert_eq!(p.segments.len(), 3);
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_module_path_dot_separator() {
        let (_f, toks) = lex_rust("com.apocky.loa");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_module_path(&mut c, &mut bag, "module path");
        assert_eq!(p.segments.len(), 3);
    }

    #[test]
    fn parse_module_path_stops_at_non_ident() {
        let (_f, toks) = lex_rust("foo :: 42");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_module_path(&mut c, &mut bag, "path");
        // `::` not followed by ident is left for the caller.
        assert_eq!(p.segments.len(), 1);
    }

    #[test]
    fn expect_advances_on_match() {
        let (_f, toks) = lex_rust(",");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let _sp = expect(&mut c, &mut bag, TokenKind::Comma, "ctx");
        assert_eq!(bag.error_count(), 0);
        assert!(c.is_eof());
    }

    #[test]
    fn expect_diagnoses_on_mismatch() {
        let (_f, toks) = lex_rust(",");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let _sp = expect(&mut c, &mut bag, TokenKind::Semi, "ctx");
        assert_eq!(bag.error_count(), 1);
    }
}
