//! T11-D40 — auto-monomorphization walker.
//!
//! § PURPOSE
//!
//! T11-D38 provided the `specialize_generic_fn` specialization API. T11-D39
//! plumbed turbofish `id::<i32>(5)` syntax through to `HirExprKind::Call.type_args`.
//! T11-D40 is the **discovery pass** that joins them : walk the HIR module,
//! find every `Call` whose callee is a generic fn and whose `type_args` are
//! populated, and produce one specialized `MirFunc` per unique (fn, type-arg-tuple)
//! combination.
//!
//! § PIPELINE
//!
//! ```text
//!   HirModule
//!     │
//!     ▼  index generic fn-decls by name
//!   fn_index : Symbol → &HirFn
//!     │
//!     ▼  walk every HirExpr ; collect Call nodes with non-empty type_args
//!   call_sites : Vec<(callee_sym, Vec<HirType>, HirId)>
//!     │
//!     ▼  deduplicate by (callee_sym, type_args_signature)
//!   unique_specs : Vec<(callee_sym, TypeSubst)>
//!     │
//!     ▼  invoke specialize_generic_fn per unique tuple
//!   [MirFunc]  ← append to MirModule
//! ```
//!
//! § SCOPE (this slice — T11-D40 MVP)
//!   - Single-segment path callees only (`id::<i32>(…)`, not `mod::id::<i32>(…)`).
//!   - Type-arg matching purely positional (no inference, no bounds checking).
//!   - No rewriting of existing MIR bodies — callers run the walker, receive the
//!     specialization list, and append to the `MirModule` themselves. Rewriting
//!     `func.call @id` → `func.call @id_i32` in already-lowered MIR bodies is
//!     deferred : requires threading a per-call-site mangled-name map through
//!     `lower_fn_body`'s call-lowering path.
//!   - Only `@differentiable fn` / plain `fn` items inspected at the top level ;
//!     impl / interface / effect / handler method bodies are scanned for call
//!     sites (since their bodies are HirBlock too) but the generic decls
//!     themselves must be top-level fn items (stage-0).
//!
//! § EXAMPLE
//!
//! ```ignore
//! let src = r"
//!   fn id<T>(x : T) -> T { x }
//!   fn main() -> i32 { id::<i32>(5) }
//! ";
//! let (hir, interner, _) = cssl_hir::lower_module(...);
//! let report = auto_monomorphize(&hir, &interner, None);
//! // `report.specializations` contains a MirFunc for `id_i32`.
//! ```

use std::collections::{HashMap, HashSet};

use cssl_ast::SourceFile;
use cssl_hir::{
    HirArrayExpr, HirBlock, HirCallArg, HirExpr, HirExprKind, HirFn, HirId, HirItem, HirMatchArm,
    HirModule, HirStmtKind, HirType, Interner, Symbol,
};

use crate::func::MirFunc;
use crate::monomorph::{mangle_specialization_name, specialize_generic_fn, TypeSubst};

/// Report returned by the auto-monomorphization walker.
#[derive(Debug, Clone, Default)]
pub struct AutoMonomorphReport {
    /// One `MirFunc` per unique (generic-fn, type-arg-tuple) tuple discovered
    /// at a call site. Callers append these to their `MirModule`.
    pub specializations: Vec<MirFunc>,
    /// Per-call-site resolution : maps each turbofish `Call`'s `HirId` to the
    /// mangled name of the `MirFunc` that should be invoked instead of the
    /// generic callee. Consumers that rewrite existing MIR bodies query by
    /// the call site's `HirId` (once body_lower exposes the mapping — deferred).
    pub call_site_names: HashMap<HirId, String>,
    /// Count of generic fn-decls indexed.
    pub generic_fn_count: u32,
    /// Count of turbofish call sites discovered.
    pub call_site_count: u32,
    /// Count of unique specializations emitted (≤ call_site_count).
    pub specialization_count: u32,
}

impl AutoMonomorphReport {
    /// Short diagnostic summary for the report.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "auto-monomorph : {} generic fns / {} call sites / {} unique specializations",
            self.generic_fn_count, self.call_site_count, self.specialization_count
        )
    }

    /// `true` iff no generic call sites needed specialization.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specializations.is_empty()
    }
}

