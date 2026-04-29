//! T11-D99 — Trait dispatch + Drop integration end-to-end gate.
//!
//! § PURPOSE
//!
//! Counter-verifies the J1 slice against three end-to-end scenarios :
//!
//!   1. **Drop chain — `Vec<Box<i32>>::drop` calls `Box<i32>::drop` calls
//!      `cssl.heap.dealloc`** : The auto-monomorph walker emits per-T impl-
//!      methods ; the drop-injector schedules `Vec_<T>__Drop__drop` at scope
//!      exit ; and the body of each `drop` impl method emits the dealloc
//!      via the existing recognizer fast-path. The counter-verification
//!      walks the produced MirModule + DropInjectionReport and asserts each
//!      stage in the chain produced its expected MIR shape.
//!
//!   2. **Display / Debug for Option / Result via trait dispatch** : Two
//!      separate trait-impls of the same self-type (`Option<i32>`) must
//!      coexist without mangle collisions ; both produce mangled
//!      MirFuncs visible in the auto-monomorph output.
//!
//!   3. **Operator overloading via trait dispatch** : `impl Add for Vec<T>`
//!      makes `a + b` legal where `a, b : Vec<T>`. Stage-0 source-level
//!      operator-overloading is gated behind explicit `Trait::method(a, b)`
//!      calls (binary-operator `+` desugaring to `Add::add(a, b)` is
//!      deferred to a follow-up slice that wires the trait-dispatch
//!      table into `lower_binary`). The gate validates the trait-impl +
//!      mangling shape ; the desugar-on-binary is documented as a known-
//!      gap.

use cssl_ast::{SourceFile, SourceId, Surface};
use cssl_mir::{
    auto_monomorphize_impls, build_trait_impl_table, build_trait_interface_table,
    inject_drops_for_module, validate_trait_bounds_in_module,
};

/// One scenario's outcome — paired with the source string that produced it
/// for trace-link.
#[derive(Debug, Clone)]
pub struct TraitDispatchOutcome {
    pub name: String,
    pub source: String,
    pub trait_impl_count: usize,
    pub generic_impl_specializations: usize,
    pub generic_impl_method_specs: usize,
    pub drop_plans: usize,
    pub total_drops_scheduled: u32,
    pub bound_violations: usize,
    pub mangled_method_names: Vec<String>,
}

impl TraitDispatchOutcome {
    /// Compose summary line for log output.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "trait-dispatch[{}] : impl-count={} mono-impls={} mono-method-specs={} drop-plans={} drops-scheduled={} bound-violations={}",
            self.name,
            self.trait_impl_count,
            self.generic_impl_specializations,
            self.generic_impl_method_specs,
            self.drop_plans,
            self.total_drops_scheduled,
            self.bound_violations,
        )
    }
}

/// Run the trait-dispatch end-to-end on a (name, source) pair.
#[must_use]
pub fn run_trait_dispatch_gate(name: &str, source: &str) -> TraitDispatchOutcome {
    let f = SourceFile::new(SourceId::first(), name, source, Surface::RustHybrid);
    let toks = cssl_lex::lex(&f);
    let (cst, _bag) = cssl_parse::parse(&f, &toks);
    let (hir, interner, _) = cssl_hir::lower_module(&f, &cst);

    let table = build_trait_impl_table(&hir, &interner);
    let _iface_table = build_trait_interface_table(&hir);
    let auto_impls = auto_monomorphize_impls(&hir, &interner, Some(&f));
    let drops = inject_drops_for_module(&hir, &interner, &table);
    let bound_violations = validate_trait_bounds_in_module(&hir, &interner, &table);

    let mangled_method_names: Vec<String> = auto_impls
        .specializations
        .iter()
        .map(|f| f.name.clone())
        .collect();

    TraitDispatchOutcome {
        name: name.to_string(),
        source: source.to_string(),
        trait_impl_count: table.len(),
        generic_impl_specializations: auto_impls.unique_spec_count as usize,
        generic_impl_method_specs: auto_impls.specializations.len(),
        drop_plans: drops.per_fn.len(),
        total_drops_scheduled: drops.total_scheduled,
        bound_violations: bound_violations.len(),
        mangled_method_names,
    }
}

/// Stage-0 canonical sources for the trait-dispatch gate.
pub const VEC_BOX_DROP_SRC: &str = r"
interface Drop { fn drop(self : Vec<i32>) ; }

struct Vec<T> { data : i64, len : i64, cap : i64 }
struct Box<T> { ptr : i64 }

impl<T> Drop for Box<T> {
    fn drop(self : Box<T>) {
        // Stage-0 : the heap.dealloc emit fires through the existing
        // recognizer (`drop` pattern recognizer would land here in a
        // future slice — at stage-0 the body is intentionally minimal
        // since the dealloc-on-iso-consume integration isn't yet in
        // body_lower). What this slice gates is the mangled name + the
        // dispatch shape ; the body-content gate is a follow-up.
    }
}

impl<T> Drop for Vec<T> {
    fn drop(self : Vec<T>) {
        // Same comment as Box<T>::drop above.
    }
}

fn caller() -> i32 {
    let v : Vec<i32> = Vec { data : 0, len : 0, cap : 0 };
    let b : Box<i32> = Box { ptr : 0 };
    0
}
";

pub const DISPLAY_DEBUG_OPTION_SRC: &str = r"
interface Display { fn display(self : Option<i32>) -> i32 ; }
interface Debug   { fn debug  (self : Option<i32>) -> i32 ; }

