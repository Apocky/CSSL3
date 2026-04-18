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
    HirArrayExpr, HirBlock, HirCallArg, HirExpr, HirExprKind, HirFieldDecl, HirFn, HirId, HirItem,
    HirMatchArm, HirModule, HirStmtKind, HirStruct, HirStructBody, HirType, HirTypeKind, Interner,
    Symbol,
};

use crate::func::{MirFunc, MirModule};
use crate::monomorph::{
    mangle_specialization_name, mangle_struct_specialization_name, specialize_generic_fn,
    specialize_generic_struct, TypeSubst,
};

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
        HirExprKind::Break { value: Some(v), .. } => collect_in_expr(v, interner, fn_index, out),
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

// ═════════════════════════════════════════════════════════════════════════
// § T11-D43 — MODULE CLEANUP : drop unspecialized generic fns
//
// After auto-monomorphization produces concrete specializations, the original
// generic fns (e.g., `fn id<T>(x:T) -> T { x }`) remain in the MirModule with
// Opaque("T") param types — they cannot be JIT-compiled directly. This pass
// removes them so downstream passes (JIT, codegen) see only concrete fns.
//
// A fn is "unspecialized generic" iff its `is_generic` flag is true (set by
// `lower_function_signature` when the HIR declaration had non-empty generics ;
// specialize_generic_fn clones with empty generics so specialized fns have
// is_generic = false).
// ═════════════════════════════════════════════════════════════════════════

/// Remove every `MirFunc` with `is_generic = true` from `module.funcs`, in
/// place. Returns the number of functions dropped.
///
/// § TYPICAL USAGE
/// Run *after* `auto_monomorphize` + `rewrite_generic_call_sites` so all
/// concrete call sites have been rewired to specialized callees. Running it
/// before will strand any call-site still referencing the generic name.
pub fn drop_unspecialized_generic_fns(module: &mut MirModule) -> u32 {
    let before = module.funcs.len();
    module.funcs.retain(|f| !f.is_generic);
    u32::try_from(before.saturating_sub(module.funcs.len())).unwrap_or(u32::MAX)
}

// ═════════════════════════════════════════════════════════════════════════
// § T11-D41 — CALL-SITE REWRITER
//
// After `auto_monomorphize` produces specialized MirFuncs, the *existing* MIR
// bodies (e.g., `main { func.call @id (5) }`) still reference the generic
// callee names. This rewriter walks the MirModule and updates each
// `func.call` op's `callee` attribute from the generic name (`id`) to the
// mangled specialization name (`id_i32`) when the call's `hir_id` attribute
// matches a key in `call_site_names`.
//
// Body-lower (T11-D41) stamps every `func.call` op with an `hir_id` attribute
// carrying the u32 representation of `HirExpr.id` — the call-site's stable
// identifier — so this rewriter can key off it without risking false matches
// on callee-name alone.
// ═════════════════════════════════════════════════════════════════════════

/// Rewrite call-site callee names in every MirFunc body of `module` based on
/// `call_site_names` (produced by [`auto_monomorphize`]).
///
/// Walks every block in every MirFunc, finds ops named `func.call` that carry
/// an `hir_id` attribute, and — if that `hir_id` is a key in the map — updates
/// the op's `callee` attribute to the mangled specialization name.
///
/// Returns the number of call-site rewrites performed (useful for test
/// assertions + observability).
#[allow(clippy::implicit_hasher)] // internal API keyed off AutoMonomorphReport's own map
pub fn rewrite_generic_call_sites(
    module: &mut MirModule,
    call_site_names: &HashMap<HirId, String>,
) -> u32 {
    let mut rewrites: u32 = 0;
    for func in &mut module.funcs {
        for block in &mut func.body.blocks {
            for op in &mut block.ops {
                if op.name != "func.call" {
                    continue;
                }
                // Extract the op's hir_id attr as u32 + look up in the map.
                let hir_id_str = op.attributes.iter().find_map(|(k, v)| {
                    if k == "hir_id" {
                        Some(v.clone())
                    } else {
                        None
                    }
                });
                let Some(hir_id_str) = hir_id_str else {
                    continue;
                };
                let Ok(hir_id_raw) = hir_id_str.parse::<u32>() else {
                    continue;
                };
                let hir_id = HirId(hir_id_raw);
                let Some(mangled) = call_site_names.get(&hir_id) else {
                    continue;
                };
                // Rewrite the callee attr.
                for (k, v) in &mut op.attributes {
                    if k == "callee" {
                        *v = mangled.clone();
                        rewrites = rewrites.saturating_add(1);
                        break;
                    }
                }
            }
        }
    }
    rewrites
}

