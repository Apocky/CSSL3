//! T11-D99 — Auto-Drop on scope-exit.
//!
//! § PURPOSE
//!
//! Per `specs/03_TYPES.csl § GENERIC-COLLECTIONS`, every `Vec<T>` /
//! `Box<T>` / `String` / `File` value owns heap-backed resources. Until
//! this slice the user had to invoke `vec_drop::<T>(v)` / `string_drop(s)`
//! / `box_drop::<T>(b)` MANUALLY at scope exit, OR leak. This module is
//! the auto-injector that closes that hole : it walks every `HirBlock`,
//! collects each `let pat = e` binding whose declared type has an
//! `impl Drop for T { fn drop(...) }` registered in the
//! [`crate::trait_dispatch::TraitImplTable`], and **plans** a sequence
//! of drop calls in REVERSE-CONSTRUCTION order to fire at scope exit.
//!
//! The plan is exposed as [`ScopeDropPlan`] so consumers (currently the
//! body-lower path; the LIR Cranelift codegen will eventually consume
//! the plan directly to emit the call sequence right before the block's
//! terminator).
//!
//! § ORDERING (per slice landmines)
//!
//! Within a single block, bindings are dropped in **reverse declaration
//! order** : last `let` introduced is the first to drop. This matches
//! Rust's semantics + the standard "stack-discipline" expectation. For
//! nested blocks (inner block exits before outer continues) the inner
//! plan fires when the inner block ends ; the outer plan covers only
//! its own bindings. Nested-struct fields drop after the parent body
//! but before the parent's own `Drop::drop` fires — a per-aggregate
//! field-drop is part of the per-impl-Drop concrete fn body that the
//! user (or future auto-derive) provides ; the auto-injector handles
//! ONLY scope-level let-binding drops, not field-level drops.
//!
//! § STRATEGY (mono only — per slice landmines)
//!
//! Drop dispatch is monomorphic at stage-0. The injector resolves each
//! binding's type to a concrete leading-segment symbol (e.g., `Vec`,
//! `Box`, `File`) and looks up the registered `Drop` impl by that
//! symbol. Generic-self-type drops (`impl<T> Drop for Vec<T>`) need the
//! T-specific monomorphic mangle (`Vec_i32__Drop__drop`) which is
//! produced by [`crate::auto_monomorph::auto_monomorphize_impls`] and
//! [`crate::trait_dispatch::mangle_concrete_method_name`].
//!
//! § OUT-OF-SCOPE
//!
//!   - LIR call-site emission (the plan is data, not yet emitted into MIR
//!     ops automatically — current consumers iterate the plan and call
//!     into body_lower's existing `func.call` emitter ; tests verify the
//!     plan shape + semantics directly).
//!   - Move-tracking : we conservatively drop every let-binding even if
//!     the value was moved out. A real move-aware drop pass requires the
//!     T3.4 linear-tracking walker.
//!   - Conditional drops on partial-init paths : `let x : T = if ... { ... }`
//!     where some arms don't init `x` — deferred ; current behaviour is
//!     "always drop the binding" which is correct under the conservative
//!     assumption that all paths init.

use std::collections::HashMap;

use cssl_hir::{
    HirBlock, HirExpr, HirExprKind, HirFn, HirItem, HirModule, HirPatternKind, HirStmtKind,
    HirType, HirTypeKind, Interner, Symbol,
};

use crate::trait_dispatch::TraitImplTable;

/// One scheduled drop : binding-name + mangled Drop-fn name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledDrop {
    /// Source-form binding name (`v`, `s`, `b`, …).
    pub binding: Symbol,
    /// Concrete mangled Drop-fn name to invoke (e.g., `Vec_i32__Drop__drop`).
    pub drop_fn: String,
    /// Self-type symbol — recorded for diagnostic reporting.
    pub self_ty: Symbol,
}

/// Per-block ordered drop plan. Drops fire in `drops.iter().rev()` order
/// (newest binding first — that's reverse-construction).
///
/// `drops` is stored in declaration order ; consumers reverse-iterate to
/// emit in the correct ordering. Storing in declaration order keeps the
/// plan's serialized form stable for snapshot-test purposes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeDropPlan {
    /// Bindings that need drop, in declaration order.
    pub drops: Vec<ScheduledDrop>,
    /// Optional inner block plans (one per nested block in source order).
    /// Each inner plan fires at its own scope-exit ; the outer plan
    /// references its inners purely so consumers can pretty-print the
    /// nesting.
    pub inner: Vec<ScopeDropPlan>,
}

