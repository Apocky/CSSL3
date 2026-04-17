//! Type-expression parser.
//!
//! § SPEC : `specs/09_SYNTAX.csl` § type-annotations + `specs/03_TYPES.csl`.
//!
//! § COVERED
//!   - Path : `T` / `mod::T<A, B>`
//!   - Tuple : `(T, U, V)` (arity 0 → unit)
//!   - Array : `[T ; N]`
//!   - Slice : `[T]`
//!   - Reference : `&T` / `&mut T`
//!   - Capability : `iso<T>` / `val<T>` / `ref<T>` / `trn<T>` / `box<T>` / `tag<T>`
//!   - Function : `fn(T1, …) -> U / ε`
//!   - Refined : `T'tag` (surface sugar) or `{v : T | P(v)}` (full form)
//!   - Infer : `_`
//!
//! § NOT-YET (deferred past T3.2 proper)
//!   - Effect-row polymorphism tail (`μ` variable in rows) — partially covered by
//!     `EffectRow::tail : Option<Ident>`; parser records it but elaborator enforces shape.

use cssl_ast::{
    CapKind, DiagnosticBag, EffectAnnotation, EffectArg, EffectRow, Ident, RefinementKind, Span,
    Type, TypeKind,
};
use cssl_lex::{BracketKind, BracketSide, Keyword, TokenKind};

use crate::common::{parse_ident, parse_module_path};
use crate::cursor::TokenCursor;
use crate::error::{custom, expected_any};
use crate::rust_hybrid::expr;

/// Parse a single type expression.
#[must_use]
pub fn parse_type(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Type {
    let start = cursor.peek().span;
    let kind = parse_type_kind(cursor, bag);
    let end = cursor.peek().span.start;
    let span = Span::new(start.source, start.start, end.max(start.end));

    // Post-process : `T'tag` refinement-sugar.
    let mut ty = Type { span, kind };
    while cursor.check(TokenKind::Apostrophe) {
        let ap = cursor.bump();
        let tag_name = parse_ident(cursor, bag, "refinement tag after `'`");
        let end_off = tag_name.span.end;
        // Detect the `SDF'L<k>` Lipschitz-bound form : tag name is `L` and next token `<`.
        let kind = if cursor.check(TokenKind::Lt) {
            cursor.bump(); // <
            let bound = expr::parse_expr(cursor, bag);
            if cursor.check(TokenKind::Gt) {
                cursor.bump();
            } else {
                bag.push(custom(
                    "expected `>` to close Lipschitz bound",
                    cursor.peek().span,
                ));
            }
            RefinementKind::Lipschitz {
                bound: Box::new(bound),
            }
        } else {
            RefinementKind::Tag { name: tag_name }
        };
        ty = Type {
            span: Span::new(ty.span.source, ty.span.start, end_off.max(ap.span.end)),
            kind: TypeKind::Refined {
                base: Box::new(ty),
                kind,
            },
        };
    }
    ty
}

fn parse_type_kind(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> TypeKind {
    let t = cursor.peek();
    match t.kind {
        // `_` → inferred type
        TokenKind::Ident => {
            // Check if the identifier text is underscore via span — for now the parser
            // treats `_` identifier as a regular Path. The lexer currently lexes `_` as
            // Ident; elaborator may promote it to `Infer`. We choose the conservative path
            // here and let elaboration decide.
            parse_path_or_capability(cursor, bag)
        }
        // `(` → tuple or parenthesized type
        TokenKind::Bracket(BracketKind::Paren, BracketSide::Open) => {
            parse_tuple_or_paren(cursor, bag)
        }
        // `[` → array or slice
        TokenKind::Bracket(BracketKind::Square, BracketSide::Open) => {
            parse_array_or_slice(cursor, bag)
        }
        // `{` → refined-type predicate form `{v : T | P(v)}`
        TokenKind::Bracket(BracketKind::Brace, BracketSide::Open) => {
            parse_refined_predicate(cursor, bag)
        }
        // `&` → reference
        TokenKind::Amp => parse_reference(cursor, bag),
        // `fn` → function type
        TokenKind::Keyword(Keyword::Fn) => parse_fn_type(cursor, bag),
        // capability keywords `iso`, `trn`, `ref`, `val`, `box`, `tag`
        TokenKind::Keyword(
            Keyword::Iso | Keyword::Trn | Keyword::Ref | Keyword::Val | Keyword::Box | Keyword::Tag,
        ) => parse_capability(cursor, bag),
        _ => {
            bag.push(expected_any(vec![TokenKind::Ident], t.kind, t.span, "type"));
            // Consume one token to make progress and return Infer placeholder.
            cursor.bump();
            TypeKind::Infer
        }
    }
}

fn parse_path_or_capability(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> TypeKind {
    let path = parse_module_path(cursor, bag, "type name");
    // Optional `<args>`
    let type_args = if cursor.check(TokenKind::Lt) {
        parse_type_arg_list(cursor, bag)
    } else {
        Vec::new()
    };
    TypeKind::Path { path, type_args }
}

fn parse_tuple_or_paren(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> TypeKind {
    cursor.bump(); // (
    let mut elems = Vec::new();
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
        cursor.bump();
        return TypeKind::Tuple { elems };
    }
    loop {
        let t = parse_type(cursor, bag);
        elems.push(t);
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `)` to close tuple type",
            cursor.peek().span,
        ));
    }
    // Arity 1 without trailing comma → parenthesized type, not tuple.
    if elems.len() == 1 {
        return elems.pop().expect("len 1").kind;
    }
    TypeKind::Tuple { elems }
}

fn parse_array_or_slice(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> TypeKind {
    cursor.bump(); // [
    let elem = parse_type(cursor, bag);
    if cursor.eat(TokenKind::Semi).is_some() {
        // [T ; N]
        let len = expr::parse_expr(cursor, bag);
        if cursor.check(TokenKind::Bracket(BracketKind::Square, BracketSide::Close)) {
            cursor.bump();
        } else {
            bag.push(custom(
                "expected `]` to close array type",
                cursor.peek().span,
            ));
        }
        return TypeKind::Array {
            elem: Box::new(elem),
            len: Box::new(len),
        };
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Square, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `]` to close slice type",
            cursor.peek().span,
        ));
    }
    TypeKind::Slice {
        elem: Box::new(elem),
    }
}

