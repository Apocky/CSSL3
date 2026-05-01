//! Item parser — top-level declarations.
//!
//! § SPEC : `specs/09_SYNTAX.csl` § item-level (fn-def / struct-def / enum-def / interface-def
//!          / impl-block / effect-def / handler-def).
//!
//! § COVERED FULLY
//!   - `fn`        fn-item (signature + optional body + generics + where + effect-row)
//!   - `struct`    named / tuple / unit structs
//!   - `enum`      named-/tuple-/unit-variant enums
//!   - `use`       `use a::b::c [as d]` single-path import (grouped + glob planned at T3.3)
//!   - `const`     constant binding with type + initializer
//!   - `type`      type alias
//!   - `module`    nested module (inline body or declaration)
//!
//! § STRUCTURALLY COVERED (body-parsing may be coarse; elaborator fills in)
//!   - `interface` interface item with associated items list
//!   - `impl`      impl block with associated items list
//!   - `effect`    effect item with operation list
//!   - `handler`   handler item with operations + optional return-clause

use cssl_ast::{
    AssocTypeDecl, AssocTypeDef, Attr, Block, ConstItem, DiagnosticBag, EffectItem, EnumItem,
    EnumVariant, ExternFnItem, FieldDecl, FnItem, HandlerItem, ImplAssocItem, ImplItem,
    InterfaceAssocItem, InterfaceItem, Item, ModuleItem, ModulePath, Param, Span, StructBody,
    StructItem, Type, TypeAliasItem, TypeKind, UseItem, UseTree, Visibility, VisibilityKind,
};
use cssl_lex::{BracketKind, BracketSide, Keyword, TokenKind};

use crate::common::{parse_ident, parse_module_path};
use crate::cursor::TokenCursor;
use crate::error::custom;
use crate::rust_hybrid::{attr, expr, generics, pat, ty};

/// Parse an optional `module a.b.c` declaration at file head.
#[must_use]
pub fn parse_optional_module_path(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
) -> Option<ModulePath> {
    if !cursor.check(TokenKind::Keyword(Keyword::Module)) {
        return None;
    }
    // Disambiguate: `module ident;` as top-level decl vs `module ident { … }` as item.
    // File-head form has no following `{` — if we see `{` after the ident path, this is
    // a nested module *item*, not a file-level path declaration.
    let mut lookahead = cursor.clone();
    lookahead.bump(); // module
    while lookahead.peek().kind == TokenKind::Ident {
        lookahead.bump();
        if !matches!(
            lookahead.peek().kind,
            TokenKind::Dot | TokenKind::ColonColon
        ) {
            break;
        }
        lookahead.bump();
    }
    if lookahead.peek().kind == TokenKind::Bracket(BracketKind::Brace, BracketSide::Open) {
        // Not a file-level module path; leave for item-level parsing.
        return None;
    }
    cursor.bump(); // module
    let path = parse_module_path(cursor, bag, "module declaration");
    // Optional `;` terminator.
    cursor.eat(TokenKind::Semi);
    Some(path)
}

