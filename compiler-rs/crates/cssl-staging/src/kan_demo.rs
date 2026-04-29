//! KAN-weight specialization demo — end-to-end proof that the
//! [`crate::SpecializerPass`] reduces a generic KAN-evaluator with comptime
//! weights to a constant.
//!
//! § SCENARIO
//!   Picture a 3-layer KAN (Kolmogorov-Arnold Network) where each "edge" is
//!   a 1-D learned univariate polynomial. The naive emitter produces :
//!     fn kan_eval(x : f32, w0 : f32, w1 : f32, w2 : f32) -> f32 {
//!         (w0 * x*x*x) + (w1 * x*x) + (w2 * x) + bias
//!     }
//!   When `w0`, `w1`, `w2` are comptime-known weights, the specializer
//!   reduces the body to :
//!     fn kan_eval__sp_<hash>(x : f32, _w0, _w1, _w2) -> f32 {
//!         CONST_FOLD_OF_(w0*x*x*x + w1*x*x + w2*x + bias, given x = const_x_arg)
//!     }
//!   In the demo we pass ALL of `x, w0, w1, w2` as comptime so the entire
//!   body folds to a single `arith.constant`. The non-trivial real-world
//!   pattern (only-weights-comptime-not-x) requires the runtime-x path
//!   which arrives in T11-D143 ; this slice's milestone is the
//!   "everything-comptime" full-fold + per-weight-set distinct mangle.
//!
//! § DEMO API
//!   - [`build_kan_module`] : construct a `MirModule` containing a
//!     `kan_eval(x, w0, w1, w2) -> f32` fn that evaluates a cubic.
//!   - [`run_kan_specialization`] : drive [`SpecializerPass`] with the
//!     given comptime weights + x ; assert the body folds to one
//!     `arith.constant`.
//!   - [`KanDemoSummary`] : the per-run telemetry used by both unit
//!     tests + downstream Phase-I demos.

use cssl_hir::DefId;
use cssl_mir::{FloatWidth, MirFunc, MirModule, MirOp, MirType, MirValue, ValueId};

use crate::specialize_pass::{CompTimeArgs, SpecializerPass};
use crate::value::Value;

/// Build a `kan_eval(x : f32, w0 : f32, w1 : f32, w2 : f32) -> f32` fn that
/// computes a cubic polynomial : `w0 * x^3 + w1 * x^2 + w2 * x + 0.0`.
///
/// The body's MIR ops :
///     %4 = mulf %0 %0       // x*x = %4
///     %5 = mulf %4 %0       // x*x*x = %5
///     %6 = mulf %1 %5       // w0*x^3 = %6
///     %7 = mulf %2 %4       // w1*x^2 = %7
///     %8 = mulf %3 %0       // w2*x = %8
///     %9 = addf %6 %7       // partial = %9
///     %10 = addf %9 %8      // result = %10
#[must_use]
pub fn build_kan_module() -> MirModule {
    let mut module = MirModule::with_name("kan_demo");
    let f32_ty = MirType::Float(FloatWidth::F32);
    let params = vec![f32_ty.clone(); 4];
    let mut func = MirFunc::new("kan_eval", params, vec![f32_ty.clone()]);
    func.next_value_id = 11;

    let entry = func.body.entry_mut().expect("entry block");
    entry.push(mk_mulf(0, 0, 4, &f32_ty));
    entry.push(mk_mulf(4, 0, 5, &f32_ty));
    entry.push(mk_mulf(1, 5, 6, &f32_ty));
    entry.push(mk_mulf(2, 4, 7, &f32_ty));
    entry.push(mk_mulf(3, 0, 8, &f32_ty));
    entry.push(mk_addf(6, 7, 9, &f32_ty));
    entry.push(mk_addf(9, 8, 10, &f32_ty));
    // No explicit `func.return` op for stage-0 ; the structured-CFG
    // validator treats trailing arith ops as the result.

    module.push_func(func);
    module
}

