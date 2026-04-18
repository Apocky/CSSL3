//! MIR-walking AD rule-application transform.
//!
//! § SPEC : `specs/05_AUTODIFF.csl` § IMPLEMENTATION § source-to-source on MIR.
//!
//! § SCOPE (T7-phase-2b / this commit)
//!   For every `@differentiable` fn in a `MirModule`, emit two NEW `MirFunc`s :
//!   `<name>_fwd` + `<name>_bwd`. Each variant is built by calling
//!   [`crate::substitute::apply_fwd`] / [`crate::substitute::apply_bwd`] — these
//!   walk the primal body and emit **real tangent-carrying / adjoint-accumulation
//!   MIR ops** (phase-2b) rather than recipe-attributes alone (phase-2a). See
//!   [`crate::substitute`] for the per-primitive emission rules.
//!
//!   § OP-NAME → [`Primitive`] MAPPING (unchanged from phase-2a)
//!     arith.addf → FAdd      arith.mulf → FMul    arith.negf → FNeg
//!     arith.subf → FSub      arith.divf → FDiv    arith.remf → Mod (no rule ; skip)
//!     func.call              → Primitive::Call (+ callee-name attr used for
//!                              transcendental detection : sqrt / sin / cos /
//!                              exp / log → Sqrt/Sin/Cos/Exp/Log variants)
//!     scf.if                 → Primitive::If
//!     scf.for / scf.while /
//!      scf.loop / scf.while_loop → Primitive::Loop
//!     memref.load            → Primitive::Load
//!     memref.store           → Primitive::Store
//!
//! § T7-phase-2c DEFERRED
//!   - **Tape-record** (reverse-mode) for control-flow ops : iso-capability-scoped
//!     buffer + replay reversed.
//!   - **`@checkpoint` selective re-computation** (trade memory for FLOPs).
//!   - **GPU-AD tape-location resolution** (device / shared / unified memory).
//!   - **Killer-app gate verification** : `bwd_diff(sphere_sdf)(p).d_p` bit-
//!     exact vs analytic (composes w/ T9-phase-2 SMT).
//!   - Higher-order AD via `Jet<T, N>` (§§ 17).
//!   - Multi-result tangent-tuple emission.

use std::collections::HashSet;

use cssl_hir::{HirModule, Interner};
use cssl_mir::MirModule;

use crate::decl::collect_differentiable_fns;
use crate::rules::{DiffRuleTable, Primitive};
use crate::substitute::{apply_bwd, apply_fwd, SubstitutionReport};

/// Map a MIR op-name to the corresponding AD primitive, if any.
///
/// Returns `None` for ops that are not differentiable primitives (e.g., integer
/// arithmetic, comparisons, logical ops) — these are passed through unchanged.
///
/// § Transcendental detection : `func.call` may carry a `callee` attribute naming
/// `sqrt` / `sin` / `cos` / `exp` / `log` / `log2` / `log10` — the walker
/// inspects this attribute to specialize `Primitive::Call` → the concrete
/// transcendental variant.
#[must_use]
pub fn op_to_primitive(op_name: &str) -> Option<Primitive> {
    match op_name {
        "arith.addf" => Some(Primitive::FAdd),
        "arith.subf" => Some(Primitive::FSub),
        "arith.mulf" => Some(Primitive::FMul),
        "arith.divf" => Some(Primitive::FDiv),
        "arith.negf" => Some(Primitive::FNeg),
        "arith.minimumf" | "arith.minf" => Some(Primitive::Min),
        "arith.maximumf" | "arith.maxf" => Some(Primitive::Max),
        "math.absf" | "math.abs" => Some(Primitive::Abs),
        "math.copysign" => Some(Primitive::Sign), // closest MLIR analog
        "func.call" | "cssl.call_indirect" => Some(Primitive::Call),
        "scf.if" => Some(Primitive::If),
        "scf.for" | "scf.while" | "scf.loop" | "scf.while_loop" => Some(Primitive::Loop),
        "memref.load" => Some(Primitive::Load),
        "memref.store" => Some(Primitive::Store),
        _ => None,
    }
}

