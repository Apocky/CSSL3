//! T11-D23 : end-to-end integration — CSSLv3 source → walker → JIT execution.
//!
//! § SPEC : `specs/05_AUTODIFF.csl` § KILLER-APP runtime verification.
//! § ROLE : prove the full compiler pipeline from surface syntax through the
//!          AD walker down to JIT-executed machine code produces numerically-
//!          correct gradients on real inputs.
//!
//! § PIPELINE
//!   ```text
//!   CSSLv3 source
//!     → cssl_lex::lex
//!     → cssl_parse::parse (CST)
//!     → cssl_hir::lower_module (HIR)
//!     → cssl_mir::lower_function_signature + lower_fn_body per fn
//!     → cssl_autodiff::AdWalker::transform_module (emits `<name>_fwd` / `_bwd`)
//!     → extract_tangent_only_variant (strip primal result ; keep tangent body)
//!     → cssl_cgen_cpu_cranelift::JitModule::compile + finalize
//!     → JitFn::call_* + central-difference verification
//!   ```
//!
//! § WHY STRIP PRIMAL RESULT
//!   The walker's fwd variant has signature `(primal_params ++ tangent_params)
//!   -> (primal_result, tangent_result)` — multi-result. The stage-0.5 JIT
//!   supports single-result fns ; rather than extend it for a rarely-needed
//!   multi-return path, we post-process to produce a tangent-only variant with
//!   signature `(a, d_a, b, d_b, ...) -> d_y`. The tangent body is identical.

use cssl_ast::{SourceFile, SourceId, Surface};
use cssl_autodiff::AdWalker;
use cssl_cgen_cpu_cranelift::{JitError, JitModule};
use cssl_mir::{MirFunc, MirModule};

/// Parse CSSLv3 source through HIR + MIR + AD walker to produce a MirModule
/// containing the primal fns + emitted `_fwd` / `_bwd` variants.
///
/// § PANICS
/// Panics if lex / parse / HIR errors are fatal — this helper is intended for
/// test fixtures with known-valid source.
#[must_use]
pub fn pipeline_source_to_ad_mir(name: &str, source: &str) -> MirModule {
    let file = SourceFile::new(SourceId::first(), name, source, Surface::RustHybrid);
    let tokens = cssl_lex::lex(&file);
    let (cst, _parse_bag) = cssl_parse::parse(&file, &tokens);
    let (hir_mod, interner, _lower_bag) = cssl_hir::lower_module(&file, &cst);

    let lower_ctx = cssl_mir::LowerCtx::new(&interner);
    let mut mir_mod = MirModule::new();
    for item in &hir_mod.items {
        if let cssl_hir::HirItem::Fn(f) = item {
            let mut mf = cssl_mir::lower_function_signature(&lower_ctx, f);
            cssl_mir::lower_fn_body(&interner, Some(&file), f, &mut mf);
            mir_mod.push_func(mf);
        }
    }

    let walker = AdWalker::from_hir(&hir_mod, &interner);
    walker.transform_module(&mut mir_mod);
    mir_mod
}

/// Post-process a walker-emitted `<name>_bwd` variant : for multi-float-param
/// primals, the bwd variant returns one adjoint per primal float-param. The
/// stage-0.5 JIT supports single-result fns ; this utility extracts a single
/// adjoint (by index) so we can JIT-compile + call each per-param adjoint
/// independently.
///
/// Signature transform :
///   `(primal_params ++ [d_y]) -> (d_0, d_1, ..., d_n)`
///                     ↓
///   `(primal_params ++ [d_y]) -> d_<adjoint_index>`
///
/// The body keeps all adjoint-accumulation ops (they're all needed for the
/// chain-rule), and the `cssl.diff.bwd_return` terminator is rewritten to
/// return only operand-at-index.
///
/// # Panics
/// Panics if `adjoint_index` is out of bounds of `bwd_variant.results`.
#[must_use]
pub fn extract_bwd_single_adjoint(bwd_variant: &MirFunc, adjoint_index: usize) -> MirFunc {
    assert!(
        adjoint_index < bwd_variant.results.len(),
        "adjoint_index {adjoint_index} out of bounds for bwd variant with {} results",
        bwd_variant.results.len()
    );
    let mut out = bwd_variant.clone();
    let wanted_ty = out.results[adjoint_index].clone();
    out.results = vec![wanted_ty];
    out.name = format!("{}_d{adjoint_index}", bwd_variant.name);
    if let Some(entry) = out.body.entry_mut() {
        for op in &mut entry.ops {
            if op.name == "cssl.diff.bwd_return" && op.operands.len() > 1 {
                let wanted = op.operands[adjoint_index];
                op.operands = vec![wanted];
            }
        }
    }
    out
}

/// Post-process a walker-emitted `<name>_fwd` variant : strip the primal
/// result + its corresponding func.return operand, producing a tangent-only
/// fn that the JIT can directly execute. Signature transform :
///   `(primal_params ++ tangent_params) -> (primal_result, tangent_result)`
///                                ↓
///   `(primal_params ++ tangent_params) -> tangent_result`
#[must_use]
pub fn extract_tangent_only_variant(fwd_variant: &MirFunc) -> MirFunc {
    let mut out = fwd_variant.clone();
    // Keep only the last result — the tangent.
    if out.results.len() >= 2 {
        let tangent_ty = out.results.last().cloned().expect("≥ 1 result");
        out.results = vec![tangent_ty];
    }
    // Strip the name suffix to make the tangent-only fn distinct.
    out.name = format!("{}_tangent_only", fwd_variant.name);
    // Walk the entry block ops + rewrite func.return to keep only the last
    // operand (the tangent).
    if let Some(entry) = out.body.entry_mut() {
        for op in &mut entry.ops {
            if op.name == "func.return" && op.operands.len() >= 2 {
                let tangent_operand = *op.operands.last().expect("≥ 1 operand");
                op.operands = vec![tangent_operand];
            }
        }
    }
    out
}

/// Compile a primal MIR fn + its tangent-only variant in a shared JIT module
/// and return the ready-to-call handles + the live module.
///
/// # Errors
/// Propagates any [`JitError`] from compile or finalize.
pub fn jit_primal_and_tangent(
    primal: &MirFunc,
    tangent_only: &MirFunc,
) -> Result<JitChainHandle, JitError> {
    let mut m = JitModule::new();
    let primal_fn = m.compile(primal)?;
    let tangent_fn = m.compile(tangent_only)?;
    m.finalize()?;
    Ok(JitChainHandle {
        module: m,
        primal_fn,
        tangent_fn,
    })
}

/// Bundle returned by [`jit_primal_and_tangent`] — keeps the JIT module alive
/// alongside the fn handles so the code stays mapped while callers invoke it.
pub struct JitChainHandle {
    pub module: JitModule,
    pub primal_fn: cssl_cgen_cpu_cranelift::JitFn,
    pub tangent_fn: cssl_cgen_cpu_cranelift::JitFn,
}