/// Parse a single top-level item.
#[must_use]
pub fn parse_item(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Option<Item> {
    let attrs = attr::parse_outer_attrs(cursor, bag);
    let visibility = parse_visibility(cursor);
    let t = cursor.peek();
    match t.kind {
        TokenKind::Keyword(Keyword::Fn) => {
            Some(Item::Fn(parse_fn_item(cursor, bag, attrs, visibility)))
        }
        TokenKind::Keyword(Keyword::Extern) => Some(Item::ExternFn(parse_extern_fn_decl(
            cursor, bag, attrs, visibility,
        ))),
        TokenKind::Keyword(Keyword::Struct) => Some(Item::Struct(parse_struct_item(
            cursor, bag, attrs, visibility,
        ))),
        TokenKind::Keyword(Keyword::Enum) => {
            Some(Item::Enum(parse_enum_item(cursor, bag, attrs, visibility)))
        }
        TokenKind::Keyword(Keyword::Interface) => Some(Item::Interface(parse_interface_item(
            cursor, bag, attrs, visibility,
        ))),
        TokenKind::Keyword(Keyword::Impl) => Some(Item::Impl(parse_impl_item(cursor, bag, attrs))),
        TokenKind::Keyword(Keyword::Effect) => Some(Item::Effect(parse_effect_item(
            cursor, bag, attrs, visibility,
        ))),
        TokenKind::Keyword(Keyword::Handler) => Some(Item::Handler(parse_handler_item(
            cursor, bag, attrs, visibility,
        ))),
        TokenKind::Keyword(Keyword::Type) => Some(Item::TypeAlias(parse_type_alias(
            cursor, bag, attrs, visibility,
        ))),
        TokenKind::Keyword(Keyword::Use) => {
            Some(Item::Use(parse_use_item(cursor, bag, attrs, visibility)))
        }
        TokenKind::Keyword(Keyword::Const) => Some(Item::Const(parse_const_item(
            cursor, bag, attrs, visibility,
        ))),
        TokenKind::Keyword(Keyword::Module) => Some(Item::Module(parse_module_item(
            cursor, bag, attrs, visibility,
        ))),
        TokenKind::Eof => None,
        _ => {
            bag.push(custom(
                "expected an item (`fn`, `extern fn`, `struct`, `enum`, `interface`, `impl`, \
                 `effect`, `handler`, `type`, `use`, `const`, `module`)",
                t.span,
            ));
            // Skip one token to make progress.
            cursor.bump();
            None
        }
    }
}

fn parse_visibility(cursor: &mut TokenCursor<'_>) -> Visibility {
    if cursor.check(TokenKind::Keyword(Keyword::Pub)) {
        let t = cursor.bump();
        Visibility {
            span: t.span,
            kind: VisibilityKind::Public,
        }
    } else {
        let t = cursor.peek();
        Visibility {
            span: Span::new(t.span.source, t.span.start, t.span.start),
            kind: VisibilityKind::Private,
        }
    }
}

// ─ Fn ───────────────────────────────────────────────────────────────────────

fn parse_fn_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> FnItem {
    let kw = cursor.bump(); // fn
    let name = parse_ident(cursor, bag, "fn name");
    let gens = generics::parse_generics(cursor, bag);
    let params = parse_param_list(cursor, bag);
    let return_ty = if cursor.eat(TokenKind::Arrow).is_some() {
        Some(ty::parse_type(cursor, bag))
    } else {
        None
    };
    let effect_row = ty::parse_optional_effect_row(cursor, bag);
    let where_clauses = generics::parse_where_clauses(cursor, bag);
    let body = parse_fn_body(cursor, bag);
    let end = body
        .as_ref()
        .map_or_else(|| cursor.peek().span.start, |b| b.span.end);
    FnItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        visibility,
        name,
        generics: gens,
        params,
        return_ty,
        effect_row,
        where_clauses,
        body,
    }
}

// ─ Extern Fn ────────────────────────────────────────────────────────────────

/// Parse `extern fn name(params) -> ret` body-less FFI declaration.
///
/// § GRAMMAR (stage-0)
///   extern-fn-decl := `extern` `fn` IDENT `(` param-list `)` (`->` TYPE)? `;`?
///
/// The trailing `;` is optional ; if absent, the next item-boundary
/// terminates the declaration. No body, no generics, no effect-row, no
/// where-clauses are accepted ; emit a parse error if any appear.
///
/// Stage-0 ABI is implicit "C" — future revisions will accept
/// `extern "abi" fn name(…)`.
fn parse_extern_fn_decl(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> ExternFnItem {
    let kw = cursor.bump(); // extern
    if cursor.eat(TokenKind::Keyword(Keyword::Fn)).is_none() {
        bag.push(custom(
            "expected `fn` after `extern` in extern-fn declaration",
            cursor.peek().span,
        ));
    }
    let name = parse_ident(cursor, bag, "extern fn name");
    let params = parse_param_list(cursor, bag);
    let return_ty = if cursor.eat(TokenKind::Arrow).is_some() {
        Some(ty::parse_type(cursor, bag))
    } else {
        None
    };
    // Optional `;` terminator — accept either form.
    cursor.eat(TokenKind::Semi);
    let end = return_ty
        .as_ref()
        .map_or_else(|| cursor.peek().span.start.max(name.span.end), |t| t.span.end);
    ExternFnItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        visibility,
        name,
        params,
        return_ty,
        abi: "C".to_string(),
    }
}