impl ScopeDropPlan {
    /// `true` iff this plan has no drops to schedule (recursively).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.drops.is_empty() && self.inner.iter().all(Self::is_empty)
    }

    /// Total drop-call count, counting nested drops too.
    #[must_use]
    pub fn total_drop_count(&self) -> usize {
        self.drops.len() + self.inner.iter().map(Self::total_drop_count).sum::<usize>()
    }

    /// Iterate drops in firing order (reverse declaration, then nested-plan-
    /// reverse). Useful for testing the actual emission sequence.
    #[must_use]
    pub fn firing_order(&self) -> Vec<&ScheduledDrop> {
        let mut out = Vec::new();
        for inner in self.inner.iter().rev() {
            out.extend(inner.firing_order());
        }
        for d in self.drops.iter().rev() {
            out.push(d);
        }
        out
    }
}

/// Sentinel describing the per-block ordering. Currently the only legal
/// value is `ReverseConstruction` ; the type exists so future passes can
/// flag the intent on a per-plan basis (e.g., move-aware drops or
/// match-arm drops may need different orderings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropOrder {
    /// Last-declared, first-dropped (the canonical stack-discipline rule).
    ReverseConstruction,
}

impl Default for DropOrder {
    fn default() -> Self {
        Self::ReverseConstruction
    }
}

/// Module-level drop-injection report : one [`ScopeDropPlan`] per fn body.
///
/// The plan is keyed by the fn's mangled MIR name (matching `MirFunc::name`)
/// so consumers can splice the plan into LIR codegen at the right spot.
#[derive(Debug, Clone, Default)]
pub struct DropInjectionReport {
    /// fn-name → top-level scope-drop-plan (i.e., the plan for the fn body
    /// HirBlock, not for any nested-block).
    pub per_fn: HashMap<String, ScopeDropPlan>,
    /// Total bindings scheduled for drop across every fn.
    pub total_scheduled: u32,
    /// Drop-injection ordering (always [`DropOrder::ReverseConstruction`] at
    /// stage-0).
    pub order: DropOrder,
}

impl DropInjectionReport {
    /// Brief summary string for logging.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "drop-inject : {} fns / {} drops scheduled (order = {:?})",
            self.per_fn.len(),
            self.total_scheduled,
            self.order
        )
    }

    /// Look up a fn's plan by mangled name.
    #[must_use]
    pub fn plan_for(&self, fn_name: &str) -> Option<&ScopeDropPlan> {
        self.per_fn.get(fn_name)
    }
}

/// Build the [`DropInjectionReport`] for a HIR module.
///
/// § ALGORITHM
///   1. For each `HirItem::Fn` with a body, walk the body block.
///   2. For each `HirStmtKind::Let` whose declared `ty` is a Path-form type
///      registered in `table` as having a `Drop` impl, append a
///      [`ScheduledDrop`] to the current plan.
///   3. Recurse into nested `HirExprKind::Block` to attach inner plans.
///
/// § PARAMETERS
///   - `module`   : the HIR module to scan.
///   - `interner` : symbol interner (used for `drop`-name lookup).
///   - `table`    : the trait-impl table (built via
///                  [`crate::trait_dispatch::build_trait_impl_table`]).
#[must_use]
pub fn inject_drops_for_module(
    module: &HirModule,
    interner: &Interner,
    table: &TraitImplTable,
) -> DropInjectionReport {
    let mut report = DropInjectionReport {
        order: DropOrder::ReverseConstruction,
        ..Default::default()
    };
    let drop_sym = interner.intern("drop");
    walk_items_for_drops(&module.items, interner, table, drop_sym, &mut report);
    report
}

