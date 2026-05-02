//! Shared parser helpers — identifiers, paths, literal recognition.
//!
//! § Both surfaces consume `Ident` tokens into `cssl_ast::Ident` nodes; both consume
//!   dotted / `::`-separated paths into `cssl_ast::ModulePath`. The helpers here are
//!   surface-independent : they work against a `TokenCursor` and push diagnostics when
//!   the stream doesn't match.

use cssl_ast::{DiagnosticBag, Ident, ModulePath, Span};
use cssl_lex::{Keyword, TokenKind};

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

/// § T11-W15-CSSLC-KWBIND : "soft keywords" — Pony-6-capability + namespacy-noun
///   keywords that real-world LoA source uses as variable / parameter / field names.
///   Eligible-set : tag · ref · val · box · iso · trn · type · module · where ·
///                  comptime · in · as.
///
///   `let tag : T = ...` previously failed @ pat::parse_pattern because the lexer
///   tokenized `tag` as `Keyword(Tag)` and the pattern parser only-accepted `Ident`.
///   The fix : in binding-position (let-bindings + fn-params + struct-field-names +
///   const/static-names + use-aliases + struct-constructor-field-names + field-
///   access-rhs-names + named-call-arg-names + expression-prefix-as-path) we
///   accept these "soft" keywords as plain identifiers — span points at the
///   keyword's source-bytes ; downstream HIR/MIR takes the slice as a regular
///   identifier name.
///
///   Hard keywords (always rejected here) : fn · let · const · mut · pub · use ·
///   struct · enum · interface · impl · extern · if · else · while · for · return ·
///   break · continue · true · false · self · Self — these collide with grammar
///   boundaries.
///
///   Expression-flavored keywords (also EXCLUDED from soft-binding) : match · loop ·
///   region · run · perform · with · effect · handler — these only-mean-themselves
///   at expression-prefix position, so allowing them as binding-names would fork the
///   grammar without enabling any real-world LoA source pattern. Future-LoA can
///   re-name `region`/`run`-bindings to `region_id`/`run_idx` etc.
#[must_use]
pub const fn keyword_is_soft_for_binding(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::Tag
            | Keyword::Ref
            | Keyword::Val
            | Keyword::Box
            | Keyword::Iso
            | Keyword::Trn
            | Keyword::Type
            | Keyword::Module
            | Keyword::Where
            | Keyword::Comptime
            | Keyword::In
            | Keyword::As
    )
}

/// Consume an identifier token OR a "soft keyword" usable as a binding-ident.
/// On mismatch, push a diagnostic and return a zero-width `Ident`.
///
/// § T11-W15-CSSLC-KWBIND : Used at let-binding / fn-param / struct-field-name /
///   const-stmt-name / use-alias positions. Hard keywords still rejected.
pub fn parse_binding_ident(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    context: &'static str,
) -> Ident {
    let t = cursor.peek();
    match t.kind {
        TokenKind::Ident => {
            cursor.bump();
            Ident { span: t.span }
        }
        TokenKind::Keyword(k) if keyword_is_soft_for_binding(k) => {
            cursor.bump();
            Ident { span: t.span }
        }
        _ => {
            bag.push(expected_one(TokenKind::Ident, t.kind, t.span, context));
            Ident {
                span: Span::new(t.span.source, t.span.start, t.span.start),
            }
        }
    }
}

/// Predicate : peek the cursor — does the current token look like a binding-ident?
/// (Either a regular `Ident` or a soft-keyword that's usable as a binding name.)
#[must_use]
pub fn token_is_binding_ident(kind: TokenKind) -> bool {
    matches!(kind, TokenKind::Ident)
        || matches!(kind, TokenKind::Keyword(k) if keyword_is_soft_for_binding(k))
}

/// Parse a `::`-separated path (`foo::bar::baz`) or a `.`-separated path
/// (`foo.bar.baz`) depending on the surface convention. The first segment is
/// required; subsequent segments are opt-in via the separator.
///
/// Both separators are accepted, so that Rust-hybrid `module com.apocky.loa`
/// module-declarations and CSLv3-native `foo.bar.baz` paths converge on the same
/// `ModulePath` node.
///
/// § T11-CSSLC-MODKW (T11-W11) : keyword-tokens are accepted as segment names.
///   ∵ Real-world LoA modules use names like `loa.systems.run`, `loa.systems.fn`,
///   etc. where the trailing segment collides with a Rust-hybrid keyword. The
///   per-segment-keyword-tolerance only applies AFTER a `.`/`::` separator
///   (i.e. never on the first segment) — so `module fn` is still rejected as
///   it should be, but `module com.apocky.loa.systems.run` is accepted.
///
/// § DO NOT use in expression / pattern contexts : `.` is field-access there,
/// not a path separator. Use [`parse_colon_path`] instead.
pub fn parse_module_path(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    context: &'static str,
) -> ModulePath {
    parse_path_with_seps(cursor, bag, context, true, true)
}

