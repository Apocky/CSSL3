//! MIR pass-pipeline : sequenced transforms over a `MirModule`.
//!
//! § SPEC : `specs/15_MLIR.csl` § PASS-PIPELINE + `specs/07_CODEGEN.csl` § flow.
//!
//! § SCOPE (T6-phase-2a / this commit)
//!   - [`MirPass`] trait : `name()` + `run(&mut MirModule) -> PassResult`.
//!   - [`PassPipeline`] : ordered `Vec<Box<dyn MirPass>>` with `run_all`.
//!   - [`PassResult`]  : per-pass diagnostics + `changed` flag + name.
//!   - [`PassDiagnostic`] : severity + message + optional pass-internal code.
//!   - Stock passes (mostly stubs pending phase-2b content) :
//!     * [`StructuredCfgValidator`] — **real** : checks every region has ≥ 1 block.
//!     * [`MonomorphizationPass`]   — stub.
//!     * [`AdTransformPass`]        — stub (delegates to `cssl-autodiff` at phase-2b).
//!     * [`IfcLoweringPass`]        — stub (needs T3.4-phase-3 IFC slice first).
//!     * [`SmtDischargeQueuePass`]  — stub (needs T9-phase-2 HIR-to-SMT-Term).
//!     * [`TelemetryProbeInsertPass`] — stub (needs T11-phase-2 effect-lowering).
//!
//! § DESIGN
//!   Passes run in declaration order. Each pass returns a `PassResult` carrying :
//!   * `changed : bool` — did the pass modify the module ?
//!   * `diagnostics : Vec<PassDiagnostic>` — validation / optimization notes.
//!   The pipeline terminates on first pass that returns a `diag.severity ==
//!   PassSeverity::Error` ; warnings are accumulated but do not halt.
//!
//! § T6-phase-2b DEFERRED
//!   - Real `MonomorphizationPass` walking generic call-sites + cloning concrete-types.
//!   - Real `AdTransformPass` integrating `cssl_autodiff::DiffRuleTable` walk.
//!   - Real `IfcLoweringPass` emitting `cssl.ifc.label` / `cssl.ifc.declassify` ops
//!     from HIR label-annotations (gated on T3.4-phase-3 IFC slice).
//!   - Real `SmtDischargeQueuePass` emitting `cssl.verify.assert` ops + queuing
//!     corresponding `RefinementObligation`s into `cssl_smt::Query`s.
//!   - Real `TelemetryProbeInsertPass` scope-gated probe-op emission per
//!     `specs/22` § COMPILE-TIME PROBE INSERTION.
//!   - Pass-ordering constraints + dependency-graph enforcement.
//!   - Per-pass timing + summary reporting.

use crate::block::MirRegion;
use crate::func::MirModule;

/// Severity of a pass-emitted diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PassSeverity {
    /// Informational — does not affect pipeline flow.
    Info,
    /// Warning — logged + accumulated but pipeline continues.
    Warning,
    /// Error — pipeline halts after current pass returns.
    Error,
}

impl PassSeverity {
    /// Short name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// One diagnostic from a MIR-pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassDiagnostic {
    /// Severity.
    pub severity: PassSeverity,
    /// Pass-internal code (e.g., `"CFG0001"` for structured-CFG-validator).
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

impl PassDiagnostic {
    /// Build a new info-level diagnostic.
    #[must_use]
    pub fn info(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: PassSeverity::Info,
            code: code.into(),
            message: message.into(),
        }
    }

    /// Build a new warning-level diagnostic.
    #[must_use]
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: PassSeverity::Warning,
            code: code.into(),
            message: message.into(),
        }
    }

    /// Build a new error-level diagnostic.
    #[must_use]
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: PassSeverity::Error,
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Per-pass execution result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PassResult {
    /// Pass name.
    pub name: String,
    /// Did the pass mutate the module ?
    pub changed: bool,
    /// Diagnostics emitted.
    pub diagnostics: Vec<PassDiagnostic>,
}

impl PassResult {
    /// Return true iff any diagnostic has severity Error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == PassSeverity::Error)
    }

    /// Count diagnostics by severity.
    #[must_use]
    pub fn count_by(&self, sev: PassSeverity) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == sev)
            .count()
    }
}

