//! Generics + where-clauses.
//!
//! § SPEC : `specs/09_SYNTAX.csl` § item-level (fn-def shows `where T : Additive`).
//!
//! § FORMS
//!   - `<T>` / `<T : Bound>` / `<T, U : Bound1 + Bound2>` / `<const N : usize>` / `<'r>`
//!   - `where T : Bound, U : Bound1 + Bound2`

use cssl_ast::{DiagnosticBag, GenericParam, GenericParamKind, Generics, Span, WhereClause};
use cssl_lex::{Keyword, TokenKind};

use crate::common::parse_ident;
use crate::cursor::TokenCursor;
use crate::error::custom;
use crate::rust_hybrid::ty;

/// Parse an optional `<…>` generics list. Returns `Generics::default()` if absent.
#[must_use]
pub fn parse_generics(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Generics {
    if !cursor.check(TokenKind::Lt) {
        return Generics::default();
    }
    let open = cursor.bump();
    let mut params = Vec::new();
    while !cursor.check(TokenKind::Gt) && !cursor.is_eof() {
        let p = parse_generic_param(cursor, bag);
        params.push(p);
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    let close = cursor.peek();
    let span = if cursor.check(TokenKind::Gt) {
        cursor.bump();
        Some(Span::new(open.span.source, open.span.start, close.span.end))
    } else {
        bag.push(custom("expected `>` to close generics", cursor.peek().span));
        Some(Span::new(
            open.span.source,
            open.span.start,
            close.span.start,
        ))
    };
    Generics { span, params }
}

fn parse_generic_param(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> GenericParam {
    let start_span = cursor.peek().span;
    let kind;
    let name;
    // `const N : T` → const-param
    if cursor.check(TokenKind::Keyword(Keyword::Const)) {
        cursor.bump();
        kind = GenericParamKind::Const;
        name = parse_ident(cursor, bag, "const generic name");
    // `'r` → region / lifetime
    } else if cursor.check(TokenKind::Apostrophe) {
        cursor.bump();
        kind = GenericParamKind::Region;
        name = parse_ident(cursor, bag, "region name");
    } else {
        kind = GenericParamKind::Type;
        name = parse_ident(cursor, bag, "type-parameter name");
    }
    // Optional `: Bound1 + Bound2`
    let mut bounds = Vec::new();
    if cursor.eat(TokenKind::Colon).is_some() {
        loop {
            let b = ty::parse_type(cursor, bag);
            bounds.push(b);
            if cursor.eat(TokenKind::Plus).is_none() {
                break;
            }
        }
    }
    // Optional `= Default`
    let default = if cursor.eat(TokenKind::Eq).is_some() {
        Some(ty::parse_type(cursor, bag))
    } else {
        None
    };
    // For const-params we need an explicit `: Type` — if `bounds` is empty we synthesize
    // an `Infer` type and record the omission as a diagnostic downstream. For T3 scope we
    // keep the CST shape uniform and defer const-typing validation to the elaborator.
    let end_span = default
        .as_ref()
        .map(|t| t.span)
        .or_else(|| bounds.last().map(|t| t.span))
        .unwrap_or(name.span);
    GenericParam {
        span: Span::new(start_span.source, start_span.start, end_span.end),
        name,
        kind,
        bounds,
        default,
    }
}

/// Parse an optional `where … ,` clause list. Returns `Vec::new()` if absent.
#[must_use]
pub fn parse_where_clauses(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
) -> Vec<WhereClause> {
    if !cursor.check(TokenKind::Keyword(Keyword::Where)) {
        return Vec::new();
    }
    cursor.bump(); // where
    let mut clauses = Vec::new();
    loop {
        let ty = ty::parse_type(cursor, bag);
        let colon = cursor.peek();
        if colon.kind != TokenKind::Colon {
            bag.push(custom(
                "expected `:` after where-clause subject",
                colon.span,
            ));
            break;
        }
        cursor.bump(); // :
        let mut bounds = Vec::new();
        loop {
            let b = ty::parse_type(cursor, bag);
            bounds.push(b);
            if cursor.eat(TokenKind::Plus).is_none() {
                break;
            }
        }
        let span_start = ty.span.start;
        let span_end = bounds.last().map_or(ty.span.end, |b| b.span.end);
        clauses.push(WhereClause {
            span: Span::new(ty.span.source, span_start, span_end),
            ty,
            bounds,
        });
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    clauses
}

#[cfg(test)]
mod tests {
    use super::{parse_generics, parse_where_clauses};
    use crate::cursor::TokenCursor;
    use cssl_ast::{DiagnosticBag, GenericParamKind, SourceFile, SourceId, Surface};

    fn prep(src: &str) -> (SourceFile, Vec<cssl_lex::Token>) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        (f, toks)
    }

    #[test]
    fn absent_generics_default() {
        let (_f, toks) = prep("foo");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let g = parse_generics(&mut c, &mut bag);
        assert!(g.params.is_empty());
    }

    #[test]
    fn single_type_param() {
        let (_f, toks) = prep("<T>");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let g = parse_generics(&mut c, &mut bag);
        assert_eq!(g.params.len(), 1);
        assert_eq!(g.params[0].kind, GenericParamKind::Type);
    }

    #[test]
    fn type_param_with_bound() {
        let (_f, toks) = prep("<T : Bound>");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let g = parse_generics(&mut c, &mut bag);
        assert_eq!(g.params.len(), 1);
        assert_eq!(g.params[0].bounds.len(), 1);
    }

    #[test]
    fn multiple_type_params() {
        let (_f, toks) = prep("<T : A + B, U>");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let g = parse_generics(&mut c, &mut bag);
        assert_eq!(g.params.len(), 2);
        assert_eq!(g.params[0].bounds.len(), 2);
    }

    #[test]
    fn where_clause_single() {
        let (_f, toks) = prep("where T : Additive");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let w = parse_where_clauses(&mut c, &mut bag);
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn absent_where_empty() {
        let (_f, toks) = prep("{");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let w = parse_where_clauses(&mut c, &mut bag);
        assert!(w.is_empty());
    }
}
