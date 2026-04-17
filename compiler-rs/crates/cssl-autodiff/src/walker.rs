//! MIR-walking AD rule-application transform.
//!
//! § SPEC : `specs/05_AUTODIFF.csl` § IMPLEMENTATION § source-to-source on MIR.
//!
//! § SCOPE (T7-phase-2a / this commit)
//!   For every `@differentiable` fn in a `MirModule`, emit two NEW `MirFunc`s :
//!   `<name>_fwd` + `<name>_bwd`. Each variant is built by walking the primal's
//!   body + annotating every recognized primitive-op with a `diff_recipe`
//!   attribute derived from [`crate::rules::DiffRuleTable`]. The recipe is
//!   currently a textual symbolic rule (e.g., `"dy = dx_0 + dx_1"`) — **real
//!   op-substitution into dual-valued MIR** is T7-phase-2b.
//!
//!   § OP-NAME → [`Primitive`] MAPPING
//!     arith.addf → FAdd      arith.mulf → FMul    arith.negf → FNeg
//!     arith.subf → FSub      arith.divf → FDiv    arith.remf → Mod (no rule ; skip)
//!     arith.addi / subi / muli / divsi : NOT recognized (integer, not diff)
//!     func.call              → Primitive::Call (+ callee-name attr used for
//!                              transcendental detection : sqrt / sin / cos /
//!                              exp / log → Sqrt/Sin/Cos/Exp/Log variants)
//!     scf.if                 → Primitive::If
//!     scf.for / scf.while /
//!      scf.loop / scf.while_loop → Primitive::Loop
//!     memref.load            → Primitive::Load
//!     memref.store           → Primitive::Store
//!
//! § T7-phase-2b DEFERRED
//!   - **Real dual-substitution** : replace each primitive with its (primal,
//!     tangent) tuple computed via the rules. Current phase emits the recipe-
//!     as-attribute ; phase-2b expands it into actual `arith.addf d_x_0 d_x_1`
//!     ops that propagate tangent values.
//!   - **Tape-record** (reverse-mode) : for `bwd` variants, record forward
//!     intermediates on an iso-capability-scoped tape buffer + replay reversed.
//!   - **`@checkpoint` selective re-computation** (trade memory for FLOPs).
//!   - **GPU-AD tape-location resolution** (device / shared / unified memory).
//!   - **Killer-app gate verification** : `bwd_diff(sphere_sdf)(p).d_p` bit-
//!     exact vs analytic (composes w/ T9-phase-2 SMT).
//!   - Higher-order AD via `Jet<T, N>` (§§ 17).

use std::collections::HashSet;

use cssl_hir::{HirModule, Interner};
use cssl_mir::{MirFunc, MirModule, MirOp, MirRegion};

use crate::decl::collect_differentiable_fns;
use crate::rules::{DiffMode, DiffRuleTable, Primitive};

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
    /// Number of MirOps recognized as differentiable primitives.
    pub ops_matched: u32,
    /// Number of rules successfully applied (should == 2 × ops_matched per matched fn).
    pub rules_applied: u32,
    /// Number of MirOps that looked like primitives but had no rule — deferred.
    pub unsupported_ops: u32,
}

