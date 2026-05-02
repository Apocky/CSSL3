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

/// Parse an outer attribute. Two forms accepted :
///   1. `@name(args)`        — CSLv3-native sigil-form
///   2. `#[name]` / `#[name(args)]` / `#[name = expr]` — Rust-hybrid bracket-form
///      (matches `#[test]` / `#[derive(...)]` / `#[cfg(target = "x")]`)
///
/// § T11-W15-CSSLC-TESTATTR : real-world LoA source uses `#[test]` for inline-
///   test functions per Rust conventions ; the `@` form is reserved for CSLv3-
///   native effect/handler decorators. Both surface forms produce the same
///   `AttrKind::Outer` AST node.
///
/// Returns `None` if the current token is neither `@` nor `#[` (a `#` followed
/// by `!` is the inner-attr form `#![...]` ; that's handled by `parse_inner`).
#[must_use]
pub fn parse_outer(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Option<Attr> {
    let head = cursor.peek();
    if head.kind == TokenKind::At {
        return parse_outer_at(cursor, bag);
    }
    if head.kind == TokenKind::Hash
        && cursor.peek2().kind == TokenKind::Bracket(BracketKind::Square, BracketSide::Open)
    {
        return parse_outer_hash(cursor, bag);
    }
    None
}

/// Parse `@name(args)` outer-attribute form.
fn parse_outer_at(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Option<Attr> {
    let at = cursor.peek();
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

/// § T11-W15-CSSLC-TESTATTR : Parse `#[name]` outer-attribute form.
///
/// Forms accepted :
///   `#[test]`                   — bare name
///   `#[derive(Foo, Bar)]`       — name + arg-list
///   `#[cfg(target = "linux")]`  — name + key=value-list
///   `#[doc = "..."]`            — name = expr
///
/// Reuses `parse_attr_args` for the arg-list shape ; the AST node is identical
/// to the `@`-form so HIR/MIR don't need to know which surface was used.
fn parse_outer_hash(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Option<Attr> {
    let hash = cursor.peek();
    cursor.bump(); // #
    cursor.bump(); // [
    let path = parse_module_path(cursor, bag, "attribute name");
    let args = if cursor.check(TokenKind::Eq) {
        cursor.bump();
        let e = expr::parse_expr(cursor, bag);
        vec![AttrArg::Positional(e)]
    } else if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)) {
        parse_attr_args(cursor, bag)
    } else {
        Vec::new()
    };
    let close = cursor.peek();
    let end = if close.kind == TokenKind::Bracket(BracketKind::Square, BracketSide::Close) {
        cursor.bump();
        close.span.end
    } else {
        bag.push(custom("expected `]` to close outer attribute", close.span));
        args.last().map_or(path.span.end, |a| attr_arg_span(a).end)
    };
    Some(Attr {
        span: Span::new(hash.span.source, hash.span.start, end),
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
    use cssl_ast::{
        AttrArg, AttrKind, DiagnosticBag, ExprKind, Item, LiteralKind, SourceFile, SourceId, Span,
        StructBody, Surface, VisibilityKind,
    };

    use crate::cursor::TokenCursor;
    use crate::rust_hybrid::parse_module;

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

    // ── § T11-W15-CSSLC-TESTATTR : `#[name]` outer-attribute form ─────────────

    #[test]
    fn parse_hash_test_attr_bare() {
        // `#[test]` — bare-name form ; most-common case (rust-style inline test).
        let (_f, toks) = prep("#[test]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let a = parse_outer(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        assert_eq!(a.kind, AttrKind::Outer);
        assert_eq!(a.path.segments.len(), 1);
        assert!(a.args.is_empty());
    }

    #[test]
    fn parse_hash_attr_with_args() {
        // `#[derive(Foo, Bar)]` — name + paren-arg-list.
        let (_f, toks) = prep("#[derive(Foo, Bar)]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let a = parse_outer(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        assert_eq!(a.kind, AttrKind::Outer);
        assert_eq!(a.args.len(), 2);
    }

    #[test]
    fn parse_hash_attr_with_eq_value() {
        // `#[doc = "summary"]` — name = value form.
        let (_f, toks) = prep("#[doc = \"summary\"]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let a = parse_outer(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        assert_eq!(a.kind, AttrKind::Outer);
        assert_eq!(a.args.len(), 1);
    }

    #[test]
    fn parse_hash_test_attr_followed_by_fn_in_module() {
        // End-to-end : `#[test] fn foo() { 0 }` parses as fn-item with attached attr.
        let src = "#[test]\nfn t1() -> u32 { 0u32 }\n";
        let (f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0);
        assert_eq!(m.items.len(), 1);
        if let Item::Fn(fn_item) = &m.items[0] {
            assert_eq!(fn_item.attrs.len(), 1);
            assert_eq!(fn_item.attrs[0].kind, AttrKind::Outer);
        } else {
            panic!("expected fn item");
        }
    }

    #[test]
    fn parse_mixed_at_and_hash_attrs() {
        // Both surface-forms can co-exist on the same item.
        let src = "@vertex\n#[test]\nfn shader() -> i32 { 0 }\n";
        let (f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0);
        if let Item::Fn(fn_item) = &m.items[0] {
            assert_eq!(fn_item.attrs.len(), 2);
        } else {
            panic!("expected fn item");
        }
    }

    // ── T11-CC-PARSER-2 mission-required tests ───────────────────────────
    // Cover the LoA-scene attribute surface (`@vertex` / `@fragment` /
    // `@compute` / `@layout(std430|packed)` / `@workgroup_size(x,y,z)` /
    // `@test`) end-to-end through `parse_module`, asserting each attribute
    // attaches to its item and is preserved in the CST `attrs` field for
    // downstream HIR / codegen / shader-emit consumers.

    fn span_text(src: &SourceFile, span: Span) -> &str {
        src.slice(span.start, span.end).unwrap_or("")
    }

    fn first_arg_path_segment_zero<'a>(arg: &'a AttrArg, src: &'a SourceFile) -> &'a str {
        match arg {
            AttrArg::Positional(e) => match &e.kind {
                ExprKind::Path(p) => p.segments.first().map_or("", |s| span_text(src, s.span)),
                _ => "",
            },
            AttrArg::Named { .. } => "",
        }
    }

    fn first_arg_int_text<'a>(arg: &'a AttrArg, src: &'a SourceFile) -> Option<&'a str> {
        match arg {
            AttrArg::Positional(e) => match &e.kind {
                ExprKind::Literal(lit) if matches!(lit.kind, LiteralKind::Int) => {
                    Some(span_text(src, lit.span))
                }
                _ => None,
            },
            AttrArg::Named { .. } => None,
        }
    }

    #[test]
    fn parse_at_vertex_on_fn() {
        let src = "@vertex\nfn vs_main(vid: u32) -> i32 { 42 }\n";
        let (f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "parse errors > 0");
        assert_eq!(m.items.len(), 1);
        let Item::Fn(fn_item) = &m.items[0] else {
            panic!("expected Item::Fn");
        };
        assert_eq!(fn_item.attrs.len(), 1);
        assert_eq!(fn_item.attrs[0].kind, AttrKind::Outer);
        assert_eq!(fn_item.attrs[0].path.segments.len(), 1);
        assert_eq!(
            span_text(&f, fn_item.attrs[0].path.segments[0].span),
            "vertex"
        );
        assert!(fn_item.attrs[0].args.is_empty());
        assert_eq!(span_text(&f, fn_item.name.span), "vs_main");
    }

    #[test]
    fn parse_at_layout_std430_on_struct() {
        let src = "@layout(std430)\nstruct FieldCell {\n    morton: u64,\n    energy: f32,\n}\n";
        let (f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "parse errors > 0");
        assert_eq!(m.items.len(), 1);
        let Item::Struct(s) = &m.items[0] else {
            panic!("expected Item::Struct");
        };
        assert_eq!(s.attrs.len(), 1);
        assert_eq!(span_text(&f, s.attrs[0].path.segments[0].span), "layout");
        assert_eq!(s.attrs[0].args.len(), 1);
        assert_eq!(
            first_arg_path_segment_zero(&s.attrs[0].args[0], &f),
            "std430"
        );
        match &s.body {
            StructBody::Named(fields) => assert_eq!(fields.len(), 2),
            _ => panic!("expected Named struct body"),
        }
    }

    #[test]
    fn parse_multiple_attributes_stack() {
        // Each attribute starts on its own line per the LoA shader convention.
        let src = "@compute\n@workgroup_size(64, 1, 1)\nfn cs_step(id: u32) -> i32 { 0 }\n";
        let (f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "parse errors > 0");
        assert_eq!(m.items.len(), 1);
        let Item::Fn(fn_item) = &m.items[0] else {
            panic!("expected Item::Fn");
        };
        assert_eq!(
            fn_item.attrs.len(),
            2,
            "expected stacked @compute + @workgroup_size"
        );
        assert_eq!(
            span_text(&f, fn_item.attrs[0].path.segments[0].span),
            "compute"
        );
        assert!(fn_item.attrs[0].args.is_empty());
        assert_eq!(
            span_text(&f, fn_item.attrs[1].path.segments[0].span),
            "workgroup_size"
        );
        assert_eq!(fn_item.attrs[1].args.len(), 3);
    }

    #[test]
    fn parse_at_test_on_fn() {
        let src = "@test\nfn test_field_cell_size() { 42 }\n";
        let (f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "parse errors > 0");
        let Item::Fn(fn_item) = &m.items[0] else {
            panic!("expected Item::Fn");
        };
        assert_eq!(fn_item.attrs.len(), 1);
        assert_eq!(
            span_text(&f, fn_item.attrs[0].path.segments[0].span),
            "test"
        );
        assert!(
            fn_item.attrs[0].args.is_empty(),
            "@test takes no arguments"
        );
    }

    #[test]
    fn parse_attribute_with_int_args() {
        let src = "@workgroup_size(64, 1, 1)\nfn cs_step(id: u32) -> i32 { 0 }\n";
        let (f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "parse errors > 0");
        let Item::Fn(fn_item) = &m.items[0] else {
            panic!("expected Item::Fn");
        };
        assert_eq!(fn_item.attrs.len(), 1);
        let attr = &fn_item.attrs[0];
        assert_eq!(span_text(&f, attr.path.segments[0].span), "workgroup_size");
        assert_eq!(attr.args.len(), 3);
        assert_eq!(first_arg_int_text(&attr.args[0], &f), Some("64"));
        assert_eq!(first_arg_int_text(&attr.args[1], &f), Some("1"));
        assert_eq!(first_arg_int_text(&attr.args[2], &f), Some("1"));
    }

    #[test]
    fn parse_at_layout_with_ident_arg() {
        // `@layout(packed)` — single positional ident argument (no `=` value).
        let src = "@layout(packed)\nstruct Foo {\n    morton: u64,\n}\n";
        let (f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "parse errors > 0");
        let Item::Struct(s) = &m.items[0] else {
            panic!("expected Item::Struct");
        };
        assert_eq!(s.attrs.len(), 1);
        assert_eq!(s.attrs[0].args.len(), 1);
        assert_eq!(
            first_arg_path_segment_zero(&s.attrs[0].args[0], &f),
            "packed"
        );
    }

    #[test]
    fn at_attribute_followed_by_pub_keyword() {
        // Regression : `@vertex` must attach to the `pub fn …` that follows it
        // (the `pub` visibility comes between `@attr` and `fn`).
        let src = "@vertex\npub fn vs_main(vid: u32) -> i32 { vid }\n";
        let (f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let m = parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "parse errors > 0");
        assert_eq!(m.items.len(), 1);
        let Item::Fn(fn_item) = &m.items[0] else {
            panic!("expected Item::Fn");
        };
        assert_eq!(fn_item.attrs.len(), 1);
        assert_eq!(
            span_text(&f, fn_item.attrs[0].path.segments[0].span),
            "vertex"
        );
        assert_eq!(fn_item.visibility.kind, VisibilityKind::Public);
    }
}