/// Walk `module` and produce one `MirFunc` per unique generic-call-site
/// specialization.
///
/// § SOURCE THREADING
/// The `source` parameter is passed through to `specialize_generic_fn`'s body-
/// lowering pass so literal-value extraction (from `HirLiteral` spans) works.
/// Callers without a source can pass `None`.
#[must_use]
pub fn auto_monomorphize(
    module: &HirModule,
    interner: &Interner,
    source: Option<&SourceFile>,
) -> AutoMonomorphReport {
    let mut report = AutoMonomorphReport::default();

    // § Index generic fns by name. Non-generic fns are ignored (call sites
    //   referencing them don't need specialization).
    let mut fn_index: HashMap<Symbol, &HirFn> = HashMap::new();
    for item in &module.items {
        if let HirItem::Fn(f) = item {
            if !f.generics.params.is_empty() {
                fn_index.insert(f.name, f);
            }
        }
    }
    report.generic_fn_count = u32::try_from(fn_index.len()).unwrap_or(u32::MAX);

    // § Collect turbofish call sites across all fn bodies.
    let mut call_sites: Vec<(Symbol, Vec<HirType>, HirId)> = Vec::new();
    for item in &module.items {
        if let HirItem::Fn(f) = item {
            if let Some(body) = &f.body {
                collect_turbofish_calls(body, interner, &fn_index, &mut call_sites);
            }
        }
    }
    report.call_site_count = u32::try_from(call_sites.len()).unwrap_or(u32::MAX);

    // § Deduplicate by (fn-name, type-arg-mangle-key). Build a set of unique
    //   specializations and track which call sites map to which mangled name.
    let mut seen: HashSet<String> = HashSet::new();
    for (callee_sym, type_args, hir_id) in call_sites {
        let fn_decl = match fn_index.get(&callee_sym) {
            Some(f) => *f,
            None => continue,
        };

        // Build TypeSubst by zipping generics.params with type_args.
        let mut subst = TypeSubst::new();
        for (param, ty) in fn_decl.generics.params.iter().zip(type_args.iter()) {
            subst.bind(param.name, ty.clone());
        }

        let base_name = interner.resolve(fn_decl.name);
        let mangled = mangle_specialization_name(&base_name, interner, &subst);
        report.call_site_names.insert(hir_id, mangled.clone());

        if seen.insert(mangled.clone()) {
            // First occurrence — emit the specialization.
            let specialized = specialize_generic_fn(interner, source, fn_decl, &subst);
            report.specializations.push(specialized);
        }
    }
    report.specialization_count = u32::try_from(report.specializations.len()).unwrap_or(u32::MAX);

    report
}

// ═════════════════════════════════════════════════════════════════════════
// § HIR expression walker — collects every turbofish Call node.
// ═════════════════════════════════════════════════════════════════════════

fn collect_turbofish_calls(
    block: &HirBlock,
    interner: &Interner,
    fn_index: &HashMap<Symbol, &HirFn>,
    out: &mut Vec<(Symbol, Vec<HirType>, HirId)>,
) {
    for stmt in &block.stmts {
        match &stmt.kind {
            HirStmtKind::Let { value, .. } => {
                if let Some(v) = value {
                    collect_in_expr(v, interner, fn_index, out);
                }
            }
            HirStmtKind::Expr(e) => collect_in_expr(e, interner, fn_index, out),
            HirStmtKind::Item(_) => {}
        }
    }
    if let Some(t) = &block.trailing {
        collect_in_expr(t, interner, fn_index, out);
    }
}

