//! Item parser — top-level declarations.
//!
//! § SPEC : `specs/09_SYNTAX.csl` § item-level (fn-def / struct-def / enum-def / interface-def
//!          / impl-block / effect-def / handler-def).
//!
//! § COVERED FULLY
//!   - `fn`        fn-item (signature + optional body + generics + where + effect-row)
//!   - `struct`    named / tuple / unit structs
//!   - `enum`      named-/tuple-/unit-variant enums
//!   - `use`       `use a::b::c [as d]`, `use a.b::*`, `use a::b.{x, y as z}` —
//!                 dot- + double-colon-style separators, alias, glob, and group all covered.
//!                 Resolution + scope-population is deferred to HIR / sibling
//!                 W-CC-link-multi-source pass; parser only produces the syntactic shape.
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
///   extern-fn-decl := `extern` ABI-LITERAL? `fn` IDENT `(` param-list `)` (`->` TYPE)? `;`?
///   ABI-LITERAL    := STRING-LITERAL              (e.g. `"C"`, `"system"`, `"Rust"`)
///
/// The trailing `;` is optional ; if absent, the next item-boundary
/// terminates the declaration. No body, no generics, no effect-row, no
/// where-clauses are accepted ; emit a parse error if any appear.
///
/// § ABI (T11-CC-PARSER-8 / W-CC-extern-c-abi)
///   The parser accepts an optional string-literal between `extern` and `fn`.
///   When present, its span is preserved in `ExternFnItem::abi_span` so HIR
///   lowering can resolve the verbatim ABI text via `SourceFile::slice`.
///   The parser cannot decode the literal here (no `SourceFile` access at
///   this layer), so it leaves `abi: "C".to_string()` as a placeholder —
///   lowering replaces it with the actual unquoted text when `abi_span` is
///   `Some(_)`. When the literal is omitted, the implicit ABI is "C".
fn parse_extern_fn_decl(
    cursor: &mut TokenCursor<'_>,
    bag: &mut DiagnosticBag,
    attrs: Vec<Attr>,
    visibility: Visibility,
) -> ExternFnItem {
    let kw = cursor.bump(); // extern

    // Optional ABI tag : `"<abi>"` between `extern` and `fn`.
    // Match either flavor of string literal (Normal or Raw) — both are
    // syntactically legal as ABI tags. The parser preserves the literal's
    // span so downstream passes can recover the verbatim text. The parser
    // does not validate the ABI string here ; HIR/MIR/codegen are
    // responsible for rejecting unrecognized ABI strings at link time.
    let abi_span = if matches!(
        cursor.peek().kind,
        TokenKind::StringLiteral(_)
    ) {
        Some(cursor.bump().span)
    } else {
        None
    };

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
        abi_span,
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
            // Variant body : tuple `(T, U)`, struct `{ a: T }`, or unit (none).
            // Unlike top-level structs, enum-variant unit form does NOT use `;` —
            // separation is by `,` between variants.
            let body = parse_enum_variant_body(cursor, bag);
            // Optional explicit discriminant : `Variant = expr`.
            let discriminant = if cursor.eat(TokenKind::Eq).is_some() {
                Some(expr::parse_expr(cursor, bag))
            } else {
                None
            };
            let end = discriminant
                .as_ref()
                .map_or(cursor.peek().span.start.max(v_name.span.end), |e| e.span.end);
            variants.push(EnumVariant {
                span: Span::new(v_name.span.source, v_name.span.start, end),
                attrs: v_attrs,
                name: v_name,
                body,
                discriminant,
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

/// Parse the body shape of an enum variant — tuple `(T, U)`, struct `{ a: T }`,
/// or unit (no following bracket, possibly followed by `=` discriminant or `,`).
fn parse_enum_variant_body(cursor: &mut TokenCursor<'_>, bag: &mut DiagnosticBag) -> StructBody {
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
            let f_name = parse_ident(cursor, bag, "field name");
            if cursor.eat(TokenKind::Colon).is_none() {
                bag.push(custom("expected `:` after field name", cursor.peek().span));
            }
            let t = ty::parse_type(cursor, bag);
            fields.push(FieldDecl {
                span: Span::new(f_name.span.source, f_name.span.start, t.span.end),
                attrs: f_attrs,
                visibility: vis,
                name: Some(f_name),
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

    // ─ § T11-CC-PARSER-6 (W-CC-module-path) — multi-segment dotted module paths ─
    //
    // The audit (compiler-rs/docs/loa_scene_compat_matrix.md) flagged 49/50
    // LoA scenes failing at `module com.apocky.loa.scenes.X` (5-segment
    // dotted paths) with an `expected an item @ 0:0` cascade. The parser
    // machinery already supports N-segment dotted paths via
    // `parse_module_path` (see common.rs); these tests pin down that
    // 1/2/5-segment forms all parse cleanly, no errors, and that the
    // path-end detection correctly hands off to subsequent items
    // (`use`, `fn`).

    #[test]
    fn parse_module_single_segment() {
        // Regression : `module foo` must still parse as a single-segment path.
        let (_f, toks) = prep("module simple\n");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_optional_module_path(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0, "errors: {:?}", bag);
        assert!(p.is_some());
        assert_eq!(p.unwrap().segments.len(), 1);
    }

    #[test]
    fn parse_module_two_segments() {
        // Regression : `module loa.main` (the only currently-passing LoA
        // file uses this 2-segment form).
        let (_f, toks) = prep("module loa.main\n");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_optional_module_path(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0, "errors: {:?}", bag);
        assert!(p.is_some());
        assert_eq!(p.unwrap().segments.len(), 2);
    }

    #[test]
    fn parse_module_five_segments() {
        // The headline audit failure : `module com.apocky.loa.scenes.coliseum`
        // — 49 of 50 LoA scenes use this 5-segment form.
        let (_f, toks) = prep("module com.apocky.loa.scenes.coliseum\n");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_optional_module_path(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0, "errors: {:?}", bag);
        let path = p.expect("expected Some(ModulePath)");
        assert_eq!(path.segments.len(), 5, "expected 5 dotted segments");
    }

    #[test]
    fn parse_module_followed_by_use() {
        // Regression : path-end detection must correctly stop at the
        // newline + `use` keyword and let the item parser pick it up.
        let src = "\
module com.apocky.loa.scenes.torii
use std::option::Option
use std::vec::Vec
";
        let (_f, toks) = prep(src);
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "errors: {:?}", bag);
        let path = m.path.expect("expected module-path declaration");
        assert_eq!(path.segments.len(), 5);
        assert_eq!(m.items.len(), 2, "expected 2 use-items");
        for it in &m.items {
            assert!(matches!(it, Item::Use(_)), "expected Item::Use, got {it:?}");
        }
    }

    #[test]
    fn parse_module_followed_by_fn() {
        // The acceptance test from the dispatch : module + fn must
        // parse without any error.
        let src = "\
module com.apocky.loa.scenes.coliseum

fn main() -> i32 { 42 }
";
        let (_f, toks) = prep(src);
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "errors: {:?}", bag);
        let path = m.path.expect("expected module-path declaration");
        assert_eq!(path.segments.len(), 5);
        assert_eq!(m.items.len(), 1, "expected 1 fn item");
        assert!(matches!(m.items[0], Item::Fn(_)));
    }

    #[test]
    fn parse_module_with_underscored_segments() {
        // Bonus : segments with underscores must work — LoA uses
        // `test_room`, `player_input`, etc.
        let src = "module com.apocky_corp.loa_scenes.test_room\n";
        let (_f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let p = parse_optional_module_path(&mut c, &mut bag);
        assert_eq!(bag.error_count(), 0, "errors: {:?}", bag);
        let path = p.expect("expected Some(ModulePath)");
        assert_eq!(path.segments.len(), 4);
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

    // ─ § T11-CC-PARSER-5 (W-CC-use-stmt) — comprehensive `use` coverage ─────
    //
    // These tests pin down every variant of `use` declaration we want to
    // accept from CSSL source. Resolution / scope-population is deferred to
    // HIR + the sibling W-CC-link-multi-source pass; the parser only emits
    // a `UseTree` with the right syntactic shape.

    use cssl_ast::{Item as ItemAst, UseTree};

    fn parse_one_use(src: &str) -> (cssl_ast::UseItem, DiagnosticBag) {
        let (_f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).expect("expected an item");
        match it {
            ItemAst::Use(u) => (u, bag),
            other => panic!("expected Item::Use, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_simple_dot_path() {
        let (u, bag) = parse_one_use("use std.gpu.vec3\n");
        assert_eq!(bag.error_count(), 0);
        match u.tree {
            UseTree::Path { path, alias } => {
                assert_eq!(path.segments.len(), 3, "expected 3 dot-segments");
                assert!(alias.is_none(), "expected no alias");
            }
            other => panic!("expected UseTree::Path, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_simple_double_colon_path() {
        let (u, bag) = parse_one_use("use std::gpu::vec4\n");
        assert_eq!(bag.error_count(), 0);
        match u.tree {
            UseTree::Path { path, alias } => {
                assert_eq!(path.segments.len(), 3, "expected 3 ::-segments");
                assert!(alias.is_none());
            }
            other => panic!("expected UseTree::Path, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_with_alias() {
        let (u, bag) = parse_one_use("use std.gpu.vec3 as Vec3\n");
        assert_eq!(bag.error_count(), 0);
        match u.tree {
            UseTree::Path { path, alias } => {
                assert_eq!(path.segments.len(), 3);
                assert!(alias.is_some(), "expected `as Vec3` alias");
            }
            other => panic!("expected UseTree::Path with alias, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_glob_at_end() {
        let (u, bag) = parse_one_use("use std.io.*\n");
        assert_eq!(bag.error_count(), 0);
        match u.tree {
            UseTree::Glob { path } => {
                assert_eq!(path.segments.len(), 2, "expected `std.io` 2-segment prefix");
            }
            other => panic!("expected UseTree::Glob, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_group_braces() {
        let (u, bag) = parse_one_use("use std.collections.{HashMap, BTreeMap, Vec}\n");
        assert_eq!(bag.error_count(), 0);
        match u.tree {
            UseTree::Group { prefix, trees } => {
                assert_eq!(prefix.segments.len(), 2);
                assert_eq!(trees.len(), 3, "expected 3 group entries");
                for (idx, t) in trees.iter().enumerate() {
                    match t {
                        UseTree::Path { path, alias } => {
                            assert_eq!(path.segments.len(), 1, "group entry {idx} must be a single ident");
                            assert!(alias.is_none(), "group entry {idx} must not be aliased");
                        }
                        other => panic!("group entry {idx} expected Path, got {:?}", other),
                    }
                }
            }
            other => panic!("expected UseTree::Group, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_group_with_alias_inside() {
        let (u, bag) = parse_one_use("use std.io.{Read, Write as W}\n");
        assert_eq!(bag.error_count(), 0);
        match u.tree {
            UseTree::Group { trees, .. } => {
                assert_eq!(trees.len(), 2);
                let aliased = trees.iter().any(|t| matches!(t, UseTree::Path { alias: Some(_), .. }));
                assert!(aliased, "expected one aliased entry inside group");
            }
            other => panic!("expected UseTree::Group, got {:?}", other),
        }
    }

    #[test]
    fn parse_multiple_use_decls_in_same_file() {
        let src = "\
use std.gpu.vec3
use std.gpu.vec4
use std::gpu::SurfaceFormat
use cssl.runtime.{alloc, free}
fn main() -> i32 { 42 }\n";
        let (_f, toks) = prep(src);
        let mut bag = DiagnosticBag::new();
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(bag.error_count(), 0, "errors: {:?}", bag);
        assert_eq!(m.items.len(), 5, "expected 4 use + 1 fn");
        let use_count = m.items.iter().filter(|i| matches!(i, ItemAst::Use(_))).count();
        let fn_count = m.items.iter().filter(|i| matches!(i, ItemAst::Fn(_))).count();
        assert_eq!(use_count, 4);
        assert_eq!(fn_count, 1);
    }

    #[test]
    fn parse_use_mixed_dot_and_colon() {
        // `parse_module_path` accepts both `.` and `::` as path separators, so
        // these should both produce the same 3-segment path.
        let (u_dot, bag1) = parse_one_use("use std.gpu.vec3\n");
        let (u_col, bag2) = parse_one_use("use std::gpu::vec3\n");
        assert_eq!(bag1.error_count(), 0);
        assert_eq!(bag2.error_count(), 0);
        let n_dot = match u_dot.tree {
            UseTree::Path { path, .. } => path.segments.len(),
            _ => panic!("dot variant should be Path"),
        };
        let n_col = match u_col.tree {
            UseTree::Path { path, .. } => path.segments.len(),
            _ => panic!("colon variant should be Path"),
        };
        assert_eq!(n_dot, n_col);
        assert_eq!(n_dot, 3);
    }

    #[test]
    fn parse_use_loa_module_relative() {
        // LoA scenes use module-relative paths into the `loa` namespace.
        let (u, bag) = parse_one_use("use loa.scenes.test_room\n");
        assert_eq!(bag.error_count(), 0);
        match u.tree {
            UseTree::Path { path, .. } => {
                assert_eq!(path.segments.len(), 3);
            }
            other => panic!("expected UseTree::Path, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_no_semicolon_required() {
        // CSSL convention : statements newline-terminate, no `;` required. We
        // accept either, but parsing must succeed without a `;`.
        let (u, bag) = parse_one_use("use std.io.Read\n");
        assert_eq!(bag.error_count(), 0);
        assert!(matches!(u.tree, UseTree::Path { .. }));
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

    // ─ Rich struct + enum coverage ──────────────────────────────────────────
    //
    // § T11-CC-PARSER-3 (W-CC-struct-rich) : these tests pin the surface forms
    // used by all 47 LoA scenes — named-field structs with `pub` fields, arrays,
    // tuple structs, unit structs, generic params, and enums with C-style
    // discriminants + tuple variants + struct variants.
    // PLUS T11-CC-PARSER-1 (W-CC-extern-fn) : extern fn declarations.

    #[test]
    fn parse_named_struct_basic() {
        let (_f, toks) = prep(
            "pub struct PlayerState { pub hp_q14 : i32 , pub stamina_q14 : i32 , pub level : u8 }",
        );
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let s = match it {
            Item::Struct(s) => s,
            _ => panic!("expected Struct"),
        };
        assert_eq!(s.visibility.kind, cssl_ast::VisibilityKind::Public);
        let fields = match s.body {
            StructBody::Named(f) => f,
            _ => panic!("expected Named struct"),
        };
        assert_eq!(fields.len(), 3);
        assert_eq!(bag.error_count(), 0, "diagnostics : {:?}", bag);
    }

    #[test]
    fn parse_struct_with_pub_fields() {
        let (_f, toks) = prep("pub struct Foo { pub a : i32 , b : u8 , pub c : f32 }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let s = match it {
            Item::Struct(s) => s,
            _ => panic!("expected Struct"),
        };
        let fields = match s.body {
            StructBody::Named(f) => f,
            _ => panic!("expected Named"),
        };
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].visibility.kind, cssl_ast::VisibilityKind::Public);
        assert_eq!(
            fields[1].visibility.kind,
            cssl_ast::VisibilityKind::Private
        );
        assert_eq!(fields[2].visibility.kind, cssl_ast::VisibilityKind::Public);
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_struct_with_array_field() {
        let (_f, toks) = prep("pub struct ActiveBranch { pub options : [BranchOption ; 8] }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let s = match it {
            Item::Struct(s) => s,
            _ => panic!("expected Struct"),
        };
        let fields = match s.body {
            StructBody::Named(f) => f,
            _ => panic!("expected Named"),
        };
        assert_eq!(fields.len(), 1);
        assert!(matches!(fields[0].ty.kind, cssl_ast::TypeKind::Array { .. }));
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_struct_trailing_comma_ok() {
        let (_f, toks) = prep("struct Foo { a : i32 , b : u8 , }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let s = match it {
            Item::Struct(s) => s,
            _ => panic!("expected Struct"),
        };
        match s.body {
            StructBody::Named(fs) => assert_eq!(fs.len(), 2),
            _ => panic!("expected Named"),
        }
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_unit_struct() {
        let (_f, toks) = prep("pub struct Sovereign ;");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let s = match it {
            Item::Struct(s) => s,
            _ => panic!("expected Struct"),
        };
        assert!(matches!(s.body, StructBody::Unit));
        assert_eq!(s.visibility.kind, cssl_ast::VisibilityKind::Public);
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_tuple_struct_one_field() {
        let (_f, toks) = prep("pub struct Q14 ( pub i32 ) ;");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let s = match it {
            Item::Struct(s) => s,
            _ => panic!("expected Struct"),
        };
        let fields = match s.body {
            StructBody::Tuple(f) => f,
            _ => panic!("expected Tuple"),
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].visibility.kind, cssl_ast::VisibilityKind::Public);
        assert!(fields[0].name.is_none(), "tuple field has no name");
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_struct_with_generic_param() {
        let (_f, toks) = prep("pub struct Foo < T > { value : T }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let s = match it {
            Item::Struct(s) => s,
            _ => panic!("expected Struct"),
        };
        assert_eq!(s.generics.params.len(), 1);
        let fields = match s.body {
            StructBody::Named(f) => f,
            _ => panic!("expected Named"),
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_enum_c_style() {
        let (_f, toks) = prep("pub enum Color { Red , Green , Blue }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let e = match it {
            Item::Enum(e) => e,
            _ => panic!("expected Enum"),
        };
        assert_eq!(e.variants.len(), 3);
        for v in &e.variants {
            assert!(matches!(v.body, StructBody::Unit));
            assert!(v.discriminant.is_none());
        }
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_enum_with_discriminants() {
        let (_f, toks) = prep(
            "pub enum ReviewState { Pending = 0 , Approved = 1 , Refused = 2 , Expired = 3 }",
        );
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let e = match it {
            Item::Enum(e) => e,
            _ => panic!("expected Enum"),
        };
        assert_eq!(e.variants.len(), 4);
        for v in &e.variants {
            assert!(
                v.discriminant.is_some(),
                "variant {:?} missing discriminant",
                v.name.span
            );
            assert!(matches!(v.body, StructBody::Unit));
        }
        assert_eq!(bag.error_count(), 0, "diagnostics : {:?}", bag);
    }

    #[test]
    fn parse_enum_tuple_variants() {
        let (_f, toks) = prep("pub enum Result < T , E > { Ok ( T ) , Err ( E ) }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let e = match it {
            Item::Enum(e) => e,
            _ => panic!("expected Enum"),
        };
        assert_eq!(e.generics.params.len(), 2);
        assert_eq!(e.variants.len(), 2);
        match &e.variants[0].body {
            StructBody::Tuple(f) => assert_eq!(f.len(), 1),
            _ => panic!("expected Ok(T) tuple"),
        }
        match &e.variants[1].body {
            StructBody::Tuple(f) => assert_eq!(f.len(), 1),
            _ => panic!("expected Err(E) tuple"),
        }
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_enum_struct_variants() {
        let (_f, toks) = prep(
            "pub enum Shape { Circle { r : f32 } , Rect { w : f32 , h : f32 } }",
        );
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let e = match it {
            Item::Enum(e) => e,
            _ => panic!("expected Enum"),
        };
        assert_eq!(e.variants.len(), 2);
        match &e.variants[0].body {
            StructBody::Named(f) => assert_eq!(f.len(), 1),
            _ => panic!("expected Circle struct-variant"),
        }
        match &e.variants[1].body {
            StructBody::Named(f) => assert_eq!(f.len(), 2),
            _ => panic!("expected Rect struct-variant"),
        }
        assert_eq!(bag.error_count(), 0);
    }

    #[test]
    fn parse_enum_mixed_variants() {
        // Mixed unit / tuple / struct variants in the same enum.
        let (_f, toks) =
            prep("enum E { A , B ( i32 ) , C { x : f32 , y : f32 } , D = 7 }");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        let e = match it {
            Item::Enum(e) => e,
            _ => panic!("expected Enum"),
        };
        assert_eq!(e.variants.len(), 4);
        assert!(matches!(e.variants[0].body, StructBody::Unit));
        assert!(matches!(e.variants[1].body, StructBody::Tuple(_)));
        assert!(matches!(e.variants[2].body, StructBody::Named(_)));
        assert!(e.variants[3].discriminant.is_some());
        assert_eq!(bag.error_count(), 0);
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

    // ─ § T11-CC-PARSER-8 (W-CC-extern-c-abi) — `extern "C" fn` ABI tag ─────
    //
    // The 44+ LoA scenes (`Labyrinth of Apocalypse/main.cssl`, `engine.cssl`,
    // `scenes/torii.cssl`, etc.) all forward-declare host symbols using
    // `extern "C" fn name(…)`. The previous extern-fn parser (T11-CC-PARSER-1)
    // accepted only `extern fn …` and rejected the explicit ABI tag with
    // `expected `fn` after `extern` …`. These tests pin the optional
    // string-literal between `extern` and `fn`, covering :
    //   1. regression : implicit-C still works (no ABI literal present)
    //   2. explicit "C" — the LoA-scene canonical form
    //   3. forward-friendly : "system" parses without rejection
    //   4. composability : explicit "C" + raw-pointer params (the LoA
    //      `__cssl_telemetry_emit` shape) parses end-to-end

    #[test]
    fn parse_extern_fn_no_abi_defaults_to_c() {
        // Regression : the implicit-C form (no ABI literal) must continue
        // to parse cleanly and yield `abi == "C"` with `abi_span == None`.
        let (_f, toks) = prep("extern fn write(fd : i32) -> i32");
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0);
        if let Item::ExternFn(e) = it {
            assert_eq!(e.abi, "C", "implicit ABI defaults to C");
            assert!(
                e.abi_span.is_none(),
                "no literal present ⇒ abi_span must be None, got {:?}",
                e.abi_span
            );
            assert_eq!(e.params.len(), 1);
            assert!(e.return_ty.is_some());
        } else {
            panic!("expected ExternFn, got {:?}", it);
        }
    }

    #[test]
    fn parse_extern_c_abi_explicit() {
        // The canonical LoA-scene shape : `extern "C" fn …`.
        // Verify the literal parses without error, the literal-span covers
        // the `"C"` token (quotes included), and the slice text matches.
        let src = "extern \"C\" fn bar(x : i32) -> i32";
        let (f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0, "expected zero errors");
        if let Item::ExternFn(e) = it {
            let span = e.abi_span.expect("explicit ABI ⇒ abi_span must be Some");
            let slice = f.slice(span.start, span.end).expect("abi span in-bounds");
            assert_eq!(
                slice, "\"C\"",
                "abi_span must cover the literal incl. quotes"
            );
            // Parser leaves `abi` as the placeholder "C" ; HIR lowering
            // resolves the verbatim text via SourceFile slice.
            assert_eq!(e.abi, "C");
            assert_eq!(e.params.len(), 1);
            assert!(e.return_ty.is_some());
        } else {
            panic!("expected ExternFn, got {:?}", it);
        }
    }

    #[test]
    fn parse_extern_system_abi() {
        // Forward-friendly : the parser accepts any ABI string verbatim.
        // Stage-0 only links `"C"` ; downstream codegen rejects unknowns
        // at link time. The parser's job is to NOT reject the literal.
        let src = "extern \"system\" fn baz() -> i32";
        let (f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0, "expected zero errors");
        if let Item::ExternFn(e) = it {
            let span = e.abi_span.expect("explicit ABI ⇒ abi_span must be Some");
            let slice = f.slice(span.start, span.end).expect("abi span in-bounds");
            assert_eq!(
                slice, "\"system\"",
                "abi_span must cover `\"system\"` literal verbatim"
            );
            assert!(e.params.is_empty());
            assert!(e.return_ty.is_some());
        } else {
            panic!("expected ExternFn, got {:?}", it);
        }
    }

    #[test]
    fn parse_extern_c_abi_with_pointer_param() {
        // LoA-scene composability : explicit `"C"` ABI + raw-pointer params,
        // mirroring `__cssl_telemetry_emit(event_id, payload, len)`.
        let src = "extern \"C\" fn foo(p : *const u8) -> i32";
        let (f, toks) = prep(src);
        let mut c = TokenCursor::new(&toks);
        let mut bag = DiagnosticBag::new();
        let it = parse_item(&mut c, &mut bag).unwrap();
        assert_eq!(bag.error_count(), 0, "expected zero errors");
        if let Item::ExternFn(e) = it {
            let span = e.abi_span.expect("explicit ABI ⇒ abi_span must be Some");
            assert_eq!(f.slice(span.start, span.end), Some("\"C\""));
            assert_eq!(e.params.len(), 1);
            // Verify the pointer-type lowered to RawPointer{mutable=false}.
            use cssl_ast::TypeKind;
            assert!(
                matches!(
                    e.params[0].ty.kind,
                    TypeKind::RawPointer { mutable: false, .. }
                ),
                "expected `*const u8`, got {:?}",
                e.params[0].ty.kind
            );
        } else {
            panic!("expected ExternFn, got {:?}", it);
        }
    }

    #[test]
    fn parse_extern_c_abi_loa_acceptance_module() {
        // End-to-end acceptance corpus mirroring the prompt's `csslc check`
        // example. Both extern-"C" forwards + the trailing `fn main()` must
        // parse cleanly with zero parser errors.
        let src = "module com.apocky.loa.test_extern_c\n\
                   extern \"C\" fn __cssl_telemetry_emit(event_id : u32, payload : *const u8, \
                   len : i32) -> i32\n\
                   extern \"C\" fn __cssl_alloc(size : u64, align : u64) -> *mut u8\n\
                   fn main() -> i32 { 42 }";
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let mut bag = DiagnosticBag::new();
        let m = crate::rust_hybrid::parse_module(&f, &toks, &mut bag);
        assert_eq!(
            bag.error_count(),
            0,
            "expected zero parser errors for extern-\"C\" LoA acceptance corpus"
        );
        assert_eq!(m.items.len(), 3);
        assert!(matches!(m.items[0], Item::ExternFn(_)));
        assert!(matches!(m.items[1], Item::ExternFn(_)));
        assert!(matches!(m.items[2], Item::Fn(_)));
        // Both extern fns should carry an abi_span pointing at `"C"`.
        for (i, expected_quoted) in [(0_usize, "\"C\""), (1, "\"C\"")] {
            if let Item::ExternFn(e) = &m.items[i] {
                let span = e
                    .abi_span
                    .unwrap_or_else(|| panic!("item[{i}] missing abi_span"));
                assert_eq!(
                    f.slice(span.start, span.end),
                    Some(expected_quoted),
                    "item[{i}] abi_span text mismatch"
                );
            }
        }
    }
}