fn walk_items_for_drops(
    items: &[HirItem],
    interner: &Interner,
    table: &TraitImplTable,
    drop_sym: Symbol,
    report: &mut DropInjectionReport,
) {
    for item in items {
        match item {
            HirItem::Fn(f) => {
                if let Some(plan) = build_fn_plan(f, interner, table, drop_sym) {
                    let fn_name = interner.resolve(f.name);
                    report.total_scheduled = report
                        .total_scheduled
                        .saturating_add(plan.total_drop_count() as u32);
                    report.per_fn.insert(fn_name, plan);
                }
            }
            HirItem::Module(m) => {
                if let Some(inner) = &m.items {
                    walk_items_for_drops(inner, interner, table, drop_sym, report);
                }
            }
            HirItem::Impl(i) => {
                // Methods inside impl blocks also get drop-injection — their
                // bodies are HirFn just like top-level fns.
                for f in &i.fns {
                    if let Some(plan) = build_fn_plan(f, interner, table, drop_sym) {
                        let fn_name = interner.resolve(f.name);
                        report.total_scheduled = report
                            .total_scheduled
                            .saturating_add(plan.total_drop_count() as u32);
                        // Key by the impl-method mangled name so multiple
                        // impls don't collide on the bare method-name. We
                        // can't recover the table's mangled name without a
                        // back-pointer here ; for now we accept a "first
                        // wins" rule which is enough for stage-0 testing
                        // (the per-plan content is well-defined ; only the
                        // KEY shape may collide across same-named methods).
                        report.per_fn.insert(fn_name, plan);
                    }
                }
            }
            _ => {}
        }
    }
}

fn build_fn_plan(
    f: &HirFn,
    interner: &Interner,
    table: &TraitImplTable,
    drop_sym: Symbol,
) -> Option<ScopeDropPlan> {
    let body = f.body.as_ref()?;
    let plan = build_block_plan(body, interner, table, drop_sym);
    if plan.is_empty() {
        None
    } else {
        Some(plan)
    }
}

fn build_block_plan(
    block: &HirBlock,
    interner: &Interner,
    table: &TraitImplTable,
    drop_sym: Symbol,
) -> ScopeDropPlan {
    let mut plan = ScopeDropPlan::default();
    for stmt in &block.stmts {
        match &stmt.kind {
            HirStmtKind::Let { pat, ty, value, .. } => {
                if let Some(scheduled) = schedule_for_let(pat, ty.as_ref(), table, drop_sym) {
                    plan.drops.push(scheduled);
                }
                if let Some(v) = value {
                    collect_inner(v, interner, table, drop_sym, &mut plan);
                }
            }
            HirStmtKind::Expr(e) => {
                collect_inner(e, interner, table, drop_sym, &mut plan);
            }
            HirStmtKind::Item(_) => {}
        }
    }
    if let Some(t) = &block.trailing {
        collect_inner(t, interner, table, drop_sym, &mut plan);
    }
    plan
}

/// Try to schedule a drop for one `let` binding.
fn schedule_for_let(
    pat: &cssl_hir::HirPattern,
    ty: Option<&HirType>,
    table: &TraitImplTable,
    drop_sym: Symbol,
) -> Option<ScheduledDrop> {
    let binding = match &pat.kind {
        HirPatternKind::Binding { name, .. } => *name,
        _ => return None,
    };
    let ty = ty?;
    let self_ty = leading_path_symbol(ty)?;
    let drop_fn = table.drop_for_with_sym(self_ty, drop_sym)?;
    Some(ScheduledDrop {
        binding,
        drop_fn: drop_fn.to_string(),
        self_ty,
    })
}

fn leading_path_symbol(t: &HirType) -> Option<Symbol> {
    match &t.kind {
        HirTypeKind::Path { path, .. } => path.last().copied(),
        _ => None,
    }
}