/// Trait every MIR-pass implements.
pub trait MirPass {
    /// Pass name (stable identifier).
    fn name(&self) -> &'static str;

    /// Run the pass over the module.
    fn run(&self, module: &mut MirModule) -> PassResult;
}

/// Ordered pass-pipeline.
pub struct PassPipeline {
    /// Passes in execution order.
    passes: Vec<Box<dyn MirPass>>,
}

impl core::fmt::Debug for PassPipeline {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let names: Vec<&str> = self.passes.iter().map(|p| p.name()).collect();
        f.debug_struct("PassPipeline")
            .field("passes", &names)
            .finish()
    }
}

impl Default for PassPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl PassPipeline {
    /// New empty pipeline.
    #[must_use]
    pub fn new() -> Self {
        Self { passes: Vec::new() }
    }

    /// Build the canonical stage-0 pipeline with all stock passes in spec-order.
    ///
    /// Order (per `specs/15` § PASS-PIPELINE) :
    ///   1. monomorphization  (clones generic-fn call-sites)
    ///   2. ad-transform      (emits primal/fwd/bwd variants for `@differentiable`)
    ///   3. ifc-lowering      (emits `cssl.ifc.label` + `cssl.ifc.declassify`)
    ///   4. smt-discharge-queue (emits `cssl.verify.assert` + queues obligations)
    ///   5. telemetry-probe-insert (inserts `cssl.telemetry.probe` per-scope)
    ///   6. structured-cfg-validator (final sanity-check ; must-pass)
    #[must_use]
    pub fn canonical() -> Self {
        let mut p = Self::new();
        p.push(Box::new(MonomorphizationPass));
        p.push(Box::new(AdTransformPass));
        p.push(Box::new(IfcLoweringPass));
        p.push(Box::new(SmtDischargeQueuePass));
        p.push(Box::new(TelemetryProbeInsertPass));
        p.push(Box::new(StructuredCfgValidator));
        p
    }

    /// Append a pass.
    pub fn push(&mut self, pass: Box<dyn MirPass>) {
        self.passes.push(pass);
    }

    /// Number of passes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.passes.len()
    }

    /// Empty check.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }

    /// Iterate pass-names in order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.passes.iter().map(|p| p.name())
    }

    /// Run every pass in order. Returns the `PassResult` sequence ; on first
    /// pass returning `has_errors()` the pipeline halts (remaining passes are
    /// not executed).
    #[must_use]
    pub fn run_all(&self, module: &mut MirModule) -> Vec<PassResult> {
        let mut results = Vec::with_capacity(self.passes.len());
        for pass in &self.passes {
            let r = pass.run(module);
            let halt = r.has_errors();
            results.push(r);
            if halt {
                break;
            }
        }
        results
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Stock passes
// ─────────────────────────────────────────────────────────────────────────

/// Real structured-CFG validator : every region must have at-least one block,
/// and every block must terminate (empty bodies are treated as implicitly
/// terminated by `func.return` at stage-0).
#[derive(Debug, Clone, Copy, Default)]
pub struct StructuredCfgValidator;

impl MirPass for StructuredCfgValidator {
    fn name(&self) -> &'static str {
        "structured-cfg-validator"
    }

    fn run(&self, module: &mut MirModule) -> PassResult {
        let mut diagnostics = Vec::new();
        for f in &module.funcs {
            validate_region(&f.body, &f.name, &mut diagnostics);
        }
        PassResult {
            name: self.name().to_string(),
            changed: false,
            diagnostics,
        }
    }
}

fn validate_region(region: &MirRegion, fn_name: &str, out: &mut Vec<PassDiagnostic>) {
    if region.blocks.is_empty() {
        out.push(PassDiagnostic::error(
            "CFG0001",
            format!("fn `{fn_name}` has empty region (no entry block)"),
        ));
        return;
    }
    for block in &region.blocks {
        for op in &block.ops {
            for sub in &op.regions {
                validate_region(sub, fn_name, out);
            }
        }
    }
}

/// Stub monomorphization pass — phase-2b walks generic call-sites.
#[derive(Debug, Clone, Copy, Default)]
pub struct MonomorphizationPass;