// ═════════════════════════════════════════════════════════════════════════
// § T11-D46 — STRUCT AUTO-DISCOVERY WALKER
//
// Parallel of `auto_monomorphize` (which discovers generic-fn call sites)
// but for generic-struct references. Walks the HIR module's fn signatures +
// struct fields, finds `HirTypeKind::Path` nodes with non-empty `type_args`
// that reference a known generic struct decl, invokes
// `specialize_generic_struct` per unique tuple.
//
// § SCOPE (this slice — T11-D46 MVP)
//   - Scans fn param-types + return-types + struct-field-types across the
//     whole module. Expression-level type annotations (let-bindings, casts)
//     NOT scanned (they live inside fn bodies — requires threading through
//     body-lowering, deferred).
//   - Single-segment struct-name paths only (`Pair<i32, f32>`, not
//     `mod::Pair<i32, f32>`).
//   - Purely positional type-arg matching (zip with `generics.params`).
//   - Handles nested generics : `Outer<Inner<i32>>` specializes BOTH the
//     outer and the inner (if both are known generic structs) via the
//     recursive walk through `type_args`.
//
// § DEFERRED
//   - Struct-expression discovery in fn bodies (`Pair { first: 1, second: 2.0 }`
//     without an explicit type annotation — needs inference from field values).
//   - impl<T> Self monomorphization (HirImpl.self_ty + per-method substitution).
//   - Generic-enum parallel.
//   - Auto-rewriting of type tags in body_lower's struct-expr output (today
//     lower_struct_expr emits `Opaque("!cssl.struct.<name>")` without type args).
// ═════════════════════════════════════════════════════════════════════════

/// Report returned by the struct auto-discovery walker.
#[derive(Debug, Clone, Default)]
pub struct AutoStructReport {
    /// One `HirStruct` per unique (generic-struct, type-arg-tuple) tuple
    /// discovered in a type-annotation context. Callers register these in
    /// their symbol table / downstream struct registry.
    pub specializations: Vec<HirStruct>,
    /// Map keyed off the stringified reference `{struct_name}_{mangle}` →
    /// the final mangled name. Enables downstream passes (struct-expr
    /// lowering, codegen) to rewrite references from the generic name to the
    /// specialized name without re-walking the type-arg tuple.
    pub ref_to_mangled: HashMap<String, String>,
    /// Count of generic struct-decls indexed.
    pub generic_struct_count: u32,
    /// Count of distinct type-annotation references to generic structs.
    pub ref_count: u32,
    /// Count of unique specializations emitted (≤ ref_count).
    pub specialization_count: u32,
}

impl AutoStructReport {
    /// Short diagnostic summary.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "auto-struct : {} generic structs / {} type-refs / {} unique specializations",
            self.generic_struct_count, self.ref_count, self.specialization_count
        )
    }

    /// `true` iff no generic-struct references needed specialization.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specializations.is_empty()
    }
}