/// Specialize `Primitive::Call` to a transcendental primitive if the callee-attribute
/// names one of the known math fns. Returns the input primitive unchanged otherwise.
#[must_use]
pub fn specialize_transcendental(prim: Primitive, callee: Option<&str>) -> Primitive {
    if prim != Primitive::Call {
        return prim;
    }
    match callee.unwrap_or("") {
        "sqrt" => Primitive::Sqrt,
        "sin" => Primitive::Sin,
        "cos" => Primitive::Cos,
        "exp" => Primitive::Exp,
        "log" | "ln" => Primitive::Log,
        "min" | "math.min" | "fmin" => Primitive::Min,
        "max" | "math.max" | "fmax" => Primitive::Max,
        "abs" | "math.abs" | "fabs" => Primitive::Abs,
        "sign" | "math.sign" | "signum" => Primitive::Sign,
        _ => Primitive::Call,
    }
}

/// Per-walker telemetry : what happened during the transform.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdWalkerReport {
    /// Number of @differentiable fns transformed.
    pub fns_transformed: u32,
    /// Number of variant fns emitted (should == 2 × fns_transformed).
    pub variants_emitted: u32,
    /// Number of MirOps recognized as differentiable primitives (across all variants).
    pub ops_matched: u32,
    /// Number of rules successfully applied (counted via [`SubstitutionReport`]).
    pub rules_applied: u32,
    /// Number of MirOps that looked like primitives but had no rule — deferred.
    pub unsupported_ops: u32,
    /// Number of tangent / adjoint ops emitted across all variants.
    pub tangent_ops_emitted: u32,
    /// Number of tangent / adjoint params synthesized on variant signatures.
    pub tangent_params_added: u32,
}

impl AdWalkerReport {
    /// Short diagnostic-summary line.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "AD-walker : {} fns → {} variants / {} ops matched / {} rules applied \
             / {} unsupported / {} tangent-ops emitted / {} tangent-params",
            self.fns_transformed,
            self.variants_emitted,
            self.ops_matched,
            self.rules_applied,
            self.unsupported_ops,
            self.tangent_ops_emitted,
            self.tangent_params_added,
        )
    }

    /// Accumulate a per-variant [`SubstitutionReport`] into this summary.
    fn accumulate(&mut self, sub: &SubstitutionReport) {
        self.ops_matched = self
            .ops_matched
            .saturating_add(sub.primitives_substituted + sub.unsupported_primitives);
        self.rules_applied = self
            .rules_applied
            .saturating_add(sub.primitives_substituted);
        self.unsupported_ops = self
            .unsupported_ops
            .saturating_add(sub.unsupported_primitives);
        self.tangent_ops_emitted = self
            .tangent_ops_emitted
            .saturating_add(sub.tangent_ops_emitted);
        self.tangent_params_added = self
            .tangent_params_added
            .saturating_add(sub.tangent_params_added);
    }
}

/// The AD walker : owns the rules + the set of @differentiable fn names
/// (resolved from the HIR side).
#[derive(Debug)]
pub struct AdWalker {
    pub rules: DiffRuleTable,
    pub diff_fn_names: HashSet<String>,
}

impl AdWalker {
    /// Build a walker from a HIR module — auto-discovers @differentiable fns.
    #[must_use]
    pub fn from_hir(hir_module: &HirModule, interner: &Interner) -> Self {
        let decls = collect_differentiable_fns(hir_module, interner);
        let mut diff_fn_names = HashSet::new();
        for d in decls {
            if !d.no_diff {
                diff_fn_names.insert(interner.resolve(d.name));
            }
        }
        Self {
            rules: DiffRuleTable::canonical(),
            diff_fn_names,
        }
    }

    /// Explicit-name-set constructor — useful for tests or hand-wired driving.
    #[must_use]
    pub fn with_names(names: impl IntoIterator<Item = String>) -> Self {
        Self {
            rules: DiffRuleTable::canonical(),
            diff_fn_names: names.into_iter().collect(),
        }
    }

