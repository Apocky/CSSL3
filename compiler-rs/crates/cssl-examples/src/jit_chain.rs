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

/// Post-process a walker-emitted `<name>_fwd` variant : strip the primal result
/// + its corresponding func.return operand, producing a tangent-only fn that
/// the JIT can directly execute.
///
/// The walker emits fwd variants with signature :
///   `(primal_params ++ tangent_params) -> (primal_result, tangent_result)`
///
/// This utility converts that to :
///   `(primal_params ++ tangent_params) -> tangent_result`
///
/// by keeping only the last result type + the last func.return operand.
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
    use super::{extract_tangent_only_variant, pipeline_source_to_ad_mir};
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