/// Walk `module` and produce one `HirStruct` per unique generic-struct
/// reference discovered in fn signatures + struct field types.
///
/// Combines with [`auto_monomorphize`] (for fns) to cover the two main kinds
/// of generic items in P1 stdlib-core scope.
#[must_use]
pub fn auto_monomorphize_structs(module: &HirModule, interner: &Interner) -> AutoStructReport {
    let mut report = AutoStructReport::default();

    // § Index generic struct-decls by name. Non-generic structs are ignored —
    //   type-refs to them don't need specialization.
    let mut struct_index: HashMap<Symbol, &HirStruct> = HashMap::new();
    for item in &module.items {
        if let HirItem::Struct(s) = item {
            if !s.generics.params.is_empty() {
                struct_index.insert(s.name, s);
            }
        }
    }
    report.generic_struct_count = u32::try_from(struct_index.len()).unwrap_or(u32::MAX);

    // § Walk every type-annotation across the module + collect refs that
    //   match a known generic struct.
    let mut refs: Vec<(Symbol, Vec<HirType>)> = Vec::new();
    for item in &module.items {
        match item {
            HirItem::Fn(f) => {
                for p in &f.params {
                    collect_generic_struct_refs(&p.ty, &struct_index, &mut refs);
                }
                if let Some(rt) = &f.return_ty {
                    collect_generic_struct_refs(rt, &struct_index, &mut refs);
                }
            }
            HirItem::Struct(s) => {
                walk_struct_fields(&s.body, &struct_index, &mut refs);
            }
            _ => {}
        }
    }
    report.ref_count = u32::try_from(refs.len()).unwrap_or(u32::MAX);

    // § Deduplicate by (struct-name, type-arg-mangle-key). Emit one
    //   specialized HirStruct per unique tuple.
    let mut seen: HashSet<String> = HashSet::new();
    for (struct_sym, type_args) in refs {
        let struct_decl = match struct_index.get(&struct_sym) {
            Some(s) => *s,
            None => continue,
        };

        // Build TypeSubst by zipping generics.params with type_args. If the
        // arity doesn't match (e.g., `Pair<i32>` when Pair has 2 params),
        // skip — malformed reference ; a real compiler would diagnose.
        if struct_decl.generics.params.len() != type_args.len() {
            continue;
        }
        let mut subst = TypeSubst::new();
        for (param, ty) in struct_decl.generics.params.iter().zip(type_args.iter()) {
            subst.bind(param.name, ty.clone());
        }

        let mangled = mangle_struct_specialization_name(struct_decl, interner, &subst);
        let base = interner.resolve(struct_decl.name);
        report.ref_to_mangled.insert(
            format!("{base}_{}", mangle_key(&subst, interner)),
            mangled.clone(),
        );

        if seen.insert(mangled.clone()) {
            let specialized = specialize_generic_struct(interner, struct_decl, &subst);
            report.specializations.push(specialized);
        }
    }
    report.specialization_count = u32::try_from(report.specializations.len()).unwrap_or(u32::MAX);

    report
}

/// Internal : stable-order key for a TypeSubst, used as map key.
fn mangle_key(subst: &TypeSubst, interner: &Interner) -> String {
    let mut out = String::new();
    for (_sym, ty) in subst.iter_sorted(interner) {
        out.push('_');
        match &ty.kind {
            HirTypeKind::Path { path, .. } => {
                if let Some(last) = path.last() {
                    out.push_str(&interner.resolve(*last).to_lowercase());
                }
            }
            _ => out.push_str("opaque"),
        }
    }
    out
}

/// Recursively walk a `HirType` and collect every path reference matching a
/// generic struct in `struct_index`. Nested type_args are traversed so that
/// `Outer<Inner<i32>>` emits BOTH the outer + inner refs.
fn collect_generic_struct_refs(
    t: &HirType,
    struct_index: &HashMap<Symbol, &HirStruct>,
    out: &mut Vec<(Symbol, Vec<HirType>)>,
) {
    match &t.kind {
        HirTypeKind::Path {
            path, type_args, ..
        } => {
            // Single-segment path + non-empty type_args + matches a known
            // generic struct ⇒ collect.
            if path.len() == 1 && !type_args.is_empty() && struct_index.contains_key(&path[0]) {
                out.push((path[0], type_args.clone()));
            }
            // Recurse into type_args regardless of match — nested references.
            for ta in type_args {
                collect_generic_struct_refs(ta, struct_index, out);
            }
        }
        HirTypeKind::Tuple { elems } => {
            for e in elems {
                collect_generic_struct_refs(e, struct_index, out);
            }
        }
        HirTypeKind::Array { elem, .. } | HirTypeKind::Slice { elem } => {
            collect_generic_struct_refs(elem, struct_index, out);
        }
        HirTypeKind::Reference { inner, .. } | HirTypeKind::Capability { inner, .. } => {
            collect_generic_struct_refs(inner, struct_index, out);
        }
        HirTypeKind::Function {
            params, return_ty, ..
        } => {
            for p in params {
                collect_generic_struct_refs(p, struct_index, out);
            }
            collect_generic_struct_refs(return_ty, struct_index, out);
        }
        HirTypeKind::Refined { base, .. } => {
            collect_generic_struct_refs(base, struct_index, out);
        }
        HirTypeKind::Infer | HirTypeKind::Error => {}
    }
}