fn parse_fn_body(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Option<Block> {
    if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
        Some(expr::parse_block(cursor, bag))
    } else if cursor.eat(TokenKind::Semi).is_some() {
        None
    } else {
        bag.push(custom(
            "expected `{` body or `;` after fn signature",
            cursor.peek().span,
        ));
        None
    }
}

fn parse_param_list(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> Vec<Param> {
    let mut params = Vec::new();
    if !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)) {
        bag.push(custom(
            "expected `(` to open parameter list",
            cursor.peek().span,
        ));
        return params;
    }
    cursor.bump(); // (
    while !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close))
        && !cursor.is_eof()
    {
        let p_attrs = attr::parse_outer_attrs(cursor, bag);
        let pat_node = pat::parse_pattern(cursor, bag);
        let ty_node = if cursor.eat(TokenKind::Colon).is_some() {
            ty::parse_type(cursor, bag)
        } else {
            Type {
                span: pat_node.span,
                kind: TypeKind::Infer,
            }
        };
        let default = if cursor.eat(TokenKind::Eq).is_some() {
            Some(expr::parse_expr(cursor, bag))
        } else {
            None
        };
        let end = default.as_ref().map_or(ty_node.span.end, |e| e.span.end);
        params.push(Param {
            span: Span::new(pat_node.span.source, pat_node.span.start, end),
            attrs: p_attrs,
            pat: pat_node,
            ty: ty_node,
            default,
        });
        if cursor.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
        cursor.bump();
    } else {
        bag.push(custom(
            "expected `)` to close parameter list",
            cursor.peek().span,
        ));
    }
    params
}

// ─ Struct ───────────────────────────────────────────────────────────────────

fn parse_struct_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> StructItem {
    let kw = cursor.bump(); // struct
    let name = parse_ident(cursor, bag, "struct name");
    let gens = generics::parse_generics(cursor, bag);
    let body = parse_struct_body(cursor, bag);
    let end = cursor.peek().span.start;
    StructItem {
        span: Span::new(kw.span.source, kw.span.start, end.max(name.span.end)),
        attrs,
        visibility,
        name,
        generics: gens,
        body,
    }
}

fn parse_struct_body(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> StructBody {
    if cursor.eat(TokenKind::Semi).is_some() {
        return StructBody::Unit;
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Open)) {
        cursor.bump(); // (
        let mut fields = Vec::new();
        while !cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close))
            && !cursor.is_eof()
        {
            let f_attrs = attr::parse_outer_attrs(cursor, bag);
            let vis = parse_visibility(cursor);
            let t = ty::parse_type(cursor, bag);
            fields.push(FieldDecl {
                span: Span::new(
                    vis.span.source,
                    vis.span.start.min(t.span.start),
                    t.span.end,
                ),
                attrs: f_attrs,
                visibility: vis,
                name: None,
                ty: t,
            });
            if cursor.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Paren, BracketSide::Close)) {
            cursor.bump();
        }
        cursor.eat(TokenKind::Semi);
        return StructBody::Tuple(fields);
    }
    if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
        cursor.bump(); // {
        let mut fields = Vec::new();
        while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
            && !cursor.is_eof()
        {
            let f_attrs = attr::parse_outer_attrs(cursor, bag);
            let vis = parse_visibility(cursor);
            let name = parse_ident(cursor, bag, "field name");
            if cursor.eat(TokenKind::Colon).is_none() {
                bag.push(custom("expected `:` after field name", cursor.peek().span));
            }
            let t = ty::parse_type(cursor, bag);
            fields.push(FieldDecl {
                span: Span::new(name.span.source, name.span.start, t.span.end),
                attrs: f_attrs,
                visibility: vis,
                name: Some(name),
                ty: t,
            });
            if cursor.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
            cursor.bump();
        }
        return StructBody::Named(fields);
    }
    StructBody::Unit
}

// ─ Enum ─────────────────────────────────────────────────────────────────────