    /// Transform a MIR module : for every fn whose name is in `diff_fn_names`,
    /// emit `<name>_fwd` + `<name>_bwd` variants appended to `module.funcs`.
    ///
    /// The variants are built by [`crate::substitute::apply_fwd`] and
    /// [`crate::substitute::apply_bwd`] respectively — they carry real tangent
    /// / adjoint MIR ops in addition to the preserved primal ops.
    pub fn transform_module(&self, module: &mut MirModule) -> AdWalkerReport {
        let mut report = AdWalkerReport::default();
        // Collect primal-fn indices upfront so we don't walk our freshly-appended variants.
        let primal_indices: Vec<usize> = module
            .funcs
            .iter()
            .enumerate()
            .filter(|(_, f)| self.diff_fn_names.contains(&f.name))
            .map(|(i, _)| i)
            .collect();

        for idx in primal_indices {
            let primal = module.funcs[idx].clone();
            let (fwd, _, fwd_sub) = apply_fwd(&primal, &self.rules);
            let (bwd, _, bwd_sub) = apply_bwd(&primal, &self.rules);
            report.accumulate(&fwd_sub);
            report.accumulate(&bwd_sub);
            module.funcs.push(fwd);
            module.funcs.push(bwd);
            report.fns_transformed = report.fns_transformed.saturating_add(1);
            report.variants_emitted = report.variants_emitted.saturating_add(2);
        }
        report
    }
}

/// MirPass wrapper — a thin adapter so the walker can be pushed into a
/// `cssl_mir::PassPipeline` as a replacement for the stock `AdTransformPass`.
///
/// Wiring example :
/// ```ignore
/// use cssl_mir::{PassPipeline, StructuredCfgValidator};
/// use cssl_autodiff::walker::{AdWalker, AdWalkerPass};
///
/// let walker = AdWalker::from_hir(&hir_module, &interner);
/// let mut pipeline = PassPipeline::new();
/// pipeline.push(Box::new(AdWalkerPass { walker }));
/// pipeline.push(Box::new(StructuredCfgValidator));
/// let results = pipeline.run_all(&mut mir_module);
/// ```
pub struct AdWalkerPass {
    pub walker: AdWalker,
}

impl core::fmt::Debug for AdWalkerPass {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AdWalkerPass")
            .field("diff_fn_count", &self.walker.diff_fn_names.len())
            .field("rule_count", &self.walker.rules.len())
            .finish()
    }
}