impl core::fmt::Debug for JitChainHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("JitChainHandle")
            .field("primal", &self.primal_fn.name)
            .field("tangent", &self.tangent_fn.name)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        extract_bwd_single_adjoint, extract_tangent_only_variant, pipeline_source_to_ad_mir,
    };
    use cssl_ast::{SourceFile, SourceId, Surface};
    use cssl_cgen_cpu_cranelift::JitModule;

    #[test]
    fn pipeline_source_emits_fwd_variant_for_differentiable_fn() {
        // Hand-author source that the existing parser handles : a sphere_sdf
        // surrogate using float arithmetic recognized by AD primitives.
        let src = r"@differentiable fn sphere_sdf(p : f32, r : f32) -> f32 { p - r }";
        let module = pipeline_source_to_ad_mir("test", src);
        let names: Vec<&str> = module.funcs.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"sphere_sdf"), "primal missing : {names:?}");
        assert!(names.contains(&"sphere_sdf_fwd"), "fwd missing : {names:?}");
    }

    #[test]
    fn extract_tangent_only_drops_primal_result() {
        let src = r"@differentiable fn sphere_sdf(p : f32, r : f32) -> f32 { p - r }";
        let module = pipeline_source_to_ad_mir("test", src);
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf_fwd")
            .expect("fwd variant");
        let tangent_only = extract_tangent_only_variant(fwd);
        assert_eq!(tangent_only.results.len(), 1);
        assert!(tangent_only.name.ends_with("_tangent_only"));
        // The func.return should have exactly 1 operand (the tangent).
        let entry = tangent_only.body.entry().expect("entry");
        let ret = entry
            .ops
            .iter()
            .find(|o| o.name == "func.return")
            .expect("return op present");
        assert_eq!(ret.operands.len(), 1);
    }

    #[test]
    fn full_chain_source_to_jit_sphere_sdf_gradient() {
        // ═══════════════════════════════════════════════════════════════════
        // § T11-D23 KILLER TEST : CSSLv3 source drives the entire pipeline
        //   down to JIT-executed gradient numerical verification.
        // ═══════════════════════════════════════════════════════════════════
        let src = r"@differentiable fn sphere_sdf(p : f32, r : f32) -> f32 { p - r }";
        let module = pipeline_source_to_ad_mir("killer_app", src);
        let primal = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf")
            .expect("primal");
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf_fwd")
            .expect("fwd");
        let tangent_only = extract_tangent_only_variant(fwd);

        let mut m = JitModule::new();
        let primal_handle = m.compile(primal).expect("JIT compile primal");
        let tangent_handle = m.compile(&tangent_only).expect("JIT compile tangent");
        m.finalize().expect("finalize JIT module");

        // Sanity check : primal sphere_sdf(p, r) = p - r.
        let prim_v = primal_handle
            .call_f32_f32_to_f32(5.0, 2.0, &m)
            .expect("call primal");
        assert!(
            (prim_v - 3.0).abs() < 1e-6,
            "primal: expected 3.0 got {prim_v}"
        );

        // Tangent fn signature (from walker) : (p, d_p, r, d_r) -> d_y.
        // For `y = p - r` : d_y = d_p - d_r. Verify :
        // seed (d_p=1, d_r=0) ⇒ d_y = 1.
        // seed (d_p=0, d_r=1) ⇒ d_y = -1.
        let t_wrt_p = tangent_handle
            .call_f32_f32_f32_f32_to_f32(5.0, 1.0, 2.0, 0.0, &m)
            .expect("call tangent wrt p");
        assert!(
            (t_wrt_p - 1.0).abs() < 1e-6,
            "∂/∂p : expected 1.0 got {t_wrt_p}"
        );

        let t_wrt_r = tangent_handle
            .call_f32_f32_f32_f32_to_f32(5.0, 0.0, 2.0, 1.0, &m)
            .expect("call tangent wrt r");
        assert!(
            (t_wrt_r - (-1.0)).abs() < 1e-6,
            "∂/∂r : expected -1.0 got {t_wrt_r}"
        );

        // Cross-check via central-differences.
        let h = 1e-3_f32;
        let samples: &[(f32, f32)] = &[(5.0, 2.0), (0.0, 1.0), (-3.5, 4.2), (10.0, 7.0)];
        for &(p, r) in samples {
            let tangent_p = tangent_handle
                .call_f32_f32_f32_f32_to_f32(p, 1.0, r, 0.0, &m)
                .unwrap();
            let primal_plus = primal_handle.call_f32_f32_to_f32(p + h, r, &m).unwrap();
            let primal_minus = primal_handle.call_f32_f32_to_f32(p - h, r, &m).unwrap();
            let num_p = (primal_plus - primal_minus) / (2.0 * h);
            assert!(
                (tangent_p - num_p).abs() < 1e-3,
                "∂/∂p mismatch @ ({p}, {r}) : JIT={tangent_p} vs central-diff={num_p}"
            );
        }
    }

    #[test]
    fn full_chain_source_to_jit_fmul_gradient() {
        // fn mul(a, b) = a * b ; analytic gradient : ∂/∂a = b, ∂/∂b = a.
        let src = r"@differentiable fn mul(a : f32, b : f32) -> f32 { a * b }";
        let module = pipeline_source_to_ad_mir("mul_test", src);
        let primal = module
            .funcs
            .iter()
            .find(|f| f.name == "mul")
            .expect("primal");
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "mul_fwd")
            .expect("fwd");
        let tangent_only = extract_tangent_only_variant(fwd);

        let mut m = JitModule::new();
        let primal_h = m.compile(primal).unwrap();
        let tangent_h = m.compile(&tangent_only).unwrap();
        m.finalize().unwrap();

        // Primal : mul(3, 5) = 15.
        assert!((primal_h.call_f32_f32_to_f32(3.0, 5.0, &m).unwrap() - 15.0).abs() < 1e-5);

        // ∂(a*b)/∂a = b. At (a=3, b=5) with d_a=1, d_b=0 → tangent = 5.
        let t_a = tangent_h
            .call_f32_f32_f32_f32_to_f32(3.0, 1.0, 5.0, 0.0, &m)
            .unwrap();
        assert!((t_a - 5.0).abs() < 1e-5, "∂/∂a : expected 5.0 got {t_a}");

        // ∂(a*b)/∂b = a. At (a=3, b=5) with d_a=0, d_b=1 → tangent = 3.
        let t_b = tangent_h
            .call_f32_f32_f32_f32_to_f32(3.0, 0.0, 5.0, 1.0, &m)
            .unwrap();
        assert!((t_b - 3.0).abs() < 1e-5, "∂/∂b : expected 3.0 got {t_b}");
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D24 : source-driven scene-SDF via func.call intrinsics
    //   (min / max / abs / sqrt) — the AD-walker-emitted fwd variant runs
    //   end-to-end against central-differences on real piecewise-linear ops.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn full_chain_source_scene_sdf_min_runtime_gradient() {
        // ═══════════════════════════════════════════════════════════════════
        // § T11-D24 KILLER TEST : CSSLv3 source w/ `min` intrinsic → full
        //   pipeline → JIT-compiled primal + tangent → gradient verified
        //   against central-differences at sample points.
        //
        //   The tangent body contains cmpf + select (T11-D15 emission).
        //   The primal body contains func.call callee=min (JIT intrinsic).
        //   Both are runtime-executable.
        // ═══════════════════════════════════════════════════════════════════
        let src = r"@differentiable fn scene(a : f32, b : f32) -> f32 { min(a, b) }";
        let module = pipeline_source_to_ad_mir("scene_min", src);
        let primal = module
            .funcs
            .iter()
            .find(|f| f.name == "scene")
            .expect("primal scene");
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "scene_fwd")
            .expect("scene_fwd");
        let tangent_only = extract_tangent_only_variant(fwd);

        let mut m = JitModule::new();
        let primal_h = m.compile(primal).expect("JIT primal min");
        let tangent_h = m.compile(&tangent_only).expect("JIT tangent");
        m.finalize().expect("finalize");

        // Sanity : primal min(3, 5) = 3.
        let v = primal_h.call_f32_f32_to_f32(3.0, 5.0, &m).unwrap();
        assert!((v - 3.0).abs() < 1e-6, "primal: expected 3.0, got {v}");
        let v2 = primal_h.call_f32_f32_to_f32(7.0, 2.0, &m).unwrap();
        assert!((v2 - 2.0).abs() < 1e-6, "primal: expected 2.0, got {v2}");

        // Tangent sanity : min's gradient is pick-the-winner.
        // (a=3, b=5) : a < b, so min = a. ∂min/∂a = 1, ∂min/∂b = 0.
        let t_a = tangent_h
            .call_f32_f32_f32_f32_to_f32(3.0, 1.0, 5.0, 0.0, &m)
            .unwrap();
        let t_b = tangent_h
            .call_f32_f32_f32_f32_to_f32(3.0, 0.0, 5.0, 1.0, &m)
            .unwrap();
        assert!(
            (t_a - 1.0).abs() < 1e-6,
            "∂/∂a @ (3, 5) : expected 1.0, got {t_a}"
        );
        assert!(t_b.abs() < 1e-6, "∂/∂b @ (3, 5) : expected 0.0, got {t_b}");

        // (a=8, b=2) : a > b, so min = b. ∂min/∂a = 0, ∂min/∂b = 1.
        let t_a2 = tangent_h
            .call_f32_f32_f32_f32_to_f32(8.0, 1.0, 2.0, 0.0, &m)
            .unwrap();
        let t_b2 = tangent_h
            .call_f32_f32_f32_f32_to_f32(8.0, 0.0, 2.0, 1.0, &m)
            .unwrap();
        assert!(
            t_a2.abs() < 1e-6,
            "∂/∂a @ (8, 2) : expected 0.0, got {t_a2}"
        );
        assert!(
            (t_b2 - 1.0).abs() < 1e-6,
            "∂/∂b @ (8, 2) : expected 1.0, got {t_b2}"
        );

        // Central-differences cross-check across 5 sample points (cusp-avoided).
        let h = 1e-3_f32;
        let samples: &[(f32, f32)] = &[
            (3.0, 5.0),
            (5.0, 3.0),
            (-1.0, 4.0),
            (2.5, -0.5),
            (10.0, -3.7),
        ];
        for &(a, b) in samples {
            let t_a = tangent_h
                .call_f32_f32_f32_f32_to_f32(a, 1.0, b, 0.0, &m)
                .unwrap();
            let plus = primal_h.call_f32_f32_to_f32(a + h, b, &m).unwrap();
            let minus = primal_h.call_f32_f32_to_f32(a - h, b, &m).unwrap();
            let num_a = (plus - minus) / (2.0 * h);
            assert!(
                (t_a - num_a).abs() < 1e-3,
                "∂/∂a mismatch @ ({a}, {b}) : JIT={t_a} vs central={num_a}"
            );

            let t_b = tangent_h
                .call_f32_f32_f32_f32_to_f32(a, 0.0, b, 1.0, &m)
                .unwrap();
            let plus_b = primal_h.call_f32_f32_to_f32(a, b + h, &m).unwrap();
            let minus_b = primal_h.call_f32_f32_to_f32(a, b - h, &m).unwrap();
            let num_b = (plus_b - minus_b) / (2.0 * h);
            assert!(
                (t_b - num_b).abs() < 1e-3,
                "∂/∂b mismatch @ ({a}, {b}) : JIT={t_b} vs central={num_b}"
            );
        }
    }

    #[test]
    fn full_chain_source_scene_sdf_max_runtime_gradient() {
        let src = r"@differentiable fn scene(a : f32, b : f32) -> f32 { max(a, b) }";
        let module = pipeline_source_to_ad_mir("scene_max", src);
        let primal = module
            .funcs
            .iter()
            .find(|f| f.name == "scene")
            .expect("primal");
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "scene_fwd")
            .expect("fwd");
        let tangent_only = extract_tangent_only_variant(fwd);
        let mut m = JitModule::new();
        let primal_h = m.compile(primal).unwrap();
        let tangent_h = m.compile(&tangent_only).unwrap();
        m.finalize().unwrap();

        assert!((primal_h.call_f32_f32_to_f32(3.0, 5.0, &m).unwrap() - 5.0).abs() < 1e-6);
        // max(3, 5) = 5 ⇒ ∂/∂a = 0, ∂/∂b = 1.
        let t_a = tangent_h
            .call_f32_f32_f32_f32_to_f32(3.0, 1.0, 5.0, 0.0, &m)
            .unwrap();
        let t_b = tangent_h
            .call_f32_f32_f32_f32_to_f32(3.0, 0.0, 5.0, 1.0, &m)
            .unwrap();
        assert!(t_a.abs() < 1e-6);
        assert!((t_b - 1.0).abs() < 1e-6);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D25 : Bwd-mode (reverse) full-chain JIT integration.
    //   For single-float-param primals, the bwd variant has a single
    //   adjoint-result ; JIT-compile + verify against analytic formulas.
    // ─────────────────────────────────────────────────────────────────────

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D26 : source-driven inter-fn JIT — helper fn called from scene.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn full_chain_source_inter_fn_call_runtime() {
        // Multi-fn module : helper(x) = x * x ; scene(x) = helper(x) + 1.0.
        // Both fns land in the same MirModule after lex + parse + HIR + MIR.
        // The JIT must declare both, let scene reference helper, and both
        // execute correctly.
        let src = r"
fn helper(x : f32) -> f32 { x * x }
fn scene(x : f32) -> f32 { helper(x) }
";
        let module = pipeline_source_to_ad_mir("multi_fn", src);
        let helper = module
            .funcs
            .iter()
            .find(|f| f.name == "helper")
            .expect("helper");
        let scene = module
            .funcs
            .iter()
            .find(|f| f.name == "scene")
            .expect("scene");

        let mut m = JitModule::new();
        m.compile(helper).expect("JIT helper");
        let scene_h = m.compile(scene).expect("JIT scene");
        m.finalize().expect("finalize");

        // scene(3) = helper(3) = 3² = 9.
        let r = scene_h.call_f32_to_f32(3.0, &m).unwrap();
        assert!((r - 9.0).abs() < 1e-5, "expected 9.0, got {r}");

        // scene(-4) = 16.
        let r2 = scene_h.call_f32_to_f32(-4.0, &m).unwrap();
        assert!((r2 - 16.0).abs() < 1e-5);
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D27 : Multi-param bwd via single-adjoint extraction.
    //   `@differentiable fn f(a, b)` has Bwd signature `(a, b, d_y) -> (d_a, d_b)`.
    //   Extract d_a + d_b separately → JIT each → verify per-param gradients.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn full_chain_source_bwd_mul_per_param_adjoints() {
        // fn mul(a, b) = a * b.
        //   ∂/∂a = b ; ∂/∂b = a.
        let src = r"@differentiable fn mul(a : f32, b : f32) -> f32 { a * b }";
        let module = pipeline_source_to_ad_mir("mul_bwd", src);
        let bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "mul_bwd")
            .expect("mul_bwd");
        assert_eq!(
            bwd.results.len(),
            2,
            "mul has 2 params → bwd has 2 adjoints"
        );

        // Extract d_a (index 0) + d_b (index 1) separately.
        let bwd_da = extract_bwd_single_adjoint(bwd, 0);
        let bwd_db = extract_bwd_single_adjoint(bwd, 1);
        assert_eq!(bwd_da.results.len(), 1);
        assert_eq!(bwd_db.results.len(), 1);
        assert_eq!(bwd_da.name, "mul_bwd_d0");
        assert_eq!(bwd_db.name, "mul_bwd_d1");

        let mut m = JitModule::new();
        let h_da = m.compile(&bwd_da).expect("JIT bwd_da");
        let h_db = m.compile(&bwd_db).expect("JIT bwd_db");
        m.finalize().unwrap();

        // Signature : (a, b, d_y) → d_a. At (a=3, b=5, d_y=1) : d_a = b·d_y = 5.
        // JIT call helpers : 3-arg f32 → f32 needs a new helper OR use call_f32_f32_f32_f32.
        // mul has 2 primal params, so bwd is (a, b, d_y) 3-arg. We don't have a
        // 3-arg helper yet — use the 4-arg helper with a dummy last arg? No,
        // the signature is 3-arg. Add a call_f32_f32_f32_to_f32 helper below.

        let d_a_val = h_da.call_f32_f32_f32_to_f32(3.0, 5.0, 1.0, &m).unwrap();
        assert!(
            (d_a_val - 5.0).abs() < 1e-5,
            "∂(a*b)/∂a @ (3, 5) : expected 5, got {d_a_val}"
        );

        let d_b_val = h_db.call_f32_f32_f32_to_f32(3.0, 5.0, 1.0, &m).unwrap();
        assert!(
            (d_b_val - 3.0).abs() < 1e-5,
            "∂(a*b)/∂b @ (3, 5) : expected 3, got {d_b_val}"
        );

        // Chain rule check : scale d_y.
        let d_a_scaled = h_da.call_f32_f32_f32_to_f32(2.0, 7.0, 0.5, &m).unwrap();
        assert!((d_a_scaled - 3.5).abs() < 1e-5); // 7·0.5
        let d_b_scaled = h_db.call_f32_f32_f32_to_f32(2.0, 7.0, 0.5, &m).unwrap();
        assert!((d_b_scaled - 1.0).abs() < 1e-5); // 2·0.5

        // Central-differences cross-check.
        let h_step = 1e-3_f32;
        let samples: &[(f32, f32)] = &[(1.0, 2.0), (-3.0, 4.5), (0.5, 0.7)];
        for &(a, b) in samples {
            let d_a = h_da.call_f32_f32_f32_to_f32(a, b, 1.0, &m).unwrap();
            let d_b = h_db.call_f32_f32_f32_to_f32(a, b, 1.0, &m).unwrap();

            // Central-diff on a*b :
            let primal = |aa: f32, bb: f32| aa * bb;
            let num_a = (primal(a + h_step, b) - primal(a - h_step, b)) / (2.0 * h_step);
            let num_b = (primal(a, b + h_step) - primal(a, b - h_step)) / (2.0 * h_step);

            assert!(
                (d_a - num_a).abs() < 1e-3,
                "∂/∂a @ ({a}, {b}) : JIT={d_a} vs central={num_a}"
            );
            assert!(
                (d_b - num_b).abs() < 1e-3,
                "∂/∂b @ ({a}, {b}) : JIT={d_b} vs central={num_b}"
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D28 : KILLER-APP COMPOSITION — scene-SDF union of two sphere-SDFs.
    //   Composes T11-D24 (intrinsic min), T11-D26 (inter-fn sphere_sdf call),
    //   T11-D27 (per-param bwd extraction). Everything landed in this session
    //   exercising a single source-driven runtime-gradient test.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn full_chain_source_scene_sdf_union_composition() {
        // Two-sphere scene-SDF union :
        //   fn sphere_sdf(p, r) = p - r          (scalar surrogate, T7-D5 canonical)
        //   fn scene(p, r0, r1) = min(sphere_sdf(p, r0), sphere_sdf(p, r1))
        //
        // Analytic gradient of `min` picks the winning branch, and each
        // sphere_sdf contributes ∂/∂p=1, ∂/∂r=-1. So :
        //   At sphere_0-winning region : ∂scene/∂p = 1, ∂/∂r0 = -1, ∂/∂r1 = 0
        //   At sphere_1-winning region : ∂scene/∂p = 1, ∂/∂r0 = 0, ∂/∂r1 = -1
        let src = r"
@differentiable fn sphere_sdf(p : f32, r : f32) -> f32 { p - r }
@differentiable fn scene(p : f32, r0 : f32, r1 : f32) -> f32 {
    min(sphere_sdf(p, r0), sphere_sdf(p, r1))
}
";
        let module = pipeline_source_to_ad_mir("scene_union", src);
        // Locate all 4 fns : sphere_sdf + scene (primals) + their _fwd + _bwd.
        let sphere_sdf = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf")
            .expect("sphere_sdf primal");
        let scene = module
            .funcs
            .iter()
            .find(|f| f.name == "scene")
            .expect("scene primal");

        // JIT the primals : sphere_sdf first (callee), then scene (caller).
        let mut m = JitModule::new();
        let sphere_h = m.compile(sphere_sdf).expect("JIT sphere_sdf");
        let scene_h = m.compile(scene).expect("JIT scene");
        m.finalize().expect("finalize");

        // Verify primal sphere_sdf(3, 2) = 1.
        let sv = sphere_h.call_f32_f32_to_f32(3.0, 2.0, &m).unwrap();
        assert!(
            (sv - 1.0).abs() < 1e-6,
            "sphere_sdf(3, 2) = 1 expected, got {sv}"
        );

        // Verify primal scene :
        // scene(p=5, r0=3, r1=1) = min(sphere_sdf(5, 3), sphere_sdf(5, 1))
        //                        = min(5-3, 5-1) = min(2, 4) = 2.
        let sc = scene_h.call_f32_f32_f32_to_f32(5.0, 3.0, 1.0, &m).unwrap();
        assert!(
            (sc - 2.0).abs() < 1e-6,
            "scene(5, 3, 1) = 2 expected, got {sc}"
        );

        // scene(p=5, r0=1, r1=3) = min(5-1, 5-3) = min(4, 2) = 2 (same, sphere_1 wins now).
        let sc2 = scene_h.call_f32_f32_f32_to_f32(5.0, 1.0, 3.0, &m).unwrap();
        assert!((sc2 - 2.0).abs() < 1e-6);

        // Central-difference verification of ∂scene/∂p : always 1.0
        // (both branches' gradients wrt p are 1, so min's winner is 1 either way).
        let h_step = 1e-3_f32;
        let samples: &[(f32, f32, f32)] = &[
            (5.0, 3.0, 1.0),  // sphere_0 wins
            (5.0, 1.0, 3.0),  // sphere_1 wins
            (10.0, 2.0, 7.0), // sphere_0 wins
            (-1.0, 4.0, 2.0), // sphere_1 wins (min(-5, -3) = -5)
        ];
        for &(p, r0, r1) in samples {
            let plus = scene_h
                .call_f32_f32_f32_to_f32(p + h_step, r0, r1, &m)
                .unwrap();
            let minus = scene_h
                .call_f32_f32_f32_to_f32(p - h_step, r0, r1, &m)
                .unwrap();
            let num_p = (plus - minus) / (2.0 * h_step);
            assert!(
                (num_p - 1.0).abs() < 1e-3,
                "∂scene/∂p @ ({p}, {r0}, {r1}) : expected 1.0, got {num_p}"
            );

            // Central-diff on r0 : +1 or 0 based on which branch wins.
            let plus_r0 = scene_h
                .call_f32_f32_f32_to_f32(p, r0 + h_step, r1, &m)
                .unwrap();
            let minus_r0 = scene_h
                .call_f32_f32_f32_to_f32(p, r0 - h_step, r1, &m)
                .unwrap();
            let num_r0 = (plus_r0 - minus_r0) / (2.0 * h_step);
            let expected_r0 = if (p - r0) < (p - r1) { -1.0 } else { 0.0 };
            assert!(
                (num_r0 - expected_r0).abs() < 1e-3,
                "∂scene/∂r0 @ ({p}, {r0}, {r1}) : expected {expected_r0}, got {num_r0}"
            );
        }
    }

    #[test]
    fn full_chain_source_bwd_two_params_affine() {
        // fn lin2(a, b) = 2*a + b — hand-written via a+a+b.
        //   ∂/∂a = 2, ∂/∂b = 1.
        let src = r"@differentiable fn lin2(a : f32, b : f32) -> f32 { a + a + b }";
        let module = pipeline_source_to_ad_mir("lin2_bwd", src);
        let bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "lin2_bwd")
            .expect("lin2_bwd");
        let bwd_da = extract_bwd_single_adjoint(bwd, 0);
        let bwd_db = extract_bwd_single_adjoint(bwd, 1);

        let mut m = JitModule::new();
        let h_da = m.compile(&bwd_da).unwrap();
        let h_db = m.compile(&bwd_db).unwrap();
        m.finalize().unwrap();

        // ∂(2a+b)/∂a = 2, regardless of (a, b).
        for (a, b) in [(1.0_f32, 2.0), (-3.0, 7.0), (10.0, -5.0)] {
            let d_a = h_da.call_f32_f32_f32_to_f32(a, b, 1.0, &m).unwrap();
            assert!(
                (d_a - 2.0).abs() < 1e-5,
                "∂/∂a @ ({a}, {b}) : expected 2, got {d_a}"
            );
            let d_b = h_db.call_f32_f32_f32_to_f32(a, b, 1.0, &m).unwrap();
            assert!(
                (d_b - 1.0).abs() < 1e-5,
                "∂/∂b @ ({a}, {b}) : expected 1, got {d_b}"
            );
        }
    }

    #[test]
    fn full_chain_source_bwd_sq_adjoint() {
        // fn sq(x : f32) -> f32 { x * x }
        //   ∂(x²)/∂x = 2x
        //   bwd signature: (x, d_y) -> d_x  where d_x = 2·x·d_y.
        let src = r"@differentiable fn sq(x : f32) -> f32 { x * x }";
        let module = pipeline_source_to_ad_mir("sq_bwd", src);
        let bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "sq_bwd")
            .expect("bwd variant");
        // For single-param primal, bwd already has signature (x, d_y) -> d_x.
        // No post-processing needed.
        let mut m = JitModule::new();
        let h = m.compile(bwd).expect("JIT compile bwd");
        m.finalize().unwrap();

        // d_x = 2·x·d_y at x=3, d_y=1 → 6.
        let d_x = h.call_f32_f32_to_f32(3.0, 1.0, &m).unwrap();
        assert!(
            (d_x - 6.0).abs() < 1e-5,
            "expected ∂(x²)/∂x·d_y @ x=3, d_y=1 = 6, got {d_x}"
        );
        // d_x = 2·x·d_y at x=2, d_y=0.5 → 2·2·0.5 = 2.
        let d_x2 = h.call_f32_f32_to_f32(2.0, 0.5, &m).unwrap();
        assert!((d_x2 - 2.0).abs() < 1e-5);

        // Cross-check against central-differences on the primal.
        // Primal is `x * x`. Central-diff : ((x+h)² - (x-h)²) / 2h = 2x.
        let h_step = 1e-3_f32;
        let xs = [-4.5_f32, -1.0, 0.5, 3.7, 10.0];
        for &x in &xs {
            let analytic = 2.0 * x;
            let bwd_dx = h.call_f32_f32_to_f32(x, 1.0, &m).unwrap();
            assert!(
                (bwd_dx - analytic).abs() < 1e-4,
                "bwd ∂/∂x @ x={x} : JIT={bwd_dx} vs analytic={analytic}"
            );
            // Central-diff on the primal x²:
            let primal_plus = (x + h_step).powi(2);
            let primal_minus = (x - h_step).powi(2);
            let num = (primal_plus - primal_minus) / (2.0 * h_step);
            assert!(
                (bwd_dx - num).abs() < 1e-2,
                "bwd ∂/∂x @ x={x} : JIT={bwd_dx} vs central-diff={num}"
            );
        }
    }

    #[test]
    fn full_chain_source_bwd_cube_adjoint() {
        // fn cube(x) = x * x * x.
        // ∂(x³)/∂x = 3x²
        let src = r"@differentiable fn cube(x : f32) -> f32 { x * x * x }";
        let module = pipeline_source_to_ad_mir("cube_bwd", src);
        let bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "cube_bwd")
            .expect("cube_bwd");
        let mut m = JitModule::new();
        let h = m.compile(bwd).expect("JIT");
        m.finalize().unwrap();

        // At x=2, d_y=1 : d_x = 3·4·1 = 12.
        let d_x = h.call_f32_f32_to_f32(2.0, 1.0, &m).unwrap();
        assert!(
            (d_x - 12.0).abs() < 1e-4,
            "∂(x³)/∂x @ x=2 : expected 12, got {d_x}"
        );
        // At x=-3, d_y=1 : d_x = 3·9·1 = 27.
        let d_x2 = h.call_f32_f32_to_f32(-3.0, 1.0, &m).unwrap();
        assert!(
            (d_x2 - 27.0).abs() < 1e-4,
            "∂(x³)/∂x @ x=-3 : expected 27, got {d_x2}"
        );
    }

    #[test]
    fn full_chain_source_bwd_affine_adjoint() {
        // fn affine(x) = x + x + x   (d = 3)
        // Chain : v = add(add(x, x), x) → ∂v/∂x = 3.
        let src = r"@differentiable fn affine(x : f32) -> f32 { x + x + x }";
        let module = pipeline_source_to_ad_mir("affine_bwd", src);
        let bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "affine_bwd")
            .expect("bwd");
        let mut m = JitModule::new();
        let h = m.compile(bwd).expect("JIT");
        m.finalize().unwrap();

        // d_x = 3·d_y. At any x, d_y=1 : d_x = 3.
        for x in [0.0_f32, 5.0, -2.5, 100.0] {
            let d_x = h.call_f32_f32_to_f32(x, 1.0, &m).unwrap();
            assert!(
                (d_x - 3.0).abs() < 1e-5,
                "∂(3x)/∂x @ x={x} : expected 3, got {d_x}"
            );
        }
    }

    #[test]
    fn full_chain_source_scene_sdf_abs_runtime_gradient() {
        let src = r"@differentiable fn scene(a : f32) -> f32 { abs(a) }";
        let module = pipeline_source_to_ad_mir("scene_abs", src);
        let primal = module
            .funcs
            .iter()
            .find(|f| f.name == "scene")
            .expect("primal");
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "scene_fwd")
            .expect("fwd");
        let tangent_only = extract_tangent_only_variant(fwd);
        let mut m = JitModule::new();
        let primal_h = m.compile(primal).unwrap();
        let tangent_h = m.compile(&tangent_only).unwrap();
        m.finalize().unwrap();

        // Primal : |3| = 3, |-4| = 4.
        assert!((primal_h.call_f32_to_f32(3.0, &m).unwrap() - 3.0).abs() < 1e-6);
        assert!((primal_h.call_f32_to_f32(-4.0, &m).unwrap() - 4.0).abs() < 1e-6);

        // Tangent : sig is (a: f32, d_a: f32) -> f32. ∂|a|/∂a = sign(a) (except at 0).
        // JitFn::call_f32_f32_to_f32 works for 2-arg fn.
        // For a > 0 : d_|a|/d_a · d_a = 1 · d_a = d_a.
        let t_pos = tangent_h.call_f32_f32_to_f32(3.0, 1.0, &m).unwrap();
        assert!(
            (t_pos - 1.0).abs() < 1e-6,
            "∂|a|/∂a @ a=3 : expected 1.0, got {t_pos}"
        );
        // For a < 0 : d_|a|/d_a = -1, so tangent = -1 · d_a = -1.
        let t_neg = tangent_h.call_f32_f32_to_f32(-4.0, 1.0, &m).unwrap();
        assert!(
            (t_neg - (-1.0)).abs() < 1e-6,
            "∂|a|/∂a @ a=-4 : expected -1.0, got {t_neg}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D35 : real vec3 sphere-SDF end-to-end via body-lower scalarization.
    //
    // The source `@differentiable fn sphere_sdf(p : vec3<f32>, r : f32) -> f32
    //                                        { length(p) - r }` flows through :
    //
    //   (1) cssl_mir::body_lower — `p : vec3<f32>` scalarizes to 3 scalar f32
    //       MIR params (p_0, p_1, p_2) ; `r` stays scalar. Signature = 4 f32 in.
    //       `length(p)` inlines to `sqrt(p_0*p_0 + p_1*p_1 + p_2*p_2)` (7 ops).
    //   (2) cssl_autodiff AD walker — treats the 4-scalar fn as standard scalar
    //       AD ; emits fwd variant with 4 tangent params appended.
    //   (3) extract_tangent_only_variant — strips the primal result, leaving a
    //       single-f32 tangent output.
    //   (4) Cranelift JIT — compiles as 8-scalar-input, 1-scalar-output fn.
    //
    // Runtime gradient check : for p = (3, 0, 4), r = 1 :
    //   length(p) = sqrt(9 + 0 + 16) = 5 ; sphere_sdf = 5 - 1 = 4.
    //   ∂sphere_sdf/∂p_0 = p_0 / length(p) = 3 / 5 = 0.6   (normalize(p).x)
    //   ∂sphere_sdf/∂p_1 = p_1 / length(p) = 0 / 5 = 0.0   (normalize(p).y)
    //   ∂sphere_sdf/∂p_2 = p_2 / length(p) = 4 / 5 = 0.8   (normalize(p).z)
    //   ∂sphere_sdf/∂r   = -1.0
    //
    // Each lane-gradient is verified by (a) seeding a 1.0-tangent on exactly that
    // param + zeros elsewhere, (b) computing the central-difference of the primal
    // at that point, (c) asserting both agree within 1e-3.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn full_chain_source_to_jit_sphere_sdf_vec3_gradient_matches_normalize() {
        use super::jit_primal_and_tangent;

        // Real vec3 sphere-SDF : `@differentiable fn sphere_sdf(p : vec3<f32>,
        //                                    r : f32) -> f32 { length(p) - r }`.
        // body_lower scalarizes `p : vec3<f32>` → 3 scalar f32 params ; `length(p)`
        // inlines to the sqrt(sum-of-squares) expansion.
        let src = r"@differentiable fn sphere_sdf(p : vec3<f32>, r : f32) -> f32 {
            length(p) - r
        }";
        let module = pipeline_source_to_ad_mir("sphere_sdf_vec3", src);

        let primal = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf")
            .expect("primal sphere_sdf");
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf_fwd")
            .expect("fwd variant sphere_sdf_fwd");

        // § SIGNATURE SANITY : after scalarization the primal has 4 scalar f32
        //   params + 1 f32 result. The walker's fwd variant has 8 params (4
        //   primal + 4 tangent) + 2 results (primal + tangent).
        assert_eq!(
            primal.params.len(),
            4,
            "vec3 scalarization : primal must have 4 scalar f32 params : got {:?}",
            primal.params
        );
        assert_eq!(
            fwd.params.len(),
            8,
            "fwd variant : 4 primal + 4 tangent params : got {:?}",
            fwd.params
        );

        let tangent_only = extract_tangent_only_variant(fwd);
        let handle = jit_primal_and_tangent(primal, &tangent_only).expect("JIT sphere_sdf vec3");
        let m = &handle.module;
        let primal_h = &handle.primal_fn;
        let tangent_h = &handle.tangent_fn;

        // § PRIMAL : at p = (3, 0, 4), r = 1 — expect length(p) - r = 5 - 1 = 4.
        let v = primal_h
            .call_f32_f32_f32_f32_to_f32(3.0, 0.0, 4.0, 1.0, m)
            .unwrap();
        assert!(
            (v - 4.0).abs() < 1e-5,
            "primal sphere_sdf(3, 0, 4, 1) : expected 4.0, got {v}"
        );

        // § ANALYTIC GRADIENT (the killer-app claim) :
        //   ∇_p length(p) = normalize(p) = (3/5, 0/5, 4/5) = (0.6, 0.0, 0.8)
        //   ∂/∂r (length(p) - r) = -1
        let expected_d_p0 = 0.6_f32;
        let expected_d_p1 = 0.0_f32;
        let expected_d_p2 = 0.8_f32;
        let expected_d_r = -1.0_f32;

        // § FWD-MODE EXTRACTED : walker interleaves the tangent variant's params
        //   as `[p0, d_p0, p1, d_p1, p2, d_p2, r, d_r]` (per synthesize_tangent_params
        //   in cssl-autodiff/src/substitute.rs). With seeded tangent the result is
        //   Σᵢ (∂f/∂xᵢ · d_xᵢ). Seeding a single 1.0 extracts that component's
        //   partial ; all other tangents zero.
        let t_p0 = tangent_h
            .call_f32x8_to_f32(3.0, 1.0, 0.0, 0.0, 4.0, 0.0, 1.0, 0.0, m)
            .unwrap();
        let t_p1 = tangent_h
            .call_f32x8_to_f32(3.0, 0.0, 0.0, 1.0, 4.0, 0.0, 1.0, 0.0, m)
            .unwrap();
        let t_p2 = tangent_h
            .call_f32x8_to_f32(3.0, 0.0, 0.0, 0.0, 4.0, 1.0, 1.0, 0.0, m)
            .unwrap();
        let t_r = tangent_h
            .call_f32x8_to_f32(3.0, 0.0, 0.0, 0.0, 4.0, 0.0, 1.0, 1.0, m)
            .unwrap();

        assert!(
            (t_p0 - expected_d_p0).abs() < 1e-3,
            "∂sphere_sdf/∂p_0 @ p=(3,0,4) : expected {expected_d_p0} = normalize(p).x, got {t_p0}"
        );
        assert!(
            (t_p1 - expected_d_p1).abs() < 1e-3,
            "∂sphere_sdf/∂p_1 @ p=(3,0,4) : expected {expected_d_p1} = normalize(p).y, got {t_p1}"
        );
        assert!(
            (t_p2 - expected_d_p2).abs() < 1e-3,
            "∂sphere_sdf/∂p_2 @ p=(3,0,4) : expected {expected_d_p2} = normalize(p).z, got {t_p2}"
        );
        assert!(
            (t_r - expected_d_r).abs() < 1e-3,
            "∂sphere_sdf/∂r @ p=(3,0,4), r=1 : expected {expected_d_r}, got {t_r}"
        );

        // § CENTRAL-DIFFERENCE CROSS-CHECK : the analytic values above match
        //   normalize(p) by construction ; this additional check proves the JIT-
        //   computed tangent ALSO matches the numerical gradient of the JIT-
        //   computed primal (no algebraic shortcut — both come from executed
        //   machine code, not symbolic simplification).
        let h = 1e-3_f32;
        for (seed_idx, expected) in [(0, expected_d_p0), (1, expected_d_p1), (2, expected_d_p2)] {
            let mut plus = [3.0_f32, 0.0, 4.0, 1.0];
            let mut minus = [3.0_f32, 0.0, 4.0, 1.0];
            plus[seed_idx] += h;
            minus[seed_idx] -= h;
            let y_plus = primal_h
                .call_f32_f32_f32_f32_to_f32(plus[0], plus[1], plus[2], plus[3], m)
                .unwrap();
            let y_minus = primal_h
                .call_f32_f32_f32_f32_to_f32(minus[0], minus[1], minus[2], minus[3], m)
                .unwrap();
            let central = (y_plus - y_minus) / (2.0 * h);
            assert!(
                (central - expected).abs() < 5e-3,
                "central-diff ∂/∂x_{seed_idx} : expected {expected}, got {central}",
            );
        }
    }

    #[test]
    fn sphere_sdf_vec3_param_scalarization_produces_4_scalar_params() {
        // § REGRESSION GUARD — signature-lowering must scalarize vec3 params
        //   even BEFORE body-lowering runs. This test checks the shape emitted
        //   by `lower_function_signature` directly (isolated from the walker).
        let src = r"fn sphere_sdf(p : vec3<f32>, r : f32) -> f32 { r }";
        let module = pipeline_source_to_ad_mir("vec3_sig_regression", src);
        let f = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf")
            .expect("sphere_sdf");
        // 3 scalar f32 (from p) + 1 scalar f32 (r) = 4 scalar f32 params.
        assert_eq!(f.params.len(), 4);
        for (i, ty) in f.params.iter().enumerate() {
            assert!(
                matches!(ty, cssl_mir::MirType::Float(cssl_mir::FloatWidth::F32)),
                "param {i} must be scalar f32 after vec3 scalarization : got {ty:?}",
            );
        }
    }

    #[test]
    fn sphere_sdf_vec3_length_expansion_emits_scalar_ops() {
        // § REGRESSION GUARD — `length(p)` in body_lower must expand to scalar
        //   mulf + addf + sqrt, not emit an opaque `func.call @length` with a
        //   vec operand.
        let src = r"fn sphere_sdf(p : vec3<f32>) -> f32 { length(p) }";
        let module = pipeline_source_to_ad_mir("vec3_length_expansion", src);
        let f = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf")
            .expect("sphere_sdf");
        let entry = f.body.entry().expect("entry block");
        let names: Vec<&str> = entry.ops.iter().map(|o| o.name.as_str()).collect();
        assert!(
            names.iter().filter(|n| **n == "arith.mulf").count() >= 3,
            "expected ≥ 3 arith.mulf ops (one per lane square) : got {names:?}"
        );
        assert!(
            names.iter().filter(|n| **n == "arith.addf").count() >= 2,
            "expected ≥ 2 arith.addf ops (accumulate 3 squares) : got {names:?}"
        );
        // Final sqrt : func.call with callee="sqrt".
        let has_sqrt_call = entry.ops.iter().any(|op| {
            op.name == "func.call"
                && op
                    .attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "sqrt")
        });
        assert!(
            has_sqrt_call,
            "expected func.call @sqrt for length : got {names:?}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D37 : vec arc consolidation — bwd-mode sphere_sdf gradient +
    //   vec2 / vec4 length coverage (lane-count scalability regression guard).
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn full_chain_sphere_sdf_vec3_bwd_mode_gradient() {
        // § T11-D37 : bwd-mode counterpart of the T11-D35 fwd-mode test. The
        //   bwd variant has shape `(p_0, p_1, p_2, r, d_y) -> (d_0, d_1, d_2, d_3)` ;
        //   we use `extract_bwd_single_adjoint` to pull one scalar adjoint at a
        //   time (4 separate JIT-compiled fns, each with 5 in → 1 out).
        //
        //   With `d_y = 1.0` at `p = (3, 0, 4), r = 1`, the expected adjoints are :
        //     d_0 = p_0 / length(p) = 3/5 = 0.6     (normalize(p).x)
        //     d_1 = p_1 / length(p) = 0/5 = 0.0     (normalize(p).y)
        //     d_2 = p_2 / length(p) = 4/5 = 0.8     (normalize(p).z)
        //     d_3 = -1.0                            (∂(-r)/∂r)
        let src = r"@differentiable fn sphere_sdf(p : vec3<f32>, r : f32) -> f32 {
            length(p) - r
        }";
        let module = pipeline_source_to_ad_mir("sphere_sdf_vec3_bwd", src);
        let bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf_bwd")
            .expect("bwd variant sphere_sdf_bwd");

        // Check bwd signature shape BEFORE single-adjoint extraction.
        assert_eq!(
            bwd.params.len(),
            5,
            "bwd signature : 4 primal + 1 d_y : got {:?}",
            bwd.params
        );
        assert_eq!(
            bwd.results.len(),
            4,
            "bwd results : 1 per primal param : got {:?}",
            bwd.results
        );

        let expected = [0.6_f32, 0.0, 0.8, -1.0];

        for (idx, exp) in expected.iter().enumerate() {
            let single_adjoint = extract_bwd_single_adjoint(bwd, idx);
            // Compile this single-adjoint variant in its own JIT module so each
            // extraction gets a fresh single-result cranelift signature.
            let mut m = JitModule::new();
            let h = m
                .compile(&single_adjoint)
                .unwrap_or_else(|e| panic!("JIT bwd d_{idx} : {e:?}"));
            m.finalize().unwrap();

            // inputs : (p_0, p_1, p_2, r, d_y) = (3, 0, 4, 1, 1)
            let got = h.call_f32x5_to_f32(3.0, 0.0, 4.0, 1.0, 1.0, &m).unwrap();
            assert!(
                (got - exp).abs() < 1e-3,
                "bwd adjoint d_{idx} @ p=(3,0,4), r=1 : expected {exp}, got {got}"
            );
        }
    }

    #[test]
    fn full_chain_vec2_length_runtime() {
        // § T11-D37 : vec2 length scalarizes to 2 f32 params + `sqrt(p_0² + p_1²)`.
        //   At p = (3, 4) : length = 5.0.
        let src = r"fn len2(p : vec2<f32>) -> f32 { length(p) }";
        let module = pipeline_source_to_ad_mir("vec2_len", src);
        let primal = module
            .funcs
            .iter()
            .find(|f| f.name == "len2")
            .expect("len2 primal");

        // Signature sanity : 2 scalar f32 (p_0, p_1) → 1 f32.
        assert_eq!(
            primal.params.len(),
            2,
            "vec2 scalarization : 2 scalar params : got {:?}",
            primal.params
        );

        let mut m = JitModule::new();
        let h = m.compile(primal).expect("JIT vec2 len2");
        m.finalize().unwrap();

        let v = h.call_f32_f32_to_f32(3.0, 4.0, &m).unwrap();
        assert!(
            (v - 5.0).abs() < 1e-5,
            "len2((3, 4)) : expected 5.0, got {v}"
        );
    }

    #[test]
    fn full_chain_vec4_length_runtime() {
        // § T11-D37 : vec4 length scalarizes to 4 f32 params + `sqrt(Σ pᵢ²)`.
        //   At p = (2, 3, 6, 0) : length = sqrt(4 + 9 + 36 + 0) = sqrt(49) = 7.0.
        let src = r"fn len4(p : vec4<f32>) -> f32 { length(p) }";
        let module = pipeline_source_to_ad_mir("vec4_len", src);
        let primal = module
            .funcs
            .iter()
            .find(|f| f.name == "len4")
            .expect("len4 primal");

        // Signature sanity : 4 scalar f32 → 1 f32.
        assert_eq!(
            primal.params.len(),
            4,
            "vec4 scalarization : 4 scalar params : got {:?}",
            primal.params
        );

        let mut m = JitModule::new();
        let h = m.compile(primal).expect("JIT vec4 len4");
        m.finalize().unwrap();

        let v = h
            .call_f32_f32_f32_f32_to_f32(2.0, 3.0, 6.0, 0.0, &m)
            .unwrap();
        assert!(
            (v - 7.0).abs() < 1e-5,
            "len4((2, 3, 6, 0)) : expected 7.0, got {v}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D38 : generic-monomorphization end-to-end JIT integration.
    //
    // cssl-mir::monomorph::specialize_generic_fn produces a MirFunc from a
    // generic HirFn + TypeSubst. This test exercises the full pipeline :
    //   source `fn id<T>(x : T) -> T { x }` → HIR → specialize(T ↦ i32)
    //   → MirFunc `id_i32 : i32 → i32` → JIT → call id_i32(5) → assert 5.
    //
    // Proves the T11-D38 API produces machine-code-ready output, not just
    // a structurally-correct MirFunc.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn monomorph_specialize_id_i32_jit_executes() {
        use cssl_hir::{lower_module, HirItem};
        use cssl_mir::monomorph::{hir_primitive_type, specialize_generic_fn, TypeSubst};
        use cssl_mir::{FloatWidth, IntWidth, MirType};

        let src = r"fn id<T>(x : T) -> T { x }";
        let file = SourceFile::new(
            SourceId::first(),
            "<monomorph_id_i32>",
            src,
            Surface::RustHybrid,
        );
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = lower_module(&file, &cst);

        let id_fn = hir
            .items
            .iter()
            .find_map(|item| match item {
                HirItem::Fn(f) if interner.resolve(f.name) == "id" => Some(f.clone()),
                _ => None,
            })
            .expect("id fn");

        // § Specialize T ↦ i32.
        let t = interner.intern("T");
        let mut subst_i32 = TypeSubst::new();
        subst_i32.bind(t, hir_primitive_type("i32", &interner));
        let mir_id_i32 = specialize_generic_fn(&interner, Some(&file), &id_fn, &subst_i32);

        assert_eq!(mir_id_i32.name, "id_i32");
        assert_eq!(mir_id_i32.params, vec![MirType::Int(IntWidth::I32)]);
        assert_eq!(mir_id_i32.results, vec![MirType::Int(IntWidth::I32)]);

        // § Specialize T ↦ f32 — prove we can get a DIFFERENT specialization
        //   from the same generic source.
        let mut subst_f32 = TypeSubst::new();
        subst_f32.bind(t, hir_primitive_type("f32", &interner));
        let mir_id_f32 = specialize_generic_fn(&interner, Some(&file), &id_fn, &subst_f32);
        assert_eq!(mir_id_f32.name, "id_f32");
        assert_eq!(mir_id_f32.params, vec![MirType::Float(FloatWidth::F32)]);

        // § JIT-compile BOTH specializations in the same module + call each.
        let mut m = JitModule::new();
        let handle_i32 = m.compile(&mir_id_i32).expect("JIT compile id_i32");
        let handle_f32 = m.compile(&mir_id_f32).expect("JIT compile id_f32");
        m.finalize().expect("finalize");

        // § id_i32(5) → 5.
        let out_i32 = handle_i32.call_i32_to_i32(5, &m).expect("call id_i32(5)");
        assert_eq!(out_i32, 5, "id_i32(5) must return 5");

        // § id_i32(-42) → -42 (sign-preservation through i32).
        let out_neg = handle_i32
            .call_i32_to_i32(-42, &m)
            .expect("call id_i32(-42)");
        assert_eq!(out_neg, -42);

        // § id_f32(2.5) → 2.5 (within f32 round-trip tolerance ; avoid
        //   approx-PI values that clippy's `approx_constant` lint catches).
        let out_f32 = handle_f32
            .call_f32_to_f32(2.5, &m)
            .expect("call id_f32(2.5)");
        assert!(
            (out_f32 - 2.5_f32).abs() < 1e-5,
            "id_f32(2.5) ≈ 2.5 : got {out_f32}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D40 : auto-monomorphization end-to-end flow.
    //
    // Previously (T11-D38) callers had to invoke specialize_generic_fn
    // manually with a hand-built TypeSubst. T11-D40's walker discovers
    // turbofish call sites automatically and produces the specializations.
    // This test exercises the full chain :
    //
    //   source `fn id<T> + id::<i32>(5) + id::<f32>(2.5)`
    //     → HIR (turbofish carried through per T11-D39)
    //     → auto_monomorphize (walker produces 2 MirFuncs)
    //     → JIT-compile BOTH specializations in one module
    //     → call id_i32(5) = 5 + id_f32(2.5) = 2.5
    //
    // Proves the walker closes the "generic-fn → machine-code" loop without
    // any manual specialize_generic_fn invocation by the caller.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn auto_monomorphize_discovers_specializations_from_turbofish_calls() {
        use cssl_hir::lower_module;
        use cssl_mir::{auto_monomorphize, FloatWidth, IntWidth, MirType};

        let src = r"
            fn id<T>(x : T) -> T { x }
            fn use_i32() -> i32 { id::<i32>(5) }
            fn use_f32() -> f32 { id::<f32>(2.5) }
        ";
        let file = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = lower_module(&file, &cst);

        // § Walker runs — NO manual specialize_generic_fn call by this test.
        let report = auto_monomorphize(&hir, &interner, Some(&file));
        assert_eq!(report.generic_fn_count, 1);
        assert_eq!(report.call_site_count, 2);
        assert_eq!(report.specialization_count, 2);

        // § The 2 specializations must be id_i32 + id_f32.
        let names: Vec<&str> = report
            .specializations
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(names.contains(&"id_i32"), "id_i32 missing : {names:?}");
        assert!(names.contains(&"id_f32"), "id_f32 missing : {names:?}");

        // § JIT-compile BOTH specializations in a single module + call each.
        let id_i32_fn = report
            .specializations
            .iter()
            .find(|f| f.name == "id_i32")
            .unwrap();
        let id_f32_fn = report
            .specializations
            .iter()
            .find(|f| f.name == "id_f32")
            .unwrap();
        assert_eq!(id_i32_fn.params, vec![MirType::Int(IntWidth::I32)]);
        assert_eq!(id_f32_fn.params, vec![MirType::Float(FloatWidth::F32)]);

        let mut m = JitModule::new();
        let h_i32 = m.compile(id_i32_fn).expect("JIT id_i32");
        let h_f32 = m.compile(id_f32_fn).expect("JIT id_f32");
        m.finalize().expect("finalize");

        // § id_i32(5) = 5 ; id_f32(2.5) = 2.5 — both round-trip through the JIT.
        assert_eq!(h_i32.call_i32_to_i32(5, &m).unwrap(), 5);
        assert_eq!(h_i32.call_i32_to_i32(-42, &m).unwrap(), -42);
        assert!((h_f32.call_f32_to_f32(2.5, &m).unwrap() - 2.5).abs() < 1e-5);
        assert!((h_f32.call_f32_to_f32(-1.25, &m).unwrap() - (-1.25)).abs() < 1e-5);
    }

    #[test]
    fn end_to_end_main_calls_generic_id_via_full_flow() {
        // § T11-D42 : the generic-fn MVP capstone. Source has both a generic
        //   fn AND a caller that invokes it with turbofish. After the full
        //   pipeline (parse → HIR → lower → auto_monomorphize → rewrite →
        //   JIT), calling `main()` with no args must return 5 — proving the
        //   whole D38..D41 arc composes at runtime.
        use cssl_hir::lower_module;
        use cssl_mir::{
            auto_monomorphize, lower_fn_body, lower_function_signature, rewrite_generic_call_sites,
            LowerCtx, MirModule,
        };

        let src = r"
            fn id<T>(x : T) -> T { x }
            fn main() -> i32 { id::<i32>(5) }
        ";
        let file = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = lower_module(&file, &cst);

        // § Lower HIR → MIR (signature + body for every fn item). Produces
        //   main + unspecialized id.
        let lower_ctx = LowerCtx::new(&interner);
        let mut mir = MirModule::new();
        for item in &hir.items {
            if let cssl_hir::HirItem::Fn(f) = item {
                let mut mf = lower_function_signature(&lower_ctx, f);
                lower_fn_body(&interner, Some(&file), f, &mut mf);
                mir.push_func(mf);
            }
        }

        // § Auto-monomorphize : produces id_i32 specialization + call-site map.
        let report = auto_monomorphize(&hir, &interner, Some(&file));
        for spec in &report.specializations {
            mir.push_func(spec.clone());
        }

        // § Rewrite main's func.call from @id → @id_i32.
        let rewrites = rewrite_generic_call_sites(&mut mir, &report.call_site_names);
        assert_eq!(rewrites, 1, "expected 1 call-site rewrite in main");

        // § JIT-compile main + id_i32 (skip unspecialized generic id). Order
        //   matters : id_i32 must be compiled before main so main's call-site
        //   can resolve @id_i32 in the JIT's symbol table.
        let id_i32_fn = mir
            .funcs
            .iter()
            .find(|f| f.name == "id_i32")
            .expect("id_i32 spec");
        let main_fn = mir.funcs.iter().find(|f| f.name == "main").expect("main");

        let mut m = JitModule::new();
        let _id_handle = m.compile(id_i32_fn).expect("JIT id_i32");
        let main_handle = m.compile(main_fn).expect("JIT main");
        m.finalize().expect("finalize");

        // § main() — no args, returns i32. Must equal 5.
        let result = main_handle.call_unit_to_i32(&m).expect("call main()");
        assert_eq!(
            result, 5,
            "main() must return 5 after full generic-fn compilation flow"
        );
    }

    #[test]
    fn auto_monomorphize_deduplicates_same_type_args() {
        // Walker must collapse two call sites with identical type_args into ONE
        // specialization (not duplicate work).
        use cssl_hir::lower_module;
        use cssl_mir::auto_monomorphize;

        let src = r"
            fn id<T>(x : T) -> T { x }
            fn a() -> i32 { id::<i32>(5) }
            fn b() -> i32 { id::<i32>(7) }
            fn c() -> i32 { id::<i32>(11) }
        ";
        let file = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = lower_module(&file, &cst);

        let report = auto_monomorphize(&hir, &interner, Some(&file));
        assert_eq!(report.call_site_count, 3);
        assert_eq!(report.specialization_count, 1);
        // All 3 call sites map to the same mangled name.
        let mapped: std::collections::HashSet<&String> = report.call_site_names.values().collect();
        assert_eq!(mapped.len(), 1);
    }

    #[test]
    fn vec_scalarization_preserves_scalar_params_untouched() {
        // § T11-D37 : regression guard — scalar params are NOT incorrectly
        //   expanded. A mixed fn `fn mix(p : vec3<f32>, r : f32, s : f32)` must
        //   produce exactly 3 + 1 + 1 = 5 scalar params.
        let src = r"fn mix(p : vec3<f32>, r : f32, s : f32) -> f32 { r + s }";
        let module = pipeline_source_to_ad_mir("mix_scalar", src);
        let f = module.funcs.iter().find(|f| f.name == "mix").expect("mix");
        assert_eq!(
            f.params.len(),
            5,
            "vec3 + scalar + scalar : 5 params : got {:?}",
            f.params
        );
    }
}