fn parse_enum_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> EnumItem {
    let kw = cursor.bump(); // enum
    let name = parse_ident(cursor, bag, "enum name");
    let gens = generics::parse_generics(cursor, bag);
    let mut variants = Vec::new();
    if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
        cursor.bump(); // {
        while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
            && !cursor.is_eof()
        {
            let v_attrs = attr::parse_outer_attrs(cursor, bag);
            let v_name = parse_ident(cursor, bag, "enum variant");
            let body = parse_struct_body(cursor, bag);
            let end = cursor.peek().span.start.max(v_name.span.end);
            variants.push(EnumVariant {
                span: Span::new(v_name.span.source, v_name.span.start, end),
                attrs: v_attrs,
                name: v_name,
                body,
            });
            if cursor.eat(TokenKind::Comma).is_none() {
                break;
            }
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
            cursor.bump();
        }
    }
    let end = cursor.peek().span.start.max(name.span.end);
    EnumItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        visibility,
        name,
        generics: gens,
        variants,
    }
}

// ─ Interface / Impl ─────────────────────────────────────────────────────────

fn parse_interface_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> InterfaceItem {
    let kw = cursor.bump(); // interface
    let name = parse_ident(cursor, bag, "interface name");
    let gens = generics::parse_generics(cursor, bag);
    let super_bounds = if cursor.eat(TokenKind::Colon).is_some() {
        let mut v = Vec::new();
        loop {
            v.push(ty::parse_type(cursor, bag));
            if cursor.eat(TokenKind::Plus).is_none() {
                break;
            }
        }
        v
    } else {
        Vec::new()
    };
    let items = if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
        cursor.bump(); // {
        let mut v = Vec::new();
        while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
            && !cursor.is_eof()
        {
            if let Some(ai) = parse_interface_assoc(cursor, bag) {
                v.push(ai);
            } else {
                break;
            }
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
            cursor.bump();
        }
        v
    } else {
        Vec::new()
    };
    let end = cursor.peek().span.start.max(name.span.end);
    InterfaceItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        visibility,
        name,
        generics: gens,
        super_bounds,
        items,
    }
}

fn parse_interface_assoc(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
) -> Option<InterfaceAssocItem> {
    let at = cursor.peek();
    let inner_attrs = attr::parse_outer_attrs(cursor, bag);
    let _vis = parse_visibility(cursor);
    match cursor.peek().kind {
        TokenKind::Keyword(Keyword::Fn) => {
            let f = parse_fn_item(
                cursor,
                bag,
                inner_attrs,
                Visibility {
                    span: at.span,
                    kind: VisibilityKind::Private,
                },
            );
            Some(InterfaceAssocItem::Fn(f))
        }
        TokenKind::Ident => {
            // Recognize `associated type Name : Bounds [= Default]`.
            let id_span = cursor.peek().span;
            if cursor.peek().kind == TokenKind::Ident
                && cursor.peek2().kind == TokenKind::Keyword(Keyword::Type)
            {
                cursor.bump(); // "associated"
                cursor.bump(); // type
                let name = parse_ident(cursor, bag, "associated type name");
                let bounds = if cursor.eat(TokenKind::Colon).is_some() {
                    let mut v = Vec::new();
                    loop {
                        v.push(ty::parse_type(cursor, bag));
                        if cursor.eat(TokenKind::Plus).is_none() {
                            break;
                        }
                    }
                    v
                } else {
                    Vec::new()
                };
                let default = if cursor.eat(TokenKind::Eq).is_some() {
                    Some(ty::parse_type(cursor, bag))
                } else {
                    None
                };
                let end = default.as_ref().map_or(name.span.end, |t| t.span.end);
                Some(InterfaceAssocItem::AssociatedType(AssocTypeDecl {
                    span: Span::new(id_span.source, id_span.start, end),
                    attrs: inner_attrs,
                    name,
                    bounds,
                    default,
                }))
            } else {
                bag.push(custom(
                    "expected `fn`, `associated type`, or `const` in interface body",
                    at.span,
                ));
                cursor.bump();
                None
            }
        }
        TokenKind::Keyword(Keyword::Const) => Some(InterfaceAssocItem::Const(parse_const_item(
            cursor,
            bag,
            inner_attrs,
            Visibility {
                span: at.span,
                kind: VisibilityKind::Private,
            },
        ))),
        _ => {
            bag.push(custom(
                "expected interface associated item",
                cursor.peek().span,
            ));
            cursor.bump();
            None
        }
    }
}

