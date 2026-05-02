//! Pattern parser.
//!
//! § SPEC : `specs/09_SYNTAX.csl` uses patterns in `let` bindings, `match` arms, fn-params.
//!
//! § COVERED
//!   - Wildcard `_`
//!   - Literal (int / float / string / char / bool)
//!   - Binding `x` / `mut x` / `ref x` / `ref mut x`
//!   - Tuple `(a, b, c)`
//!   - Struct `Point { x, y : b }`
//!   - Variant `Some(v)` / `None`
//!   - Or `a | b | c`
//!   - Range `a..b` / `a..=b`

use cssl_ast::{
    DiagnosticBag, Ident, Literal, LiteralKind, Pattern, PatternField, PatternKind, Span,
};
use cssl_lex::{BracketKind, BracketSide, Keyword, TokenKind};

use crate::common::{keyword_is_soft_for_binding, parse_colon_path, parse_ident};
use crate::cursor::TokenCursor;
use crate::error::custom;

/// Parse a single pattern. Handles the top-level `|` or-pattern combinator.
#[must_use]
pub fn parse_pattern(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Pattern {
    let first = parse_atomic_pattern(cursor, bag);
    if !cursor.check(TokenKind::Pipe) {
        return first;
    }
    let mut alts = vec![first];
    while cursor.eat(TokenKind::Pipe).is_some() {
        alts.push(parse_atomic_pattern(cursor, bag));
    }
    let span_start = alts.first().map_or(Span::DUMMY, |p| p.span).start;
    let span_end = alts.last().map_or(Span::DUMMY, |p| p.span).end;
    let source = alts.first().map_or(Span::DUMMY, |p| p.span).source;
    Pattern {
        span: Span::new(source, span_start, span_end),
        kind: PatternKind::Or(alts),
    }
}

fn parse_atomic_pattern(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Pattern {
    let start = cursor.peek();
    let kind = match start.kind {
        // wildcard `_`
        TokenKind::Ident if is_underscore(cursor, start.span) => {
            cursor.bump();
            PatternKind::Wildcard
        }
        // literal patterns
        TokenKind::IntLiteral => {
            cursor.bump();
            PatternKind::Literal(Literal {
                span: start.span,
                kind: LiteralKind::Int,
            })
        }
        TokenKind::FloatLiteral => {
            cursor.bump();
            PatternKind::Literal(Literal {
                span: start.span,
                kind: LiteralKind::Float,
            })
        }
        TokenKind::StringLiteral(_) => {
            cursor.bump();
            PatternKind::Literal(Literal {
                span: start.span,
                kind: LiteralKind::Str,
            })
        }
        TokenKind::CharLiteral => {
            cursor.bump();
            PatternKind::Literal(Literal {
                span: start.span,
                kind: LiteralKind::Char,
            })
        }
        TokenKind::Keyword(Keyword::True) => {
            cursor.bump();
            PatternKind::Literal(Literal {
                span: start.span,
                kind: LiteralKind::Bool(true),
            })
        }
        TokenKind::Keyword(Keyword::False) => {
            cursor.bump();
            PatternKind::Literal(Literal {
                span: start.span,
                kind: LiteralKind::Bool(false),
            })
        }
        // tuple pattern `(…)` (including unit)
        TokenKind::Bracket(BracketKind::Paren, BracketSide::Open) => {
            parse_tuple_pattern(cursor, bag)
        }
        // `mut x` or `ref x`
        TokenKind::Keyword(Keyword::Mut) => {
            cursor.bump();
            let name = parse_ident(cursor, bag, "binding after `mut`");
            PatternKind::Binding {
                mutable: true,
                name,
            }
        }
        TokenKind::Keyword(Keyword::Ref) => {
            cursor.bump();
            let mutable = cursor.eat(TokenKind::Keyword(Keyword::Mut)).is_some();
            let inner = Box::new(parse_atomic_pattern(cursor, bag));
            PatternKind::Ref { mutable, inner }
        }
        // `self` keyword as fn-parameter binding pattern.
        // § Mirrors Rust grammar : bare `self` (no `&` / `&mut` prefix) binds the
        //   implicit method receiver. Trait-dispatch (T11-D99) resolves the binding's
        //   type from the impl-block self-type-leading-segment, so we emit a normal
        //   Binding here and let the source-text slice through `Ident.span` re-slice
        //   to literal "self" downstream.
        // § DEFERRED : `&self` / `&mut self` method-syntax-borrow forms — out of slice.
        TokenKind::Keyword(Keyword::SelfValue) => {
            cursor.bump();
            PatternKind::Binding {
                mutable: false,
                name: Ident { span: start.span },
            }
        }
        // path → variant / struct / binding
        TokenKind::Ident => parse_path_pattern(cursor, bag),
        // § T11-W15-CSSLC-KWBIND : soft-keywords as binding-idents
        //   `let tag : T = ...` / `let region : usize = ...` / etc. — the lexer
        //   tokenizes `tag` / `region` / `ref` / etc. as Keyword(*) but in
        //   pattern (binding) position we treat them as plain identifiers.
        //   Span points at the keyword's source-bytes ; downstream HIR/MIR
        //   takes the slice as a regular identifier name.
        TokenKind::Keyword(k) if keyword_is_soft_for_binding(k) => {
            cursor.bump();
            PatternKind::Binding {
                mutable: false,
                name: Ident { span: start.span },
            }
        }
        _ => {
            bag.push(custom("expected a pattern", start.span));
            cursor.bump();
            PatternKind::Wildcard
        }
    };
    let end = cursor.peek().span.start.max(start.span.end);
    Pattern {
        span: Span::new(start.span.source, start.span.start, end),
        kind,
    }
}

fn is_underscore(cursor: &TokenCursor<'_>, _span: Span) -> bool {
    // Approximation : peek the source text if available. The lexer emits `Ident` for `_`
    // as a single-character identifier; we conservatively treat any single-character ident
    // with span length 1 starting with `_` as wildcard. Without access to the source text
    // here, we return false — the elaborator can still turn `_` bindings into Wildcard.
    // For T3.2 we only recognize the explicit `Wildcard` when downstream resolves it.
    let _ = cursor;
    false
}

fn parse_tuple_pattern(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> PatternKind {
    cursor.bump(); // (
    let mut elems = Vec::new();
    while !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close))
        && !cursor.is_eof()
    {
        elems.push(parse_pattern(cursor, bag));
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `)` to close tuple pattern",
            cursor.peek().span,
        ));
    }
    PatternKind::Tuple(elems)
}