#[allow(clippy::too_many_lines)] // one-match per HirExprKind variant
fn collect_in_expr(
    expr: &HirExpr,
    interner: &Interner,
    fn_index: &HashMap<Symbol, &HirFn>,
    out: &mut Vec<(Symbol, Vec<HirType>, HirId)>,
) {
    match &expr.kind {
        HirExprKind::Call {
            callee,
            args,
            type_args,
        } => {
            // Turbofish site detection : non-empty type_args + single-segment
            // path callee + callee maps to a known generic fn.
            if !type_args.is_empty() {
                if let HirExprKind::Path { segments, .. } = &callee.kind {
                    if segments.len() == 1 && fn_index.contains_key(&segments[0]) {
                        out.push((segments[0], type_args.clone(), expr.id));
                    }
                }
            }
            // Always recurse into callee + args (they may contain further calls).
            collect_in_expr(callee, interner, fn_index, out);
            for a in args {
                let a_expr = match a {
                    HirCallArg::Positional(e) | HirCallArg::Named { value: e, .. } => e,
                };
                collect_in_expr(a_expr, interner, fn_index, out);
            }
        }
        HirExprKind::Binary { lhs, rhs, .. } => {
            collect_in_expr(lhs, interner, fn_index, out);
            collect_in_expr(rhs, interner, fn_index, out);
        }
        HirExprKind::Unary { operand, .. } => collect_in_expr(operand, interner, fn_index, out),
        HirExprKind::Field { obj, .. } => collect_in_expr(obj, interner, fn_index, out),
        HirExprKind::Index { obj, index } => {
            collect_in_expr(obj, interner, fn_index, out);
            collect_in_expr(index, interner, fn_index, out);
        }
        HirExprKind::Block(b) => collect_turbofish_calls(b, interner, fn_index, out),
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_in_expr(cond, interner, fn_index, out);
            collect_turbofish_calls(then_branch, interner, fn_index, out);
            if let Some(e) = else_branch {
                collect_in_expr(e, interner, fn_index, out);
            }
        }
        HirExprKind::Match { scrutinee, arms } => {
            collect_in_expr(scrutinee, interner, fn_index, out);
            for arm in arms {
                let _: &HirMatchArm = arm; // doc link
                collect_in_expr(&arm.body, interner, fn_index, out);
                if let Some(g) = &arm.guard {
                    collect_in_expr(g, interner, fn_index, out);
                }
            }
        }
        HirExprKind::Return { value: Some(v) } => collect_in_expr(v, interner, fn_index, out),
        HirExprKind::Return { value: None } => {}
        HirExprKind::Break {
            value: Some(v), ..
        } => collect_in_expr(v, interner, fn_index, out),
        HirExprKind::Break { value: None, .. } => {}
        HirExprKind::Cast { expr: inner, .. } => collect_in_expr(inner, interner, fn_index, out),
        HirExprKind::Paren(inner) => collect_in_expr(inner, interner, fn_index, out),
        HirExprKind::Tuple(elems) => {
            for e in elems {
                collect_in_expr(e, interner, fn_index, out);
            }
        }
        HirExprKind::Array(arr) => match arr {
            HirArrayExpr::List(es) => {
                for e in es {
                    collect_in_expr(e, interner, fn_index, out);
                }
            }
            HirArrayExpr::Repeat { elem, len } => {
                collect_in_expr(elem, interner, fn_index, out);
                collect_in_expr(len, interner, fn_index, out);
            }
        },
        HirExprKind::Assign { lhs, rhs, .. } => {
            collect_in_expr(lhs, interner, fn_index, out);
            collect_in_expr(rhs, interner, fn_index, out);
        }
        HirExprKind::For { iter, body, .. } => {
            collect_in_expr(iter, interner, fn_index, out);
            collect_turbofish_calls(body, interner, fn_index, out);
        }
        HirExprKind::While { cond, body } => {
            collect_in_expr(cond, interner, fn_index, out);
            collect_turbofish_calls(body, interner, fn_index, out);
        }
        HirExprKind::Loop { body } => collect_turbofish_calls(body, interner, fn_index, out),
        HirExprKind::Range { lo, hi, .. } => {
            if let Some(l) = lo {
                collect_in_expr(l, interner, fn_index, out);
            }
            if let Some(h) = hi {
                collect_in_expr(h, interner, fn_index, out);
            }
        }
        HirExprKind::Pipeline { lhs, rhs, .. } => {
            collect_in_expr(lhs, interner, fn_index, out);
            collect_in_expr(rhs, interner, fn_index, out);
        }
        HirExprKind::TryDefault {
            expr: inner,
            default,
        } => {
            collect_in_expr(inner, interner, fn_index, out);
            collect_in_expr(default, interner, fn_index, out);
        }
        HirExprKind::Try { expr: inner } => collect_in_expr(inner, interner, fn_index, out),
        HirExprKind::Run { expr: inner } => collect_in_expr(inner, interner, fn_index, out),
        // Leaf + opaque variants : Path (already handled at Call site), Literal,
        // Lambda (body walked-in-own-context), Perform / With / Region / Compound /
        // SectionRef / Struct — stage-0 doesn't need them for generic call discovery.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::auto_monomorphize;
    use cssl_ast::{SourceFile, SourceId, Surface};
    use cssl_hir::lower_module;

    fn walk(src: &str) -> super::AutoMonomorphReport {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _) = lower_module(&f, &cst);
        auto_monomorphize(&hir, &interner, Some(&f))
    }

    #[test]
    fn empty_module_produces_empty_report() {
        let r = walk("");
        assert!(r.is_empty());
        assert_eq!(r.generic_fn_count, 0);
        assert_eq!(r.call_site_count, 0);
    }

    #[test]
    fn non_generic_fn_with_call_produces_no_specializations() {
        // Plain fn + plain call = no specialization needed.
        let r = walk("fn add(a : i32, b : i32) -> i32 { a + b } fn main() -> i32 { add(1, 2) }");
        assert_eq!(r.generic_fn_count, 0);
        assert_eq!(r.call_site_count, 0);
        assert!(r.is_empty());
    }

    #[test]
    fn generic_fn_without_call_is_indexed_but_not_specialized() {
        // Generic fn declared but never called ⇒ indexed, but no specializations.
        let r = walk("fn id<T>(x : T) -> T { x }");
        assert_eq!(r.generic_fn_count, 1);
        assert_eq!(r.call_site_count, 0);
        assert!(r.is_empty());
    }

    #[test]
    fn turbofish_call_triggers_single_specialization() {
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main() -> i32 { id::<i32>(5) }
        ";
        let r = walk(src);
        assert_eq!(r.generic_fn_count, 1);
        assert_eq!(r.call_site_count, 1);
        assert_eq!(r.specialization_count, 1);
        assert_eq!(r.specializations.len(), 1);
        assert_eq!(r.specializations[0].name, "id_i32");
    }

    #[test]
    fn two_distinct_type_args_produce_two_specializations() {
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main_i32() -> i32 { id::<i32>(5) }
            fn main_f32() -> f32 { id::<f32>(1.5) }
        ";
        let r = walk(src);
        assert_eq!(r.call_site_count, 2);
        assert_eq!(r.specialization_count, 2);
        let names: Vec<&str> = r.specializations.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"id_i32"));
        assert!(names.contains(&"id_f32"));
    }

    #[test]
    fn same_type_args_twice_produce_one_specialization_two_call_sites() {
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main_a() -> i32 { id::<i32>(5) }
            fn main_b() -> i32 { id::<i32>(7) }
        ";
        let r = walk(src);
        assert_eq!(r.call_site_count, 2, "two call sites discovered");
        assert_eq!(
            r.specialization_count, 1,
            "deduplicated to one specialization"
        );
        assert_eq!(r.specializations[0].name, "id_i32");
        // Both call sites should map to the same mangled name.
        let names: std::collections::HashSet<&String> = r.call_site_names.values().collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn two_generic_fns_each_with_one_call_produce_two_specializations() {
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn wrap<U>(y : U) -> U { y }
            fn main() -> i32 { id::<i32>(5) }
            fn main2() -> f32 { wrap::<f32>(2.5) }
        ";
        let r = walk(src);
        assert_eq!(r.generic_fn_count, 2);
        assert_eq!(r.call_site_count, 2);
        assert_eq!(r.specialization_count, 2);
    }

    #[test]
    fn multi_type_arg_generic_specializes_correctly() {
        let src = r"
            fn pair<T, U>(a : T, b : U) -> i32 { 0 }
            fn main() -> i32 { pair::<i32, f32>(1, 2.0) }
        ";
        let r = walk(src);
        assert_eq!(r.call_site_count, 1);
        assert_eq!(r.specialization_count, 1);
        // Mangling order is by param NAME (iter_sorted) — so T-binding then U-binding.
        assert_eq!(r.specializations[0].name, "pair_i32_f32");
    }

    #[test]
    fn nested_call_in_binary_op_is_discovered() {
        // Call inside a binary op — walker must recurse into lhs/rhs.
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main() -> i32 { id::<i32>(5) + 1 }
        ";
        let r = walk(src);
        assert_eq!(r.call_site_count, 1);
        assert_eq!(r.specialization_count, 1);
    }

    #[test]
    fn call_without_turbofish_not_captured_even_if_callee_generic() {
        // `id(5)` without turbofish ⇒ type_args empty ⇒ NOT captured. Type-
        // inference lands as a future slice. For now stage-0 requires
        // explicit turbofish.
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main() -> i32 { id(5) }
        ";
        let r = walk(src);
        assert_eq!(r.generic_fn_count, 1);
        assert_eq!(
            r.call_site_count, 0,
            "bare call on generic fn needs inference (follow-up)"
        );
        assert!(r.is_empty());
    }

    #[test]
    fn report_summary_shape_includes_all_three_counts() {
        let r = walk("fn id<T>(x : T) -> T { x } fn main() -> i32 { id::<i32>(5) }");
        let s = r.summary();
        assert!(s.contains("generic fns"));
        assert!(s.contains("call sites"));
        assert!(s.contains("specializations"));
    }

    #[test]
    fn call_site_names_map_records_mangled_name_per_hir_id() {
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main() -> i32 { id::<i32>(5) }
        ";
        let r = walk(src);
        assert_eq!(r.call_site_names.len(), 1);
        // Every recorded name must match a produced specialization.
        let spec_names: std::collections::HashSet<&str> =
            r.specializations.iter().map(|f| f.name.as_str()).collect();
        for name in r.call_site_names.values() {
            assert!(
                spec_names.contains(name.as_str()),
                "call_site_names referenced unknown spec `{name}`"
            );
        }
    }

    #[test]
    fn specialized_mirfunc_has_correct_signature() {
        // Regression guard : id_i32 must have (i32 → i32), not (T → T) or opaque.
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main() -> i32 { id::<i32>(5) }
        ";
        let r = walk(src);
        let id_i32 = &r.specializations[0];
        assert_eq!(id_i32.name, "id_i32");
        assert_eq!(
            id_i32.params,
            vec![crate::value::MirType::Int(crate::value::IntWidth::I32)]
        );
        assert_eq!(
            id_i32.results,
            vec![crate::value::MirType::Int(crate::value::IntWidth::I32)]
        );
    }
}