fn parse_impl_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
) -> ImplItem {
    let kw = cursor.bump(); // impl
    let gens = generics::parse_generics(cursor, bag);
    // `impl X for Y` — need to detect the `for` keyword to split trait/self.
    let first_ty = ty::parse_type(cursor, bag);
    let (trait_, self_ty) = if cursor.eat(TokenKind::Keyword(Keyword::For)).is_some() {
        let self_ty = ty::parse_type(cursor, bag);
        (Some(first_ty), self_ty)
    } else {
        (None, first_ty)
    };
    let where_clauses = generics::parse_where_clauses(cursor, bag);
    let items = if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
        cursor.bump(); // {
        let mut v = Vec::new();
        while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
            && !cursor.is_eof()
        {
            if let Some(ai) = parse_impl_assoc(cursor, bag) {
                v.push(ai);
            } else {
                break;
            }
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
            cursor.bump();
        }
        v
    } else {
        Vec::new()
    };
    let end = cursor.peek().span.start.max(self_ty.span.end);
    ImplItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        generics: gens,
        trait_,
        self_ty,
        where_clauses,
        items,
    }
}

fn parse_impl_assoc(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
) -> Option<ImplAssocItem> {
    let at = cursor.peek();
    let attrs = attr::parse_outer_attrs(cursor, bag);
    let vis = parse_visibility(cursor);
    match cursor.peek().kind {
        TokenKind::Keyword(Keyword::Fn) => {
            Some(ImplAssocItem::Fn(parse_fn_item(cursor, bag, attrs, vis)))
        }
        TokenKind::Keyword(Keyword::Const) => Some(ImplAssocItem::Const(parse_const_item(
            cursor, bag, attrs, vis,
        ))),
        TokenKind::Ident if cursor.peek2().kind == TokenKind::Keyword(Keyword::Type) => {
            let id_span = cursor.peek().span;
            cursor.bump(); // "associated"
            cursor.bump(); // type
            let name = parse_ident(cursor, bag, "associated type name");
            if cursor.eat(TokenKind::Eq).is_none() {
                bag.push(custom(
                    "expected `=` after associated type name in impl",
                    cursor.peek().span,
                ));
            }
            let t = ty::parse_type(cursor, bag);
            let end = t.span.end;
            Some(ImplAssocItem::AssociatedType(AssocTypeDef {
                span: Span::new(id_span.source, id_span.start, end),
                attrs,
                name,
                ty: t,
            }))
        }
        _ => {
            bag.push(custom(
                "expected `fn`, `associated type`, or `const` in impl body",
                at.span,
            ));
            cursor.bump();
            None
        }
    }
}

// ─ Effect / Handler ─────────────────────────────────────────────────────────

fn parse_effect_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> EffectItem {
    let kw = cursor.bump(); // effect
    let name = parse_ident(cursor, bag, "effect name");
    let gens = generics::parse_generics(cursor, bag);
    let mut ops = Vec::new();
    if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
        cursor.bump(); // {
        while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
            && !cursor.is_eof()
        {
            let op_attrs = attr::parse_outer_attrs(cursor, bag);
            if !cursor.check(TokenKind::Keyword(Keyword::Fn)) {
                bag.push(custom(
                    "expected `fn` in effect operation list",
                    cursor.peek().span,
                ));
                break;
            }
            let op = parse_fn_item(
                cursor,
                bag,
                op_attrs,
                Visibility {
                    span: cursor.peek().span,
                    kind: VisibilityKind::Private,
                },
            );
            ops.push(op);
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
            cursor.bump();
        }
    }
    let end = cursor.peek().span.start.max(name.span.end);
    EffectItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        visibility,
        name,
        generics: gens,
        ops,
    }
}

