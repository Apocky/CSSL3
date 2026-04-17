//! Attribute parsing — outer `@name(args)` + inner `#![name = …]`.
//!
//! § SPEC : `specs/09_SYNTAX.csl` § ATTRIBUTE-BODY DSL.
//!
//! § OUTER FORM : `@path` or `@path(arg1, key = value, …)`
//! § INNER FORM : `#![path = "value"]` or `#![path(args)]`
//!
//! Args may be literals or expressions. At CST level both resolve via `AttrArg::Positional`
//! (for positional expressions) or `AttrArg::Named { name, value }` (for `key = value` pairs).

use cssl_ast::{Attr, AttrArg, AttrKind, DiagnosticBag, Span};
use cssl_lex::{BracketKind, BracketSide, TokenKind};

use crate::common::{parse_ident, parse_module_path};
use crate::cursor::TokenCursor;
use crate::error::custom;
use crate::rust_hybrid::expr;

/// Parse an outer attribute `@name(args)`. Returns `None` if the current token is not `@`.
#[must_use]
pub fn parse_outer(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Option<Attr> {
    let at = cursor.peek();
    if at.kind != TokenKind::At {
        return None;
    }
    cursor.bump();
    let path = parse_module_path(cursor, bag, "attribute name");
    let args = if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)) {
        parse_attr_args(cursor, bag)
    } else {
        Vec::new()
    };
    let end = args.last().map_or(path.span.end, |a| attr_arg_span(a).end);
    Some(Attr {
        span: Span::new(at.span.source, at.span.start, end),
        kind: AttrKind::Outer,
        path,
        args,
    })
}

/// Parse an inner attribute `#![path = "value"]` or `#![path(args)]`.
/// Returns `None` if the current tokens are not `#!`.
#[must_use]
pub fn parse_inner(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Option<Attr> {
    let hash = cursor.peek();
    if hash.kind != TokenKind::Hash {
        return None;
    }
    let bang_tok = cursor.peek2();
    if bang_tok.kind != TokenKind::Bang {
        return None;
    }
    cursor.bump(); // #
    cursor.bump(); // !
    let open = cursor.peek();
    if open.kind != TokenKind::Bracket(BracketKind::Square, BracketSide::Open) {
        bag.push(custom("expected `[` after `#!`", open.span));
        return None;
    }
    cursor.bump(); // [
    let path = parse_module_path(cursor, bag, "inner attribute path");
    let mut args = Vec::new();
    // Accept either `= expr` (sugar for single-positional) or `(arg, …)` argument list.
    if cursor.check(TokenKind::Eq) {
        cursor.bump();
        let e = expr::parse_expr(cursor, bag);
        args.push(AttrArg::Positional(e));
    } else if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)) {
        args = parse_attr_args(cursor, bag);
    }
    let close = cursor.peek();
    if close.kind == TokenKind::Bracket(BracketKind::Square, BracketSide::Close) {
        cursor.bump();
    } else {
        bag.push(custom("expected `]` to close inner attribute", close.span));
    }
    let end = close.span.end;
    Some(Attr {
        span: Span::new(hash.span.source, hash.span.start, end),
        kind: AttrKind::Inner,
        path,
        args,
    })
}

/// Parse a comma-separated `( arg, … )` argument list for an attribute.
/// Each arg is either `expr` (positional) or `name = expr` (named).
fn parse_attr_args(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Vec<AttrArg> {
    cursor.bump(); // consume `(`
    let mut args = Vec::new();
    while !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close))
        && !cursor.is_eof()
    {
        // Peek for a named-arg form : `ident =`.
        if cursor.peek().kind == TokenKind::Ident && cursor.peek2().kind == TokenKind::Eq {
            let name = parse_ident(cursor, bag, "attribute arg name");
            cursor.bump(); // =
            let value = expr::parse_expr(cursor, bag);
            args.push(AttrArg::Named { name, value });
        } else {
            let e = expr::parse_expr(cursor, bag);
            args.push(AttrArg::Positional(e));
        }
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    // Consume closing `)` if present.
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `)` to close attribute arguments",
            cursor.peek().span,
        ));
    }
    args
}

fn attr_arg_span(arg: &AttrArg) -> Span {
    match arg {
        AttrArg::Positional(e) => e.span,
        AttrArg::Named { name, value } => {
            Span::new(name.span.source, name.span.start, value.span.end)
        }
    }
}

/// Parse zero-or-more outer attributes in sequence. Stops at the first non-`@` token.
#[must_use]
pub fn parse_outer_attrs(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Vec<Attr> {
    let mut out = Vec::new();
    while let Some(a) = parse_outer(cursor, bag) {
        out.push(a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{parse_inner, parse_outer, parse_outer_attrs};
    use cssl_ast::{AttrKind, DiagnosticBag, SourceFile, SourceId, Surface};

    use crate::cursor::TokenCursor;

    fn prep(src: &str) -> (SourceFile, Vec<cssl_lex::Token>) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        (f, toks)
    }

    #[test]
    fn outer_attribute_bare() {
        let (_f, toks) = prep("@differentiable");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let a = parse_outer(&mut c, &mut bag).unwrap();
        assert_eq!(a.kind, AttrKind::Outer);
        assert_eq!(a.path.segments.len(), 1);
        assert!(a.args.is_empty());
    }

    #[test]
    fn outer_attribute_with_args() {
        let (_f, toks) = prep("@lipschitz(k = 1.0)");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let a = parse_outer(&mut c, &mut bag).unwrap();
        assert_eq!(a.args.len(), 1);
    }

    #[test]
    fn inner_attribute_key_value() {
        let (_f, toks) = prep("#![surface = \"csl\"]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let a = parse_inner(&mut c, &mut bag).unwrap();
        assert_eq!(a.kind, AttrKind::Inner);
        assert_eq!(a.args.len(), 1);
    }

    #[test]
    fn outer_attrs_stops_at_non_at() {
        let (_f, toks) = prep("@a @b fn");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let attrs = parse_outer_attrs(&mut c, &mut bag);
        assert_eq!(attrs.len(), 2);
    }
}