impl MirPass for MonomorphizationPass {
    fn name(&self) -> &'static str {
        "monomorphization"
    }

    fn run(&self, _module: &mut MirModule) -> PassResult {
        PassResult {
            name: self.name().to_string(),
            changed: false,
            diagnostics: vec![PassDiagnostic::info(
                "MONO0000",
                "stage-0 stub : no generic call-site cloning yet (T6-phase-2b)",
            )],
        }
    }
}

/// Stub AD-transform pass — phase-2b delegates to `cssl_autodiff::DiffRuleTable`.
#[derive(Debug, Clone, Copy, Default)]
pub struct AdTransformPass;

impl MirPass for AdTransformPass {
    fn name(&self) -> &'static str {
        "ad-transform"
    }

    fn run(&self, _module: &mut MirModule) -> PassResult {
        PassResult {
            name: self.name().to_string(),
            changed: false,
            diagnostics: vec![PassDiagnostic::info(
                "AD0000",
                "stage-0 stub : primal/fwd/bwd variants not yet emitted (T7-phase-2)",
            )],
        }
    }
}

/// Stub IFC-lowering pass — phase-2b emits `cssl.ifc.label` + `cssl.ifc.declassify`.
#[derive(Debug, Clone, Copy, Default)]
pub struct IfcLoweringPass;

impl MirPass for IfcLoweringPass {
    fn name(&self) -> &'static str {
        "ifc-lowering"
    }

    fn run(&self, _module: &mut MirModule) -> PassResult {
        PassResult {
            name: self.name().to_string(),
            changed: false,
            diagnostics: vec![PassDiagnostic::info(
                "IFC0000",
                "stage-0 stub : IFC ops not yet emitted (T3.4-phase-3-IFC + T6-phase-2b)",
            )],
        }
    }
}

/// Stub SMT-discharge-queue pass — phase-2b emits `cssl.verify.assert`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SmtDischargeQueuePass;

impl MirPass for SmtDischargeQueuePass {
    fn name(&self) -> &'static str {
        "smt-discharge-queue"
    }

    fn run(&self, _module: &mut MirModule) -> PassResult {
        PassResult {
            name: self.name().to_string(),
            changed: false,
            diagnostics: vec![PassDiagnostic::info(
                "SMT0000",
                "stage-0 stub : verify.assert ops not yet emitted (T9-phase-2)",
            )],
        }
    }
}

/// Stub telemetry-probe-insertion pass — phase-2b scope-gated probe emission.
#[derive(Debug, Clone, Copy, Default)]
pub struct TelemetryProbeInsertPass;