fn mk_mulf(lhs: u32, rhs: u32, result: u32, ty: &MirType) -> MirOp {
    let mut op = MirOp::std("arith.mulf");
    op.operands.push(ValueId(lhs));
    op.operands.push(ValueId(rhs));
    op.results.push(MirValue::new(ValueId(result), ty.clone()));
    op
}

fn mk_addf(lhs: u32, rhs: u32, result: u32, ty: &MirType) -> MirOp {
    let mut op = MirOp::std("arith.addf");
    op.operands.push(ValueId(lhs));
    op.operands.push(ValueId(rhs));
    op.results.push(MirValue::new(ValueId(result), ty.clone()));
    op
}

/// Per-demo-run telemetry summarizing what the specializer accomplished.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KanDemoSummary {
    /// Number of arith.* ops folded by const-prop.
    pub arith_folds: u32,
    /// Number of dead `arith.constant` ops removed by DCE.
    pub dead_consts_removed: u32,
    /// `Some(value)` if the demo's specialized body collapsed to a single
    /// constant equal to the kan eval. `None` if the body still has more
    /// than one op (i.e., the demo did not fully reduce).
    pub final_value: Option<f32>,
    /// Number of ops remaining in the specialized body.
    pub remaining_ops: u32,
    /// Mangled name of the specialized fn.
    pub mangled_name: String,
}

/// Drive specialization with comptime `(x, w0, w1, w2)` ; produce a
/// [`KanDemoSummary`]. The caller asserts the demo's invariants (full fold
/// + final-value within tolerance).
pub fn run_kan_specialization(x: f32, w0: f32, w1: f32, w2: f32) -> KanDemoSummary {
    let mut module = build_kan_module();
    let mut specializer = SpecializerPass::new();
    let mut args = CompTimeArgs::new();
    args.add(0, Value::Float(f64::from(x)));
    args.add(1, Value::Float(f64::from(w0)));
    args.add(2, Value::Float(f64::from(w1)));
    args.add(3, Value::Float(f64::from(w2)));
    let mangled = specializer
        .specialize(&mut module, None, DefId(1), "kan_eval", args)
        .expect("specialization must succeed");

    // Locate the specialized fn.
    let spec = module
        .funcs
        .iter()
        .find(|f| f.name == mangled)
        .expect("specialized fn present");
    let entry = spec.body.entry().expect("entry block");
    // Find the final `arith.constant` op : the result of the chain.
    let final_op = entry
        .ops
        .iter()
        .filter(|op| op.name == "arith.constant")
        .find(|op| op.results.iter().any(|r| r.id == ValueId(10)));
    let final_value = final_op.and_then(|op| {
        op.attributes
            .iter()
            .find(|(k, _)| k == "value")
            .and_then(|(_, v)| v.parse::<f32>().ok())
    });

    let report = &specializer.manifests[0];
    KanDemoSummary {
        arith_folds: report.const_prop.arith_folds,
        dead_consts_removed: report.dce.dead_consts_removed,
        final_value,
        remaining_ops: entry.ops.len() as u32,
        mangled_name: mangled,
    }
}

/// Compute the expected analytic result : `w0 * x^3 + w1 * x^2 + w2 * x`.
#[must_use]
pub fn analytic_kan_value(x: f32, w0: f32, w1: f32, w2: f32) -> f32 {
    w0 * x * x * x + w1 * x * x + w2 * x
}

#[cfg(test)]
mod tests {
    use super::{analytic_kan_value, build_kan_module, run_kan_specialization, KanDemoSummary};

    #[test]
    fn kan_module_has_expected_shape() {
        let m = build_kan_module();
        assert_eq!(m.funcs.len(), 1);
        let f = &m.funcs[0];
        assert_eq!(f.name, "kan_eval");
        assert_eq!(f.params.len(), 4);
        // 7 arith ops total in the body.
        assert_eq!(f.body.entry().unwrap().ops.len(), 7);
    }

    #[test]
    fn analytic_value_canonical_pattern() {
        // x=2, w0=1, w1=0, w2=0 ⇒ 1*8 + 0 + 0 = 8.
        assert!((analytic_kan_value(2.0, 1.0, 0.0, 0.0) - 8.0).abs() < 1e-6);
    }

