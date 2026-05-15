//! § primitive_shape.rs · spec-70 § item-02 (A02.1 + A02.3)
//!
//! Surface-level primitive type-shape mismatches that the inference engine
//! in `infer.rs` collapses away.
//!
//! `infer::lower_hir_type` maps every integer primitive (i8 / i16 / i32 /
//! i64 / u8 / u16 / u32 / u64 / isize / usize) onto a single `Ty::Int`,
//! and every float primitive onto `Ty::Float`. That makes stage-0 inference
//! ergonomic but lets `fn f(x : u32) -> i64 { x }` pass type-check
//! silently — exactly the silent-coerce class spec-70 item-02 wants closed.
//!
//! This pass walks the resolved HIR AST (pre-inference) and, for every fn
//! body, compares the declared return type against the trailing expression
//! and any `return <expr>` exprs. The check is intentionally narrow to avoid
//! false positives during stage-0: only single-segment Paths resolving to
//! fn parameters and tuple-arity mismatches are flagged. Let-binding chains
//! and complex expressions are deferred to later passes.
//!
//! On a mismatch the pass emits an `Error`-severity `Diagnostic` carrying
//! the offending expression's span, a note pointing at the declared return
//! type, and (for numeric pairs) a `did you mean ... as <DECL>` help.
//!
//! See `compiler-rs/crates/cssl-mir/docs/A02_3_silent_path_audit.md` for
//! the companion `MirType::None` audit.

use std::collections::HashMap;

use cssl_ast::Diagnostic;

use crate::expr::{HirBlock, HirExpr, HirExprKind};
use crate::item::{HirFn, HirFnParam, HirItem, HirModule};
use crate::pat::HirPatternKind;
use crate::stmt::HirStmtKind;
use crate::symbol::{Interner, Symbol};
use crate::ty::{HirType, HirTypeKind};

/// Run the primitive-shape check on a resolved HIR module.
///
/// Returns a list of `Error`-severity `Diagnostic`s, each carrying the
/// span of the offending expression. The caller is responsible for
/// pushing them into a `DiagnosticBag` (see `lower::lower_module`).
#[must_use]
pub fn check_primitive_shape(module: &HirModule, interner: &Interner) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for item in &module.items {
        check_item(item, interner, &mut out);
    }
    out
}

fn check_item(item: &HirItem, interner: &Interner, out: &mut Vec<Diagnostic>) {
    match item {
        HirItem::Fn(f) => check_fn(f, interner, out),
        HirItem::Impl(i) => {
            for f in &i.fns {
                check_fn(f, interner, out);
            }
        }
        HirItem::Interface(i) => {
            for f in &i.fns {
                check_fn(f, interner, out);
            }
        }
        HirItem::Effect(e) => {
            for f in &e.ops {
                check_fn(f, interner, out);
            }
        }
        HirItem::Handler(h) => {
            for f in &h.ops {
                check_fn(f, interner, out);
            }
        }
        HirItem::Module(m) => {
            if let Some(items) = &m.items {
                for sub in items {
                    check_item(sub, interner, out);
                }
            }
        }
        HirItem::Struct(_)
        | HirItem::Enum(_)
        | HirItem::Const(_)
        | HirItem::TypeAlias(_)
        | HirItem::Use(_)
        | HirItem::ExternFn(_) => {}
    }
}

fn check_fn(f: &HirFn, interner: &Interner, out: &mut Vec<Diagnostic>) {
    let body = match &f.body {
        Some(b) => b,
        None => return,
    };
    let declared = match &f.return_ty {
        Some(t) => t,
        None => return,
    };
    let params = build_param_table(&f.params);
    if let Some(trail) = &body.trailing {
        check_expr_against_return(trail, declared, &params, interner, out);
    }
    walk_block_returns(body, declared, &params, interner, out);
}

fn build_param_table(params: &[HirFnParam]) -> HashMap<Symbol, HirType> {
    let mut tbl = HashMap::new();
    for p in params {
        if let HirPatternKind::Binding { name, .. } = &p.pat.kind {
            tbl.insert(*name, p.ty.clone());
        }
    }
    tbl
}

fn walk_block_returns(
    block: &HirBlock,
    declared: &HirType,
    params: &HashMap<Symbol, HirType>,
    interner: &Interner,
    out: &mut Vec<Diagnostic>,
) {
    for s in &block.stmts {
        if let HirStmtKind::Expr(e) = &s.kind {
            walk_expr_for_returns(e, declared, params, interner, out);
        }
        if let HirStmtKind::Let { value: Some(e), .. } = &s.kind {
            walk_expr_for_returns(e, declared, params, interner, out);
        }
    }
    if let Some(t) = &block.trailing {
        walk_expr_for_returns(t, declared, params, interner, out);
    }
}