impl MirPass for TelemetryProbeInsertPass {
    fn name(&self) -> &'static str {
        "telemetry-probe-insert"
    }

    fn run(&self, _module: &mut MirModule) -> PassResult {
        PassResult {
            name: self.name().to_string(),
            changed: false,
            diagnostics: vec![PassDiagnostic::info(
                "TEL0000",
                "stage-0 stub : probe ops not yet emitted (T11-phase-2)",
            )],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AdTransformPass, IfcLoweringPass, MirPass, MonomorphizationPass, PassDiagnostic,
        PassPipeline, PassResult, PassSeverity, SmtDischargeQueuePass, StructuredCfgValidator,
        TelemetryProbeInsertPass,
    };
    use crate::func::{MirFunc, MirModule};

    #[test]
    fn severity_names() {
        assert_eq!(PassSeverity::Info.as_str(), "info");
        assert_eq!(PassSeverity::Warning.as_str(), "warning");
        assert_eq!(PassSeverity::Error.as_str(), "error");
    }

    #[test]
    fn diagnostic_builders_shape() {
        let a = PassDiagnostic::info("X0000", "hello");
        assert_eq!(a.severity, PassSeverity::Info);
        assert_eq!(a.code, "X0000");
        let b = PassDiagnostic::warning("X0001", "warn");
        assert_eq!(b.severity, PassSeverity::Warning);
        let c = PassDiagnostic::error("X0002", "err");
        assert_eq!(c.severity, PassSeverity::Error);
    }

    #[test]
    fn pass_result_has_errors() {
        let mut r = PassResult {
            name: "t".into(),
            changed: false,
            diagnostics: vec![PassDiagnostic::info("A", "a")],
        };
        assert!(!r.has_errors());
        r.diagnostics.push(PassDiagnostic::error("B", "b"));
        assert!(r.has_errors());
    }

    #[test]
    fn pass_result_count_by() {
        let r = PassResult {
            name: "t".into(),
            changed: false,
            diagnostics: vec![
                PassDiagnostic::info("A", "a"),
                PassDiagnostic::info("B", "b"),
                PassDiagnostic::warning("C", "c"),
                PassDiagnostic::error("D", "d"),
            ],
        };
        assert_eq!(r.count_by(PassSeverity::Info), 2);
        assert_eq!(r.count_by(PassSeverity::Warning), 1);
        assert_eq!(r.count_by(PassSeverity::Error), 1);
    }

    #[test]
    fn empty_pipeline() {
        let p = PassPipeline::new();
        assert!(p.is_empty());
        assert_eq!(p.len(), 0);
    }

    #[test]
    fn canonical_pipeline_shape() {
        let p = PassPipeline::canonical();
        let names: Vec<&str> = p.names().collect();
        assert_eq!(names.len(), 6);
        assert!(names.contains(&"monomorphization"));
        assert!(names.contains(&"ad-transform"));
        assert!(names.contains(&"ifc-lowering"));
        assert!(names.contains(&"smt-discharge-queue"));
        assert!(names.contains(&"telemetry-probe-insert"));
        assert!(names.contains(&"structured-cfg-validator"));
    }

    #[test]
    fn canonical_runs_all_on_empty_module() {
        let p = PassPipeline::canonical();
        let mut module = MirModule::new();
        let results = p.run_all(&mut module);
        // All 6 stock passes should execute (no errors on empty module).
        assert_eq!(results.len(), 6);
        // No pass should report `changed` on an empty module.
        for r in &results {
            assert!(
                !r.changed,
                "{} reported changed=true on empty module",
                r.name
            );
        }
    }

    #[test]
    fn stub_passes_emit_info_diagnostic() {
        // Each stub emits exactly one Info diagnostic with stable code.
        let mut module = MirModule::new();
        let mono = MonomorphizationPass.run(&mut module);
        assert_eq!(mono.diagnostics.len(), 1);
        assert_eq!(mono.diagnostics[0].code, "MONO0000");
        let ad = AdTransformPass.run(&mut module);
        assert_eq!(ad.diagnostics[0].code, "AD0000");
        let ifc = IfcLoweringPass.run(&mut module);
        assert_eq!(ifc.diagnostics[0].code, "IFC0000");
        let smt = SmtDischargeQueuePass.run(&mut module);
        assert_eq!(smt.diagnostics[0].code, "SMT0000");
        let tel = TelemetryProbeInsertPass.run(&mut module);
        assert_eq!(tel.diagnostics[0].code, "TEL0000");
    }

    #[test]
    fn structured_cfg_validator_passes_on_well_formed() {
        let mut module = MirModule::new();
        module.push_func(MirFunc::new("noop", vec![], vec![]));
        let r = StructuredCfgValidator.run(&mut module);
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
    }

    #[test]
    fn structured_cfg_validator_flags_empty_region() {
        let mut module = MirModule::new();
        let mut f = MirFunc::new("bad", vec![], vec![]);
        // Deliberately blow away the entry block to simulate a malformed fn.
        f.body.blocks.clear();
        module.push_func(f);
        let r = StructuredCfgValidator.run(&mut module);
        assert!(r.has_errors());
        assert_eq!(r.diagnostics[0].code, "CFG0001");
    }

    #[test]
    fn pipeline_halts_on_error() {
        // Build a pipeline : [StructuredCfgValidator, MonomorphizationPass]
        // with a deliberately-malformed fn. The validator should emit error,
        // the pipeline should halt before MonomorphizationPass runs.
        let mut module = MirModule::new();
        let mut f = MirFunc::new("bad", vec![], vec![]);
        f.body.blocks.clear();
        module.push_func(f);
        let mut p = PassPipeline::new();
        p.push(Box::new(StructuredCfgValidator));
        p.push(Box::new(MonomorphizationPass));
        let results = p.run_all(&mut module);
        // Only 1 result : validator errored, mono did not run.
        assert_eq!(results.len(), 1);
        assert!(results[0].has_errors());
    }

    #[test]
    fn pipeline_debug_shape() {
        let p = PassPipeline::canonical();
        let s = format!("{p:?}");
        assert!(s.contains("PassPipeline"));
        assert!(s.contains("ad-transform"));
    }
}
