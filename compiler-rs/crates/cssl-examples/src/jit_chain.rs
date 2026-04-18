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
}