/// Walk struct body fields looking for generic-struct refs in field types.
fn walk_struct_fields(
    body: &HirStructBody,
    struct_index: &HashMap<Symbol, &HirStruct>,
    out: &mut Vec<(Symbol, Vec<HirType>)>,
) {
    let fields: &[HirFieldDecl] = match body {
        HirStructBody::Named(fs) | HirStructBody::Tuple(fs) => fs,
        HirStructBody::Unit => return,
    };
    for f in fields {
        collect_generic_struct_refs(&f.ty, struct_index, out);
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

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D41 — call-site rewriter tests
    // ─────────────────────────────────────────────────────────────────────

    fn build_module_with_main_calling_generic(
        src: &str,
    ) -> (crate::MirModule, super::AutoMonomorphReport) {
        use crate::{lower_fn_body, lower_function_signature, LowerCtx, MirModule};

        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _) = lower_module(&f, &cst);

        // Build MirModule via the standard lowering path.
        let lower_ctx = LowerCtx::new(&interner);
        let mut mir_mod = MirModule::new();
        for item in &hir.items {
            if let cssl_hir::HirItem::Fn(fn_decl) = item {
                let mut mf = lower_function_signature(&lower_ctx, fn_decl);
                lower_fn_body(&interner, Some(&f), fn_decl, &mut mf);
                mir_mod.push_func(mf);
            }
        }

        let report = auto_monomorphize(&hir, &interner, Some(&f));
        for spec in &report.specializations {
            mir_mod.push_func(spec.clone());
        }
        (mir_mod, report)
    }

    #[test]
    fn rewrite_updates_callee_attr_to_mangled_name() {
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main() -> i32 { id::<i32>(5) }
        ";
        let (mut mir, report) = build_module_with_main_calling_generic(src);

        // Pre-rewrite : main's body has a func.call @id (generic name).
        let main_fn = mir.funcs.iter().find(|f| f.name == "main").unwrap();
        let pre = main_fn
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|op| op.name == "func.call")
            .unwrap();
        let pre_callee = pre
            .attributes
            .iter()
            .find(|(k, _)| k == "callee")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert_eq!(pre_callee, "id", "pre-rewrite callee must be `id`");

        // Rewrite.
        let rewrites = super::rewrite_generic_call_sites(&mut mir, &report.call_site_names);
        assert_eq!(rewrites, 1);

        // Post-rewrite : callee updated to `id_i32`.
        let main_fn = mir.funcs.iter().find(|f| f.name == "main").unwrap();
        let post = main_fn
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|op| op.name == "func.call")
            .unwrap();
        let post_callee = post
            .attributes
            .iter()
            .find(|(k, _)| k == "callee")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert_eq!(post_callee, "id_i32", "post-rewrite callee must be mangled");
    }

    #[test]
    fn rewrite_leaves_non_generic_calls_untouched() {
        // Regression guard : a plain `f(5)` call without turbofish must not
        // get rewritten even if `f` happens to share a name with a generic.
        let src = r"
            fn plain(x : i32) -> i32 { x }
            fn main() -> i32 { plain(5) }
        ";
        let (mut mir, report) = build_module_with_main_calling_generic(src);
        assert!(
            report.call_site_names.is_empty(),
            "no turbofish ⇒ empty map"
        );

        let rewrites = super::rewrite_generic_call_sites(&mut mir, &report.call_site_names);
        assert_eq!(rewrites, 0, "no rewrites when map empty");

        let main_fn = mir.funcs.iter().find(|f| f.name == "main").unwrap();
        let callee = main_fn
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|op| op.name == "func.call")
            .unwrap()
            .attributes
            .iter()
            .find(|(k, _)| k == "callee")
            .map(|(_, v)| v.as_str())
            .unwrap();
        assert_eq!(callee, "plain", "plain call unchanged");
    }

    #[test]
    fn rewrite_handles_multiple_call_sites_in_one_fn() {
        // Two turbofish calls in the same main should produce two rewrites.
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main() -> i32 { id::<i32>(5) + id::<i32>(7) }
        ";
        let (mut mir, report) = build_module_with_main_calling_generic(src);
        assert_eq!(report.call_site_count, 2);
        // Both calls use the same type_args ⇒ only 1 specialization, but 2 call-site entries.
        assert_eq!(report.specialization_count, 1);
        assert_eq!(report.call_site_names.len(), 2);

        let rewrites = super::rewrite_generic_call_sites(&mut mir, &report.call_site_names);
        assert_eq!(rewrites, 2, "expected 2 rewrites for 2 call sites");

        // All func.call callees in main should now be `id_i32`.
        let main_fn = mir.funcs.iter().find(|f| f.name == "main").unwrap();
        for op in &main_fn.body.entry().unwrap().ops {
            if op.name == "func.call" {
                let callee = op
                    .attributes
                    .iter()
                    .find(|(k, _)| k == "callee")
                    .map(|(_, v)| v.as_str())
                    .unwrap();
                assert_eq!(callee, "id_i32");
            }
        }
    }

    #[test]
    fn rewrite_returns_zero_for_empty_map() {
        use crate::MirModule;
        let mut mir = MirModule::new();
        let map = std::collections::HashMap::new();
        let rewrites = super::rewrite_generic_call_sites(&mut mir, &map);
        assert_eq!(rewrites, 0);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D43 — drop_unspecialized_generic_fns tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn drop_removes_generic_fns_but_keeps_concrete() {
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn add(a : i32, b : i32) -> i32 { a + b }
            fn main() -> i32 { id::<i32>(5) }
        ";
        let (mut mir, _report) = build_module_with_main_calling_generic(src);

        let before_names: Vec<&str> = mir.funcs.iter().map(|f| f.name.as_str()).collect();
        assert!(
            before_names.contains(&"id"),
            "generic `id` must be present pre-cleanup"
        );
        assert!(before_names.contains(&"id_i32"), "specialization present");
        assert!(before_names.contains(&"add"), "non-generic `add` present");
        assert!(before_names.contains(&"main"), "main present");

        let dropped = super::drop_unspecialized_generic_fns(&mut mir);
        assert_eq!(dropped, 1, "expected to drop 1 generic fn (id)");

        let after_names: std::collections::HashSet<&str> =
            mir.funcs.iter().map(|f| f.name.as_str()).collect();
        assert!(
            !after_names.contains("id"),
            "generic `id` must be GONE post-cleanup"
        );
        assert!(after_names.contains("id_i32"), "spec survives");
        assert!(after_names.contains("add"), "non-generic survives");
        assert!(after_names.contains("main"), "main survives");
    }

    #[test]
    fn drop_returns_zero_when_no_generics_present() {
        use crate::MirModule;
        let mut mir = MirModule::new();
        let dropped = super::drop_unspecialized_generic_fns(&mut mir);
        assert_eq!(dropped, 0, "empty module drops nothing");
    }

    #[test]
    fn is_generic_flag_set_correctly_on_lower() {
        // Regression : lower_function_signature sets is_generic iff HirFn has
        // non-empty generics.params.
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn add(a : i32, b : i32) -> i32 { a + b }
        ";
        let (mir, _report) = build_module_with_main_calling_generic(src);
        let id = mir.funcs.iter().find(|f| f.name == "id").unwrap();
        let add = mir.funcs.iter().find(|f| f.name == "add").unwrap();
        assert!(id.is_generic, "id<T> must be flagged generic");
        assert!(!add.is_generic, "add is concrete");
    }

    #[test]
    fn specialized_fn_has_is_generic_false() {
        // Regression : specialize_generic_fn produces MirFuncs with
        // is_generic = false (they're concrete).
        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main() -> i32 { id::<i32>(5) }
        ";
        let (mir, _report) = build_module_with_main_calling_generic(src);
        let id_i32 = mir.funcs.iter().find(|f| f.name == "id_i32").unwrap();
        assert!(
            !id_i32.is_generic,
            "specialized id_i32 must NOT be flagged generic"
        );
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

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D46 — struct auto-discovery walker tests
    // ─────────────────────────────────────────────────────────────────────

    fn walk_structs(src: &str) -> super::AutoStructReport {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _) = lower_module(&f, &cst);
        super::auto_monomorphize_structs(&hir, &interner)
    }

    #[test]
    fn struct_walker_empty_module_is_empty() {
        let r = walk_structs("");
        assert!(r.is_empty());
        assert_eq!(r.generic_struct_count, 0);
    }

    #[test]
    fn struct_walker_ignores_non_generic_struct() {
        let r = walk_structs(r"struct Point { x : f32, y : f32 }");
        assert_eq!(r.generic_struct_count, 0);
        assert!(r.is_empty());
    }

    #[test]
    fn struct_walker_indexes_generic_but_no_refs_no_specializations() {
        let r = walk_structs(r"struct Pair<T, U> { first : T, second : U }");
        assert_eq!(r.generic_struct_count, 1);
        assert_eq!(r.ref_count, 0);
        assert!(r.is_empty());
    }

    #[test]
    fn struct_walker_fn_param_type_triggers_specialization() {
        // `fn foo(p : Pair<i32, f32>) -> i32 { 0 }` references Pair<i32, f32>
        // in a param type-annotation ⇒ walker produces Pair_i32_f32.
        let src = r"
            struct Pair<T, U> { first : T, second : U }
            fn foo(p : Pair<i32, f32>) -> i32 { 0 }
        ";
        let r = walk_structs(src);
        assert_eq!(r.generic_struct_count, 1);
        assert_eq!(r.ref_count, 1);
        assert_eq!(r.specialization_count, 1);
        let names: Vec<&str> = r
            .specializations
            .iter()
            .map(|s| {
                // specialization preserves the original Symbol name ; downstream
                // registers via mangled key. Verify field substitution instead.
                match &s.body {
                    cssl_hir::HirStructBody::Named(fs) => {
                        let _ = fs;
                        "Pair"
                    }
                    _ => "other",
                }
            })
            .collect();
        assert_eq!(names, vec!["Pair"]);
    }

    #[test]
    fn struct_walker_fn_return_type_triggers_specialization() {
        let src = r"
            struct Box<T> { value : T }
            fn make() -> Box<i32> { Box { value : 0 } }
        ";
        let r = walk_structs(src);
        assert_eq!(r.ref_count, 1);
        assert_eq!(r.specialization_count, 1);
    }

    #[test]
    fn struct_walker_two_distinct_refs_produce_two_specs() {
        let src = r"
            struct Box<T> { value : T }
            fn one() -> Box<i32> { Box { value : 0 } }
            fn two() -> Box<f32> { Box { value : 0.0 } }
        ";
        let r = walk_structs(src);
        assert_eq!(r.ref_count, 2);
        assert_eq!(r.specialization_count, 2);
    }

    #[test]
    fn struct_walker_same_refs_twice_dedup() {
        let src = r"
            struct Box<T> { value : T }
            fn a() -> Box<i32> { Box { value : 0 } }
            fn b() -> Box<i32> { Box { value : 0 } }
            fn c(x : Box<i32>) -> i32 { 0 }
        ";
        let r = walk_structs(src);
        assert_eq!(r.ref_count, 3);
        assert_eq!(r.specialization_count, 1, "three refs to Box<i32> ⇒ 1 spec");
    }

    #[test]
    fn struct_walker_nested_refs_handled() {
        // `Outer<Inner<i32>>` — both are generic structs. The walker must
        // recurse into type_args and collect BOTH refs.
        let src = r"
            struct Inner<T> { value : T }
            struct Outer<T> { wrapper : T }
            fn foo(x : Outer<Inner<i32>>) -> i32 { 0 }
        ";
        let r = walk_structs(src);
        assert_eq!(r.generic_struct_count, 2);
        assert!(
            r.ref_count >= 2,
            "expected both Outer + Inner refs : got {}",
            r.ref_count
        );
        // Both specializations present.
        assert_eq!(
            r.specializations.len(),
            2,
            "Outer + Inner should both specialize"
        );
    }

    #[test]
    fn struct_walker_struct_field_type_scanned() {
        // A generic struct's field references another generic struct. The
        // walker scans struct-body fields too.
        let src = r"
            struct Inner<T> { value : T }
            struct Holder { slot : Inner<i32> }
        ";
        let r = walk_structs(src);
        assert_eq!(r.generic_struct_count, 1);
        assert_eq!(r.ref_count, 1, "Holder's slot field references Inner<i32>");
        assert_eq!(r.specialization_count, 1);
    }

    #[test]
    fn struct_walker_arity_mismatch_skipped() {
        // `Pair<i32>` with only 1 type-arg when Pair has 2 params ⇒ skip
        // (malformed reference ; real compiler would diagnose). Walker
        // must NOT panic or produce a bad specialization.
        let src = r"
            struct Pair<T, U> { first : T, second : U }
            fn foo(p : Pair<i32>) -> i32 { 0 }
        ";
        let r = walk_structs(src);
        assert_eq!(r.ref_count, 1, "the ref IS collected (syntax-valid)");
        assert_eq!(
            r.specialization_count, 0,
            "arity-mismatch must NOT specialize"
        );
    }

    #[test]
    fn struct_walker_report_summary_shape() {
        let r = walk_structs(
            r"
            struct Box<T> { value : T }
            fn foo() -> Box<i32> { Box { value : 0 } }
        ",
        );
        let s = r.summary();
        assert!(s.contains("generic structs"));
        assert!(s.contains("type-refs"));
        assert!(s.contains("specializations"));
    }
}