fn parse_refined_predicate(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> TypeKind {
    cursor.bump(); // {
    let binder = parse_ident(cursor, bag, "refinement binder");
    if cursor.check(TokenKind::Colon) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `:` after refinement binder",
            cursor.peek().span,
        ));
    }
    let base = parse_type(cursor, bag);
    if cursor.check(TokenKind::Pipe) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `|` before refinement predicate",
            cursor.peek().span,
        ));
    }
    let predicate = expr::parse_expr(cursor, bag);
    if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `}` to close refinement type",
            cursor.peek().span,
        ));
    }
    TypeKind::Refined {
        base: Box::new(base),
        kind: RefinementKind::Predicate {
            binder,
            predicate: Box::new(predicate),
        },
    }
}

fn parse_reference(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> TypeKind {
    cursor.bump(); // &
    let mutable = cursor.eat(TokenKind::Keyword(Keyword::Mut)).is_some();
    let inner = parse_type(cursor, bag);
    TypeKind::Reference {
        mutable,
        inner: Box::new(inner),
    }
}

fn parse_fn_type(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> TypeKind {
    cursor.bump(); // fn
    if !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)) {
        bag.push(custom(
            "expected `(` after `fn` in type",
            cursor.peek().span,
        ));
        return TypeKind::Infer;
    }
    cursor.bump(); // (
    let mut params = Vec::new();
    while !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close))
        && !cursor.is_eof()
    {
        params.push(parse_type(cursor, bag));
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
        cursor.bump();
    }
    let return_ty = if cursor.eat(TokenKind::Arrow).is_some() {
        parse_type(cursor, bag)
    } else {
        // Default : `()`
        Type {
            span: cursor.peek().span,
            kind: TypeKind::Tuple { elems: Vec::new() },
        }
    };
    let effect_row = parse_optional_effect_row(cursor, bag);
    TypeKind::Function {
        params,
        return_ty: Box::new(return_ty),
        effect_row,
    }
}

