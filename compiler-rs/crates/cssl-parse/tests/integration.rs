//! End-to-end parser integration tests : lex → parse → CST inspection.
//!
//! § These tests exercise the public `cssl_parse::parse(source, tokens)` entry point on
//!   realistic multi-item fragments covering both surfaces.

use cssl_ast::{ExprKind, Item, SourceFile, SourceId, StructBody, Surface};
use cssl_parse::parse;

fn lex_parse(src: &str, surface: Surface) -> (cssl_ast::Module, cssl_ast::DiagnosticBag) {
    let file = SourceFile::new(SourceId::first(), "<integration>", src, surface);
    let tokens = cssl_lex::lex(&file);
    parse(&file, &tokens)
}

// ─ Rust-hybrid fragments ────────────────────────────────────────────────────

#[test]
fn rust_hybrid_empty() {
    let (m, bag) = lex_parse("", Surface::RustHybrid);
    assert!(m.items.is_empty());
    assert_eq!(bag.error_count(), 0);
}

#[test]
fn rust_hybrid_single_fn() {
    let src = "fn hello() -> i32 { 42 }";
    let (m, bag) = lex_parse(src, Surface::RustHybrid);
    assert_eq!(m.items.len(), 1);
    assert_eq!(bag.error_count(), 0);
    match &m.items[0] {
        Item::Fn(f) => {
            assert!(f.body.is_some());
            assert!(f.return_ty.is_some());
        }
        _ => panic!("expected Fn"),
    }
}

#[test]
fn rust_hybrid_struct_enum_use() {
    let src = r"
        use std::math::vec3 ;
        struct Point { x : f32, y : f32 }
        enum Option<T> { Some(T), None }
    ";
    let (m, bag) = lex_parse(src, Surface::RustHybrid);
    assert_eq!(bag.error_count(), 0, "{:?}", bag.iter().collect::<Vec<_>>());
    assert_eq!(m.items.len(), 3);
    assert!(matches!(m.items[0], Item::Use(_)));
    if let Item::Struct(s) = &m.items[1] {
        if let StructBody::Named(fields) = &s.body {
            assert_eq!(fields.len(), 2);
        } else {
            panic!("expected named struct");
        }
    } else {
        panic!("item 1 should be Struct");
    }
    if let Item::Enum(e) = &m.items[2] {
        assert_eq!(e.variants.len(), 2);
    } else {
        panic!("item 2 should be Enum");
    }
}

#[test]
fn rust_hybrid_fn_with_generics_and_effects() {
    let src = r"
        fn render<S>(cam : Camera) -> Image / {GPU, NoAlloc} {
            cam
        }
    ";
    let (m, bag) = lex_parse(src, Surface::RustHybrid);
    assert_eq!(bag.error_count(), 0);
    assert_eq!(m.items.len(), 1);
    if let Item::Fn(f) = &m.items[0] {
        assert_eq!(f.generics.params.len(), 1);
        assert!(f.effect_row.is_some());
        assert_eq!(f.effect_row.as_ref().unwrap().effects.len(), 2);
    } else {
        panic!("expected Fn");
    }
}

#[test]
fn rust_hybrid_attributed_fn() {
    let src = r"
        @differentiable
        @lipschitz(k = 1.0)
        fn sphere_sdf(p : vec3, r : f32) -> f32 {
            p - r
        }
    ";
    let (m, bag) = lex_parse(src, Surface::RustHybrid);
    assert_eq!(bag.error_count(), 0);
    if let Item::Fn(f) = &m.items[0] {
        assert_eq!(f.attrs.len(), 2);
    } else {
        panic!("expected Fn");
    }
}

#[test]
fn rust_hybrid_module_path_declaration() {
    let src = r"
        module com.apocky.loa
        fn f() {}
    ";
    let (m, _bag) = lex_parse(src, Surface::RustHybrid);
    assert!(m.path.is_some());
    assert_eq!(m.path.as_ref().unwrap().segments.len(), 3);
    assert_eq!(m.items.len(), 1);
}

#[test]
fn rust_hybrid_precedence_and_pipeline() {
    // `x + 2 * 3` — mul binds tighter than add ; pipeline is lowest.
    let src = "fn f() -> i32 { 1 + 2 * 3 |> double }";
    let (m, bag) = lex_parse(src, Surface::RustHybrid);
    assert_eq!(bag.error_count(), 0);
    if let Item::Fn(f) = &m.items[0] {
        let body = f.body.as_ref().expect("body");
        let trailing = body.trailing.as_ref().expect("trailing expr");
        assert!(matches!(trailing.kind, ExprKind::Pipeline { .. }));
    } else {
        panic!("expected Fn");
    }
}