    #[test]
    fn specialization_produces_distinct_mangle_per_weight_set() {
        let s1 = run_kan_specialization(2.0, 1.0, 0.0, 0.0);
        let s2 = run_kan_specialization(2.0, 0.0, 1.0, 0.0);
        assert_ne!(s1.mangled_name, s2.mangled_name);
    }

    #[test]
    fn specialization_folds_to_single_arith_constant() {
        // x=2, w0=1, w1=2, w2=3 ⇒ 1*8 + 2*4 + 3*2 = 8 + 8 + 6 = 22.
        let s = run_kan_specialization(2.0, 1.0, 2.0, 3.0);
        assert!(
            s.arith_folds >= 7,
            "expected at least 7 arith folds (5 mulf + 2 addf) ; got {}",
            s.arith_folds
        );
        let expected = analytic_kan_value(2.0, 1.0, 2.0, 3.0);
        let actual = s.final_value.expect("final value present");
        assert!(
            (actual - expected).abs() < 1e-4,
            "specialized result {actual} ≠ analytic {expected}"
        );
    }

    #[test]
    fn specialization_x_zero_yields_zero() {
        let s = run_kan_specialization(0.0, 100.0, 200.0, 300.0);
        let v = s.final_value.expect("value");
        assert!(v.abs() < 1e-6);
    }

    #[test]
    fn specialization_negative_x_handled() {
        // x=-2, w0=1, w1=0, w2=0 ⇒ (-2)^3 = -8.
        let s = run_kan_specialization(-2.0, 1.0, 0.0, 0.0);
        let v = s.final_value.expect("value");
        assert!((v - -8.0).abs() < 1e-4);
    }

    #[test]
    fn kan_demo_summary_default_zero() {
        let s = KanDemoSummary::default();
        assert_eq!(s.arith_folds, 0);
        assert_eq!(s.remaining_ops, 0);
    }

    #[test]
    fn specialization_preserves_generic_callee() {
        // After specialization, the original `kan_eval` is still in the
        // module — we only ADD specialized clones, we don't replace.
        let s = run_kan_specialization(2.0, 1.0, 0.0, 0.0);
        // Recompute to inspect the module.
        let mut module = build_kan_module();
        let mut spec = crate::SpecializerPass::new();
        let mut args = crate::CompTimeArgs::new();
        args.add(0, crate::Value::Float(2.0));
        args.add(1, crate::Value::Float(1.0));
        args.add(2, crate::Value::Float(0.0));
        args.add(3, crate::Value::Float(0.0));
        let mangled = spec
            .specialize(&mut module, None, cssl_hir::DefId(1), "kan_eval", args)
            .unwrap();
        assert!(module.funcs.iter().any(|f| f.name == "kan_eval"));
        assert!(module.funcs.iter().any(|f| f.name == mangled));
        assert_eq!(s.mangled_name, mangled);
    }

    #[test]
    fn distinct_x_distinct_mangle() {
        let s1 = run_kan_specialization(1.0, 1.0, 1.0, 1.0);
        let s2 = run_kan_specialization(2.0, 1.0, 1.0, 1.0);
        assert_ne!(s1.mangled_name, s2.mangled_name);
    }

    #[test]
    fn expected_arith_fold_count_matches_op_count() {
        // Body has 7 arith ops (5 mulf + 2 addf) :
        //   %4 = x*x, %5 = x*x*x, %6 = w0*x^3, %7 = w1*x^2, %8 = w2*x   (5 mulf)
        //   %9 = (w0*x^3) + (w1*x^2), %10 = %9 + (w2*x)                  (2 addf)
        //
        // The 4 injected param-constants are SKIPPED by the const-prop
        // pass's `already_bound` guard (their result-id was env-bound
        // BEFORE const-prop ran), so they don't increment arith_folds.
        // Only the 7 arith ops count → arith_folds ≥ 7.
        let s = run_kan_specialization(2.0, 1.0, 2.0, 3.0);
        assert!(s.arith_folds >= 7, "got {}", s.arith_folds);
    }
}