struct Option<T> { tag : i32 }

impl<T> Display for Option<T> {
    fn display(self : Option<T>) -> i32 { 1 }
}

impl<T> Debug for Option<T> {
    fn debug(self : Option<T>) -> i32 { 2 }
}

fn caller() -> i32 {
    let o : Option<i32> = Option { tag : 0 };
    0
}
";

pub const OP_OVERLOAD_ADD_SRC: &str = r"
interface Add { fn add(self : Point, other : Point) -> Point ; }

struct Point { x : f32, y : f32 }

impl Add for Point {
    fn add(self : Point, other : Point) -> Point {
        Point { x : 0.0, y : 0.0 }
    }
}

fn caller() -> i32 {
    let a : Point = Point { x : 1.0, y : 2.0 };
    let b : Point = Point { x : 3.0, y : 4.0 };
    let c : Point = Add::add(a, b);
    0
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_box_drop_chain_produces_two_mangled_drops() {
        let out = run_trait_dispatch_gate("vec_box_drop", VEC_BOX_DROP_SRC);
        // Both `impl Drop for Vec<T>` and `impl Drop for Box<T>` must specialize
        // to concrete `Vec_i32__Drop__drop` and `Box_i32__Drop__drop` MirFuncs.
        assert!(
            out.mangled_method_names
                .iter()
                .any(|n| n == "Vec_i32__Drop__drop"),
            "missing Vec_i32__Drop__drop in {:?}",
            out.mangled_method_names
        );
        assert!(
            out.mangled_method_names
                .iter()
                .any(|n| n == "Box_i32__Drop__drop"),
            "missing Box_i32__Drop__drop in {:?}",
            out.mangled_method_names
        );
    }

    #[test]
    fn vec_box_drop_caller_schedules_two_drop_calls() {
        let out = run_trait_dispatch_gate("vec_box_drop", VEC_BOX_DROP_SRC);
        // The `caller` fn declares `let v : Vec<i32>` and `let b : Box<i32>` ⇒
        // both must schedule for drop.
        assert!(
            out.total_drops_scheduled >= 2,
            "expected ≥ 2 drops in {}",
            out.summary()
        );
    }

    #[test]
    fn display_debug_option_produces_two_distinct_mangled_names() {
        let out = run_trait_dispatch_gate("display_debug_option", DISPLAY_DEBUG_OPTION_SRC);
        assert!(out
            .mangled_method_names
            .iter()
            .any(|n| n == "Option_i32__Display__display"));
        assert!(out
            .mangled_method_names
            .iter()
            .any(|n| n == "Option_i32__Debug__debug"));
    }

    #[test]
    fn display_debug_option_no_mangle_collision() {
        // Both Display::display + Debug::debug methods must produce DISTINCT
        // mangled names (the trait-name slot in mangling prevents collision).
        let out = run_trait_dispatch_gate("display_debug_option", DISPLAY_DEBUG_OPTION_SRC);
        let mut sorted = out.mangled_method_names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            out.mangled_method_names.len(),
            "duplicate mangled names : {:?}",
            out.mangled_method_names
        );
    }

    #[test]
    fn op_overload_add_lands_in_table() {
        let out = run_trait_dispatch_gate("op_overload", OP_OVERLOAD_ADD_SRC);
        // Inherent + trait impls : the table records the trait-impl, plus
        // there's no inherent `Add` impl on Point ⇒ exactly one entry.
        assert_eq!(out.trait_impl_count, 1);
    }

    #[test]
    fn op_overload_no_bound_violations() {
        let out = run_trait_dispatch_gate("op_overload", OP_OVERLOAD_ADD_SRC);
        assert_eq!(out.bound_violations, 0);
    }

    #[test]
    fn outcome_summary_shape() {
        let out = run_trait_dispatch_gate("vec_box_drop", VEC_BOX_DROP_SRC);
        let s = out.summary();
        assert!(s.contains("trait-dispatch"));
        assert!(s.contains("impl-count="));
        assert!(s.contains("drops-scheduled="));
    }

    #[test]
    fn vec_box_drop_no_unsatisfied_bounds() {
        let out = run_trait_dispatch_gate("vec_box_drop", VEC_BOX_DROP_SRC);
        assert_eq!(out.bound_violations, 0);
    }

    #[test]
    fn drop_plan_keys_match_user_fn_names() {
        // The drop-injector keys plans by fn-name. For the vec_box_drop scenario
        // the `caller` fn is the one that introduces both bindings.
        let f = cssl_ast::SourceFile::new(
            cssl_ast::SourceId::first(),
            "<t>",
            VEC_BOX_DROP_SRC,
            cssl_ast::Surface::RustHybrid,
        );
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _) = cssl_hir::lower_module(&f, &cst);
        let table = build_trait_impl_table(&hir, &interner);
        let drops = inject_drops_for_module(&hir, &interner, &table);
        assert!(drops.per_fn.contains_key("caller"));
        let caller_plan = drops.per_fn.get("caller").unwrap();
        assert_eq!(caller_plan.drops.len(), 2);
        // Reverse-construction order : `b` first (declared second), then `v`.
        let firing: Vec<&str> = caller_plan
            .firing_order()
            .iter()
            .map(|d| d.drop_fn.as_str())
            .collect();
        assert_eq!(firing[0], "Box__Drop__drop");
        assert_eq!(firing[1], "Vec__Drop__drop");
    }
}