/// Recurse into an expr looking for `return <inner>` ; when found, run the
/// shape-check on `<inner>` against the enclosing fn's declared return.
fn walk_expr_for_returns(
    e: &HirExpr,
    declared: &HirType,
    params: &HashMap<Symbol, HirType>,
    interner: &Interner,
    out: &mut Vec<Diagnostic>,
) {
    match &e.kind {
        HirExprKind::Return { value: Some(inner) } => {
            check_expr_against_return(inner, declared, params, interner, out);
        }
        HirExprKind::Return { value: None } => {
            // `return ;` — declared must be Unit / inferred. Emit when declared is a
            // non-unit primitive path.
            if let Some(decl_name) = primitive_path_name(declared, interner) {
                if decl_name != "()" {
                    out.push(mismatch_diag(decl_name, "()", e.span, declared));
                }
            }
        }
        HirExprKind::Block(b) => walk_block_returns(b, declared, params, interner, out),
        HirExprKind::If { then_branch, else_branch, .. } => {
            walk_block_returns(then_branch, declared, params, interner, out);
            if let Some(e) = else_branch {
                walk_expr_for_returns(e, declared, params, interner, out);
            }
        }
        HirExprKind::Match { arms, .. } => {
            for a in arms {
                walk_expr_for_returns(&a.body, declared, params, interner, out);
            }
        }
        HirExprKind::Loop { body }
        | HirExprKind::While { body, .. }
        | HirExprKind::For { body, .. } => {
            walk_block_returns(body, declared, params, interner, out);
        }
        HirExprKind::Paren(inner) => walk_expr_for_returns(inner, declared, params, interner, out),
        _ => {}
    }
}

/// Compare an expression's surface-shape against the declared return type.
/// Handles direct param paths + tuple-arity + paren-unwrap.
fn check_expr_against_return(
    e: &HirExpr,
    declared: &HirType,
    params: &HashMap<Symbol, HirType>,
    interner: &Interner,
    out: &mut Vec<Diagnostic>,
) {
    match &e.kind {
        HirExprKind::Paren(inner) => {
            check_expr_against_return(inner, declared, params, interner, out);
        }
        HirExprKind::Path { segments, .. } if segments.len() == 1 => {
            let name = segments[0];
            let Some(param_ty) = params.get(&name) else {
                return;
            };
            let (Some(actual), Some(expected)) = (
                primitive_path_name(param_ty, interner),
                primitive_path_name(declared, interner),
            ) else {
                return;
            };
            if actual != expected {
                out.push(mismatch_diag(expected, actual, e.span, declared));
            }
        }
        HirExprKind::Tuple(elems) => {
            // Tuple-arity check (FM.4) : if declared is a tuple, arities must match.
            if let HirTypeKind::Tuple { elems: decl_elems } = &declared.kind {
                if decl_elems.len() != elems.len() {
                    out.push(
                        Diagnostic::error(format!(
                            "type mismatch in fn return : expected tuple of arity {}, got tuple of arity {}",
                            decl_elems.len(),
                            elems.len()
                        ))
                        .with_span(e.span)
                        .with_labeled_note(
                            "declared return type here",
                            declared.span,
                        ),
                    );
                }
            }
        }
        _ => {}
    }
}

/// Build a canonical "primary span here, declared there" mismatch diagnostic.
fn mismatch_diag(expected: &str, actual: &str, expr_span: cssl_ast::Span, declared: &HirType) -> Diagnostic {
    let mut d = Diagnostic::error(format!(
        "type mismatch in fn return : expected {expected}, got {actual}"
    ))
    .with_span(expr_span)
    .with_labeled_note("declared return type here", declared.span);
    // Help-suggestion (item-89 § A89.2 overlap) : when both are numeric primitives,
    // suggest an explicit cast.
    if is_numeric_primitive(expected) && is_numeric_primitive(actual) {
        d = d.with_help(format!("did you mean `<expr> as {expected}`?"));
    }
    d
}

fn is_numeric_primitive(name: &str) -> bool {
    matches!(
        name,
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
            | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
            | "f16" | "f32" | "f64"
    )
}

/// Extract the primitive-name of a `HirType` if it is a single-segment path
/// matching one of the recognised primitive identifiers, or `"()"` for the
/// unit type. Returns `None` for any non-primitive (Named, Tuple of arity ≠ 0,
/// Reference, generic, etc.) so the rest of the pass can short-circuit.
fn primitive_path_name(t: &HirType, interner: &Interner) -> Option<&'static str> {
    match &t.kind {
        HirTypeKind::Tuple { elems } if elems.is_empty() => Some("()"),
        HirTypeKind::Path { path, .. } if path.len() == 1 => {
            let name = interner.resolve(path[0]);
            classify_primitive(name.as_str())
        }
        _ => None,
    }
}