fn parse_capability(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> TypeKind {
    let kw = cursor.bump();
    let cap = match kw.kind {
        TokenKind::Keyword(Keyword::Iso) => CapKind::Iso,
        TokenKind::Keyword(Keyword::Trn) => CapKind::Trn,
        TokenKind::Keyword(Keyword::Ref) => CapKind::Ref,
        TokenKind::Keyword(Keyword::Val) => CapKind::Val,
        TokenKind::Keyword(Keyword::Box) => CapKind::Box,
        TokenKind::Keyword(Keyword::Tag) => CapKind::Tag,
        _ => unreachable!("parse_capability called on non-capability token"),
    };
    // Capability form : `iso<T>` ← require `<` + inner type + `>`
    if cursor.eat(TokenKind::Lt).is_none() {
        bag.push(custom(
            "expected `<` after capability keyword",
            cursor.peek().span,
        ));
        return TypeKind::Infer;
    }
    let inner = parse_type(cursor, bag);
    if cursor.eat(TokenKind::Gt).is_none() {
        bag.push(custom(
            "expected `>` to close capability",
            cursor.peek().span,
        ));
    }
    TypeKind::Capability {
        cap,
        inner: Box::new(inner),
    }
}

fn parse_type_arg_list(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Vec<Type> {
    cursor.bump(); // <
    let mut args = Vec::new();
    while !cursor.check(TokenKind::Gt) && !cursor.is_eof() {
        let t = parse_type(cursor, bag);
        args.push(t);
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    if cursor.check(TokenKind::Gt) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `>` to close type-argument list",
            cursor.peek().span,
        ));
    }
    args
}