fn parse_handler_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> HandlerItem {
    let kw = cursor.bump(); // handler
    let name = parse_ident(cursor, bag, "handler name");
    let gens = generics::parse_generics(cursor, bag);
    let params = parse_param_list(cursor, bag);
    // `for Effect<args>`
    let handled_effect = if cursor.eat(TokenKind::Keyword(Keyword::For)).is_some() {
        ty::parse_type(cursor, bag)
    } else {
        bag.push(custom(
            "expected `for` in handler header",
            cursor.peek().span,
        ));
        Type {
            span: cursor.peek().span,
            kind: TypeKind::Infer,
        }
    };
    let return_ty = if cursor.eat(TokenKind::Arrow).is_some() {
        Some(ty::parse_type(cursor, bag))
    } else {
        None
    };
    let mut ops = Vec::new();
    let mut return_clause: Option<Block> = None;
    if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
        cursor.bump(); // {
        while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
            && !cursor.is_eof()
        {
            // `let`-bindings + fn-operations + final `return` clause.
            if cursor.check(TokenKind::Keyword(Keyword::Return)) {
                cursor.bump();
                return_clause = Some(expr::parse_block(cursor, bag));
                continue;
            }
            if cursor.check(TokenKind::Keyword(Keyword::Fn)) {
                let op_attrs = attr::parse_outer_attrs(cursor, bag);
                let op = parse_fn_item(
                    cursor,
                    bag,
                    op_attrs,
                    Visibility {
                        span: cursor.peek().span,
                        kind: VisibilityKind::Private,
                    },
                );
                ops.push(op);
                continue;
            }
            // Handler bodies may also hold let-bindings; we bump past them coarsely.
            cursor.bump();
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
            cursor.bump();
        }
    }
    let end = cursor.peek().span.start.max(name.span.end);
    HandlerItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        visibility,
        name,
        generics: gens,
        params,
        handled_effect,
        return_ty,
        ops,
        return_clause,
    }
}

// ─ Use / Const / Type-alias / Module ────────────────────────────────────────

fn parse_use_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> UseItem {
    let kw = cursor.bump(); // use
    let tree = parse_use_tree(cursor, bag);
    cursor.eat(TokenKind::Semi);
    let end = cursor.peek().span.start;
    UseItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        visibility,
        tree,
    }
}

fn parse_use_tree(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> UseTree {
    // Path prefix
    let path = parse_module_path(cursor, bag, "use path");
    if cursor.eat(TokenKind::ColonColon).is_some() || cursor.eat(TokenKind::Dot).is_some() {
        if cursor.eat(TokenKind::Star).is_some() {
            return UseTree::Glob { path };
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
            cursor.bump(); // {
            let mut trees = Vec::new();
            while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
                && !cursor.is_eof()
            {
                trees.push(parse_use_tree(cursor, bag));
                if cursor.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
            if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
                cursor.bump();
            }
            return UseTree::Group {
                prefix: path,
                trees,
            };
        }
    }
    let alias = if cursor.eat(TokenKind::Keyword(Keyword::As)).is_some() {
        Some(parse_ident(cursor, bag, "use alias"))
    } else {
        None
    };
    UseTree::Path { path, alias }
}

fn parse_const_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> ConstItem {
    let kw = cursor.bump(); // const
    let name = parse_ident(cursor, bag, "const name");
    if cursor.eat(TokenKind::Colon).is_none() {
        bag.push(custom("expected `:` after const name", cursor.peek().span));
    }
    let t = ty::parse_type(cursor, bag);
    if cursor.eat(TokenKind::Eq).is_none() {
        bag.push(custom("expected `=` after const type", cursor.peek().span));
    }
    let value = expr::parse_expr(cursor, bag);
    cursor.eat(TokenKind::Semi);
    let end = value.span.end;
    ConstItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        visibility,
        name,
        ty: t,
        value,
    }
}