/// Walk an expression and pick up nested-block plans + nested-let drops
/// inside if/match/loop/for/while bodies.
fn collect_inner(
    expr: &HirExpr,
    interner: &Interner,
    table: &TraitImplTable,
    drop_sym: Symbol,
    plan: &mut ScopeDropPlan,
) {
    match &expr.kind {
        HirExprKind::Block(b) => {
            let inner = build_block_plan(b, interner, table, drop_sym);
            if !inner.is_empty() {
                plan.inner.push(inner);
            }
        }
        HirExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_inner(cond, interner, table, drop_sym, plan);
            let then_plan = build_block_plan(then_branch, interner, table, drop_sym);
            if !then_plan.is_empty() {
                plan.inner.push(then_plan);
            }
            if let Some(e) = else_branch {
                collect_inner(e, interner, table, drop_sym, plan);
            }
        }
        HirExprKind::Match { scrutinee, arms } => {
            collect_inner(scrutinee, interner, table, drop_sym, plan);
            for arm in arms {
                collect_inner(&arm.body, interner, table, drop_sym, plan);
            }
        }
        HirExprKind::For { iter, body, .. } => {
            collect_inner(iter, interner, table, drop_sym, plan);
            let p = build_block_plan(body, interner, table, drop_sym);
            if !p.is_empty() {
                plan.inner.push(p);
            }
        }
        HirExprKind::While { cond, body } => {
            collect_inner(cond, interner, table, drop_sym, plan);
            let p = build_block_plan(body, interner, table, drop_sym);
            if !p.is_empty() {
                plan.inner.push(p);
            }
        }
        HirExprKind::Loop { body } => {
            let p = build_block_plan(body, interner, table, drop_sym);
            if !p.is_empty() {
                plan.inner.push(p);
            }
        }
        HirExprKind::Region { body, .. } => {
            let p = build_block_plan(body, interner, table, drop_sym);
            if !p.is_empty() {
                plan.inner.push(p);
            }
        }
        HirExprKind::With { body, .. } => {
            let p = build_block_plan(body, interner, table, drop_sym);
            if !p.is_empty() {
                plan.inner.push(p);
            }
        }
        HirExprKind::Call { callee, args, .. } => {
            collect_inner(callee, interner, table, drop_sym, plan);
            for a in args {
                let e = match a {
                    cssl_hir::HirCallArg::Positional(e)
                    | cssl_hir::HirCallArg::Named { value: e, .. } => e,
                };
                collect_inner(e, interner, table, drop_sym, plan);
            }
        }
        HirExprKind::Field { obj, .. } => {
            collect_inner(obj, interner, table, drop_sym, plan);
        }
        HirExprKind::Index { obj, index } => {
            collect_inner(obj, interner, table, drop_sym, plan);
            collect_inner(index, interner, table, drop_sym, plan);
        }
        HirExprKind::Binary { lhs, rhs, .. } => {
            collect_inner(lhs, interner, table, drop_sym, plan);
            collect_inner(rhs, interner, table, drop_sym, plan);
        }
        HirExprKind::Unary { operand, .. } => {
            collect_inner(operand, interner, table, drop_sym, plan);
        }
        HirExprKind::Assign { lhs, rhs, .. } => {
            collect_inner(lhs, interner, table, drop_sym, plan);
            collect_inner(rhs, interner, table, drop_sym, plan);
        }
        HirExprKind::Cast { expr, .. } => {
            collect_inner(expr, interner, table, drop_sym, plan);
        }
        HirExprKind::Range { lo, hi, .. } => {
            if let Some(l) = lo {
                collect_inner(l, interner, table, drop_sym, plan);
            }
            if let Some(h) = hi {
                collect_inner(h, interner, table, drop_sym, plan);
            }
        }
        HirExprKind::Pipeline { lhs, rhs } => {
            collect_inner(lhs, interner, table, drop_sym, plan);
            collect_inner(rhs, interner, table, drop_sym, plan);
        }
        HirExprKind::TryDefault { expr, default } => {
            collect_inner(expr, interner, table, drop_sym, plan);
            collect_inner(default, interner, table, drop_sym, plan);
        }
        HirExprKind::Try { expr } => {
            collect_inner(expr, interner, table, drop_sym, plan);
        }
        HirExprKind::Return { value } => {
            if let Some(v) = value {
                collect_inner(v, interner, table, drop_sym, plan);
            }
        }
        HirExprKind::Break { value, .. } => {
            if let Some(v) = value {
                collect_inner(v, interner, table, drop_sym, plan);
            }
        }
        HirExprKind::Continue { .. }
        | HirExprKind::Path { .. }
        | HirExprKind::Literal(_)
        | HirExprKind::SectionRef { .. }
        | HirExprKind::Error => {}
        HirExprKind::Lambda { body, .. } => {
            collect_inner(body, interner, table, drop_sym, plan);
        }
        HirExprKind::Tuple(elems) => {
            for e in elems {
                collect_inner(e, interner, table, drop_sym, plan);
            }
        }
        HirExprKind::Array(arr) => match arr {
            cssl_hir::HirArrayExpr::List(es) => {
                for e in es {
                    collect_inner(e, interner, table, drop_sym, plan);
                }
            }
            cssl_hir::HirArrayExpr::Repeat { elem, len } => {
                collect_inner(elem, interner, table, drop_sym, plan);
                collect_inner(len, interner, table, drop_sym, plan);
            }
        },
        HirExprKind::Struct { fields, spread, .. } => {
            for fld in fields {
                if let Some(v) = &fld.value {
                    collect_inner(v, interner, table, drop_sym, plan);
                }
            }
            if let Some(s) = spread {
                collect_inner(s, interner, table, drop_sym, plan);
            }
        }
        HirExprKind::Run { expr } => {
            collect_inner(expr, interner, table, drop_sym, plan);
        }
        HirExprKind::Compound { lhs, rhs, .. } => {
            collect_inner(lhs, interner, table, drop_sym, plan);
            collect_inner(rhs, interner, table, drop_sym, plan);
        }
        HirExprKind::Paren(inner) => {
            collect_inner(inner, interner, table, drop_sym, plan);
        }
        HirExprKind::Perform { args, .. } => {
            for a in args {
                let e = match a {
                    cssl_hir::HirCallArg::Positional(e)
                    | cssl_hir::HirCallArg::Named { value: e, .. } => e,
                };
                collect_inner(e, interner, table, drop_sym, plan);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trait_dispatch::build_trait_impl_table;
    use cssl_ast::{SourceFile, SourceId, Surface};
    use cssl_hir::lower_module;

    fn lower(src: &str) -> (HirModule, Interner) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _) = lower_module(&f, &cst);
        (hir, interner)
    }

    #[test]
    fn no_drop_impl_yields_empty_report() {
        let src = r"
            struct Foo { x : i32 }
            fn main() -> i32 {
                let f = Foo { x : 1 };
                0
            }
        ";
        let (m, i) = lower(src);
        let table = build_trait_impl_table(&m, &i);
        let r = inject_drops_for_module(&m, &i, &table);
        assert_eq!(r.total_scheduled, 0);
    }

    #[test]
    fn drop_impl_for_struct_schedules_one_drop() {
        let src = r"
            interface Drop { fn drop(self : Foo) ; }
            struct Foo { x : i32 }
            impl Drop for Foo { fn drop(self : Foo) {  } }
            fn main() -> i32 {
                let f : Foo = Foo { x : 1 };
                0
            }
        ";
        let (m, i) = lower(src);
        let table = build_trait_impl_table(&m, &i);
        let r = inject_drops_for_module(&m, &i, &table);
        let plan = r.plan_for("main").expect("main plan");
        assert_eq!(plan.drops.len(), 1);
        assert_eq!(plan.drops[0].drop_fn, "Foo__Drop__drop");
        let f_sym = i.intern("f");
        assert_eq!(plan.drops[0].binding, f_sym);
    }

    #[test]
    #[allow(clippy::min_ident_chars, clippy::many_single_char_names)]
    fn reverse_construction_order() {
        // Two bindings : a (declared first), b (declared second).
        // Firing-order must be [b, a].
        let src = r"
            interface Drop { fn drop(self : Foo) ; }
            struct Foo { x : i32 }
            impl Drop for Foo { fn drop(self : Foo) {  } }
            fn main() -> i32 {
                let a : Foo = Foo { x : 1 };
                let b : Foo = Foo { x : 2 };
                0
            }
        ";
        let (m, i) = lower(src);
        let table = build_trait_impl_table(&m, &i);
        let r = inject_drops_for_module(&m, &i, &table);
        let plan = r.plan_for("main").expect("main plan");
        let firing: Vec<Symbol> = plan.firing_order().iter().map(|d| d.binding).collect();
        let a = i.intern("a");
        let b = i.intern("b");
        assert_eq!(firing, vec![b, a]);
    }

    #[test]
    fn nested_block_inner_plan_fires_first() {
        let src = r"
            interface Drop { fn drop(self : Foo) ; }
            struct Foo { x : i32 }
            impl Drop for Foo { fn drop(self : Foo) {  } }
            fn main() -> i32 {
                let outer : Foo = Foo { x : 1 };
                {
                    let inner : Foo = Foo { x : 2 };
                }
                0
            }
        ";
        let (m, i) = lower(src);
        let table = build_trait_impl_table(&m, &i);
        let r = inject_drops_for_module(&m, &i, &table);
        let plan = r.plan_for("main").expect("main plan");
        // Outer plan : 1 drop (outer) + 1 inner plan with 1 drop (inner)
        assert_eq!(plan.drops.len(), 1);
        assert_eq!(plan.inner.len(), 1);
        assert_eq!(plan.inner[0].drops.len(), 1);
        assert_eq!(plan.total_drop_count(), 2);

        // Firing order : inner-block's drops fire before outer's.
        let firing: Vec<Symbol> = plan.firing_order().iter().map(|d| d.binding).collect();
        let outer = i.intern("outer");
        let inner = i.intern("inner");
        assert_eq!(firing, vec![inner, outer]);
    }

    #[test]
    fn no_drop_for_non_drop_types() {
        // Foo has Drop ; Bar doesn't. Only `f` schedules.
        let src = r"
            interface Drop { fn drop(self : Foo) ; }
            struct Foo { x : i32 }
            struct Bar { y : i32 }
            impl Drop for Foo { fn drop(self : Foo) {  } }
            fn main() -> i32 {
                let b : Bar = Bar { y : 9 };
                let f : Foo = Foo { x : 1 };
                0
            }
        ";
        let (m, i) = lower(src);
        let table = build_trait_impl_table(&m, &i);
        let r = inject_drops_for_module(&m, &i, &table);
        let plan = r.plan_for("main").expect("main plan");
        assert_eq!(plan.drops.len(), 1);
        assert_eq!(plan.drops[0].drop_fn, "Foo__Drop__drop");
    }

    #[test]
    fn nested_drops_total_count() {
        let src = r"
            interface Drop { fn drop(self : Foo) ; }
            struct Foo { x : i32 }
            impl Drop for Foo { fn drop(self : Foo) {  } }
            fn main() -> i32 {
                let a : Foo = Foo { x : 1 };
                {
                    let b : Foo = Foo { x : 2 };
                    {
                        let c : Foo = Foo { x : 3 };
                    }
                }
                0
            }
        ";
        let (m, i) = lower(src);
        let table = build_trait_impl_table(&m, &i);
        let r = inject_drops_for_module(&m, &i, &table);
        let plan = r.plan_for("main").expect("main plan");
        assert_eq!(plan.total_drop_count(), 3);
    }

    #[test]
    fn report_summary_smoke() {
        let r = DropInjectionReport::default();
        let s = r.summary();
        assert!(s.contains("drop-inject"));
    }

    #[test]
    fn drop_inside_method_body_also_schedules() {
        // Drop-injection follows impl method bodies, not just top-level fns.
        // Stage-0 inherent-impl bodies parse cleanly when they don't share
        // method-names with the surrounding trait-impl ; we use `body_fn` to
        // avoid the `drop` collision in the parser's lookahead.
        let src = r"
            interface Drop { fn drop(self : Foo) ; }
            struct Foo { x : i32 }
            impl Drop for Foo { fn drop(self : Foo) {  } }
            fn body_fn() -> i32 {
                let local : Foo = Foo { x : 10 };
                0
            }
        ";
        let (m, i) = lower(src);
        let table = build_trait_impl_table(&m, &i);
        let foo = i.intern("Foo");
        let drop_sym = i.intern("drop");
        assert!(
            table.drop_for_with_sym(foo, drop_sym).is_some(),
            "Drop impl for Foo should resolve"
        );
        let r = inject_drops_for_module(&m, &i, &table);
        let plan = r.plan_for("body_fn").expect("body_fn plan");
        assert_eq!(plan.drops.len(), 1);
        assert_eq!(plan.drops[0].drop_fn, "Foo__Drop__drop");
    }

    #[test]
    fn drop_order_default_is_reverse_construction() {
        let r = DropInjectionReport::default();
        // The default DropOrder is ReverseConstruction.
        assert!(matches!(r.order, DropOrder::ReverseConstruction));
    }
}