/// Parse an optional effect row `/ { e1, e2<arg>, … }` or `/ ε`.
#[must_use]
pub fn parse_optional_effect_row(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
) -> Option<EffectRow> {
    if cursor.peek().kind != TokenKind::Slash {
        return None;
    }
    let slash = cursor.bump();
    let open = cursor.peek();
    // Accept `ε` (epsilon) as sugar for empty row — emitted as Ident by the lexer.
    if open.kind == TokenKind::Ident {
        // Consume single identifier as pure-empty shorthand.
        cursor.bump();
        return Some(EffectRow {
            span: Span::new(slash.span.source, slash.span.start, open.span.end),
            effects: Vec::new(),
            tail: None,
        });
    }
    if open.kind != TokenKind::Bracket(BracketKind::Brace, BracketSide::Open) {
        bag.push(custom(
            "expected `{` or `ε` after `/` in effect row",
            open.span,
        ));
        return None;
    }
    cursor.bump(); // {
    let mut effects = Vec::new();
    let mut tail: Option<Ident> = None;
    while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
        && !cursor.is_eof()
    {
        // Polymorphic tail `...μ`
        if cursor.check(TokenKind::DotDot) {
            cursor.bump();
            tail = Some(parse_ident(cursor, bag, "effect-row tail"));
            break;
        }
        let ann = parse_effect_annotation(cursor, bag);
        effects.push(ann);
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    let close = cursor.peek();
    if close.kind == TokenKind::Bracket(BracketKind::Brace, BracketSide::Close) {
        cursor.bump();
    } else {
        bag.push(custom("expected `}` to close effect row", close.span));
    }
    Some(EffectRow {
        span: Span::new(slash.span.source, slash.span.start, close.span.end),
        effects,
        tail,
    })
}

fn parse_effect_annotation(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
) -> EffectAnnotation {
    let name = parse_module_path(cursor, bag, "effect name");
    let mut args = Vec::new();
    if cursor.eat(TokenKind::Lt).is_some() {
        while !cursor.check(TokenKind::Gt) && !cursor.is_eof() {
            // Best-effort : try type first; fall back to expr.
            let arg = if looks_like_expr_start(cursor) {
                EffectArg::Expr(expr::parse_expr(cursor, bag))
            } else {
                EffectArg::Type(parse_type(cursor, bag))
            };
            args.push(arg);
            if cursor.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        if cursor.check(TokenKind::Gt) {
            cursor.bump();
        } else {
            bag.push(custom(
                "expected `>` to close effect arguments",
                cursor.peek().span,
            ));
        }
    }
    let end = args
        .last()
        .map_or(name.span.end, |_| cursor.peek().span.start);
    EffectAnnotation {
        span: Span::new(name.span.source, name.span.start, end),
        name,
        args,
    }
}

fn looks_like_expr_start(cursor: &TokenCursor<'_>) -> bool {
    matches!(
        cursor.peek().kind,
        TokenKind::IntLiteral | TokenKind::FloatLiteral | TokenKind::StringLiteral(_)
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_optional_effect_row, parse_type};
    use crate::cursor::TokenCursor;
    use cssl_ast::{CapKind, DiagnosticBag, SourceFile, SourceId, Surface, TypeKind};

    fn prep(src: &str) -> (SourceFile, Vec<cssl_lex::Token>) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        (f, toks)
    }

    #[test]
    fn path_type() {
        let (_f, toks) = prep("i32");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        assert!(matches!(t.kind, TypeKind::Path { .. }));
    }

    #[test]
    fn generic_type() {
        let (_f, toks) = prep("Vec<f32>");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        if let TypeKind::Path { type_args, .. } = t.kind {
            assert_eq!(type_args.len(), 1);
        } else {
            panic!("expected Path");
        }
    }

    #[test]
    fn tuple_type() {
        let (_f, toks) = prep("(i32, f32, bool)");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        if let TypeKind::Tuple { elems } = t.kind {
            assert_eq!(elems.len(), 3);
        } else {
            panic!("expected Tuple");
        }
    }

    #[test]
    fn reference_mut() {
        let (_f, toks) = prep("&mut T");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        if let TypeKind::Reference { mutable, .. } = t.kind {
            assert!(mutable);
        } else {
            panic!("expected Reference");
        }
    }

    #[test]
    fn capability_iso() {
        let (_f, toks) = prep("iso<T>");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        if let TypeKind::Capability { cap, .. } = t.kind {
            assert_eq!(cap, CapKind::Iso);
        } else {
            panic!("expected Capability");
        }
    }

    #[test]
    fn refinement_predicate_form() {
        // Predicate form `{v : T | P}` — lexer-independent canonical refinement shape.
        let (_f, toks) = prep("{v : f32 | v > 0}");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        assert!(matches!(t.kind, TypeKind::Refined { .. }));
    }

    #[test]
    fn refinement_tag_sugar_multi_letter() {
        // `f32'pos` post T2-D8 lexer fix : emits Ident + Apostrophe + Ident ; parser
        // attaches the tag as `RefinementKind::Tag`.
        let (_f, toks) = prep("f32'pos");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        assert!(matches!(t.kind, TypeKind::Refined { .. }));
    }

    #[test]
    fn slice_type() {
        let (_f, toks) = prep("[T]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        assert!(matches!(t.kind, TypeKind::Slice { .. }));
    }

    #[test]
    fn array_with_length() {
        let (_f, toks) = prep("[f32 ; 4]");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        assert!(matches!(t.kind, TypeKind::Array { .. }));
    }

    #[test]
    fn fn_type_with_return() {
        let (_f, toks) = prep("fn(i32, i32) -> f32");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let t = parse_type(&mut c, &mut bag);
        assert!(matches!(t.kind, TypeKind::Function { .. }));
    }

    #[test]
    fn effect_row_braced() {
        let (_f, toks) = prep("/ {GPU, NoAlloc}");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let r = parse_optional_effect_row(&mut c, &mut bag).unwrap();
        assert_eq!(r.effects.len(), 2);
    }
}