impl cssl_mir::MirPass for AdWalkerPass {
    fn name(&self) -> &'static str {
        "ad-walker"
    }

    fn run(&self, module: &mut MirModule) -> cssl_mir::PassResult {
        let report = self.walker.transform_module(module);
        let diagnostics = vec![cssl_mir::PassDiagnostic::info("AD0100", report.summary())];
        cssl_mir::PassResult {
            name: self.name().to_string(),
            changed: report.variants_emitted > 0,
            diagnostics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        op_to_primitive, specialize_transcendental, AdWalker, AdWalkerPass, AdWalkerReport,
    };
    use crate::rules::Primitive;
    use cssl_ast::{SourceFile, SourceId, Surface};
    use cssl_mir::{lower_fn_body, lower_function_signature, LowerCtx, MirFunc, MirModule};

    fn hir_from(src: &str) -> (cssl_hir::HirModule, cssl_hir::Interner) {
        let f = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&f);
        let (cst, _bag) = cssl_parse::parse(&f, &toks);
        let (hir, interner, _) = cssl_hir::lower_module(&f, &cst);
        (hir, interner)
    }

    fn build_mir(src: &str) -> (MirModule, cssl_hir::HirModule, cssl_hir::Interner) {
        let (hir, interner) = hir_from(src);
        let ctx = LowerCtx::new(&interner);
        let mut mir = MirModule::new();
        for item in &hir.items {
            if let cssl_hir::HirItem::Fn(f) = item {
                let mut mf = lower_function_signature(&ctx, f);
                // Tests exercise the source-less path (`None`) ; literal-value
                // fidelity isn't relevant for AD-walker shape assertions.
                lower_fn_body(&interner, None, f, &mut mf);
                mir.push_func(mf);
            }
        }
        (mir, hir, interner)
    }

    #[test]
    fn op_to_primitive_float_arith() {
        assert_eq!(op_to_primitive("arith.addf"), Some(Primitive::FAdd));
        assert_eq!(op_to_primitive("arith.subf"), Some(Primitive::FSub));
        assert_eq!(op_to_primitive("arith.mulf"), Some(Primitive::FMul));
        assert_eq!(op_to_primitive("arith.divf"), Some(Primitive::FDiv));
        assert_eq!(op_to_primitive("arith.negf"), Some(Primitive::FNeg));
    }

    #[test]
    fn op_to_primitive_ignores_integer_arith() {
        // Integer ops are not differentiable primitives.
        assert_eq!(op_to_primitive("arith.addi"), None);
        assert_eq!(op_to_primitive("arith.subi"), None);
        assert_eq!(op_to_primitive("arith.muli"), None);
        assert_eq!(op_to_primitive("arith.divsi"), None);
    }

    #[test]
    fn op_to_primitive_call_control_memory() {
        assert_eq!(op_to_primitive("func.call"), Some(Primitive::Call));
        assert_eq!(op_to_primitive("scf.if"), Some(Primitive::If));
        assert_eq!(op_to_primitive("scf.for"), Some(Primitive::Loop));
        assert_eq!(op_to_primitive("memref.load"), Some(Primitive::Load));
        assert_eq!(op_to_primitive("memref.store"), Some(Primitive::Store));
    }

    #[test]
    fn specialize_transcendental_variants() {
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("sqrt")),
            Primitive::Sqrt
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("sin")),
            Primitive::Sin
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("ln")),
            Primitive::Log
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("unknown")),
            Primitive::Call
        );
        // Non-Call primitives pass through unchanged.
        assert_eq!(
            specialize_transcendental(Primitive::FAdd, Some("sqrt")),
            Primitive::FAdd
        );
    }

    #[test]
    fn specialize_transcendental_piecewise_primitives() {
        // Min / Max / Abs / Sign call-recognition (T11-D13 + D14).
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("min")),
            Primitive::Min
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("math.min")),
            Primitive::Min
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("fmin")),
            Primitive::Min
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("max")),
            Primitive::Max
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("abs")),
            Primitive::Abs
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("fabs")),
            Primitive::Abs
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("sign")),
            Primitive::Sign
        );
        assert_eq!(
            specialize_transcendental(Primitive::Call, Some("signum")),
            Primitive::Sign
        );
    }

    #[test]
    fn op_to_primitive_maps_arith_min_max_abs() {
        assert_eq!(op_to_primitive("arith.minimumf"), Some(Primitive::Min));
        assert_eq!(op_to_primitive("arith.minf"), Some(Primitive::Min));
        assert_eq!(op_to_primitive("arith.maximumf"), Some(Primitive::Max));
        assert_eq!(op_to_primitive("arith.maxf"), Some(Primitive::Max));
        assert_eq!(op_to_primitive("math.absf"), Some(Primitive::Abs));
        assert_eq!(op_to_primitive("math.abs"), Some(Primitive::Abs));
        assert_eq!(op_to_primitive("math.copysign"), Some(Primitive::Sign));
    }

    #[test]
    fn walker_empty_module_transforms_nothing() {
        let walker = AdWalker::with_names(Vec::<String>::new());
        let mut module = MirModule::new();
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 0);
        assert_eq!(r.variants_emitted, 0);
    }

    #[test]
    fn walker_emits_fwd_and_bwd_variants() {
        let walker = AdWalker::with_names(vec!["sphere_sdf".to_string()]);
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("sphere_sdf", vec![], vec![]));
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1);
        assert_eq!(r.variants_emitted, 2);
        // Primal + 2 variants = 3 fns in module.
        assert_eq!(module.funcs.len(), 3);
        let names: Vec<_> = module.funcs.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"sphere_sdf"));
        assert!(names.contains(&"sphere_sdf_fwd"));
        assert!(names.contains(&"sphere_sdf_bwd"));
    }

    #[test]
    fn walker_emits_real_tangent_ops_for_float_arith() {
        // @differentiable fn that uses fadd (float-arith → FAdd primitive).
        let src = "@differentiable fn add(a : f32, b : f32) -> f32 { a + b }";
        let (mut module, hir, interner) = build_mir(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1);
        assert_eq!(r.variants_emitted, 2);
        assert!(r.rules_applied >= 2, "{}", r.summary());
        assert!(
            r.tangent_ops_emitted >= 2,
            "expected real tangent-op emission : {}",
            r.summary()
        );
        // Inspect the fwd variant : real `arith.addf` tangent-op must exist.
        let fwd = module.funcs.iter().find(|f| f.name == "add_fwd").unwrap();
        let has_tangent_addf = fwd.body.entry().unwrap().ops.iter().any(|o| {
            o.name == "arith.addf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        assert!(
            has_tangent_addf,
            "expected tangent arith.addf in fwd variant"
        );
    }

    #[test]
    fn walker_marks_variant_fns_with_diff_variant_attr() {
        let walker = AdWalker::with_names(vec!["f".to_string()]);
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("f", vec![], vec![]));
        walker.transform_module(&mut module);
        let fwd = module.funcs.iter().find(|f| f.name == "f_fwd").unwrap();
        let bwd = module.funcs.iter().find(|f| f.name == "f_bwd").unwrap();
        let fwd_attr = fwd.attributes.iter().find(|(k, _)| k == "diff_variant");
        let bwd_attr = bwd.attributes.iter().find(|(k, _)| k == "diff_variant");
        assert_eq!(fwd_attr.map(|(_, v)| v.as_str()), Some("fwd"));
        assert_eq!(bwd_attr.map(|(_, v)| v.as_str()), Some("bwd"));
    }

    #[test]
    fn walker_preserves_primal_function() {
        let walker = AdWalker::with_names(vec!["orig".to_string()]);
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("orig", vec![], vec![]));
        walker.transform_module(&mut module);
        // Primal should still be present unchanged (other than not being flagged).
        let primal = module.funcs.iter().find(|f| f.name == "orig").unwrap();
        assert!(primal.attributes.iter().all(|(k, _)| k != "diff_variant"));
    }

    #[test]
    fn walker_skips_non_differentiable_fns() {
        let walker = AdWalker::with_names(vec!["target".to_string()]);
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("other1", vec![], vec![]));
        module.push_func(MirFunc::new("target", vec![], vec![]));
        module.push_func(MirFunc::new("other2", vec![], vec![]));
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1);
        assert_eq!(module.funcs.len(), 5); // 3 originals + 2 variants
    }

    #[test]
    fn report_summary_shape() {
        let r = AdWalkerReport {
            fns_transformed: 1,
            variants_emitted: 2,
            ops_matched: 3,
            rules_applied: 3,
            unsupported_ops: 0,
            tangent_ops_emitted: 4,
            tangent_params_added: 2,
        };
        let s = r.summary();
        assert!(s.contains("1 fns"));
        assert!(s.contains("2 variants"));
        assert!(s.contains("3 ops matched"));
        assert!(s.contains("4 tangent-ops emitted"));
        assert!(s.contains("2 tangent-params"));
    }

    #[test]
    fn from_hir_discovers_differentiable_fns() {
        let src = r"
            fn plain(x : f32) -> f32 { x }
            @differentiable fn sdf(p : f32) -> f32 { p }
            @differentiable @NoDiff fn excluded(x : f32) -> f32 { x }
        ";
        let (hir, interner) = hir_from(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        // sdf is differentiable ; excluded is @NoDiff ; plain has no @differentiable.
        assert!(walker.diff_fn_names.contains("sdf"));
        assert!(!walker.diff_fn_names.contains("excluded"));
        assert!(!walker.diff_fn_names.contains("plain"));
    }

    #[test]
    fn ad_walker_pass_plugs_into_pipeline() {
        use cssl_mir::PassPipeline;
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("sphere_sdf", vec![], vec![]));
        let walker = AdWalker::with_names(vec!["sphere_sdf".to_string()]);
        let pass = AdWalkerPass { walker };
        let mut pipeline = PassPipeline::new();
        pipeline.push(Box::new(pass));
        let results = pipeline.run_all(&mut module);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "ad-walker");
        assert!(results[0].changed);
        assert_eq!(results[0].diagnostics[0].code, "AD0100");
        assert_eq!(module.funcs.len(), 3); // primal + fwd + bwd
    }

    #[test]
    fn ad_walker_pass_debug_shape() {
        let walker = AdWalker::with_names(vec!["x".to_string()]);
        let pass = AdWalkerPass { walker };
        let s = format!("{pass:?}");
        assert!(s.contains("AdWalkerPass"));
        assert!(s.contains("rule_count"));
    }

    #[test]
    fn sphere_sdf_integration_emits_real_tangent_and_adjoint_ops() {
        // The canonical killer-app shape : p - r.
        let src = r"@differentiable fn sphere_sdf(p : f32, r : f32) -> f32 { p - r }";
        let (mut module, hir, interner) = build_mir(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1);
        let names: Vec<_> = module.funcs.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"sphere_sdf_fwd"));
        assert!(names.contains(&"sphere_sdf_bwd"));
        // Fwd variant : tangent arith.subf emitted inline.
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf_fwd")
            .unwrap();
        let has_tangent_subf = fwd.body.entry().unwrap().ops.iter().any(|o| {
            o.name == "arith.subf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        assert!(has_tangent_subf, "{}", r.summary());
        // Bwd variant : cssl.diff.bwd_return terminator.
        let bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf_bwd")
            .unwrap();
        let last = bwd.body.entry().unwrap().ops.last().unwrap();
        assert_eq!(last.name, "cssl.diff.bwd_return");
    }

    #[test]
    fn transcendental_callee_resolution_matches_rules() {
        // Composes op_to_primitive + specialize_transcendental — the two helpers
        // the substitution walker uses internally to classify `func.call` ops.
        assert_eq!(
            specialize_transcendental(op_to_primitive("func.call").unwrap(), Some("sqrt")),
            Primitive::Sqrt
        );
        assert_eq!(
            specialize_transcendental(op_to_primitive("func.call").unwrap(), Some("exp")),
            Primitive::Exp
        );
        assert_eq!(
            specialize_transcendental(op_to_primitive("arith.addf").unwrap(), Some("sqrt")),
            Primitive::FAdd
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D16 : end-to-end integration for scene-SDF min(a, b) AD chain.
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn scene_union_min_integration_emits_branchful_tangent_and_adjoint() {
        // Scene-SDF primitive : union via min(a, b). Stage-0 source uses
        // path-resolved call `min(a, b)` so body_lower emits func.call w/
        // callee="min" ; walker specialize_transcendental → Primitive::Min ;
        // substitute emits cmpf "ole" + select (real branchful tangent body).
        let src = r"@differentiable fn scene(a : f32, b : f32) -> f32 { min(a, b) }";
        let (mut module, hir, interner) = build_mir(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1, "{}", r.summary());
        let names: Vec<_> = module.funcs.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"scene_fwd"));
        assert!(names.contains(&"scene_bwd"));

        // Fwd variant must contain branchful emission (cmpf + select with
        // diff_primitive="min"), NOT the legacy placeholder.
        let fwd = module.funcs.iter().find(|f| f.name == "scene_fwd").unwrap();
        let fwd_ops = &fwd.body.entry().unwrap().ops;

        let has_cmpf_tangent = fwd_ops.iter().any(|o| {
            o.name == "arith.cmpf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "min")
        });
        let has_select_tangent = fwd_ops.iter().any(|o| {
            o.name == "arith.select"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "min")
        });
        let has_no_placeholder = !fwd_ops
            .iter()
            .any(|o| o.name == "cssl.diff.fwd_placeholder");
        assert!(
            has_cmpf_tangent,
            "expected tangent arith.cmpf in scene_fwd : {}",
            r.summary()
        );
        assert!(
            has_select_tangent,
            "expected tangent arith.select in scene_fwd"
        );
        assert!(
            has_no_placeholder,
            "expected NO fwd_placeholder in scene_fwd (T11-D15 upgrade)"
        );

        // Bwd variant terminates with cssl.diff.bwd_return.
        let bwd = module.funcs.iter().find(|f| f.name == "scene_bwd").unwrap();
        let bwd_ops = &bwd.body.entry().unwrap().ops;
        let last = bwd_ops.last().unwrap();
        assert_eq!(last.name, "cssl.diff.bwd_return");

        // Bwd must also contain adjoint cmpf + select for min.
        let has_adjoint_select = bwd_ops.iter().any(|o| {
            o.name == "arith.select"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "adjoint")
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "min")
        });
        assert!(
            has_adjoint_select,
            "expected adjoint arith.select in scene_bwd"
        );
    }

    #[test]
    fn nested_min_emits_two_branchful_tangents() {
        // Multi-level scene : min(min(a, b), c) — two nested primitives,
        // each should dispatch independently to branchful emission.
        let src =
            r"@differentiable fn nested(a : f32, b : f32, c : f32) -> f32 { min(min(a, b), c) }";
        let (mut module, hir, interner) = build_mir(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1, "{}", r.summary());

        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "nested_fwd")
            .unwrap();
        let fwd_ops = &fwd.body.entry().unwrap().ops;

        // Should have TWO tangent-role cmpf ops (one per inner min, one per outer).
        let cmpf_min_count = fwd_ops
            .iter()
            .filter(|o| {
                o.name == "arith.cmpf"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_role" && v == "tangent")
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_primitive" && v == "min")
            })
            .count();
        assert!(
            cmpf_min_count >= 2,
            "expected ≥ 2 tangent cmpf for nested min : got {cmpf_min_count} ; {}",
            r.summary()
        );
    }

    #[test]
    fn abs_integration_emits_branchful_tangent() {
        // abs(a - b) — the AD chain must go through FSub then Abs.
        let src = r"@differentiable fn distance(a : f32, b : f32) -> f32 { abs(a - b) }";
        let (mut module, hir, interner) = build_mir(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1, "{}", r.summary());

        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "distance_fwd")
            .unwrap();
        let fwd_ops = &fwd.body.entry().unwrap().ops;

        // Must contain tangent arith.subf (from FSub rule) AND tangent arith.select (from Abs rule).
        let has_tangent_subf = fwd_ops.iter().any(|o| {
            o.name == "arith.subf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        let has_tangent_select_abs = fwd_ops.iter().any(|o| {
            o.name == "arith.select"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "abs")
        });
        assert!(has_tangent_subf, "expected tangent arith.subf for a-b");
        assert!(
            has_tangent_select_abs,
            "expected tangent arith.select with diff_primitive=abs"
        );
    }

    #[test]
    fn max_integration_emits_branchful_tangent() {
        // max(a, b) — companion gate to min-integration.
        let src = r"@differentiable fn scene(a : f32, b : f32) -> f32 { max(a, b) }";
        let (mut module, hir, interner) = build_mir(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1, "{}", r.summary());

        let fwd = module.funcs.iter().find(|f| f.name == "scene_fwd").unwrap();
        let fwd_ops = &fwd.body.entry().unwrap().ops;

        let has_cmpf_oge = fwd_ops.iter().any(|o| {
            o.name == "arith.cmpf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "predicate" && v == "oge")
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_primitive" && v == "max")
        });
        assert!(
            has_cmpf_oge,
            "expected tangent arith.cmpf predicate=oge for max : {}",
            r.summary()
        );
    }

    #[test]
    fn union_intersect_subtract_chain_emits_three_primitives() {
        // subtract(intersect(a, b), c) = max(max(a, b), -c)
        // At the HIR level, expressed as max(max(a, b), c) for simplicity.
        // Two max primitives should chain.
        let src =
            r"@differentiable fn three(a : f32, b : f32, c : f32) -> f32 { max(max(a, b), c) }";
        let (mut module, hir, interner) = build_mir(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1, "{}", r.summary());

        let fwd = module.funcs.iter().find(|f| f.name == "three_fwd").unwrap();
        let cmpf_max_count = fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .filter(|o| {
                o.name == "arith.cmpf"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_primitive" && v == "max")
            })
            .count();
        assert!(
            cmpf_max_count >= 2,
            "expected ≥ 2 tangent cmpf for nested max : {}",
            r.summary()
        );
    }
}