fn parse_type_alias(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> TypeAliasItem {
    let kw = cursor.bump(); // type
    let name = parse_ident(cursor, bag, "type alias name");
    let gens = generics::parse_generics(cursor, bag);
    if cursor.eat(TokenKind::Eq).is_none() {
        bag.push(custom("expected `=` in type alias", cursor.peek().span));
    }
    let t = ty::parse_type(cursor, bag);
    cursor.eat(TokenKind::Semi);
    TypeAliasItem {
        span: Span::new(kw.span.source, kw.span.start, t.span.end),
        attrs,
        visibility,
        name,
        generics: gens,
        ty: t,
    }
}

fn parse_module_item(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> ModuleItem {
    let kw = cursor.bump(); // module
    let name = parse_ident(cursor, bag, "module name");
    let items = if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Open)) {
        cursor.bump(); // {
        let mut v = Vec::new();
        while !cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close))
            && !cursor.is_eof()
        {
            if let Some(it) = parse_item(cursor, bag) {
                v.push(it);
            } else {
                break;
            }
        }
        if cursor.check(TokenKind::Bracket(BracketKind::Brace, BracketSide::Close)) {
            cursor.bump();
        }
        Some(v)
    } else {
        cursor.eat(TokenKind::Semi);
        None
    };
    let end = cursor.peek().span.start.max(name.span.end);
    ModuleItem {
        span: Span::new(kw.span.source, kw.span.start, end),
        attrs,
        visibility,
        name,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_item, parse_optional_module_path};
    use crate::cursor::TokenCursor;
    use cssl_ast::{DiagnosticBag, Item, SourceFile, SourceId, StructBody, Surface};

    fn prep(src: &str) -> (SourceFile, Vec<cssl_lex::Token>) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        (f, toks)
    }

    #[test]
    fn module_path_declaration() {
        let (_f, toks) = prep("module com.apocky.loa\n");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_optional_module_path(&mut c, &mut bag);
        assert!(p.is_some());
        assert_eq!(p.unwrap().segments.len(), 3);
    }

    #[test]
    fn fn_no_body() {
        let (_f, toks) = prep("fn f(x : i32) -> i32 ;");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        if let Item::Fn(f) = it {
            assert!(f.body.is_none());
            assert_eq!(f.params.len(), 1);
        } else {
            panic!("expected Fn");
        }
    }

    #[test]
    fn fn_with_body() {
        let (_f, toks) = prep("fn f() { 1 }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        if let Item::Fn(f) = it {
            assert!(f.body.is_some());
        } else {
            panic!("expected Fn");
        }
    }

    #[test]
    fn named_struct() {
        let (_f, toks) = prep("struct Point { x : f32, y : f32 }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        if let Item::Struct(s) = it {
            match s.body {
                StructBody::Named(fields) => assert_eq!(fields.len(), 2),
                _ => panic!("expected Named struct"),
            }
        } else {
            panic!("expected Struct");
        }
    }

    #[test]
    fn enum_with_variants() {
        let (_f, toks) = prep("enum Option<T> { Some(T), None }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        if let Item::Enum(e) = it {
            assert_eq!(e.variants.len(), 2);
        } else {
            panic!("expected Enum");
        }
    }

    #[test]
    fn use_path_alias() {
        let (_f, toks) = prep("use foo::bar as baz ;");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert!(matches!(it, Item::Use(_)));
    }

    #[test]
    fn const_with_value() {
        let (_f, toks) = prep("const FOO : i32 = 42 ;");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert!(matches!(it, Item::Const(_)));
    }

    #[test]
    fn type_alias() {
        let (_f, toks) = prep("type V = Vec<f32> ;");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert!(matches!(it, Item::TypeAlias(_)));
    }

    #[test]
    fn pub_visibility_recognized() {
        let (_f, toks) = prep("pub fn f() {}");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        if let Item::Fn(f) = it {
            assert_eq!(f.visibility.kind, cssl_ast::VisibilityKind::Public);
        } else {
            panic!("expected Fn");
        }
    }

    // ─ T11-CC-PARSER-1 (W-CC-extern-fn) — extern fn declarations ────────────

    #[test]
    fn parse_extern_fn_basic() {
        let (_f, toks) = prep("extern fn write(fd : i32) -> i32");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        if let Item::ExternFn(e) = it {
            assert_eq!(e.params.len(), 1);
            assert!(e.return_ty.is_some());
            assert_eq!(e.abi, "C");
        } else {
            panic!("expected ExternFn, got {:?}", it);
        }
    }

    #[test]
    fn parse_extern_fn_no_args() {
        let (_f, toks) = prep("extern fn now() -> u64");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        if let Item::ExternFn(e) = it {
            assert!(e.params.is_empty());
            assert!(e.return_ty.is_some());
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn parse_extern_fn_no_return() {
        let (_f, toks) = prep("extern fn yield_now()");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        if let Item::ExternFn(e) = it {
            assert!(e.params.is_empty());
            assert!(e.return_ty.is_none());
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn parse_extern_fn_multi_line_params() {
        // Multi-line (the lexer treats whitespace incl newlines as separators).
        let src = "extern fn __cssl_omega_field_sample_aabb(\n\
                   min_morton : u64,\n\
                   max_morton : u64,\n\
                   out_buf : *mut u8,\n\
                   max_cells : i32\n\
                   ) -> i32";
        let (_f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0, "expected zero errors");
        if let Item::ExternFn(e) = it {
            assert_eq!(e.params.len(), 4);
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn parse_extern_fn_pointer_types() {
        let src = "extern fn __cssl_telemetry_emit(\
                   event_id : u32, payload_ptr : *const u8, payload_len : i32, \
                   out_buf : *mut u8) -> i32";
        let (_f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        if let Item::ExternFn(e) = it {
            assert_eq!(e.params.len(), 4);
            // Verify pointer-types lowered to RawPointer{mutable=false} + RawPointer{mutable=true}.
            use cssl_ast::TypeKind;
            let p1_kind = &e.params[1].ty.kind;
            assert!(
                matches!(p1_kind, TypeKind::RawPointer { mutable: false, .. }),
                "param 1 expected `*const u8`, got {:?}",
                p1_kind
            );
            let p3_kind = &e.params[3].ty.kind;
            assert!(
                matches!(p3_kind, TypeKind::RawPointer { mutable: true, .. }),
                "param 3 expected `*mut u8`, got {:?}",
                p3_kind
            );
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn parse_extern_fn_followed_by_normal_fn() {
        // Regression : after an extern fn (with or without `;`), the next normal
        // fn must parse cleanly without any state corruption.
        let src = "extern fn __cssl_telemetry_emit(event_id : u32) -> i32 ; \
                   fn main() -> i32 { 42 }";
        let (_f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it1 = parse_item(&mut c, &mut bag).unwrap();
        let it2 = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        assert!(matches!(it1, Item::ExternFn(_)));
        if let Item::Fn(f) = it2 {
            assert_eq!(f.params.len(), 0);
            assert!(f.body.is_some());
        } else {
            panic!("expected Fn after ExternFn, got {:?}", it2);
        }
    }

    #[test]
    fn parse_extern_fn_with_underscored_name() {
        // The `__cssl_*` symbol naming convention used throughout LoA scenes.
        let (_f, toks) = prep("extern fn __cssl_window_create(width : i32, height : i32) -> u64");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        if let Item::ExternFn(e) = it {
            assert_eq!(e.params.len(), 2);
            assert!(e.return_ty.is_some());
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn parse_extern_fn_trailing_comma_in_params() {
        // Trailing comma in param list must be tolerated.
        let src = "extern fn __cssl_act_world_mutate(\n\
                   target_morton : u64,\n\
                   value : i32,\n\
                   sovereign_cap : u64,\n\
                   ) -> i32";
        let (_f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        if let Item::ExternFn(e) = it {
            assert_eq!(e.params.len(), 3);
        } else {
            panic!("expected ExternFn");
        }
    }

    #[test]
    fn parse_extern_fn_module_acceptance() {
        // Acceptance corpus from T11-CC-PARSER-1 prompt — must parse cleanly.
        let src = "module loa.test_extern\n\
                   extern fn __cssl_telemetry_emit(event_id : u32, payload_ptr : *const u8, \
                   payload_len : i32) -> i32\n\
                   fn main() -> i32 { 42 }";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "expected zero errors");
        assert_eq!(m.items.len(), 2);
        assert!(matches!(m.items[0], Item::ExternFn(_)));
        assert!(matches!(m.items[1], Item::Fn(_)));
    }
}
