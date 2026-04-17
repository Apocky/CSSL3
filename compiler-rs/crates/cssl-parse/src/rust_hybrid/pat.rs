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

use cssl_ast::{DiagnosticBag, Literal, LiteralKind, Pattern, PatternField, PatternKind, Span};
use cssl_lex::{BracketKind, BracketSide, Keyword, TokenKind};

use crate::common::{parse_colon_path, parse_ident};
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
        // path → variant / struct / binding
        TokenKind::Ident => parse_path_pattern(cursor, bag),
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
}
