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

use crate::call_dispatch::CalleeVariantTable;
use crate::decl::collect_differentiable_fns;
use crate::rules::{DiffRuleTable, Primitive};
use crate::substitute::{apply_bwd_with_callees, apply_fwd_with_callees, SubstitutionReport};

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
/// (resolved from the HIR side) + the call-dispatch table that maps each
/// differentiable callee to its `_fwd` / `_bwd` variants (T11-D140).
#[derive(Debug)]
pub struct AdWalker {
    pub rules: DiffRuleTable,
    pub diff_fn_names: HashSet<String>,
    /// T11-D140 : callee → (fwd, bwd) variant lookup-table. Auto-built from
    /// `diff_fn_names` ; consumed by [`crate::substitute::apply_fwd_with_callees`]
    /// + [`crate::substitute::apply_bwd_with_callees`] to rewrite `func.call`
    /// at the substitution stage.
    pub callee_table: CalleeVariantTable,
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
        let callee_table = CalleeVariantTable::from_diff_fn_names(diff_fn_names.iter().cloned());
        Self {
            rules: DiffRuleTable::canonical(),
            diff_fn_names,
            callee_table,
        }
    }

    /// Explicit-name-set constructor — useful for tests or hand-wired driving.
    #[must_use]
    pub fn with_names(names: impl IntoIterator<Item = String>) -> Self {
        let diff_fn_names: HashSet<String> = names.into_iter().collect();
        let callee_table = CalleeVariantTable::from_diff_fn_names(diff_fn_names.iter().cloned());
        Self {
            rules: DiffRuleTable::canonical(),
            diff_fn_names,
            callee_table,
        }
    }

    /// Re-build the callee-table from the current `diff_fn_names`. Useful
    /// after manual mutation of the names set ; the constructors do this
    /// automatically.
    pub fn rebuild_callee_table(&mut self) {
        self.callee_table =
            CalleeVariantTable::from_diff_fn_names(self.diff_fn_names.iter().cloned());
    }

    /// Transform a MIR module : for every fn whose name is in `diff_fn_names`,
    /// emit `<name>_fwd` + `<name>_bwd` variants appended to `module.funcs`.
    ///
    /// The variants are built by [`crate::substitute::apply_fwd_with_callees`]
    /// and [`crate::substitute::apply_bwd_with_callees`] respectively — they
    /// carry real tangent / adjoint MIR ops in addition to the preserved
    /// primal ops, and `func.call` to a `@differentiable` callee is auto-
    /// dispatched to the matching `_fwd` / `_bwd` variant.
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
            let (fwd, _, fwd_sub) =
                apply_fwd_with_callees(&primal, &self.rules, &self.callee_table);
            let (bwd, _, bwd_sub) =
                apply_bwd_with_callees(&primal, &self.rules, &self.callee_table);
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

    // ─────────────────────────────────────────────────────────────────────
    // § T11-D140 : Call-op auto-dispatch (DiffPG) + control-flow tape-record
    // ─────────────────────────────────────────────────────────────────────

    use cssl_mir::{FloatWidth, MirOp, MirRegion, MirType, ValueId};

    fn f32_ty() -> MirType {
        MirType::Float(FloatWidth::F32)
    }

    /// T11-D140 : the walker auto-builds a callee-variant table from the
    /// `@differentiable` fn names so cross-fn call AD works without manual
    /// table construction.
    #[test]
    fn walker_auto_builds_callee_table_from_diff_fn_names() {
        let walker = AdWalker::with_names(["g".into(), "h".into()]);
        // Both differentiable fns are in the callee table with canonical
        // `_fwd` / `_bwd` variant names.
        assert!(walker.callee_table.contains("g"));
        assert!(walker.callee_table.contains("h"));
        assert_eq!(walker.callee_table.lookup("g").unwrap().fwd, "g_fwd");
        assert_eq!(walker.callee_table.lookup("h").unwrap().bwd, "h_bwd");
    }

    /// T11-D140 : `from_hir` populates the callee table from the HIR module
    /// just like it populates `diff_fn_names`.
    #[test]
    fn from_hir_populates_callee_table() {
        let src = r"
            @differentiable fn g(x : f32) -> f32 { x }
            @differentiable fn h(y : f32) -> f32 { y }
        ";
        let (hir, interner) = hir_from(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        assert_eq!(walker.callee_table.len(), 2);
        assert!(walker.callee_table.contains("g"));
        assert!(walker.callee_table.contains("h"));
    }

    /// T11-D140 : `rebuild_callee_table` re-derives the table after manual
    /// mutation of `diff_fn_names`.
    #[test]
    fn rebuild_callee_table_picks_up_mutations() {
        let mut walker = AdWalker::with_names(["a".into()]);
        walker.diff_fn_names.insert("b".into());
        walker.rebuild_callee_table();
        assert!(walker.callee_table.contains("b"));
        assert_eq!(walker.callee_table.len(), 2);
    }

    /// Build a primal MIR fn whose body is `func.call <callee>` so we can
    /// exercise the call-dispatch path end-to-end via the walker.
    fn mk_primal_with_call(name: &str, callee: &str) -> cssl_mir::MirFunc {
        let mut f = cssl_mir::MirFunc::new(name, vec![f32_ty()], vec![f32_ty()]);
        let call = MirOp::std("func.call")
            .with_operand(ValueId(0))
            .with_result(ValueId(1), f32_ty())
            .with_attribute("callee", callee);
        f.push_op(call);
        f.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        f.next_value_id = 2;
        f
    }

    /// T11-D140 / Call-fwd-bwd-roundtrip : a primal that calls another
    /// `@differentiable` fn must produce a `_fwd` variant whose body issues
    /// `func.call <callee>_fwd` (NOT a placeholder).
    #[test]
    fn call_dispatch_fwd_emits_real_call_to_fwd_variant() {
        // Two fns : `g` is the inner @differentiable callee ; `outer` calls g.
        let g = MirFunc::new("g", vec![f32_ty()], vec![f32_ty()]);
        let outer = mk_primal_with_call("outer", "g");
        let mut module = MirModule::new();
        module.push_func(g);
        module.push_func(outer);
        let walker = AdWalker::with_names(["g".into(), "outer".into()]);
        walker.transform_module(&mut module);
        let outer_fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "outer_fwd")
            .expect("outer_fwd present");
        // Look for the dispatched func.call to g_fwd.
        let dispatched = outer_fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| {
                o.name == "func.call"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "callee" && v == "g_fwd")
            })
            .expect("expected dispatched func.call to g_fwd");
        // The dispatched call must carry the diff_role=tangent + primal_callee
        // attributes.
        assert!(dispatched
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "tangent"));
        assert!(dispatched
            .attributes
            .iter()
            .any(|(k, v)| k == "primal_callee" && v == "g"));
    }

    /// T11-D140 / Call-fwd-bwd-roundtrip : the `_bwd` variant of a fn that
    /// calls another `@differentiable` fn must dispatch to `<callee>_bwd`.
    #[test]
    fn call_dispatch_bwd_emits_real_call_to_bwd_variant() {
        let g = MirFunc::new("g", vec![f32_ty()], vec![f32_ty()]);
        let outer = mk_primal_with_call("outer", "g");
        let mut module = MirModule::new();
        module.push_func(g);
        module.push_func(outer);
        let walker = AdWalker::with_names(["g".into(), "outer".into()]);
        walker.transform_module(&mut module);
        let outer_bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "outer_bwd")
            .expect("outer_bwd present");
        let dispatched = outer_bwd.body.entry().unwrap().ops.iter().find(|o| {
            o.name == "func.call"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "g_bwd")
        });
        assert!(
            dispatched.is_some(),
            "expected dispatched func.call to g_bwd in outer_bwd"
        );
    }

    /// T11-D140 / non-differentiable callee falls back to placeholder (no
    /// dispatch) — keeps gradient-drop detection clean for AD-of-arbitrary-
    /// callee.
    #[test]
    fn call_dispatch_non_diff_callee_falls_back_to_placeholder() {
        let outer = mk_primal_with_call("outer", "external_lib_fn");
        let mut module = MirModule::new();
        module.push_func(outer);
        // Only `outer` is in the table ; `external_lib_fn` is unknown.
        let walker = AdWalker::with_names(["outer".into()]);
        walker.transform_module(&mut module);
        let outer_fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "outer_fwd")
            .expect("outer_fwd present");
        // No func.call to `external_lib_fn_fwd` should appear (not registered).
        let any_dispatched_external = outer_fwd.body.entry().unwrap().ops.iter().any(|o| {
            o.name == "func.call"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "external_lib_fn_fwd")
        });
        assert!(
            !any_dispatched_external,
            "non-diff callee must not get _fwd dispatch"
        );
        // The placeholder cssl.diff.fwd_placeholder must appear instead.
        let has_placeholder = outer_fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .any(|o| o.name == "cssl.diff.fwd_placeholder");
        assert!(
            has_placeholder,
            "expected cssl.diff.fwd_placeholder for non-diff callee"
        );
    }

    /// Build a primal MIR fn whose body is a 3-level nesting of calls : an
    /// outer fn that calls `mid`, which calls `inner` — exercises the
    /// transitive callee-variant resolution required for procedural-graph
    /// AD (DiffPG).
    fn build_diffpg_3_level_chain() -> MirModule {
        let mut module = MirModule::new();
        // Innermost : `inner(x) = x` (signature-only is enough — the walker
        // doesn't need a real body for the dispatch test).
        module.push_func(MirFunc::new("inner", vec![f32_ty()], vec![f32_ty()]));
        // Mid : `mid(x) = inner(x)`.
        module.push_func(mk_primal_with_call("mid", "inner"));
        // Outer : `outer(x) = mid(x)`.
        module.push_func(mk_primal_with_call("outer", "mid"));
        module
    }

    /// T11-D140 / DiffPG-procedural-graph-gradient (3-level-nested-fn) : the
    /// walker must produce dispatched fwd/bwd variants for each level + each
    /// dispatch must point at the right callee variant.
    #[test]
    fn diffpg_3_level_nested_fn_chain_dispatches_each_level() {
        let mut module = build_diffpg_3_level_chain();
        let walker = AdWalker::with_names(["inner".into(), "mid".into(), "outer".into()]);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 3, "{}", r.summary());
        assert_eq!(r.variants_emitted, 6, "{}", r.summary());
        // Outer_fwd dispatches to mid_fwd ; mid_fwd dispatches to inner_fwd.
        let outer_fwd = module.funcs.iter().find(|f| f.name == "outer_fwd").unwrap();
        let mid_fwd = module.funcs.iter().find(|f| f.name == "mid_fwd").unwrap();
        let outer_calls_mid_fwd = outer_fwd.body.entry().unwrap().ops.iter().any(|o| {
            o.name == "func.call"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "mid_fwd")
        });
        let mid_calls_inner_fwd = mid_fwd.body.entry().unwrap().ops.iter().any(|o| {
            o.name == "func.call"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "inner_fwd")
        });
        assert!(outer_calls_mid_fwd, "outer_fwd must call mid_fwd");
        assert!(mid_calls_inner_fwd, "mid_fwd must call inner_fwd");
    }

    /// T11-D140 / DiffPG-procedural-graph-gradient (3-level-nested-fn, bwd) :
    /// transitive bwd dispatch — outer_bwd → mid_bwd → inner_bwd.
    #[test]
    fn diffpg_3_level_nested_fn_chain_bwd_dispatches_each_level() {
        let mut module = build_diffpg_3_level_chain();
        let walker = AdWalker::with_names(["inner".into(), "mid".into(), "outer".into()]);
        walker.transform_module(&mut module);
        let outer_bwd = module.funcs.iter().find(|f| f.name == "outer_bwd").unwrap();
        let mid_bwd = module.funcs.iter().find(|f| f.name == "mid_bwd").unwrap();
        let outer_calls_mid_bwd = outer_bwd.body.entry().unwrap().ops.iter().any(|o| {
            o.name == "func.call"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "mid_bwd")
        });
        let mid_calls_inner_bwd = mid_bwd.body.entry().unwrap().ops.iter().any(|o| {
            o.name == "func.call"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "inner_bwd")
        });
        assert!(outer_calls_mid_bwd, "outer_bwd must call mid_bwd");
        assert!(mid_calls_inner_bwd, "mid_bwd must call inner_bwd");
    }

    /// T11-D140 / DiffPG-procedural-graph-gradient (5-level-nested-fn) :
    /// extends the 3-level chain to 5 levels — proves the scheme scales
    /// without compounded O(N²) shape blowup.
    #[test]
    fn diffpg_5_level_nested_fn_chain_dispatches_every_level() {
        let mut module = MirModule::new();
        // Build a 5-level chain : f0 → f1 → f2 → f3 → f4 (leaf).
        for i in 0..5 {
            let name = format!("f{i}");
            if i == 4 {
                module.push_func(MirFunc::new(&name, vec![f32_ty()], vec![f32_ty()]));
            } else {
                let next = format!("f{}", i + 1);
                module.push_func(mk_primal_with_call(&name, &next));
            }
        }
        let names: Vec<String> = (0..5).map(|i| format!("f{i}")).collect();
        let walker = AdWalker::with_names(names);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 5, "{}", r.summary());
        // Each non-leaf level must dispatch to its successor's _fwd.
        for i in 0..4 {
            let fname = format!("f{i}_fwd");
            let next = format!("f{}_fwd", i + 1);
            let f = module.funcs.iter().find(|f| f.name == fname).unwrap();
            let dispatches = f.body.entry().unwrap().ops.iter().any(|o| {
                o.name == "func.call"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "callee" && v == &next)
            });
            assert!(dispatches, "{fname} must dispatch to {next}");
        }
    }

    /// Build an scf.if op with two empty-body regions ; the AD pass must
    /// produce a tape-record attribute on the fwd-variant + a tape-replay
    /// attribute on the bwd-variant.
    fn build_scf_if_primal(name: &str) -> MirFunc {
        let mut f = MirFunc::new(name, vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        let then_region = MirRegion::with_entry(vec![]);
        let else_region = MirRegion::with_entry(vec![]);
        // scf.yield each branch with the existing primal arg as the yielded
        // value (need an scf.yield to keep structured-CFG terminator invariant).
        let mut then_r = then_region;
        let mut else_r = else_region;
        if let Some(b) = then_r.entry_mut() {
            b.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
        }
        if let Some(b) = else_r.entry_mut() {
            b.push(MirOp::std("scf.yield").with_operand(ValueId(1)));
        }
        let mut if_op = MirOp::std("scf.if").with_attribute("predicate", "ole");
        if_op.regions.push(then_r);
        if_op.regions.push(else_r);
        if_op = if_op.with_result(ValueId(2), f32_ty());
        f.push_op(if_op);
        f.push_op(MirOp::std("func.return").with_operand(ValueId(2)));
        f.next_value_id = 3;
        f
    }

    /// T11-D140 / scf.if-bwd-correctness : the bwd variant must carry a
    /// `tape_replay` attribute on the scf.if op so the runtime can dispatch
    /// the recorded arm.
    #[test]
    fn scf_if_bwd_marks_tape_replay() {
        let primal = build_scf_if_primal("branchy");
        let mut module = MirModule::new();
        module.push_func(primal);
        let walker = AdWalker::with_names(["branchy".into()]);
        walker.transform_module(&mut module);
        let bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "branchy_bwd")
            .unwrap();
        let if_op = bwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.if")
            .expect("scf.if preserved in bwd");
        assert!(if_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "tape_replay"));
        assert!(if_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_branch_kind" && v == "if"));
    }

    /// T11-D140 / scf.if : the fwd variant must carry a `tape_record`
    /// attribute on the scf.if op so the runtime records the arm taken on
    /// the forward pass.
    #[test]
    fn scf_if_fwd_marks_tape_record() {
        let primal = build_scf_if_primal("branchy");
        let mut module = MirModule::new();
        module.push_func(primal);
        let walker = AdWalker::with_names(["branchy".into()]);
        walker.transform_module(&mut module);
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "branchy_fwd")
            .unwrap();
        let if_op = fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.if")
            .expect("scf.if preserved in fwd");
        assert!(if_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "tape_record"));
        assert!(if_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_branch_kind" && v == "if"));
    }

    /// Build an scf.for op with an empty-body region.
    fn build_scf_for_primal(name: &str) -> MirFunc {
        let mut f = MirFunc::new(name, vec![f32_ty()], vec![f32_ty()]);
        let body = MirRegion::with_entry(vec![]);
        let mut for_op = MirOp::std("scf.for");
        for_op.regions.push(body);
        f.push_op(for_op);
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        f.next_value_id = 1;
        f
    }

    /// T11-D140 / scf.for-bwd-correctness : the bwd variant must carry a
    /// `tape_replay` attribute on the scf.for op — the iter-count is
    /// recorded on the forward pass and the bwd replays in reverse.
    #[test]
    fn scf_for_bwd_marks_tape_replay() {
        let primal = build_scf_for_primal("loopy");
        let mut module = MirModule::new();
        module.push_func(primal);
        let walker = AdWalker::with_names(["loopy".into()]);
        walker.transform_module(&mut module);
        let bwd = module.funcs.iter().find(|f| f.name == "loopy_bwd").unwrap();
        let for_op = bwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.for")
            .expect("scf.for preserved in bwd");
        assert!(for_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "tape_replay"));
        assert!(for_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_branch_kind" && v == "for"));
    }

    /// T11-D140 / scf.for-bwd-correctness (fwd side) : the fwd variant marks
    /// the scf.for op `tape_record` so the runtime records iter-count.
    #[test]
    fn scf_for_fwd_marks_tape_record() {
        let primal = build_scf_for_primal("loopy");
        let mut module = MirModule::new();
        module.push_func(primal);
        let walker = AdWalker::with_names(["loopy".into()]);
        walker.transform_module(&mut module);
        let fwd = module.funcs.iter().find(|f| f.name == "loopy_fwd").unwrap();
        let for_op = fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.for")
            .expect("scf.for preserved in fwd");
        assert!(for_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "tape_record"));
        assert!(for_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_branch_kind" && v == "for"));
    }

    fn build_loop_primal(name: &str, op_name: &'static str) -> MirFunc {
        let mut f = MirFunc::new(name, vec![f32_ty()], vec![f32_ty()]);
        let body = MirRegion::with_entry(vec![]);
        let mut op = MirOp::std(op_name);
        op.regions.push(body);
        f.push_op(op);
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        f.next_value_id = 1;
        f
    }

    /// T11-D140 / scf.while-bwd-correctness : tape-replay on the while op.
    #[test]
    fn scf_while_bwd_marks_tape_replay() {
        let primal = build_loop_primal("w", "scf.while");
        let mut module = MirModule::new();
        module.push_func(primal);
        let walker = AdWalker::with_names(["w".into()]);
        walker.transform_module(&mut module);
        let bwd = module.funcs.iter().find(|f| f.name == "w_bwd").unwrap();
        let while_op = bwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.while")
            .expect("scf.while preserved in bwd");
        assert!(while_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "tape_replay"));
        assert!(while_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_branch_kind" && v == "while"));
    }

    /// T11-D140 / scf.loop-bwd : structural-CFG branch-replay on scf.loop.
    #[test]
    fn scf_loop_bwd_marks_tape_replay() {
        let primal = build_loop_primal("l", "scf.loop");
        let mut module = MirModule::new();
        module.push_func(primal);
        let walker = AdWalker::with_names(["l".into()]);
        walker.transform_module(&mut module);
        let bwd = module.funcs.iter().find(|f| f.name == "l_bwd").unwrap();
        let loop_op = bwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.loop")
            .expect("scf.loop preserved in bwd");
        assert!(loop_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "tape_replay"));
        assert!(loop_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_branch_kind" && v == "loop"));
    }

    /// T11-D140 / nested-control-flow : an scf.for around an scf.if must get
    /// tape attributes on BOTH the outer for + the inner if.
    #[test]
    fn nested_for_around_if_emits_tape_attrs_on_both_levels() {
        // Build : fn nested(a, b) -> f32 { for { if a < b { yield a } else { yield b } } }
        let mut f = MirFunc::new("nested", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        // Inner if-op
        let mut inner_if = MirOp::std("scf.if");
        let mut then_r = MirRegion::with_entry(vec![]);
        let mut else_r = MirRegion::with_entry(vec![]);
        if let Some(b) = then_r.entry_mut() {
            b.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
        }
        if let Some(b) = else_r.entry_mut() {
            b.push(MirOp::std("scf.yield").with_operand(ValueId(1)));
        }
        inner_if.regions.push(then_r);
        inner_if.regions.push(else_r);
        inner_if = inner_if.with_result(ValueId(2), f32_ty());
        // Outer for-op containing the if-op
        let mut for_body = MirRegion::with_entry(vec![]);
        if let Some(b) = for_body.entry_mut() {
            b.push(inner_if);
            b.push(MirOp::std("scf.yield").with_operand(ValueId(2)));
        }
        let mut for_op = MirOp::std("scf.for");
        for_op.regions.push(for_body);
        f.push_op(for_op);
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        f.next_value_id = 3;

        let mut module = MirModule::new();
        module.push_func(f);
        let walker = AdWalker::with_names(["nested".into()]);
        walker.transform_module(&mut module);
        let fwd = module
            .funcs
            .iter()
            .find(|f| f.name == "nested_fwd")
            .unwrap();
        // Outer for has tape_record.
        let for_op = fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.for")
            .unwrap();
        assert!(for_op
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "tape_record"));
        // Inner if (in the for's nested region) must also have tape_record.
        let inner_block = for_op.regions[0].entry().unwrap();
        let inner_if = inner_block
            .ops
            .iter()
            .find(|o| o.name == "scf.if")
            .expect("inner scf.if present");
        assert!(inner_if
            .attributes
            .iter()
            .any(|(k, v)| k == "diff_role" && v == "tape_record"));
    }

    /// T11-D140 / recursive-call-tape-overflow : when a fn calls itself, the
    /// callee table contains its own `_fwd` entry — recursion through call-
    /// dispatch is supported until the runtime tape capacity is exhausted
    /// (see [`crate::tape::BranchTape`]).
    #[test]
    fn recursive_call_dispatches_to_self_fwd() {
        // Build : @differentiable fn rec(x : f32) -> f32 { rec(x) }  (yes,
        // structurally-recursive ; the AD pass only cares that the dispatch
        // table is consistent — runtime fuel is enforced at execution).
        let outer = mk_primal_with_call("rec", "rec");
        let mut module = MirModule::new();
        module.push_func(outer);
        let walker = AdWalker::with_names(["rec".into()]);
        walker.transform_module(&mut module);
        let rec_fwd = module.funcs.iter().find(|f| f.name == "rec_fwd").unwrap();
        // The recursive call must dispatch to rec_fwd.
        let dispatched = rec_fwd.body.entry().unwrap().ops.iter().any(|o| {
            o.name == "func.call"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "rec_fwd")
        });
        assert!(dispatched, "recursive call must dispatch to rec_fwd");
    }

    /// T11-D140 / Call-fwd marshals dual-args : the dispatched call must
    /// carry interleaved [a, d_a, b, d_b] operands.
    #[test]
    fn fwd_call_dispatch_marshals_interleaved_dual_args() {
        // Build : @differentiable fn outer(a : f32, b : f32) -> f32 { g(a, b) }
        let mut outer = MirFunc::new("outer", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        let call = MirOp::std("func.call")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), f32_ty())
            .with_attribute("callee", "g");
        outer.push_op(call);
        outer.push_op(MirOp::std("func.return").with_operand(ValueId(2)));
        outer.next_value_id = 3;

        let g = MirFunc::new("g", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        let mut module = MirModule::new();
        module.push_func(g);
        module.push_func(outer);
        let walker = AdWalker::with_names(["g".into(), "outer".into()]);
        walker.transform_module(&mut module);
        let outer_fwd = module.funcs.iter().find(|f| f.name == "outer_fwd").unwrap();
        let dispatched = outer_fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| {
                o.name == "func.call"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "callee" && v == "g_fwd")
            })
            .expect("dispatched g_fwd call");
        // Operand count must be 2 × original-operands (interleaved primal/tangent).
        assert_eq!(
            dispatched.operands.len(),
            4,
            "expected 4 operands [a, d_a, b, d_b], got {:?}",
            dispatched.operands
        );
        // First operand is the primal a (id=0).
        assert_eq!(dispatched.operands[0], ValueId(0));
        // Third operand is the primal b (id=1).
        assert_eq!(dispatched.operands[2], ValueId(1));
    }

    /// T11-D140 / Call-bwd marshals d_y as last operand : the dispatched call
    /// must carry [a, b, d_y] in that order.
    #[test]
    fn bwd_call_dispatch_marshals_d_y_last() {
        let mut outer = MirFunc::new("outer", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        let call = MirOp::std("func.call")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), f32_ty())
            .with_attribute("callee", "g");
        outer.push_op(call);
        outer.push_op(MirOp::std("func.return").with_operand(ValueId(2)));
        outer.next_value_id = 3;

        let g = MirFunc::new("g", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        let mut module = MirModule::new();
        module.push_func(g);
        module.push_func(outer);
        let walker = AdWalker::with_names(["g".into(), "outer".into()]);
        walker.transform_module(&mut module);
        let outer_bwd = module.funcs.iter().find(|f| f.name == "outer_bwd").unwrap();
        let dispatched = outer_bwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| {
                o.name == "func.call"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "callee" && v == "g_bwd")
            })
            .expect("dispatched g_bwd call");
        // 3 operands : a, b, d_y.
        assert_eq!(dispatched.operands.len(), 3);
        assert_eq!(dispatched.operands[0], ValueId(0));
        assert_eq!(dispatched.operands[1], ValueId(1));
    }

    /// T11-D140 / scf.if regions get walked recursively in fwd : an arith op
    /// inside a then-region must produce a tangent op (same as outer-block).
    #[test]
    fn scf_if_fwd_recurses_into_branch_with_real_tangent_ops() {
        // fn br(a, b) -> f32 { if cond { a + b } else { a - b } }
        let mut f = MirFunc::new("br", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        let mut then_r = MirRegion::with_entry(vec![]);
        let mut else_r = MirRegion::with_entry(vec![]);
        if let Some(b) = then_r.entry_mut() {
            b.push(
                MirOp::std("arith.addf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(3), f32_ty()),
            );
            b.push(MirOp::std("scf.yield").with_operand(ValueId(3)));
        }
        if let Some(b) = else_r.entry_mut() {
            b.push(
                MirOp::std("arith.subf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(1))
                    .with_result(ValueId(4), f32_ty()),
            );
            b.push(MirOp::std("scf.yield").with_operand(ValueId(4)));
        }
        let mut if_op = MirOp::std("scf.if");
        if_op.regions.push(then_r);
        if_op.regions.push(else_r);
        if_op = if_op.with_result(ValueId(2), f32_ty());
        f.push_op(if_op);
        f.push_op(MirOp::std("func.return").with_operand(ValueId(2)));
        f.next_value_id = 5;

        let mut module = MirModule::new();
        module.push_func(f);
        let walker = AdWalker::with_names(["br".into()]);
        walker.transform_module(&mut module);
        let fwd = module.funcs.iter().find(|f| f.name == "br_fwd").unwrap();
        let if_op = fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.if")
            .unwrap();
        // Then-branch must contain a tangent arith.addf (one for primal, one for tangent).
        let then_block = if_op.regions[0].entry().unwrap();
        let tangent_addf_count = then_block
            .ops
            .iter()
            .filter(|o| {
                o.name == "arith.addf"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_role" && v == "tangent")
            })
            .count();
        assert!(
            tangent_addf_count >= 1,
            "expected ≥ 1 tangent arith.addf in then-branch"
        );
        // Else-branch must contain a tangent arith.subf.
        let else_block = if_op.regions[1].entry().unwrap();
        let tangent_subf_count = else_block
            .ops
            .iter()
            .filter(|o| {
                o.name == "arith.subf"
                    && o.attributes
                        .iter()
                        .any(|(k, v)| k == "diff_role" && v == "tangent")
            })
            .count();
        assert!(
            tangent_subf_count >= 1,
            "expected ≥ 1 tangent arith.subf in else-branch"
        );
    }

    /// T11-D140 / scf.for fwd recurses into the body : a tangent op must be
    /// emitted inside the loop-body when a primitive op is present.
    #[test]
    fn scf_for_fwd_recurses_into_body_with_real_tangent_ops() {
        let mut f = MirFunc::new("loopy", vec![f32_ty()], vec![f32_ty()]);
        let mut body = MirRegion::with_entry(vec![]);
        if let Some(b) = body.entry_mut() {
            b.push(
                MirOp::std("arith.addf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(0))
                    .with_result(ValueId(2), f32_ty()),
            );
        }
        let mut for_op = MirOp::std("scf.for");
        for_op.regions.push(body);
        f.push_op(for_op);
        f.push_op(MirOp::std("func.return").with_operand(ValueId(0)));
        f.next_value_id = 3;

        let mut module = MirModule::new();
        module.push_func(f);
        let walker = AdWalker::with_names(["loopy".into()]);
        walker.transform_module(&mut module);
        let fwd = module.funcs.iter().find(|f| f.name == "loopy_fwd").unwrap();
        let for_op = fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.for")
            .unwrap();
        let body_block = for_op.regions[0].entry().unwrap();
        let tangent_addf = body_block.ops.iter().any(|o| {
            o.name == "arith.addf"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "diff_role" && v == "tangent")
        });
        assert!(tangent_addf, "expected tangent arith.addf in for-body");
    }

    /// T11-D140 / DiffPG demo : a procedural-graph-style fn whose body
    /// composes call + control-flow + arithmetic — the whole dispatch tree
    /// must produce a fwd variant with no placeholders.
    #[test]
    fn diffpg_procedural_graph_demo_no_placeholders_in_fwd() {
        // graph(x) = if x > 0 { x + leaf(x) } else { 0 }
        let leaf = MirFunc::new("leaf", vec![f32_ty()], vec![f32_ty()]);
        let mut graph = MirFunc::new("graph", vec![f32_ty()], vec![f32_ty()]);
        let mut then_r = MirRegion::with_entry(vec![]);
        let mut else_r = MirRegion::with_entry(vec![]);
        if let Some(b) = then_r.entry_mut() {
            b.push(
                MirOp::std("func.call")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(2), f32_ty())
                    .with_attribute("callee", "leaf"),
            );
            b.push(
                MirOp::std("arith.addf")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(3), f32_ty()),
            );
            b.push(MirOp::std("scf.yield").with_operand(ValueId(3)));
        }
        if let Some(b) = else_r.entry_mut() {
            b.push(
                MirOp::std("arith.constant")
                    .with_result(ValueId(4), f32_ty())
                    .with_attribute("value", "0.0"),
            );
            b.push(MirOp::std("scf.yield").with_operand(ValueId(4)));
        }
        let mut if_op = MirOp::std("scf.if");
        if_op.regions.push(then_r);
        if_op.regions.push(else_r);
        if_op = if_op.with_result(ValueId(1), f32_ty());
        graph.push_op(if_op);
        graph.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        graph.next_value_id = 5;

        let mut module = MirModule::new();
        module.push_func(leaf);
        module.push_func(graph);
        let walker = AdWalker::with_names(["leaf".into(), "graph".into()]);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 2, "{}", r.summary());
        let graph_fwd = module.funcs.iter().find(|f| f.name == "graph_fwd").unwrap();
        // Dispatched call to leaf_fwd inside the then-branch.
        let if_op = graph_fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "scf.if")
            .unwrap();
        let then_block = if_op.regions[0].entry().unwrap();
        let dispatches_leaf = then_block.ops.iter().any(|o| {
            o.name == "func.call"
                && o.attributes
                    .iter()
                    .any(|(k, v)| k == "callee" && v == "leaf_fwd")
        });
        assert!(dispatches_leaf, "graph_fwd must dispatch leaf_fwd");
        // No fwd_placeholder anywhere.
        let has_placeholder = graph_fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .any(|o| o.name == "cssl.diff.fwd_placeholder");
        assert!(
            !has_placeholder,
            "DiffPG demo must not emit cssl.diff.fwd_placeholder"
        );
    }

    /// T11-D140 / signature shape : a fn whose body calls a `@differentiable`
    /// fn must still get the canonical `[a, d_a, b, d_b]` interleaved fwd-
    /// param synthesis (the call-dispatch consumes those tangent ids).
    #[test]
    fn fwd_variant_signature_carries_interleaved_dual_args() {
        let g = MirFunc::new("g", vec![f32_ty(), f32_ty()], vec![f32_ty()]);
        let outer_src = mk_primal_with_call("outer", "g");
        let mut module = MirModule::new();
        module.push_func(g);
        module.push_func(outer_src);
        let walker = AdWalker::with_names(["g".into(), "outer".into()]);
        walker.transform_module(&mut module);
        let outer_fwd = module.funcs.iter().find(|f| f.name == "outer_fwd").unwrap();
        // outer is `outer(x : f32) -> f32` with one float param ; the fwd
        // variant must have 2 params [a, d_a].
        assert_eq!(outer_fwd.params.len(), 2);
        assert_eq!(outer_fwd.params[0], f32_ty());
        assert_eq!(outer_fwd.params[1], f32_ty());
    }

    /// T11-D140 / Call-fwd-bwd-roundtrip end-to-end : both fwd + bwd variants
    /// emerge for the call-chain shape.
    #[test]
    fn call_dispatch_fwd_and_bwd_both_emitted() {
        let g = MirFunc::new("g", vec![f32_ty()], vec![f32_ty()]);
        let outer = mk_primal_with_call("outer", "g");
        let mut module = MirModule::new();
        module.push_func(g);
        module.push_func(outer);
        let walker = AdWalker::with_names(["g".into(), "outer".into()]);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 2);
        assert_eq!(r.variants_emitted, 4);
        let names: Vec<&str> = module.funcs.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"g_fwd"));
        assert!(names.contains(&"g_bwd"));
        assert!(names.contains(&"outer_fwd"));
        assert!(names.contains(&"outer_bwd"));
    }
}