impl AdWalkerReport {
    /// Short diagnostic-summary line.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "AD-walker : {} fns → {} variants / {} ops matched / {} rules applied / {} unsupported",
            self.fns_transformed,
            self.variants_emitted,
            self.ops_matched,
            self.rules_applied,
            self.unsupported_ops,
        )
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
            let fwd = self.clone_with_annotations(&primal, DiffMode::Fwd, &mut report);
            let bwd = self.clone_with_annotations(&primal, DiffMode::Bwd, &mut report);
            module.funcs.push(fwd);
            module.funcs.push(bwd);
            report.fns_transformed = report.fns_transformed.saturating_add(1);
            report.variants_emitted = report.variants_emitted.saturating_add(2);
        }
        report
    }

    /// Clone a primal fn, rename it to `<name>_{mode.suffix()}`, and annotate every
    /// recognized primitive-op in the body with a `diff_recipe` attribute.
    fn clone_with_annotations(
        &self,
        primal: &MirFunc,
        mode: DiffMode,
        report: &mut AdWalkerReport,
    ) -> MirFunc {
        let mut variant = primal.clone();
        variant.name = format!("{}{}", primal.name, mode.suffix());
        variant.attributes.push((
            "diff_variant".to_string(),
            mode_attr_value(mode).to_string(),
        ));
        variant
            .attributes
            .push(("diff_primal_name".to_string(), primal.name.clone()));
        self.annotate_region(&mut variant.body, mode, report);
        variant
    }

    fn annotate_region(&self, region: &mut MirRegion, mode: DiffMode, report: &mut AdWalkerReport) {
        for block in &mut region.blocks {
            for op in &mut block.ops {
                self.annotate_op(op, mode, report);
            }
        }
    }

    fn annotate_op(&self, op: &mut MirOp, mode: DiffMode, report: &mut AdWalkerReport) {
        if let Some(mut prim) = op_to_primitive(&op.name) {
            // Specialize transcendentals via callee attribute if this is a func.call.
            if prim == Primitive::Call {
                let callee = op
                    .attributes
                    .iter()
                    .find(|(k, _)| k == "callee")
                    .map(|(_, v)| v.as_str());
                prim = specialize_transcendental(prim, callee);
            }
            report.ops_matched = report.ops_matched.saturating_add(1);
            if let Some(rule) = self.rules.lookup(prim, mode) {
                op.attributes.push((
                    format!("diff_recipe_{}", mode_attr_value(mode)),
                    rule.recipe.to_string(),
                ));
                op.attributes
                    .push(("diff_primitive".to_string(), prim.name().to_string()));
                report.rules_applied = report.rules_applied.saturating_add(1);
            } else {
                report.unsupported_ops = report.unsupported_ops.saturating_add(1);
            }
        }
        // Recurse into nested regions (e.g., scf.if branches).
        for nested in &mut op.regions {
            self.annotate_region(nested, mode, report);
        }
    }
}

const fn mode_attr_value(mode: DiffMode) -> &'static str {
    match mode {
        DiffMode::Primal => "primal",
        DiffMode::Fwd => "fwd",
        DiffMode::Bwd => "bwd",
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
                lower_fn_body(&interner, f, &mut mf);
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
    fn walker_annotates_float_arith_with_recipe() {
        // @differentiable fn that uses fadd (float-arith → FAdd primitive).
        let src = "@differentiable fn add(a : f32, b : f32) -> f32 { a + b }";
        let (mut module, hir, interner) = build_mir(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1);
        assert_eq!(r.variants_emitted, 2);
        // arith.addf + func.return visited per variant body.
        // At least 1 addf op matched per variant ; 2 variants emitted.
        assert!(r.ops_matched >= 2, "{}", r.summary());
        assert!(r.rules_applied >= 2, "{}", r.summary());
        // Inspect the fwd variant's body : arith.addf should carry diff_recipe_fwd attr.
        let fwd = module.funcs.iter().find(|f| f.name == "add_fwd").unwrap();
        let addf_op = fwd
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == "arith.addf")
            .unwrap();
        let recipe = addf_op
            .attributes
            .iter()
            .find(|(k, _)| k == "diff_recipe_fwd")
            .map(|(_, v)| v.as_str());
        assert!(recipe.is_some(), "expected diff_recipe_fwd on arith.addf");
        assert!(recipe.unwrap().contains("dx_0"));
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
        };
        let s = r.summary();
        assert!(s.contains("1 fns"));
        assert!(s.contains("2 variants"));
        assert!(s.contains("3 ops matched"));
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
    fn sphere_sdf_integration_emits_variants() {
        // The canonical killer-app shape : length(p) - r.
        let src = r"@differentiable fn sphere_sdf(p : f32, r : f32) -> f32 { p - r }";
        let (mut module, hir, interner) = build_mir(src);
        let walker = AdWalker::from_hir(&hir, &interner);
        let r = walker.transform_module(&mut module);
        assert_eq!(r.fns_transformed, 1);
        let names: Vec<_> = module.funcs.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"sphere_sdf_fwd"));
        assert!(names.contains(&"sphere_sdf_bwd"));
        // arith.subf (from `p - r`) should have gotten a diff_recipe_bwd attr in the bwd variant.
        let bwd = module
            .funcs
            .iter()
            .find(|f| f.name == "sphere_sdf_bwd")
            .unwrap();
        let has_subf_with_recipe = bwd.body.entry().unwrap().ops.iter().any(|o| {
            o.name == "arith.subf" && o.attributes.iter().any(|(k, _)| k == "diff_recipe_bwd")
        });
        assert!(has_subf_with_recipe, "{}", r.summary());
    }
}