/// Parse a `::`-only path — used in expression + pattern contexts where `.` means
/// field-access. Keyword-segments NOT accepted here (those make sense only at
/// module-decl + use-path scope).
pub fn parse_colon_path(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    context: &'static str,
) -> ModulePath {
    parse_path_with_seps(cursor, bag, context, false, false)
}

/// `next_is_path_segment` answers : may this token become a path-segment after
/// a `.` / `::` separator?
///
/// First-segment is always strict-Ident (an item-level keyword always wins
/// when it appears at the head of a token stream — that's how `fn foo()` /
/// `struct S` etc. parse). Subsequent segments tolerate any `Keyword` so
/// real-world module names like `loa.systems.run` (where `run` collides with
/// the `Keyword::Run` macro-prefix) parse cleanly.
fn token_can_act_as_path_segment(kind: TokenKind, allow_keyword: bool) -> bool {
    matches!(kind, TokenKind::Ident) || (allow_keyword && matches!(kind, TokenKind::Keyword(_)))
}

fn parse_path_with_seps(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    context: &'static str,
    accept_dot: bool,
    accept_keyword_after_sep: bool,
) -> ModulePath {
    let first = parse_ident(cursor, bag, context);
    let mut segments = vec![first];
    loop {
        let sep = cursor.peek();
        let is_sep =
            sep.kind == TokenKind::ColonColon || (accept_dot && sep.kind == TokenKind::Dot);
        if is_sep {
            // Only consume the separator if the *next* token can act as a
            // path-segment — otherwise this `.` / `::` belongs to a higher-
            // level construct (field-access, method call, etc.) and must
            // remain on the cursor for the caller.
            let peek2 = cursor.peek2();
            if token_can_act_as_path_segment(peek2.kind, accept_keyword_after_sep) {
                cursor.bump();
                let segment_tok = cursor.peek();
                cursor.bump();
                segments.push(Ident {
                    span: segment_tok.span,
                });
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

    // ───────────────────────────────────────────────────────────────────────
    // § T11-CSSLC-MODKW (T11-W11) — keyword-as-path-segment-after-separator
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn parse_module_path_accepts_keyword_segment_after_dot() {
        // Real-world LoA module : `com.apocky.loa.systems.run` ; `run` is
        // `Keyword::Run`. Pre-W11 this stopped at `systems` + the trailing
        // `.run` left a dangling `Dot` on the cursor.
        let (_f, toks) = lex_rust("com.apocky.loa.systems.run");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_module_path(&mut c, &mut bag, "module path with keyword tail");
        assert_eq!(p.segments.len(), 5, "all 5 segments must parse");
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_module_path_accepts_multiple_keyword_segments() {
        // Defensive : a path where multiple segments collide with keywords.
        // `loa.fn.struct.run` is a synthetic stress case.
        let (_f, toks) = lex_rust("loa.fn.struct.run");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_module_path(&mut c, &mut bag, "path with kw segs");
        assert_eq!(p.segments.len(), 4);
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_module_path_rejects_keyword_first_segment() {
        // First segment is parsed via `parse_ident` which strict-requires
        // `TokenKind::Ident`. A bare `fn` head must produce a diagnostic.
        let (_f, toks) = lex_rust("fn");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let _p = parse_module_path(&mut c, &mut bag, "path");
        assert!(bag.error_count() >= 1, "first-seg-as-keyword must error");
    }

    #[test]
    fn parse_colon_path_does_not_accept_keyword_segment() {
        // `parse_colon_path` is used in expression / pattern contexts where
        // accepting keyword-segments would break disambiguation. Keyword
        // tokens here remain separate tokens for the caller.
        let (_f, toks) = lex_rust("Foo::run");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = super::parse_colon_path(&mut c, &mut bag, "expr path");
        // We expect only `Foo` to be consumed as a segment ; `::run` is left
        // for the caller (because expression-path-rules don't accept a
        // keyword second-segment).
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