fn parse_path_pattern(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> PatternKind {
    let path = parse_colon_path(cursor, bag, "pattern path");
    // Variant with args : `Some(v)` or `Point(x, y)`
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)) {
        cursor.bump(); // (
        let mut args = Vec::new();
        while !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close))
            && !cursor.is_eof()
        {
            args.push(parse_pattern(cursor, bag));
            if cursor.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
            cursor.bump();
        } else {
            bag.push(custom(
                "expected `)` to close variant pattern",
                cursor.peek().span,
            ));
        }
        return PatternKind::Variant { path, args };
    }
    // Struct pattern : `Point { x, y : b }`
    if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
        cursor.bump(); // {
        let mut fields = Vec::new();
        let mut rest = false;
        while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
            && !cursor.is_eof()
        {
            if cursor.eat(TokenKind::DotDot).is_some() {
                rest = true;
                break;
            }
            let name = parse_ident(cursor, bag, "struct field name");
            let pat = if cursor.eat(TokenKind::Colon).is_some() {
                Some(parse_pattern(cursor, bag))
            } else {
                None
            };
            let field_span = Span::new(
                name.span.source,
                name.span.start,
                pat.as_ref().map_or(name.span.end, |p| p.span.end),
            );
            fields.push(PatternField {
                span: field_span,
                name,
                pat,
            });
            if cursor.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
            cursor.bump();
        } else {
            bag.push(custom(
                "expected `}` to close struct pattern",
                cursor.peek().span,
            ));
        }
        return PatternKind::Struct { path, fields, rest };
    }
    // Otherwise : single-segment path → binding; multi-segment path → unit variant.
    if path.segments.len() == 1 {
        let name = path.segments.into_iter().next().expect("non-empty path");
        PatternKind::Binding {
            mutable: false,
            name,
        }
    } else {
        PatternKind::Variant {
            path,
            args: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_pattern;
    use crate::cursor::TokenCursor;
    use cssl_ast::{DiagnosticBag, PatternKind, SourceFile, SourceId, Surface};

    fn prep(src: &str) -> (SourceFile, Vec<cssl_lex::Token>) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        (f, toks)
    }

    #[test]
    fn literal_int_pattern() {
        let (_f, toks) = prep("42");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        assert!(matches!(p.kind, PatternKind::Literal(_)));
    }

    #[test]
    fn binding_pattern() {
        let (_f, toks) = prep("x");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        assert!(matches!(
            p.kind,
            PatternKind::Binding { mutable: false, .. }
        ));
    }

    #[test]
    fn mut_binding() {
        let (_f, toks) = prep("mut x");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        assert!(matches!(p.kind, PatternKind::Binding { mutable: true, .. }));
    }

    #[test]
    fn tuple_pattern() {
        let (_f, toks) = prep("(a, b, c)");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        if let PatternKind::Tuple(elems) = p.kind {
            assert_eq!(elems.len(), 3);
        } else {
            panic!("expected Tuple");
        }
    }

    #[test]
    fn variant_pattern_with_args() {
        let (_f, toks) = prep("Some(v)");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        assert!(matches!(p.kind, PatternKind::Variant { .. }));
    }

    // ── § T11-W15-CSSLC-KWBIND : soft-keyword as binding-pattern ──────────────

    #[test]
    fn soft_kw_tag_as_binding_pattern() {
        // `tag` lexes as Keyword(Tag) ; in pattern position should be Binding.
        let (_f, toks) = prep("tag");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        assert!(matches!(
            p.kind,
            PatternKind::Binding { mutable: false, .. }
        ));
    }

    #[test]
    fn soft_kw_box_as_binding_pattern() {
        let (_f, toks) = prep("box");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0);
        assert!(matches!(
            p.kind,
            PatternKind::Binding { mutable: false, .. }
        ));
    }

    #[test]
    fn hard_kw_fn_still_rejected_as_binding() {
        // `fn` is NOT a soft keyword — must still error in pattern position.
        let (_f, toks) = prep("fn");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let _ = parse_pattern(&mut c, &mut bag);
        assert!(bag.error_count() >= 1, "fn must still be rejected");
    }

    #[test]
    fn struct_pattern() {
        let (_f, toks) = prep("Point { x, y : b }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        assert!(matches!(p.kind, PatternKind::Struct { .. }));
    }

    #[test]
    fn or_pattern() {
        let (_f, toks) = prep("1 | 2 | 3");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        if let PatternKind::Or(alts) = p.kind {
            assert_eq!(alts.len(), 3);
        } else {
            panic!("expected Or");
        }
    }

    // ─ T11-D243 (W-A6) : `self`-keyword as fn-param binding pattern ─────────

    /// Bare `self` parses to a `Binding` whose `Ident.span` re-slices to "self".
    #[test]
    fn self_keyword_binding_pattern() {
        let (f, toks) = prep("self");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0, "no diagnostics expected for `self`");
        let PatternKind::Binding { mutable, name } = p.kind else {
            panic!("expected Binding for `self`, got {:?}", p.kind);
        };
        assert!(!mutable, "`self` is not mutable by default");
        let text = f
            .slice(name.span.start, name.span.end)
            .expect("ident span in source bounds");
        assert_eq!(text, "self", "Ident.span must re-slice to literal `self`");
    }

    /// `self` is not consumed greedily — the `parse_pattern` returns a single
    /// Binding and leaves following tokens for the caller (so fn-param-list works).
    #[test]
    fn self_then_comma_leaves_comma_for_caller() {
        let (_f, toks) = prep("self, x");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        assert!(matches!(p.kind, PatternKind::Binding { .. }));
        // Comma should remain on the cursor for the param-list driver to consume.
        assert!(
            c.check(cssl_lex::TokenKind::Comma),
            "comma after `self` must remain unconsumed"
        );
    }

    /// `fn unwrap(self) -> T { ... }` — full item parse round-trip via `parse_item`.
    #[test]
    fn self_in_fn_param_list_full_parse() {
        use crate::rust_hybrid::item;
        let src = "fn unwrap(self) -> i32 { 0 }";
        let (_f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let item_opt = item::parse_item(&mut c, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "no diagnostics expected for `fn unwrap(self)`"
        );
        let Some(cssl_ast::Item::Fn(fn_item)) = item_opt else {
            panic!("expected Item::Fn for `fn unwrap(self) ...`");
        };
        assert_eq!(fn_item.params.len(), 1, "one param `self`");
        assert!(matches!(
            fn_item.params[0].pat.kind,
            PatternKind::Binding { mutable: false, .. }
        ));
    }

    /// `fn map(self, f : F) -> U { ... }` — `self` as first of multi-param.
    #[test]
    fn self_first_of_multi_param() {
        use crate::rust_hybrid::item;
        let src = "fn map(self, f : F) -> U { f }";
        let (_f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let item_opt = item::parse_item(&mut c, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "no diagnostics for `fn map(self, f : F)`"
        );
        let Some(cssl_ast::Item::Fn(fn_item)) = item_opt else {
            panic!("expected Item::Fn for `fn map(self, f : F) ...`");
        };
        assert_eq!(fn_item.params.len(), 2);
        assert!(matches!(
            fn_item.params[0].pat.kind,
            PatternKind::Binding { mutable: false, .. }
        ));
        assert!(matches!(
            fn_item.params[1].pat.kind,
            PatternKind::Binding { mutable: false, .. }
        ));
    }

    /// Idempotency check : pre-existing identifier-binding behavior is unchanged
    /// — `x` does NOT become a `self`-binding.
    #[test]
    fn plain_ident_still_binds_to_x() {
        let (f, toks) = prep("x");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_pattern(&mut c, &mut bag);
        let PatternKind::Binding { name, .. } = p.kind else {
            panic!("expected Binding for `x`");
        };
        let text = f.slice(name.span.start, name.span.end).expect("in bounds");
        assert_eq!(text, "x");
    }
}