fn classify_primitive(name: &str) -> Option<&'static str> {
    match name {
        "i8" => Some("i8"),
        "i16" => Some("i16"),
        "i32" => Some("i32"),
        "i64" => Some("i64"),
        "i128" => Some("i128"),
        "isize" => Some("isize"),
        "u8" => Some("u8"),
        "u16" => Some("u16"),
        "u32" => Some("u32"),
        "u64" => Some("u64"),
        "u128" => Some("u128"),
        "usize" => Some("usize"),
        "f16" => Some("f16"),
        "f32" => Some("f32"),
        "f64" => Some("f64"),
        "bool" => Some("bool"),
        "str" => Some("str"),
        "String" => Some("String"),
        "char" => Some("char"),
        "()" => Some("()"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::lower_module;
    use cssl_ast::{SourceFile, SourceId, Surface};

    fn lower(src: &str) -> (HirModule, Interner) {
        let file = SourceFile::new(SourceId::first(), "<test>", src, Surface::RustHybrid);
        let tokens = cssl_lex::lex(&file);
        let (cst, parse_bag) = cssl_parse::parse(&file, &tokens);
        assert_eq!(parse_bag.error_count(), 0, "parse errors: {parse_bag:?}");
        let (hir, interner, _bag) = lower_module(&file, &cst);
        (hir, interner)
    }

    #[test]
    fn a02_1_u32_param_returned_as_i64_emits_error() {
        // The canonical spec-70 § item-02 A02.1 case.
        let src = "fn f(x : u32) -> i64 { x }\n";
        let (hir, interner) = lower(src);
        let diags = check_primitive_shape(&hir, &interner);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
        let d = &diags[0];
        assert!(d.message.contains("i64"), "msg: {}", d.message);
        assert!(d.message.contains("u32"), "msg: {}", d.message);
        assert!(d.message.contains("expected"), "msg: {}", d.message);
        assert!(d.span.is_some(), "diagnostic must carry a span");
        // Help-text suggests `as <DECL>` for numeric pairs.
        assert!(
            d.notes.iter().any(|n| n.message.contains("as i64")),
            "notes: {:?}",
            d.notes
        );
    }

    #[test]
    fn a02_1_i32_param_returned_as_u32_emits_error() {
        let src = "fn f(x : i32) -> u32 { x }\n";
        let (hir, interner) = lower(src);
        let diags = check_primitive_shape(&hir, &interner);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
        assert!(diags[0].message.contains("u32"));
        assert!(diags[0].message.contains("i32"));
    }

    #[test]
    fn cross_class_f32_to_i32_emits_error() {
        let src = "fn f(x : f32) -> i32 { x }\n";
        let (hir, interner) = lower(src);
        let diags = check_primitive_shape(&hir, &interner);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
    }

    #[test]
    fn matching_primitive_passes() {
        let src = "fn f(x : i64) -> i64 { x }\n";
        let (hir, interner) = lower(src);
        let diags = check_primitive_shape(&hir, &interner);
        assert!(diags.is_empty(), "got: {diags:?}");
    }

    #[test]
    fn integer_literal_does_not_trigger() {
        // Stage-0 inference treats integer literals as Ty::Int and they may
        // legitimately bind to any int width. The pass MUST stay narrow and
        // not emit on literal-returns.
        let src = "fn f() -> i64 { 42 }\n";
        let (hir, interner) = lower(src);
        let diags = check_primitive_shape(&hir, &interner);
        assert!(diags.is_empty(), "got: {diags:?}");
    }

    #[test]
    fn return_statement_form_also_triggers() {
        let src = "fn f(x : u32) -> i64 { return x; }\n";
        let (hir, interner) = lower(src);
        let diags = check_primitive_shape(&hir, &interner);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
    }

    #[test]
    fn tuple_arity_mismatch_emits_error() {
        let src = "fn f() -> (i32, i32) { (1, 2, 3) }\n";
        let (hir, interner) = lower(src);
        let diags = check_primitive_shape(&hir, &interner);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
        assert!(diags[0].message.contains("arity"));
    }

    #[test]
    fn paren_wrap_is_transparent() {
        let src = "fn f(x : u32) -> i64 { (x) }\n";
        let (hir, interner) = lower(src);
        let diags = check_primitive_shape(&hir, &interner);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
    }

    #[test]
    fn non_param_path_does_not_trigger() {
        // Path that does NOT match any param — out of scope for this narrow
        // pass (resolver / inference handles it). Must NOT panic / emit.
        let src = "fn f() -> i64 { foo }\n";
        let (hir, interner) = lower(src);
        let diags = check_primitive_shape(&hir, &interner);
        assert!(diags.is_empty(), "got: {diags:?}");
    }
}
