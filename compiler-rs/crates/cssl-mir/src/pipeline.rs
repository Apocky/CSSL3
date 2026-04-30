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
//!     * [`StructuredCfgValidator`] — **real, T11-D70** : delegates to
//!       [`crate::structured_cfg::validate_structured_cfg`] for the full D5
//!       contract (rejects orphan scf.yield / cf.cond_br / cf.br + validates
//!       scf.if region count + validates loop region shape + flags
//!       Break/Continue placeholders). One [`PassDiagnostic`] per
//!       [`crate::structured_cfg::CfgViolation`] with the stable diagnostic-code.
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

use crate::func::MirModule;
use crate::structured_cfg::validate_structured_cfg;
use crate::tagged_union_abi::expand_module as expand_tagged_union_module;
use crate::try_op_lower::lower_try_ops_in_module;

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
    /// Order (per `specs/15` § PASS-PIPELINE +
    /// `Omniverse/02_CSSL/00_LANGUAGE_CONTRACT.csl.md` § VI.D required-pass
    /// list) :
    ///   1. monomorphization  (clones generic-fn call-sites)
    ///   2. ad-transform      (emits primal/fwd/bwd variants for `@differentiable`)
    ///   3. ifc-lowering      (emits `cssl.ifc.label` + `cssl.ifc.declassify`)
    ///   4. smt-discharge-queue (emits `cssl.verify.assert` + queues obligations)
    ///   5. telemetry-probe-insert (inserts `cssl.telemetry.probe` per-scope)
    ///   6. **biometric-egress-check** (T11-D132 / W3β-07) : refuses any
    ///      `cssl.telemetry.record` with biometric / surveillance / coercion
    ///      tagged operands per PRIME-DIRECTIVE §1. Wired AFTER
    ///      `IfcLoweringPass` so IFC-attributes are present.
    ///   7. **enforces-sigma-at-cell-touches** (T11-D138 / W3g-01) : closes
    ///      the `EnforcesΣAtCellTouches` row from the LANGUAGE_CONTRACT
    ///      required-pass list. Walks every Ω-field cell-touching op
    ///      (`cssl.fieldcell.{read,write,modify,destroy}` + `cssl.travel.*`
    ///      + `cssl.crystallize.*`) + cross-checks declared `consent_bits`
    ///      vs the kind's `required_bit` + Sovereign-handle / capacity-
    ///      floor / reversibility-scope rules. Wired AFTER
    ///      `BiometricEgressCheck` so the absolute biometric / surveillance
    ///      refusal fires first ; wired BEFORE the structured-CFG validator
    ///      so structural validation is the FINAL gate.
    ///   8. **tagged-union-abi** (W-B-RECOGNIZER / Wave-A1) : expands every
    ///      `cssl.option.{some,none}` / `cssl.result.{ok,err}` op into a
    ///      packed `{tag : u32, payload : [u8; sizeof T]}` cell shape via
    ///      `tagged_union_abi::expand_module`. Wired BEFORE `try-op-lower`
    ///      because the `?`-op rewriter consumes the tagged-union helper
    ///      surface ; wired AFTER all type/effect passes so it sees the
    ///      monomorphized + IFC-attributed op stream.
    ///   9. **try-op-lower** (W-B-RECOGNIZER / Wave-A3) : rewrites every
    ///      `cssl.try` op into a tag-dispatched short-circuit-return on
    ///      the operand's tagged-union shape via
    ///      `try_op_lower::lower_try_ops_in_module`. Wired AFTER the
    ///      tagged-union ABI pass so the cell layout is in place.
    ///   10. **tagged-union-abi (sweep-2)** (T11-D282 / W-A1-ε) : SECOND
    ///       run of the tagged-union ABI pass — required because
    ///       `TryOpLowerPass` emits NEW `cssl.option.none` /
    ///       `cssl.result.err` construct-ops inside the failure-arms of
    ///       its synthesized `scf.if` cascades. Idempotent on the body
    ///       (per `tagged_union_abi.sig_rewritten` stamp) BUT picks up
    ///       the new constructs the try-op rewrite spliced into scf.if
    ///       branch-regions. Without this sweep, cgen-cl encounters raw
    ///       `cssl.result.err` ops inside scf.if-branches and fails with
    ///       `UnsupportedMirOp`.
    ///   11. **effect-row-validator** (T11-D285 / W-E5-2) : closes the W-E4
    ///       fixed-point gate gap 2/5. Walks every `func.call` op + verifies
    ///       caller-row ⊇ callee-row per § 04 sub-effect discipline. Wired
    ///       AFTER `IfcLoweringPass` + BEFORE `BiometricEgressCheck`.
    ///   12. structured-cfg-validator (final sanity-check ; must-pass)
    #[must_use]
    pub fn canonical() -> Self {
        let mut p = Self::new();
        p.push(Box::new(MonomorphizationPass));
        p.push(Box::new(AdTransformPass));
        p.push(Box::new(IfcLoweringPass));
        p.push(Box::new(SmtDischargeQueuePass));
        p.push(Box::new(TelemetryProbeInsertPass));
        // § T11-D285 (W-E5-2) — effect-row validator. Wired BEFORE the
        // hard-no biometric refusal (whose existence is enforced at runtime
        // regardless) so violations surface cleanly during the type-effect
        // pass-block, not interleaved with structural-CFG checks.
        p.push(Box::new(crate::effect_row_check::EffectRowValidatorPass));
        p.push(Box::new(
            crate::biometric_egress_check::BiometricEgressCheck,
        ));
        p.push(Box::new(crate::sigma_enforce::EnforcesSigmaAtCellTouches));
        // § W-B-RECOGNIZER (Wave-A1 + Wave-A3) — wired AFTER all type/
        //   effect passes + BEFORE the final structured-CFG validator. The
        //   tagged-union ABI pass MUST run before the try-op rewriter
        //   because the latter expects the cell-layout the former stamps.
        p.push(Box::new(TaggedUnionAbiPass));
        // § W-A8 (T11-D245 / Wave-C1 carry-forward) — `cssl.string.*`
        //   structural-audit pass. Wired AFTER `TaggedUnionAbiPass` (so
        //   any `cssl.option.*` ops embedded in `cssl.char.from_u32`
        //   lowerings see the canonical tagged-union shape) + BEFORE
        //   `TryOpLowerPass` (so the `?`-op rewriter sees the lowered
        //   `cssl.string.from_utf8` Result-cell op). Audits the body-
        //   lower string-recognizer output by counting + summary-
        //   reporting Wave-C1 ops ; the actual cgen path lives in
        //   `cssl-cgen-cpu-cranelift::cgen_string`.
        p.push(Box::new(StringAbiPass));
        p.push(Box::new(TryOpLowerPass));
        // § T11-D282 (W-A1-ε) — sweep-2 of the tagged-union ABI pass.
        //   `TryOpLowerPass` synthesizes `cssl.option.none` /
        //   `cssl.result.err` construct-ops INSIDE the failure-arm
        //   regions of the scf.if-cascades it emits. Those construct-ops
        //   are NOT processed by the first `TaggedUnionAbiPass` (which
        //   ran BEFORE the try-op rewrite). Re-running the pass after
        //   try-op-lower finds + expands them into the canonical
        //   `heap.alloc + tag-store + payload-store` shape, with the
        //   construct-op's arg-path-references resolving in the
        //   spliced-region context (the `expand_region` walker recurses
        //   into scf.if regions naturally). The pass is idempotent so
        //   the body's already-expanded ops are untouched on this second
        //   sweep.
        p.push(Box::new(TaggedUnionAbiPass));
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

/// Real structured-CFG validator (T11-D70 / S6-D5) : delegates to
/// [`crate::structured_cfg::validate_structured_cfg`] for the full D5
/// contract. Each [`crate::structured_cfg::CfgViolation`] becomes one
/// [`PassDiagnostic`] carrying the stable diagnostic-code (CFG0001..CFG0010)
/// and an actionable message. The pre-D5 stub only checked `CFG0001` (empty
/// region) ; the D5 expansion adds CFG0002..CFG0010 covering orphan
/// terminators, unstructured CFG ops, malformed scf.* shapes, and
/// unsupported Break/Continue placeholders.
///
/// This impl mutates the module ONLY in the success-marker case : when
/// `validate_structured_cfg` returns `Ok(())`, the validator writes the
/// `("structured_cfg.validated", "true")` attribute to `module.attributes`
/// so downstream GPU emitters D1..D4 can short-circuit-check whether D5
/// passed. On any violation, no marker is written and the diagnostics
/// surface through `PassResult.diagnostics`.
#[derive(Debug, Clone, Copy, Default)]
pub struct StructuredCfgValidator;

impl MirPass for StructuredCfgValidator {
    fn name(&self) -> &'static str {
        "structured-cfg-validator"
    }

    fn run(&self, module: &mut MirModule) -> PassResult {
        match validate_structured_cfg(module) {
            Ok(()) => {
                // Apply the success marker idempotently. `validate_and_mark`
                // would re-validate ; we already did, so write the marker
                // directly.
                let already = module
                    .attributes
                    .iter()
                    .any(|(k, _)| k == crate::structured_cfg::STRUCTURED_CFG_VALIDATED_KEY);
                let changed = if already {
                    false
                } else {
                    module.attributes.push((
                        crate::structured_cfg::STRUCTURED_CFG_VALIDATED_KEY.to_string(),
                        crate::structured_cfg::STRUCTURED_CFG_VALIDATED_VALUE.to_string(),
                    ));
                    true
                };
                PassResult {
                    name: self.name().to_string(),
                    changed,
                    diagnostics: Vec::new(),
                }
            }
            Err(violations) => {
                // One PassDiagnostic per violation. The diagnostic-code
                // carries through unchanged ; the message is the
                // thiserror-rendered Display for the variant (which
                // already includes the code prefix).
                let diagnostics = violations
                    .into_iter()
                    .map(|v| PassDiagnostic::error(v.code(), format!("{v}")))
                    .collect();
                PassResult {
                    name: self.name().to_string(),
                    changed: false,
                    diagnostics,
                }
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

/// W-B-RECOGNIZER (Wave-A1) — tagged-union ABI lowering pass.
///
/// Wraps [`crate::tagged_union_abi::expand_module`] in the `MirPass` shape so
/// it can be placed in the canonical pipeline. Walks every fn in the module +
/// rewrites each `cssl.option.{some,none}` / `cssl.result.{ok,err}` op into a
/// packed `{tag : u32, payload : [u8; sizeof T]}` cell shape (the canonical
/// stage-0 ABI).
///
/// § DIAGNOSTIC-CODES
///   - `TUNI0000` (Info)  — emitted with the per-pass `ExpansionReport`
///     summary so downstream auditors can verify the rewrite count without
///     re-walking the module. The summary is only emitted when the report's
///     `total_count() > 0` — empty modules stay quiet.
///
/// The `changed` flag is set whenever any op was expanded (i.e.
/// `report.total_count() > 0`).
#[derive(Debug, Clone, Copy, Default)]
pub struct TaggedUnionAbiPass;

impl MirPass for TaggedUnionAbiPass {
    fn name(&self) -> &'static str {
        "tagged-union-abi"
    }

    fn run(&self, module: &mut MirModule) -> PassResult {
        let report = expand_tagged_union_module(module);
        let mut diagnostics = Vec::new();
        let total = report.total_count();
        if total > 0 {
            diagnostics.push(PassDiagnostic::info(
                "TUNI0000",
                format!(
                    "tagged-union ABI expansion : {total} construct ops rewritten \
                     ({some} Some, {none} None, {ok} Ok, {err} Err) ; \
                     {bytes} bytes total",
                    some = report.option_some_count,
                    none = report.option_none_count,
                    ok = report.result_ok_count,
                    err = report.result_err_count,
                    bytes = report.total_bytes_allocated,
                ),
            ));
        }
        PassResult {
            name: self.name().to_string(),
            changed: total > 0,
            diagnostics,
        }
    }
}

/// W-A8 (T11-D245 / Wave-C1 carry-forward) — `cssl.string.*` ABI audit pass.
///
/// Walks every fn in the module + counts ops in the Wave-C1 string-ABI
/// family :
///   - `cssl.string.from_utf8` / `cssl.string.from_utf8_unchecked`
///   - `cssl.string.len` / `cssl.string.byte_at` / `cssl.string.slice`
///   - `cssl.string.format` / `cssl.string.push_str`
///   - `cssl.str_slice.new` / `cssl.str_slice.len` / `cssl.str_slice.as_bytes`
///   - `cssl.char.from_u32`
///
/// The pass is structural-audit only at this slice : the recognizer arms
/// in `body_lower::try_lower_string_*` already lower the source-level
/// stdlib calls into the canonical Wave-C1 op shapes. The audit pass
/// summarizes the count so downstream consumers (cgen, IFC, telemetry)
/// can verify wire-protocol coverage without re-walking the module.
/// Future slices may extend this pass to expand
/// `cssl.string.from_utf8_unchecked` into the explicit
/// `cssl.heap.alloc + memref.store + ...` shape ; at this slice the
/// expansion is delegated to `cssl-cgen-cpu-cranelift::cgen_string`
/// at codegen time.
///
/// § DIAGNOSTIC-CODES
///   - `STRABI0000` (Info)  — per-pass count summary. Only emitted when
///     at least one Wave-C1 op is observed.
///
/// The `changed` flag is always `false` at this slice — the pass is read-
/// only. Once the pass starts expanding ops (future slice), the flag flips.
#[derive(Debug, Clone, Copy, Default)]
pub struct StringAbiPass;

impl MirPass for StringAbiPass {
    fn name(&self) -> &'static str {
        "string-abi"
    }

    fn run(&self, module: &mut MirModule) -> PassResult {
        // Per-family counters. Walked recursively so ops nested inside
        // scf.if / scf.for / cssl.region.* are also counted.
        let mut counts = StringAbiCounts::default();
        for func in module.funcs.iter() {
            count_string_abi_ops_in_region(&func.body, &mut counts);
        }
        let StringAbiCounts {
            from_utf8,
            from_utf8_unchecked,
            string_len,
            string_byte_at,
            string_format,
            string_push_str,
            str_slice_new,
            str_slice_len,
            str_slice_as_bytes,
            char_from_u32,
        } = counts;
        let total = from_utf8
            + from_utf8_unchecked
            + string_len
            + string_byte_at
            + string_format
            + string_push_str
            + str_slice_new
            + str_slice_len
            + str_slice_as_bytes
            + char_from_u32;
        let mut diagnostics = Vec::new();
        if total > 0 {
            diagnostics.push(PassDiagnostic::info(
                "STRABI0000",
                format!(
                    "string ABI audit : {total} Wave-C1 ops observed \
                     ({string_len} string.len, {string_byte_at} string.byte_at, \
                     {from_utf8} string.from_utf8, {from_utf8_unchecked} string.from_utf8_unchecked, \
                     {string_format} string.format, {string_push_str} string.push_str, \
                     {str_slice_new} str_slice.new, {str_slice_len} str_slice.len, \
                     {str_slice_as_bytes} str_slice.as_bytes, {char_from_u32} char.from_u32)",
                ),
            ));
        }
        PassResult {
            name: self.name().to_string(),
            // Read-only audit pass — never mutates ops at this slice.
            changed: false,
            diagnostics,
        }
    }
}

/// Per-family Wave-C1 op-counts collected by [`StringAbiPass`].
#[derive(Debug, Clone, Copy, Default)]
struct StringAbiCounts {
    from_utf8: usize,
    from_utf8_unchecked: usize,
    string_len: usize,
    string_byte_at: usize,
    string_format: usize,
    string_push_str: usize,
    str_slice_new: usize,
    str_slice_len: usize,
    str_slice_as_bytes: usize,
    char_from_u32: usize,
}

/// Walk a `MirRegion` recursively + tally Wave-C1 ops into `counts`.
///
/// Recurses into every nested region of every op (so ops inside scf.if
/// then/else regions and scf.for / scf.while / cssl.region.* bodies are
/// also counted). The walker is read-only ; matches by canonical op-name.
fn count_string_abi_ops_in_region(region: &crate::block::MirRegion, counts: &mut StringAbiCounts) {
    for block in region.blocks.iter() {
        for op in block.ops.iter() {
            match op.name.as_str() {
                "cssl.string.from_utf8" => counts.from_utf8 += 1,
                "cssl.string.from_utf8_unchecked" => counts.from_utf8_unchecked += 1,
                "cssl.string.len" => counts.string_len += 1,
                "cssl.string.byte_at" => counts.string_byte_at += 1,
                "cssl.string.format" => counts.string_format += 1,
                "cssl.string.push_str" => counts.string_push_str += 1,
                "cssl.str_slice.new" => counts.str_slice_new += 1,
                "cssl.str_slice.len" => counts.str_slice_len += 1,
                "cssl.str_slice.as_bytes" => counts.str_slice_as_bytes += 1,
                "cssl.char.from_u32" => counts.char_from_u32 += 1,
                _ => {}
            }
            for nested in op.regions.iter() {
                count_string_abi_ops_in_region(nested, counts);
            }
        }
    }
}

/// W-B-RECOGNIZER (Wave-A3) — `?`-operator MIR-rewrite pass.
///
/// Wraps [`crate::try_op_lower::lower_try_ops_in_module`] in the `MirPass`
/// shape. Rewrites every `cssl.try` op into a tag-dispatched short-circuit-
/// return on the operand's tagged-union shape — the failure-arm reconstructs
/// the failure value in the caller's return type (`None` / `Err(payload)`)
/// + emits `func.return` ; the success-arm extracts the payload via
/// `memref.load`.
///
/// § DIAGNOSTIC-CODES
///   - `TRY0000` (Info)    — per-pass rewrite summary (count + total-bytes).
///   - `TRY0001` (Warning) — per-pass type-mismatch counter > 0 ; HIR's
///     `infer.rs` already surfaces the source-level error, but the MIR
///     pass emits an audit-trail diagnostic so downstream tooling can
///     observe the count without trawling the HIR diagnostic-bag.
///
/// The pass MUST run AFTER [`TaggedUnionAbiPass`] (per the module-doc
/// `STAGE-0 ASSUMPTIONS` — the rewrite expects the operand's `cssl.try`
/// scrutinee to be a Ptr-to-tagged-union cell, which is the post-A1
/// shape).
#[derive(Debug, Clone, Copy, Default)]
pub struct TryOpLowerPass;

impl MirPass for TryOpLowerPass {
    fn name(&self) -> &'static str {
        "try-op-lower"
    }

    fn run(&self, module: &mut MirModule) -> PassResult {
        let report = lower_try_ops_in_module(module);
        let mut diagnostics = Vec::new();
        let total = report.total_count();
        if total > 0 {
            diagnostics.push(PassDiagnostic::info(
                "TRY0000",
                format!(
                    "try-op lowering : {rewritten} rewritten, {mismatch} type-mismatch, \
                     {malformed} malformed (total {total})",
                    rewritten = report.rewritten_count,
                    mismatch = report.type_mismatch_count,
                    malformed = report.malformed_count,
                ),
            ));
        }
        if report.type_mismatch_count > 0 {
            diagnostics.push(PassDiagnostic::warning(
                "TRY0001",
                format!(
                    "{} ?-op call-site(s) found in non-Option/non-Result fn \
                     return position ; HIR diagnoses these — MIR pass left \
                     them un-rewritten for downstream visibility",
                    report.type_mismatch_count
                ),
            ));
        }
        PassResult {
            name: self.name().to_string(),
            changed: report.rewritten_count > 0,
            diagnostics,
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
        // T11-D138 (W3g-01) : `enforces-sigma-at-cell-touches` joined the
        // canonical set, raising the pass-count to 8.
        // W-B-RECOGNIZER : `tagged-union-abi` + `try-op-lower` join the
        // canonical set, raising the pass-count from 8 to 10.
        // T11-D245 (W-A8 / Wave-C1 carry-forward) : `string-abi` joins the
        // canonical set, raising the pass-count from 10 to 11.
        // T11-D282 (W-A1-ε) : `tagged-union-abi` runs a SECOND time after
        // `try-op-lower` to catch construct-ops the try-op rewrite
        // synthesizes inside scf.if failure-arms. Pass-count 11 → 12.
        // T11-D285 (W-E5-2) : `effect-row-validator` joins the canonical set,
        // raising the pass-count from 12 to 13.
        let p = PassPipeline::canonical();
        let names: Vec<&str> = p.names().collect();
        assert_eq!(names.len(), 13);
        assert!(names.contains(&"monomorphization"));
        assert!(names.contains(&"ad-transform"));
        assert!(names.contains(&"ifc-lowering"));
        assert!(names.contains(&"smt-discharge-queue"));
        assert!(names.contains(&"telemetry-probe-insert"));
        assert!(names.contains(&"biometric-egress-check"));
        assert!(names.contains(&"enforces-sigma-at-cell-touches"));
        assert!(names.contains(&"tagged-union-abi"));
        assert!(names.contains(&"string-abi"));
        assert!(names.contains(&"try-op-lower"));
        assert!(names.contains(&"structured-cfg-validator"));
        // tagged-union-abi appears twice (sweep-1 + W-A1-ε sweep-2).
        assert_eq!(
            names.iter().filter(|n| **n == "tagged-union-abi").count(),
            2,
            "expected 2 invocations of tagged-union-abi (W-A1-ε sweep-2)",
        );
        assert!(names.contains(&"effect-row-validator"));
    }

    #[test]
    fn canonical_runs_all_on_empty_module() {
        let p = PassPipeline::canonical();
        let mut module = MirModule::new();
        let results = p.run_all(&mut module);
        // All 13 stock passes should execute on an empty module without
        // errors. (T11-D138 added enforces-sigma-at-cell-touches ;
        // W-B-RECOGNIZER added tagged-union-abi + try-op-lower ;
        // T11-D245 W-A8 added string-abi ; T11-D282 W-A1-ε added a
        // second tagged-union-abi sweep after try-op-lower ;
        // T11-D285 W-E5-2 added effect-row-validator.)
        assert_eq!(results.len(), 13);
        // Stub passes should not report `changed`. The
        // `structured-cfg-validator` legitimately reports `changed=true`
        // on first run because T11-D70 / S6-D5 made it write the
        // `("structured_cfg.validated", "true")` marker on success — the
        // marker IS a module mutation. All other stubs are no-ops.
        // The two W-B-RECOGNIZER passes (tagged-union-abi + try-op-lower)
        // also stay no-op on empty modules (no construct ops + no ?-ops).
        for r in &results {
            if r.name == "structured-cfg-validator" {
                continue;
            }
            assert!(
                !r.changed,
                "{} reported changed=true on empty module",
                r.name
            );
        }
    }

    #[test]
    fn canonical_validator_writes_marker_on_empty_module() {
        // Companion to `canonical_runs_all_on_empty_module` : the
        // structured-cfg-validator's `changed=true` corresponds to the
        // marker attribute being set. T11-D70 contract.
        let p = PassPipeline::canonical();
        let mut module = MirModule::new();
        let _ = p.run_all(&mut module);
        assert!(crate::structured_cfg::has_structured_cfg_marker(&module));
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

    // ═════════════════════════════════════════════════════════════════════
    // § W-B-RECOGNIZER tests — Wave-A1 (TaggedUnionAbiPass) +
    //   Wave-A3 (TryOpLowerPass) MirPass-impls.
    // ═════════════════════════════════════════════════════════════════════

    use super::{StringAbiPass, TaggedUnionAbiPass, TryOpLowerPass};

    #[test]
    fn tagged_union_abi_pass_name() {
        assert_eq!(TaggedUnionAbiPass.name(), "tagged-union-abi");
    }

    #[test]
    fn try_op_lower_pass_name() {
        assert_eq!(TryOpLowerPass.name(), "try-op-lower");
    }

    #[test]
    fn tagged_union_abi_empty_module_no_change() {
        // Empty module : no construct ops to expand → changed=false +
        // no diagnostics emitted.
        let mut module = MirModule::new();
        let r = TaggedUnionAbiPass.run(&mut module);
        assert_eq!(r.name, "tagged-union-abi");
        assert!(!r.changed, "empty module should not report changed=true");
        assert!(r.diagnostics.is_empty(), "diagnostics on empty: {:?}", r.diagnostics);
        assert!(!r.has_errors());
    }

    #[test]
    fn try_op_lower_empty_module_no_change() {
        let mut module = MirModule::new();
        let r = TryOpLowerPass.run(&mut module);
        assert_eq!(r.name, "try-op-lower");
        assert!(!r.changed);
        assert!(r.diagnostics.is_empty());
        assert!(!r.has_errors());
    }

    #[test]
    fn tagged_union_abi_diagnostic_code_TUNI0000() {
        // The pass emits `TUNI0000` (Info) when it has any expansion to
        // report. On empty modules nothing is emitted ; the constant is
        // reachable via the canonical-pipeline flow when real ops are
        // present (covered by `tagged_union_abi`'s own crate-internal tests).
        // Here we just confirm the pass exists + runs without error.
        let mut module = MirModule::new();
        let r = TaggedUnionAbiPass.run(&mut module);
        for d in &r.diagnostics {
            assert!(d.code.starts_with("TUNI"), "unexpected code: {}", d.code);
        }
    }

    #[test]
    fn try_op_lower_diagnostic_codes() {
        // The pass emits TRY0000 / TRY0001 codes when ?-ops are present.
        let mut module = MirModule::new();
        let r = TryOpLowerPass.run(&mut module);
        for d in &r.diagnostics {
            assert!(d.code.starts_with("TRY"), "unexpected code: {}", d.code);
        }
    }

    // ── T11-D245 (W-A8 / Wave-C1 carry-forward) — `string-abi` pass tests ─

    #[test]
    fn string_abi_pass_name() {
        assert_eq!(StringAbiPass.name(), "string-abi");
    }

    #[test]
    fn string_abi_empty_module_no_change() {
        // Empty module : no Wave-C1 ops to count → changed=false +
        // no diagnostics emitted (audit-only pass).
        let mut module = MirModule::new();
        let r = StringAbiPass.run(&mut module);
        assert_eq!(r.name, "string-abi");
        assert!(!r.changed, "empty module should not report changed=true");
        assert!(
            r.diagnostics.is_empty(),
            "diagnostics on empty: {:?}",
            r.diagnostics
        );
        assert!(!r.has_errors());
    }

    #[test]
    fn string_abi_counts_recognized_ops() {
        // Build a module containing one of each Wave-C1 op kind
        // (synthesized inline). The audit-pass should count exactly those
        // ops + emit a STRABI0000 Info-diagnostic.
        use crate::block::MirOp;
        use crate::value::{IntWidth, MirType, ValueId};
        let mut module = MirModule::new();
        let mut f = MirFunc::new("string_caller", vec![], vec![]);
        if let Some(entry) = f.body.entry_mut() {
            entry.push(
                MirOp::std("cssl.string.len")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), MirType::Int(IntWidth::I64)),
            );
            entry.push(
                MirOp::std("cssl.str_slice.len")
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Int(IntWidth::I64)),
            );
            entry.push(
                MirOp::std("cssl.char.from_u32")
                    .with_operand(ValueId(2))
                    .with_result(ValueId(3), MirType::Ptr),
            );
            entry.push(
                MirOp::std("cssl.string.byte_at")
                    .with_operand(ValueId(0))
                    .with_operand(ValueId(2))
                    .with_result(ValueId(4), MirType::Int(IntWidth::I32)),
            );
        }
        module.push_func(f);
        let r = StringAbiPass.run(&mut module);
        assert!(!r.changed, "audit-only pass must not mutate ops");
        assert_eq!(r.diagnostics.len(), 1);
        assert_eq!(r.diagnostics[0].code, "STRABI0000");
        assert!(r.diagnostics[0].message.contains("4 Wave-C1 ops observed"));
    }

    #[test]
    fn string_abi_runs_after_tagged_union_in_canonical() {
        // Per the W-A8 module-doc § DESIGN, `string-abi` MUST run AFTER
        // `tagged-union-abi` (so embedded Option<char> ops in
        // `cssl.char.from_u32` lowerings see the canonical Wave-A1 cell
        // shape) and BEFORE `try-op-lower` (so `?` on `string_from_utf8`'s
        // Result is rewritten with the lowered op-stream visible).
        let p = PassPipeline::canonical();
        let names: Vec<&str> = p.names().collect();
        let abi_idx = names
            .iter()
            .position(|n| *n == "tagged-union-abi")
            .expect("tagged-union-abi");
        let str_idx = names
            .iter()
            .position(|n| *n == "string-abi")
            .expect("string-abi");
        let try_idx = names
            .iter()
            .position(|n| *n == "try-op-lower")
            .expect("try-op-lower");
        assert!(abi_idx < str_idx, "string-abi must follow tagged-union-abi");
        assert!(str_idx < try_idx, "string-abi must precede try-op-lower");
    }

    #[test]
    fn tagged_union_runs_before_try_op_lower_in_canonical() {
        // Per the Wave-A3 module-doc § STAGE-0 ASSUMPTIONS, `try-op-lower`
        // requires `tagged-union-abi` to have run first. Verify the
        // canonical pipeline orders them correctly.
        let p = PassPipeline::canonical();
        let names: Vec<&str> = p.names().collect();
        let abi_idx = names.iter().position(|n| *n == "tagged-union-abi");
        let try_idx = names.iter().position(|n| *n == "try-op-lower");
        assert!(abi_idx.is_some());
        assert!(try_idx.is_some());
        assert!(
            abi_idx.unwrap() < try_idx.unwrap(),
            "tagged-union-abi must precede try-op-lower in canonical pipeline"
        );
    }

    #[test]
    fn tagged_union_runs_before_cfg_validator() {
        // The structured-CFG validator is the FINAL gate — both Wave-A
        // passes must run BEFORE it.
        let p = PassPipeline::canonical();
        let names: Vec<&str> = p.names().collect();
        let abi_idx = names
            .iter()
            .position(|n| *n == "tagged-union-abi")
            .expect("tagged-union-abi");
        let try_idx = names
            .iter()
            .position(|n| *n == "try-op-lower")
            .expect("try-op-lower");
        let cfg_idx = names
            .iter()
            .position(|n| *n == "structured-cfg-validator")
            .expect("structured-cfg-validator");
        assert!(abi_idx < cfg_idx);
        assert!(try_idx < cfg_idx);
    }

    #[test]
    fn tagged_union_runs_after_sigma_enforce() {
        // Wave-A passes must run AFTER all type/effect passes — verify
        // they sit AFTER `enforces-sigma-at-cell-touches` (the last of
        // the type/effect passes).
        let p = PassPipeline::canonical();
        let names: Vec<&str> = p.names().collect();
        let sigma_idx = names
            .iter()
            .position(|n| *n == "enforces-sigma-at-cell-touches")
            .expect("sigma");
        let abi_idx = names
            .iter()
            .position(|n| *n == "tagged-union-abi")
            .expect("abi");
        assert!(sigma_idx < abi_idx);
    }

    #[test]
    fn pipeline_runs_wave_a_passes_in_order() {
        // Smoke : the canonical pipeline executes all 13 passes including
        // both W-B-RECOGNIZER additions + the W-A1-ε sweep-2 + W-E5-2
        // effect-row-validator. Using the run_all path we should see results
        // from all passes in the result-sequence (with tagged-union-abi
        // appearing twice).
        let p = PassPipeline::canonical();
        let mut module = MirModule::new();
        let results = p.run_all(&mut module);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"tagged-union-abi"));
        assert!(names.contains(&"try-op-lower"));
        // W-A1-ε : tagged-union-abi appears twice in run-results too.
        assert_eq!(
            names.iter().filter(|n| **n == "tagged-union-abi").count(),
            2,
            "expected 2 result-entries for tagged-union-abi (W-A1-ε)",
        );
    }

    // ═════════════════════════════════════════════════════════════════════
    // § T11-D282 (W-A1-ε) — sweep-2 of TaggedUnionAbiPass after
    //   TryOpLowerPass. Verifies that Ok / Err / None construct-ops
    //   synthesized by `try_op_lower::build_*_failure_region` inside
    //   scf.if branches are picked up by the second sweep + expanded
    //   into the canonical heap.alloc + tag-store + payload-store
    //   shape, with the construct-op's arg-path-references resolving
    //   correctly within the spliced if/else cascade-arm.
    // ═════════════════════════════════════════════════════════════════════

    use super::TaggedUnionAbiPass as W_A1_eps_TaggedUnionAbiPass;
    use super::TryOpLowerPass as W_A1_eps_TryOpLowerPass;
    use crate::block::{MirOp, MirRegion};
    use crate::op::CsslOp;
    use crate::value::{IntWidth, MirType, MirValue, ValueId};

    /// Build a fn whose body has a single `cssl.try` op with a
    /// Result-shaped operand + return type. After `lower_fn_body`-style
    /// pipeline, the `try-op-lower` pass would synthesize `cssl.result.err`
    /// in the failure-arm of an scf.if cascade. The W-A1-ε sweep-2
    /// expands that construct-op.
    fn build_fn_with_try_on_result() -> MirFunc {
        let mut func = MirFunc::new(
            "try_caller",
            // params : a Result<i32, i32> coming in.
            vec![MirType::Opaque("!cssl.result.i32.i32".into())],
            // return : Result<i32, i32>.
            vec![MirType::Opaque("!cssl.result.i32.i32".into())],
        );
        // Stamp the entry-arg in the body.
        if let Some(entry) = func.body.entry_mut() {
            entry.args.push(MirValue {
                id: ValueId(0),
                ty: MirType::Opaque("!cssl.result.i32.i32".into()),
            });
            // % cssl.try %0  →  i32   (success-payload extracted)
            entry.push(
                MirOp::std("cssl.try")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), MirType::Int(IntWidth::I32)),
            );
            // % cssl.result.ok %1   →  Result<i32, i32>
            entry.push(
                MirOp::new(CsslOp::ResultOk)
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Opaque("!cssl.result.ok.i32".into()))
                    .with_attribute("payload_ty", "i32"),
            );
            entry.push(MirOp::std("func.return").with_operand(ValueId(2)));
        }
        func.next_value_id = 3;
        func
    }

    /// W-A1-ε goldenA : the W-A1-ε sweep-2 (TaggedUnionAbiPass run AFTER
    /// TryOpLowerPass) expands the Err construct-op that try-op-lower
    /// synthesizes inside the failure-arm scf.if region. After the
    /// W-A1-ε-relevant subset runs (try-op-lower then TaggedUnionAbiPass)
    /// there are zero raw `cssl.result.err` ops left anywhere in the
    /// module.
    ///
    /// This test isolates the ε-scope (sweep-2 catches synthesized
    /// constructs) from the wider sig-rewrite-vs-classify ordering issue
    /// in the canonical pipeline (a sister W-A1 sub-slice).
    #[test]
    fn w_a1_eps_sweep2_expands_try_emitted_err_construct_op() {
        let mut module = MirModule::new();
        module.push_func(build_fn_with_try_on_result());
        // try-op-lower runs first → synthesizes failure-arm constructs.
        let _ = W_A1_eps_TryOpLowerPass.run(&mut module);
        // sweep-2 expands those synthesized constructs.
        let _ = W_A1_eps_TaggedUnionAbiPass.run(&mut module);

        // Walk every op in every region — recursively — and assert no
        // raw construct op survives.
        let mut raw_construct_count = 0_u32;
        for func in &module.funcs {
            count_raw_construct_ops_in_region(&func.body, &mut raw_construct_count);
        }
        assert_eq!(
            raw_construct_count, 0,
            "W-A1-ε sweep-2 should leave zero raw construct ops anywhere",
        );
    }

    /// W-A1-ε goldenB : after try-op-lower runs (without sweep-1's
    /// sig-rewrite muting the type-strings), the raw `cssl.result.err`
    /// op survives inside the synthesized failure-arm. This is the BUG
    /// that sweep-2 closes : the construct-op the try-op rewriter
    /// spliced into the scf.if branch is never expanded if no further
    /// TaggedUnionAbiPass sweep is scheduled.
    ///
    /// Test runs ONLY `TryOpLowerPass` (skipping sweep-1) so the
    /// classify_caller_return logic sees the un-rewritten opaque
    /// return-type — this isolates the W-A1-ε scope (construct-arg-path
    /// in if/else cascade-arms) from the wider sig-rewrite-vs-classify
    /// ordering issue (a sister W-A1 sub-slice).
    #[test]
    fn w_a1_eps_partial_pipeline_leaves_raw_err_in_failure_arm() {
        let mut module = MirModule::new();
        module.push_func(build_fn_with_try_on_result());
        // Run try-op-lower DIRECTLY (no prior sig-rewrite). The
        // synthesized failure-arm carries `cssl.result.err` ops that
        // are NOT yet expanded.
        let _ = W_A1_eps_TryOpLowerPass.run(&mut module);
        let mut raw_construct_count = 0_u32;
        for func in &module.funcs {
            count_raw_construct_ops_in_region(&func.body, &mut raw_construct_count);
        }
        // At least one raw construct op should be present (the
        // `cssl.result.err` synthesized by try-op-lower's failure-arm).
        // This is the BUG that W-A1-ε's sweep-2 closes.
        assert!(
            raw_construct_count >= 1,
            "expected ≥ 1 raw construct op without sweep-2 ; got {raw_construct_count}",
        );
    }

    /// W-A1-ε idempotency : running the canonical pipeline twice on the
    /// same module is a no-op for the second sweep-2 (the body is
    /// already expanded ; sweep-2 finds zero new construct-ops).
    #[test]
    fn w_a1_eps_sweep2_idempotent_on_already_expanded_module() {
        let mut module = MirModule::new();
        module.push_func(build_fn_with_try_on_result());
        let p = PassPipeline::canonical();
        let r1 = p.run_all(&mut module);
        // Snapshot the op-count after the first canonical pipeline run.
        let mut count_after_run1 = 0_u32;
        for func in &module.funcs {
            count_total_ops_in_region(&func.body, &mut count_after_run1);
        }
        let r2 = p.run_all(&mut module);
        let mut count_after_run2 = 0_u32;
        for func in &module.funcs {
            count_total_ops_in_region(&func.body, &mut count_after_run2);
        }
        assert_eq!(
            count_after_run1, count_after_run2,
            "second canonical-run should produce no additional ops (idempotency)",
        );
        // Both runs should have succeeded without errors.
        for r in r1.iter().chain(r2.iter()) {
            assert!(
                !r.has_errors(),
                "pass `{}` errored : {:?}",
                r.name,
                r.diagnostics
            );
        }
    }

    /// W-A1-ε arg-path : the `cssl.result.err` construct-op synthesized
    /// by try-op-lower references `%err_payload` as its operand-0. After
    /// the W-A1-ε sweep-2 expands it, the resulting
    /// `memref.store payload, cell` lives inside the SAME scf.if
    /// failure-arm region, and its `payload`-operand still resolves to
    /// the load that produced `%err_payload` (the load is a sibling op
    /// in the same arm-region). This test pins the arg-path-resolution
    /// in the spliced cascade-arm.
    ///
    /// Runs the W-A1-ε-relevant subset directly (try-op-lower then
    /// TaggedUnionAbiPass sweep-2) to isolate the ε-fix scope.
    #[test]
    fn w_a1_eps_sweep2_preserves_err_payload_arg_path() {
        let mut module = MirModule::new();
        module.push_func(build_fn_with_try_on_result());
        // try-op-lower synthesizes the cssl.result.err inside the
        // failure-arm scf.if region.
        let _ = W_A1_eps_TryOpLowerPass.run(&mut module);
        // sweep-2 expands the synthesized construct-ops. This is the
        // exact W-A1-ε behavior under test.
        let _ = W_A1_eps_TaggedUnionAbiPass.run(&mut module);

        // Walk to the failure-arm region of the try-op's scf.if + locate
        // the post-sweep-2 op-shape : we expect the original `memref.load`
        // (loading err-payload) to feed a `memref.store` (the expanded
        // payload-store) at offset 4 of the new cell-ptr. The construct-
        // op itself (`cssl.result.err`) should be GONE.
        let func = &module.funcs[0];
        let mut found_payload_store = false;
        let mut found_raw_err = false;
        find_payload_store_in_scf_if_arms(&func.body, &mut found_payload_store, &mut found_raw_err);
        assert!(
            found_payload_store,
            "expected sweep-2 to emit a memref.store(payload) inside the scf.if failure-arm",
        );
        assert!(
            !found_raw_err,
            "expected sweep-2 to leave NO raw cssl.result.err in scf.if branches",
        );
    }

    /// W-A1-ε pass-order : sweep-2 of `tagged-union-abi` MUST appear
    /// AFTER `try-op-lower` in the canonical pipeline. This is the
    /// structural invariant the fix establishes.
    #[test]
    fn w_a1_eps_sweep2_runs_after_try_op_lower() {
        let p = PassPipeline::canonical();
        let names: Vec<&str> = p.names().collect();
        let try_idx = names
            .iter()
            .position(|n| *n == "try-op-lower")
            .expect("try-op-lower");
        // Find the LAST tagged-union-abi (sweep-2).
        let last_abi_idx = names
            .iter()
            .enumerate()
            .filter(|(_, n)| **n == "tagged-union-abi")
            .map(|(i, _)| i)
            .last()
            .expect("at least one tagged-union-abi");
        assert!(
            try_idx < last_abi_idx,
            "sweep-2 of tagged-union-abi (idx {last_abi_idx}) must run AFTER try-op-lower (idx {try_idx})",
        );
    }

    /// W-A1-ε : sweep-2 also handles `cssl.option.none` synthesized by
    /// the Option-family failure-arm path (`build_option_failure_region`).
    /// Build an Option-typed fn with `cssl.try` + verify the synthesized
    /// `cssl.option.none` is also expanded.
    ///
    /// Runs the W-A1-ε-relevant subset directly (try-op-lower then
    /// TaggedUnionAbiPass sweep-2) to isolate the ε-fix scope.
    #[test]
    fn w_a1_eps_sweep2_expands_try_emitted_option_none_construct_op() {
        let mut func = MirFunc::new(
            "try_caller_opt",
            vec![MirType::Opaque("!cssl.option.i32".into())],
            vec![MirType::Opaque("!cssl.option.i32".into())],
        );
        if let Some(entry) = func.body.entry_mut() {
            entry.args.push(MirValue {
                id: ValueId(0),
                ty: MirType::Opaque("!cssl.option.i32".into()),
            });
            entry.push(
                MirOp::std("cssl.try")
                    .with_operand(ValueId(0))
                    .with_result(ValueId(1), MirType::Int(IntWidth::I32)),
            );
            entry.push(
                MirOp::new(CsslOp::OptionSome)
                    .with_operand(ValueId(1))
                    .with_result(ValueId(2), MirType::Opaque("!cssl.option.i32".into()))
                    .with_attribute("payload_ty", "i32"),
            );
            entry.push(MirOp::std("func.return").with_operand(ValueId(2)));
        }
        func.next_value_id = 3;

        let mut module = MirModule::new();
        module.push_func(func);
        // try-op-lower synthesizes `cssl.option.none` in the failure-arm.
        let _ = W_A1_eps_TryOpLowerPass.run(&mut module);
        // sweep-2 expands the synthesized constructs.
        let _ = W_A1_eps_TaggedUnionAbiPass.run(&mut module);

        let mut raw_construct_count = 0_u32;
        for f in &module.funcs {
            count_raw_construct_ops_in_region(&f.body, &mut raw_construct_count);
        }
        assert_eq!(
            raw_construct_count, 0,
            "W-A1-ε sweep-2 should expand Option-family construct-ops too",
        );
    }

    // ── helpers used by the W-A1-ε tests above ──

    fn count_raw_construct_ops_in_region(region: &MirRegion, count: &mut u32) {
        for block in &region.blocks {
            for op in &block.ops {
                if matches!(
                    op.op,
                    CsslOp::OptionSome | CsslOp::OptionNone | CsslOp::ResultOk | CsslOp::ResultErr
                ) {
                    *count += 1;
                }
                for nested in &op.regions {
                    count_raw_construct_ops_in_region(nested, count);
                }
            }
        }
    }

    fn count_total_ops_in_region(region: &MirRegion, count: &mut u32) {
        for block in &region.blocks {
            for op in &block.ops {
                *count += 1;
                for nested in &op.regions {
                    count_total_ops_in_region(nested, count);
                }
            }
        }
    }

    fn find_payload_store_in_scf_if_arms(
        region: &MirRegion,
        found_store: &mut bool,
        found_raw_err: &mut bool,
    ) {
        for block in &region.blocks {
            for op in &block.ops {
                if op.name == "scf.if" {
                    for nested in &op.regions {
                        for nb in &nested.blocks {
                            for inner in &nb.ops {
                                // memref.store with field=payload : the
                                // post-sweep-2 expansion of an Err / None
                                // / Some / Ok construct-op INSIDE this
                                // branch.
                                let is_payload_store = inner.name == "memref.store"
                                    && inner.attributes.iter().any(|(k, v)| k == "field" && v == "payload");
                                if is_payload_store {
                                    *found_store = true;
                                }
                                if matches!(
                                    inner.op,
                                    CsslOp::ResultErr
                                        | CsslOp::ResultOk
                                        | CsslOp::OptionNone
                                        | CsslOp::OptionSome
                                ) {
                                    *found_raw_err = true;
                                }
                                // Recurse into nested scf.if (e.g. a
                                // dispatch cascade nested inside a
                                // try-op's failure-arm).
                                for deeper in &inner.regions {
                                    find_payload_store_in_scf_if_arms(
                                        deeper,
                                        found_store,
                                        found_raw_err,
                                    );
                                }
                            }
                        }
                    }
                }
                for nested in &op.regions {
                    find_payload_store_in_scf_if_arms(nested, found_store, found_raw_err);
                }
            }
        }
    }
}