#[test]
fn rust_hybrid_match_arm() {
    let src = r"
        fn test(x : i32) -> i32 {
            match x { 0 => 1, _ => 2 }
        }
    ";
    let (_m, bag) = lex_parse(src, Surface::RustHybrid);
    assert_eq!(bag.error_count(), 0);
}

// ─ CSLv3-native fragments ───────────────────────────────────────────────────

#[test]
fn csl_native_empty() {
    let (m, bag) = lex_parse("", Surface::CslNative);
    assert!(m.items.is_empty());
    assert_eq!(bag.error_count(), 0);
}

#[test]
fn csl_native_single_section() {
    let src = "§ foo\n";
    let (m, _bag) = lex_parse(src, Surface::CslNative);
    assert_eq!(m.items.len(), 1);
    assert!(matches!(m.items[0], Item::Module(_)));
}

#[test]
fn csl_native_multiple_sections() {
    let src = "§ a\n§ b\n§ c\n";
    let (m, _bag) = lex_parse(src, Surface::CslNative);
    assert_eq!(m.items.len(), 3);
}

// ─ Dispatch (Surface::Auto) ─────────────────────────────────────────────────

#[test]
fn auto_dispatches_rust_from_fn_keyword() {
    // Surface::Auto with a `fn` opener should fall into Rust-hybrid path.
    let (m, bag) = lex_parse("fn f() {}", Surface::Auto);
    assert_eq!(m.items.len(), 1);
    assert_eq!(bag.error_count(), 0);
}

// ─ T11-D39 : turbofish propagation ─────────────────────────────────────────

fn find_fn_body_trailing(src: &str) -> cssl_ast::Expr {
    let (m, bag) = lex_parse(src, Surface::RustHybrid);
    assert_eq!(bag.error_count(), 0, "expected clean parse : {src}");
    let f = m
        .items
        .iter()
        .find_map(|it| match it {
            Item::Fn(f) => Some(f),
            _ => None,
        })
        .expect("fn item");
    let body = f.body.as_ref().expect("fn body");
    body.trailing
        .as_ref()
        .expect("trailing expression")
        .as_ref()
        .clone()
}

#[test]
fn turbofish_call_captures_type_args() {
    // `id::<i32>(5)` → Call { type_args: [i32], args: [5] }.
    let src = "fn wrapper() -> i32 { id::<i32>(5) }";
    let trailing = find_fn_body_trailing(src);
    match &trailing.kind {
        ExprKind::Call {
            type_args, args, ..
        } => {
            assert_eq!(
                type_args.len(),
                1,
                "turbofish must carry 1 type-arg : got {}",
                type_args.len()
            );
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn turbofish_call_with_two_type_args() {
    // `pair::<i32, f32>(1, 2.0)` → Call { type_args: [i32, f32], args: [1, 2.0] }.
    let src = "fn wrapper() -> i32 { pair::<i32, f32>(1, 2.0) }";
    let trailing = find_fn_body_trailing(src);
    match &trailing.kind {
        ExprKind::Call {
            type_args, args, ..
        } => {
            assert_eq!(type_args.len(), 2, "expected 2 type-args");
            assert_eq!(args.len(), 2, "expected 2 args");
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn non_turbofish_call_has_empty_type_args() {
    // Regression guard : `f(5)` — no turbofish ⇒ empty type_args.
    let src = "fn wrapper() -> i32 { f(5) }";
    let trailing = find_fn_body_trailing(src);
    match &trailing.kind {
        ExprKind::Call {
            type_args, args, ..
        } => {
            assert!(type_args.is_empty(), "plain call must have empty type_args");
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn turbofish_call_with_no_args() {
    // `make::<i32>()` — 1 type-arg, 0 regular args.
    let src = "fn wrapper() -> i32 { make::<i32>() }";
    let trailing = find_fn_body_trailing(src);
    match &trailing.kind {
        ExprKind::Call {
            type_args, args, ..
        } => {
            assert_eq!(type_args.len(), 1);
            assert!(args.is_empty());
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

// ─ Error recovery ───────────────────────────────────────────────────────────

#[test]
fn unknown_top_level_produces_diagnostic_not_panic() {
    // Leading `42` is not a legal item-starter; parser should push a diagnostic and
    // advance, then continue and find the following `fn`.
    let src = "42 fn ok() {}";
    let (m, bag) = lex_parse(src, Surface::RustHybrid);
    assert!(bag.has_errors());
    // The fn should still parse after error-recovery.
    assert!(m.items.iter().any(|it| matches!(it, Item::Fn(_))));
}
