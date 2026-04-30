//! § structured_cfg — Structured-CFG validator pass for `MirModule`.
//!
//! § SPEC
//!   - `specs/02_IR.csl` § STRUCTURED-CFG RULES (CC4)         (line 164)
//!   - `specs/15_MLIR.csl` § STRUCTURED CFG PRESERVATION (CC4) (line 73)
//!   - `specs/15_MLIR.csl` § PASS-PIPELINE                     (line 80, step 1
//!     "structured-CFG validate" ; this module IS that pass)
//!
//! § ROLE — D5 (T11-D70)
//!   The CSSLv3 frontend lowers `if` / `for` / `while` / `loop` expressions to
//!   `scf.if` / `scf.for` / `scf.while` / `scf.loop` MIR ops with nested
//!   regions per C1 (T11-D58) + C2 (T11-D61). The structured form is the
//!   contract every backend consumes : the cranelift JIT + cranelift-object
//!   backends turn each scf op into the equivalent CLIF `brif` + block scaffold,
//!   and the GPU emitters D1..D4 (SPIR-V `OpSelectionMerge` / DXIL
//!   `if`/`while` / MSL/WGSL loop-control) require the same scf-tree shape
//!   to lower without falling back to unstructured CFG. Per
//!   `specs/15_MLIR.csl § STRUCTURED CFG PRESERVATION (CC4)`, **no** `cf.br` /
//!   `cf.cond_br` may flow into any backend ; doing so is a frontend bug,
//!   not a feature.
//!
//!   This pass walks every `MirFunc` body and rejects MIR that violates the
//!   structured-CFG invariant before the codegen pipeline ever sees it.
//!   Errors produced here short-circuit codegen with actionable
//!   diagnostic-codes ; the caller can surface them through the standard
//!   `csslc` diagnostic-emitter or pattern-match on the `CfgViolation` enum
//!   for programmatic recovery.
//!
//! § CONTRACT
//!   - `validate_structured_cfg(&module) -> Result<(), Vec<CfgViolation>>`
//!     is the canonical pre-codegen short-circuit. On success, returns
//!     `Ok(())` ; on any violation, returns the full list of violations
//!     (collected, not first-fail) so users see every issue per build.
//!   - `validate_and_mark(&mut module) -> Result<(), Vec<CfgViolation>>` does
//!     the same and additionally writes the `("structured_cfg.validated",
//!     "true")` attribute onto `module.attributes`. GPU emitters D1..D4
//!     check this marker before emission ; calling them on a non-validated
//!     module is a programmer-error and they panic with a clear message.
//!     The marker is the FANOUT-CONTRACT between D5 and D1..D4.
//!   - The legacy `pipeline::StructuredCfgValidator` `MirPass` impl is
//!     preserved for back-compat with the canonical pipeline + existing
//!     tests ; it now delegates to this module's full validator and emits
//!     one `PassDiagnostic` per `CfgViolation` carrying the stable
//!     diagnostic-code.
//!
//! § DIAGNOSTIC CODES (T11-D70 stable allocation)
//!   - **CFG0001** — empty region (no entry block).             [pre-existing,
//!                   from `pipeline::StructuredCfgValidator` stub before D5]
//!   - **CFG0002** — orphan `scf.yield` op (outside any
//!                   structured parent : scf.if / scf.for / scf.while
//!                   / scf.loop / scf.match).
//!   - **CFG0003** — unstructured `cf.cond_br` op encountered.
//!   - **CFG0004** — unstructured `cf.br` op encountered.
//!   - **CFG0005** — `scf.if` with region-count ≠ 2.
//!   - **CFG0006** — loop-shape (`scf.for` / `scf.while` / `scf.loop`)
//!                   with region-count ≠ 1.
//!   - **CFG0007** — multi-block region inside any scf parent
//!                   (stage-0 expects exactly one entry block per nested
//!                   region per C1+C2 sealing schedule).
//!   - **CFG0008** — orphan `scf.condition` op (outside scf.while parent
//!                   region ; reserved for future cond-reeval per C2's
//!                   deferred bullets).
//!   - **CFG0009** — `cssl.unsupported(Break)` op : `HirExprKind::Break`
//!                   currently lowers to a placeholder per C2's report ;
//!                   this is a hard frontend gap, not a runtime fallback,
//!                   so the validator surfaces it cleanly here rather than
//!                   letting it slip into codegen as `JitError::UnsupportedMirOp`.
//!   - **CFG0010** — `cssl.unsupported(Continue)` op : same shape as Break.
//!
//!   **Discipline** : these codes are STABLE per `SESSION_6_DISPATCH_PLAN.md`
//!   § 3 escalation #4. Adding a new CFG-code requires a DECISIONS sub-entry.
//!   The eight new codes above (CFG0002..CFG0010, with CFG0001 carried-over)
//!   land together as part of T11-D70 to avoid drip-allocation churn ;
//!   a single DECISIONS entry covers them all.
//!
//! § DESIGN — rejection-only, no canonicalization at D5 stage-0
//!   Per the slice handoff landmines, the validator is REJECTION-ONLY at
//!   stage-0. Loop-form canonicalization (e.g., normalizing `while true {}`
//!   to `scf.loop` instead of `scf.while`) is a transform-pass that lands
//!   when GPU emitters need it ; until then, both forms are accepted as
//!   valid scf shapes and the per-emitter D1..D4 walkers handle each.
//!   Treating D5 as transform-capable now would risk reshaping MIR that
//!   downstream backends already lower correctly ; the safer stage-0
//!   contract is "validate + mark, do not modify".
//!
//! § DESIGN — recursive walker
//!   The validator walks each fn's body region recursively : at every block,
//!   for every op, if the op has nested regions (scf.* / cssl.region.* /
//!   etc.) we recurse with the parent's name as context so orphan-detection
//!   works correctly. Orphan detection : a `scf.yield` inside a region whose
//!   parent op is one of the `STRUCTURED_PARENTS` set is FINE (consumed by
//!   the parent at lowering time) ; a `scf.yield` at the top-level region or
//!   inside a non-structured parent is a violation.
//!
//! § DESIGN — full-walk error collection
//!   Returning `Result<MirModule, Vec<CfgViolation>>` per the slice handoff
//!   surface lets the codegen pipeline report every violation per build,
//!   not just the first. This matches the existing `cssl_hir::check_*`
//!   walkers that collect into `Vec<Diagnostic>` and the pre-existing
//!   `PassDiagnostic`-list shape.

use crate::block::{MirOp, MirRegion};
use crate::func::{MirFunc, MirModule};

/// One structured-CFG violation. Each variant carries enough context for an
/// actionable diagnostic (fn-name + offending op-name + parent-op name where
/// relevant + violating region-count where relevant).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CfgViolation {
    /// **CFG0001** — region with no blocks. Stage-0 every region must have
    /// at-least one block (the entry block). An empty region is a body_lower
    /// bug.
    #[error("CFG0001: fn `{fn_name}` has empty region (no entry block)")]
    EmptyRegion { fn_name: String },

    /// **CFG0002** — orphan `scf.yield` op outside a structured parent. The
    /// `parent` field is the closest enclosing op-name (`<top-level>` when
    /// at the fn-body level), useful for diagnostics that need to point the
    /// user at the unexpected control-flow pattern.
    #[error(
        "CFG0002: fn `{fn_name}` has orphan `scf.yield` outside a structured \
         parent (enclosing op = `{parent}`)"
    )]
    OrphanScfYield { fn_name: String, parent: String },

    /// **CFG0003** — unstructured `cf.cond_br` encountered. Per
    /// `specs/15_MLIR.csl § STRUCTURED CFG PRESERVATION (CC4)`, the
    /// CSSLv3-source path emits `scf.if` exclusively ; `cf.cond_br` is
    /// produced only by hand-built MIR or by an erroneous downstream
    /// transform.
    #[error(
        "CFG0003: fn `{fn_name}` contains unstructured `cf.cond_br` op ; \
         CSSLv3 frontend must emit `scf.if` (specs/15 § CC4)"
    )]
    UnstructuredCondBr { fn_name: String },

    /// **CFG0004** — unstructured `cf.br` encountered. Same etymology as
    /// CFG0003. Allowed only if a future slice introduces an explicit
    /// "I-am-emitting-cf-on-purpose" marker ; until then this is a hard
    /// reject.
    #[error(
        "CFG0004: fn `{fn_name}` contains unstructured `cf.br` op ; \
         CSSLv3 frontend must emit structured scf.* (specs/15 § CC4)"
    )]
    UnstructuredBr { fn_name: String },

    /// **CFG0005** — `scf.if` with region-count ≠ 2. The C1 lowering
    /// (`crate::scf::lower_scf_if` in cssl-cgen-cpu-cranelift) requires
    /// exactly 2 regions (then + else, where else may be empty but is
    /// always present per body_lower's invariant).
    #[error(
        "CFG0005: fn `{fn_name}` has `scf.if` with {actual} regions ; \
         expected exactly 2 (then + else)"
    )]
    ScfIfWrongRegionCount { fn_name: String, actual: usize },

    /// **CFG0006** — `scf.for`/`scf.while`/`scf.loop` with region-count ≠ 1.
    /// The C2 lowering requires exactly one body region per loop op.
    /// `op_name` carries the bare loop-op suffix (`for` / `while` / `loop`)
    /// for actionable diagnostics.
    #[error(
        "CFG0006: fn `{fn_name}` has `scf.{op_name}` with {actual} regions ; \
         expected exactly 1 (body)"
    )]
    LoopWrongRegionCount {
        fn_name: String,
        op_name: String,
        actual: usize,
    },

    /// **CFG0007** — multi-block region inside any scf op. Stage-0 expects
    /// exactly one entry block per nested region. A multi-block region would
    /// require multi-block lowering inside `lower_scf_if` / `lower_scf_loop`
    /// per C1+C2's sealing schedule ; no slice has landed that yet.
    #[error(
        "CFG0007: fn `{fn_name}` has `scf.{op_name}` with a region containing \
         {block_count} blocks ; stage-0 expects exactly 1"
    )]
    ScfRegionMultiBlock {
        fn_name: String,
        op_name: String,
        block_count: usize,
    },

    /// **CFG0008** — orphan `scf.condition` op outside a `scf.while` parent.
    /// Reserved for the future cond-reeval slice (per C2 deferred bullets) ;
    /// until then `scf.condition` should not appear at all.
    #[error(
        "CFG0008: fn `{fn_name}` has orphan `scf.condition` outside an \
         `scf.while` parent (enclosing op = `{parent}`)"
    )]
    OrphanScfCondition { fn_name: String, parent: String },

    /// **CFG0009** — `cssl.unsupported(Break)` placeholder produced by
    /// `body_lower` for `HirExprKind::Break` (per C2 deferred bullets). At
    /// stage-0 the validator surfaces this as a hard error rather than
    /// letting it slip into codegen as a generic `UnsupportedMirOp`.
    #[error(
        "CFG0009: fn `{fn_name}` contains unsupported `Break` op ; \
         break/continue lowering is deferred to a future slice"
    )]
    UnsupportedBreak { fn_name: String },

    /// **CFG0010** — `cssl.unsupported(Continue)` placeholder. Same shape
    /// as CFG0009.
    #[error(
        "CFG0010: fn `{fn_name}` contains unsupported `Continue` op ; \
         break/continue lowering is deferred to a future slice"
    )]
    UnsupportedContinue { fn_name: String },
}

impl CfgViolation {
    /// Stable diagnostic-code (e.g. `"CFG0001"`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::EmptyRegion { .. } => "CFG0001",
            Self::OrphanScfYield { .. } => "CFG0002",
            Self::UnstructuredCondBr { .. } => "CFG0003",
            Self::UnstructuredBr { .. } => "CFG0004",
            Self::ScfIfWrongRegionCount { .. } => "CFG0005",
            Self::LoopWrongRegionCount { .. } => "CFG0006",
            Self::ScfRegionMultiBlock { .. } => "CFG0007",
            Self::OrphanScfCondition { .. } => "CFG0008",
            Self::UnsupportedBreak { .. } => "CFG0009",
            Self::UnsupportedContinue { .. } => "CFG0010",
        }
    }

    /// Fn name on which this violation occurred.
    #[must_use]
    pub fn fn_name(&self) -> &str {
        match self {
            Self::EmptyRegion { fn_name }
            | Self::OrphanScfYield { fn_name, .. }
            | Self::UnstructuredCondBr { fn_name }
            | Self::UnstructuredBr { fn_name }
            | Self::ScfIfWrongRegionCount { fn_name, .. }
            | Self::LoopWrongRegionCount { fn_name, .. }
            | Self::ScfRegionMultiBlock { fn_name, .. }
            | Self::OrphanScfCondition { fn_name, .. }
            | Self::UnsupportedBreak { fn_name }
            | Self::UnsupportedContinue { fn_name } => fn_name,
        }
    }
}

/// Module-level attribute marker the validator writes on success. GPU
/// emitters D1..D4 check this attribute before emission ; calling them on a
/// non-validated module is a programmer-error.
pub const STRUCTURED_CFG_VALIDATED_KEY: &str = "structured_cfg.validated";

/// Module-level attribute marker value (`"true"`).
pub const STRUCTURED_CFG_VALIDATED_VALUE: &str = "true";

/// Validate the structured-CFG invariant across every fn in `module`. Returns
/// `Ok(())` if every function honors the contract, or `Err(violations)`
/// containing every violation (full-walk, not first-fail).
///
/// # Errors
/// Returns the full list of `CfgViolation`s when any fn body contains an
/// unstructured op, malformed scf.* shape, orphan terminator, empty region,
/// or unsupported break/continue placeholder.
pub fn validate_structured_cfg(module: &MirModule) -> Result<(), Vec<CfgViolation>> {
    let mut violations = Vec::new();
    for f in &module.funcs {
        validate_fn(f, &mut violations);
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

/// Validate `module` and, on success, write the
/// `("structured_cfg.validated", "true")` marker attribute onto
/// `module.attributes` so downstream GPU emitters can short-circuit-check
/// for "did D5 run successfully on this module ?". On failure, no marker is
/// written and the violations are returned unchanged.
///
/// Idempotent : re-validating an already-marked module re-asserts the
/// attribute presence (no duplicate is written).
///
/// # Errors
/// Returns the full list of `CfgViolation`s ; same shape as
/// [`validate_structured_cfg`].
pub fn validate_and_mark(module: &mut MirModule) -> Result<(), Vec<CfgViolation>> {
    validate_structured_cfg(module)?;
    let already = module
        .attributes
        .iter()
        .any(|(k, _)| k == STRUCTURED_CFG_VALIDATED_KEY);
    if !already {
        module.attributes.push((
            STRUCTURED_CFG_VALIDATED_KEY.to_string(),
            STRUCTURED_CFG_VALIDATED_VALUE.to_string(),
        ));
    }
    Ok(())
}

/// Returns `true` iff the validator marker is present on `module`. GPU
/// emitters D1..D4 check this before emission.
#[must_use]
pub fn has_structured_cfg_marker(module: &MirModule) -> bool {
    module
        .attributes
        .iter()
        .any(|(k, v)| k == STRUCTURED_CFG_VALIDATED_KEY && v == STRUCTURED_CFG_VALIDATED_VALUE)
}

// ─────────────────────────────────────────────────────────────────────────
// § Per-fn + per-region recursive walkers.
// ─────────────────────────────────────────────────────────────────────────

/// Walk one fn's body region. The fn-name is threaded through every
/// violation for actionable diagnostics.
fn validate_fn(f: &MirFunc, out: &mut Vec<CfgViolation>) {
    // Empty fn = signature-only ; that's legal at MIR-level (foreign fn /
    // interface stub). Only the body-region's blocks matter for CFG.
    if f.body.blocks.is_empty() {
        // Per-CFG0001 : empty region IS a violation. body_lower always
        // creates an entry block ; an empty body.blocks list means somebody
        // cleared it, which is the bug case the original stub caught.
        out.push(CfgViolation::EmptyRegion {
            fn_name: f.name.clone(),
        });
        return;
    }
    // Top-level fn body : the parent context is "<top-level>" (fn-body
    // region has no enclosing structured parent op).
    walk_region(&f.body, &f.name, "<top-level>", out);
}

/// Walk a region : every block, every op. If the op has nested regions,
/// recurse with the op's name as the new `parent` context.
fn walk_region(region: &MirRegion, fn_name: &str, parent: &str, out: &mut Vec<CfgViolation>) {
    for block in &region.blocks {
        for op in &block.ops {
            walk_op(op, fn_name, parent, out);
        }
    }
}

/// Walk one op : check for unstructured / orphan forms first, then validate
/// scf.* shape if applicable, then recurse into nested regions with the
/// new parent context.
fn walk_op(op: &MirOp, fn_name: &str, parent: &str, out: &mut Vec<CfgViolation>) {
    // § 1. First check the op-name itself for unstructured forms /
    // orphan terminators / unsupported placeholders. These checks all
    // operate on the bare op-name and don't care about regions.
    match op.name.as_str() {
        // Unstructured CFG ops — should never appear in CSSLv3-source MIR.
        "cf.cond_br" => out.push(CfgViolation::UnstructuredCondBr {
            fn_name: fn_name.to_string(),
        }),
        "cf.br" => out.push(CfgViolation::UnstructuredBr {
            fn_name: fn_name.to_string(),
        }),
        // Region terminators : legal inside their structured parents,
        // orphan otherwise. The `parent` arg encodes the enclosing op-name
        // (or "<top-level>" for the fn-body region itself).
        "scf.yield" => {
            if !is_structured_parent_for_yield(parent) {
                out.push(CfgViolation::OrphanScfYield {
                    fn_name: fn_name.to_string(),
                    parent: parent.to_string(),
                });
            }
        }
        "scf.condition" => {
            if parent != "scf.while" {
                out.push(CfgViolation::OrphanScfCondition {
                    fn_name: fn_name.to_string(),
                    parent: parent.to_string(),
                });
            }
        }
        // Unsupported HIR-lowering placeholders. body_lower's
        // `emit_unsupported` produces ops named `cssl.unsupported(<form>)`
        // when it can't lower a particular HIR shape. C2 documented Break
        // + Continue specifically as "deferred to a future slice" ; D5
        // surfaces them cleanly here so the user gets a structured-CFG
        // diagnostic instead of `JitError::UnsupportedMirOp` later.
        "cssl.unsupported(Break)" => out.push(CfgViolation::UnsupportedBreak {
            fn_name: fn_name.to_string(),
        }),
        "cssl.unsupported(Continue)" => out.push(CfgViolation::UnsupportedContinue {
            fn_name: fn_name.to_string(),
        }),
        _ => {}
    }

    // § 2. Validate scf.* region-count + region-shape. Each shape mirrors
    // the cranelift-side helpers in `cssl_cgen_cpu_cranelift::scf` so a
    // shape-mismatch surfaces here BEFORE the backend's lowerers see it.
    match op.name.as_str() {
        "scf.if" => {
            if op.regions.len() != 2 {
                out.push(CfgViolation::ScfIfWrongRegionCount {
                    fn_name: fn_name.to_string(),
                    actual: op.regions.len(),
                });
            }
            for region in &op.regions {
                check_region_block_shape(region, fn_name, "if", out);
            }
        }
        "scf.for" | "scf.while" | "scf.loop" => {
            // op_name carries the bare suffix for actionable diagnostics
            // (`for` / `while` / `loop`).
            let suffix = op.name.strip_prefix("scf.").unwrap_or(&op.name);
            // § T11-D318 (W-CC-mut-assign) — scf.while evolved from a 1-
            //   region shape (body only · cond pre-computed) to a 2-region
            //   shape (cond_region + body_region · cond re-walked at every
            //   loop-header to observe mutated state). scf.for / scf.loop
            //   stay 1-region per the original C2 contract.
            let max_allowed = if op.name == "scf.while" { 2 } else { 1 };
            if op.regions.is_empty() || op.regions.len() > max_allowed {
                out.push(CfgViolation::LoopWrongRegionCount {
                    fn_name: fn_name.to_string(),
                    op_name: suffix.to_string(),
                    actual: op.regions.len(),
                });
            }
            for region in &op.regions {
                check_region_block_shape(region, fn_name, suffix, out);
            }
        }
        _ => {}
    }

    // § 3. Recurse into nested regions. Each nested region's enclosing
    // parent is THIS op's name. That's what makes orphan-yield detection
    // correct : the yield inside `scf.if`'s then-region sees parent =
    // "scf.if" and is fine ; a yield at fn-body sees parent =
    // "<top-level>" and is flagged.
    for region in &op.regions {
        walk_region(region, fn_name, &op.name, out);
    }
}

/// Set of structured parent op-names that legitimately consume a
/// `scf.yield`. Per C1 + C2 lowerings :
///   - `scf.if` : both branches yield (expression-form) or neither
///     (statement-form, no scf.yield emitted).
///   - `scf.for` / `scf.while` / `scf.loop` : at stage-0 do NOT yield
///     (loop-result-types deferred per C2's deferred bullets) ; future
///     slices may grow loop-yield, so we accept yields inside them as
///     no-ops rather than rejecting them. This keeps D5 forward-compatible
///     with C2's planned growth without requiring a code-change here.
///   - `scf.match` : reserved for the match-lowering slice (deferred from
///     C2 scope) ; same forward-compat reasoning.
fn is_structured_parent_for_yield(parent: &str) -> bool {
    matches!(
        parent,
        "scf.if" | "scf.for" | "scf.while" | "scf.loop" | "scf.match"
    )
}

/// Check that every region inside an scf.* op has exactly one block. Stage-0
/// constraint per C1+C2 sealing schedule ; multi-block regions inside a
/// single branch are deferred until early-return / break-out work lands.
fn check_region_block_shape(
    region: &MirRegion,
    fn_name: &str,
    op_suffix: &str,
    out: &mut Vec<CfgViolation>,
) {
    if region.blocks.len() > 1 {
        out.push(CfgViolation::ScfRegionMultiBlock {
            fn_name: fn_name.to_string(),
            op_name: op_suffix.to_string(),
            block_count: region.blocks.len(),
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — full-walk coverage of the validator. These cover every CFG-code
// (CFG0001..CFG0010) plus the marker contract + composition cases.
// ─────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{MirBlock, MirOp, MirRegion};
    use crate::func::{MirFunc, MirModule};
    use crate::value::{IntWidth, MirType, ValueId};

    /// Build a minimal well-formed fn with an empty entry block. The fn
    /// body's entry block already exists with no ops ; that's a legal
    /// stage-0 shape (the codegen side will reject it for missing a
    /// return, but D5 is structural-only and accepts it).
    fn well_formed_i32_fn(name: &str) -> MirFunc {
        MirFunc::new(name, vec![], vec![MirType::Int(IntWidth::I32)])
    }

    /// Build a `scf.if` op with the given number of regions, each empty
    /// (single entry block, no ops). Used as a fixture across the
    /// region-count + region-shape tests.
    fn scf_if_with_regions(region_count: usize) -> MirOp {
        let mut op = MirOp::std("scf.if");
        for _ in 0..region_count {
            op.regions.push(MirRegion::with_entry(Vec::new()));
        }
        op
    }

    /// Build a `scf.<suffix>` op (for / while / loop) with one region.
    fn scf_loop_one_region(suffix: &str) -> MirOp {
        let mut op = MirOp::std(format!("scf.{suffix}"));
        op.regions.push(MirRegion::with_entry(Vec::new()));
        op
    }

    // ── CFG0001 — EmptyRegion ────────────────────────────────────────────

    #[test]
    fn cfg0001_flags_empty_region() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("bad");
        f.body.blocks.clear();
        module.push_func(f);
        let result = validate_structured_cfg(&module);
        assert!(result.is_err());
        let violations = result.unwrap_err();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].code(), "CFG0001");
        assert_eq!(violations[0].fn_name(), "bad");
    }

    // ── CFG0002 — OrphanScfYield ─────────────────────────────────────────

    #[test]
    fn cfg0002_flags_orphan_scf_yield_at_top_level() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("orphan_yield");
        f.push_op(MirOp::std("scf.yield").with_operand(ValueId(0)));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].code(), "CFG0002");
        if let CfgViolation::OrphanScfYield { fn_name, parent } = &violations[0] {
            assert_eq!(fn_name, "orphan_yield");
            assert_eq!(parent, "<top-level>");
        } else {
            panic!("expected OrphanScfYield");
        }
    }

    #[test]
    fn cfg0002_accepts_scf_yield_inside_scf_if_branch() {
        // scf.if with two branches, each terminating in scf.yield. This is
        // the C1 (T11-D58) canonical expression-form shape. D5 should
        // accept it.
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("good_if");
        let mut iff = scf_if_with_regions(2);
        for region in &mut iff.regions {
            if let Some(block) = region.entry_mut() {
                block.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
            }
        }
        f.push_op(iff);
        module.push_func(f);
        let result = validate_structured_cfg(&module);
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn cfg0002_accepts_scf_yield_inside_scf_loop_body() {
        // C2's deferred-growth shape : loops don't yield at stage-0 but D5
        // is forward-compat. yield inside loop body = no-op, no violation.
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("loop_with_yield");
        let mut lp = scf_loop_one_region("loop");
        if let Some(region) = lp.regions.first_mut() {
            if let Some(block) = region.entry_mut() {
                block.push(MirOp::std("scf.yield"));
            }
        }
        f.push_op(lp);
        module.push_func(f);
        assert!(validate_structured_cfg(&module).is_ok());
    }

    // ── CFG0003 — UnstructuredCondBr ─────────────────────────────────────

    #[test]
    fn cfg0003_flags_cf_cond_br() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("ugly");
        f.push_op(MirOp::std("cf.cond_br"));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0003");
    }

    // ── CFG0004 — UnstructuredBr ─────────────────────────────────────────

    #[test]
    fn cfg0004_flags_cf_br() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("uglier");
        f.push_op(MirOp::std("cf.br"));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0004");
    }

    // ── CFG0005 — ScfIfWrongRegionCount ──────────────────────────────────

    #[test]
    fn cfg0005_flags_scf_if_with_one_region() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("if_too_few");
        f.push_op(scf_if_with_regions(1));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0005");
        if let CfgViolation::ScfIfWrongRegionCount { actual, .. } = &violations[0] {
            assert_eq!(*actual, 1);
        } else {
            panic!("expected ScfIfWrongRegionCount");
        }
    }

    #[test]
    fn cfg0005_flags_scf_if_with_three_regions() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("if_too_many");
        f.push_op(scf_if_with_regions(3));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0005");
    }

    // ── CFG0006 — LoopWrongRegionCount ───────────────────────────────────

    #[test]
    fn cfg0006_flags_scf_for_with_no_regions() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("for_too_few");
        f.push_op(MirOp::std("scf.for"));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0006");
        if let CfgViolation::LoopWrongRegionCount {
            op_name, actual, ..
        } = &violations[0]
        {
            assert_eq!(op_name, "for");
            assert_eq!(*actual, 0);
        } else {
            panic!("expected LoopWrongRegionCount");
        }
    }

    #[test]
    fn cfg0006_flags_scf_while_with_two_regions() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("while_too_many");
        let mut op = MirOp::std("scf.while");
        op.regions.push(MirRegion::with_entry(Vec::new()));
        op.regions.push(MirRegion::with_entry(Vec::new()));
        f.push_op(op);
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0006");
        if let CfgViolation::LoopWrongRegionCount { op_name, .. } = &violations[0] {
            assert_eq!(op_name, "while");
        } else {
            panic!("expected LoopWrongRegionCount");
        }
    }

    #[test]
    fn cfg0006_flags_scf_loop_with_two_regions() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("loop_too_many");
        let mut op = MirOp::std("scf.loop");
        op.regions.push(MirRegion::with_entry(Vec::new()));
        op.regions.push(MirRegion::with_entry(Vec::new()));
        f.push_op(op);
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0006");
    }

    // ── CFG0007 — ScfRegionMultiBlock ────────────────────────────────────

    #[test]
    fn cfg0007_flags_multi_block_region_in_scf_if() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("if_multi_block");
        let mut op = MirOp::std("scf.if");
        // First region has TWO blocks — this is the violation.
        let mut first = MirRegion::new();
        first.push(MirBlock::entry(Vec::new()));
        first.push(MirBlock::new("extra"));
        op.regions.push(first);
        op.regions.push(MirRegion::with_entry(Vec::new()));
        f.push_op(op);
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        // Note : only CFG0007 should fire ; region-count is correct (2).
        assert!(violations.iter().any(|v| v.code() == "CFG0007"));
        let multi = violations
            .iter()
            .find(|v| v.code() == "CFG0007")
            .expect("expected CFG0007");
        if let CfgViolation::ScfRegionMultiBlock {
            block_count,
            op_name,
            ..
        } = multi
        {
            assert_eq!(*block_count, 2);
            assert_eq!(op_name, "if");
        } else {
            panic!("expected ScfRegionMultiBlock");
        }
    }

    #[test]
    fn cfg0007_flags_multi_block_region_in_scf_loop() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("loop_multi_block");
        let mut op = MirOp::std("scf.loop");
        let mut region = MirRegion::new();
        region.push(MirBlock::entry(Vec::new()));
        region.push(MirBlock::new("extra"));
        op.regions.push(region);
        f.push_op(op);
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert!(violations.iter().any(|v| v.code() == "CFG0007"));
    }

    // ── CFG0008 — OrphanScfCondition ─────────────────────────────────────

    #[test]
    fn cfg0008_flags_orphan_scf_condition() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("orphan_cond");
        f.push_op(MirOp::std("scf.condition").with_operand(ValueId(0)));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0008");
    }

    #[test]
    fn cfg0008_accepts_scf_condition_inside_scf_while() {
        // Future-compat : scf.while with cond-reeval will use scf.condition
        // as the region terminator. D5 accepts it inside scf.while parent.
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("while_with_cond");
        let mut wh = scf_loop_one_region("while");
        if let Some(region) = wh.regions.first_mut() {
            if let Some(block) = region.entry_mut() {
                block.push(MirOp::std("scf.condition").with_operand(ValueId(0)));
            }
        }
        f.push_op(wh);
        module.push_func(f);
        assert!(validate_structured_cfg(&module).is_ok());
    }

    // ── CFG0009 — UnsupportedBreak ───────────────────────────────────────

    #[test]
    fn cfg0009_flags_unsupported_break() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("brk");
        f.push_op(MirOp::std("cssl.unsupported(Break)"));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0009");
    }

    // ── CFG0010 — UnsupportedContinue ────────────────────────────────────

    #[test]
    fn cfg0010_flags_unsupported_continue() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("cont");
        f.push_op(MirOp::std("cssl.unsupported(Continue)"));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations[0].code(), "CFG0010");
    }

    // ── Marker contract ──────────────────────────────────────────────────

    #[test]
    fn validate_and_mark_writes_marker_on_success() {
        let mut module = MirModule::new();
        module.push_func(well_formed_i32_fn("ok"));
        assert!(!has_structured_cfg_marker(&module));
        validate_and_mark(&mut module).unwrap();
        assert!(has_structured_cfg_marker(&module));
    }

    #[test]
    fn validate_and_mark_skips_marker_on_failure() {
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("bad");
        f.push_op(MirOp::std("cf.br"));
        module.push_func(f);
        let result = validate_and_mark(&mut module);
        assert!(result.is_err());
        assert!(!has_structured_cfg_marker(&module));
    }

    #[test]
    fn validate_and_mark_idempotent_on_re_run() {
        let mut module = MirModule::new();
        module.push_func(well_formed_i32_fn("ok"));
        validate_and_mark(&mut module).unwrap();
        validate_and_mark(&mut module).unwrap();
        // Marker should be present exactly once.
        let count = module
            .attributes
            .iter()
            .filter(|(k, _)| k == STRUCTURED_CFG_VALIDATED_KEY)
            .count();
        assert_eq!(count, 1);
    }

    // ── Composition cases ────────────────────────────────────────────────

    #[test]
    fn well_formed_module_with_no_ops_passes() {
        let mut module = MirModule::new();
        module.push_func(well_formed_i32_fn("empty_body"));
        module.push_func(well_formed_i32_fn("also_empty"));
        assert!(validate_structured_cfg(&module).is_ok());
    }

    #[test]
    fn empty_module_passes() {
        let module = MirModule::new();
        assert!(validate_structured_cfg(&module).is_ok());
    }

    #[test]
    fn validator_collects_multiple_violations_per_fn() {
        // Two violations in one fn : a cf.br + a cf.cond_br. Validator
        // returns BOTH, not first-fail.
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("multi_violation");
        f.push_op(MirOp::std("cf.cond_br"));
        f.push_op(MirOp::std("cf.br"));
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations.len(), 2);
        assert!(violations.iter().any(|v| v.code() == "CFG0003"));
        assert!(violations.iter().any(|v| v.code() == "CFG0004"));
    }

    #[test]
    fn validator_collects_violations_across_multiple_fns() {
        let mut module = MirModule::new();
        let mut f1 = well_formed_i32_fn("f_with_br");
        f1.push_op(MirOp::std("cf.br"));
        let mut f2 = well_formed_i32_fn("f_with_cond_br");
        f2.push_op(MirOp::std("cf.cond_br"));
        module.push_func(f1);
        module.push_func(f2);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert_eq!(violations.len(), 2);
        assert!(violations.iter().any(|v| v.fn_name() == "f_with_br"));
        assert!(violations.iter().any(|v| v.fn_name() == "f_with_cond_br"));
    }

    #[test]
    fn nested_orphan_yield_inside_unstructured_op_is_flagged() {
        // A scf.yield nested inside a non-structured cssl.region.enter
        // would be orphan — the parent op-name doesn't qualify. Important
        // for closing future-loop holes : if a downstream pass introduces
        // a non-structured op that owns regions, yields inside it are
        // still rejected.
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("yield_in_region_enter");
        let mut region_enter = MirOp::std("cssl.region.enter");
        let mut inner = MirRegion::with_entry(Vec::new());
        if let Some(block) = inner.entry_mut() {
            block.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
        }
        region_enter.regions.push(inner);
        f.push_op(region_enter);
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert!(violations.iter().any(|v| v.code() == "CFG0002"));
        let orphan = violations
            .iter()
            .find(|v| v.code() == "CFG0002")
            .expect("expected CFG0002");
        if let CfgViolation::OrphanScfYield { parent, .. } = orphan {
            assert_eq!(parent, "cssl.region.enter");
        } else {
            panic!("expected OrphanScfYield");
        }
    }

    #[test]
    fn nested_scf_if_inside_scf_loop_body_is_well_formed() {
        // C2 documented this composition (`scf_loop_nested_inside_scf_if_then_branch`
        // test) ; the validator should also accept the inverse — scf.if
        // inside scf.loop body with proper yields.
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("nested_loop_if");
        let mut outer = scf_loop_one_region("loop");
        if let Some(outer_region) = outer.regions.first_mut() {
            if let Some(outer_block) = outer_region.entry_mut() {
                let mut inner_if = scf_if_with_regions(2);
                for r in &mut inner_if.regions {
                    if let Some(b) = r.entry_mut() {
                        b.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
                    }
                }
                outer_block.push(inner_if);
            }
        }
        f.push_op(outer);
        module.push_func(f);
        assert!(validate_structured_cfg(&module).is_ok());
    }

    // ── Error display ────────────────────────────────────────────────────

    #[test]
    fn cfg_violation_display_includes_code_and_actionable_text() {
        let v = CfgViolation::UnstructuredCondBr {
            fn_name: "x".to_string(),
        };
        let s = format!("{v}");
        assert!(s.contains("CFG0003"), "got: {s}");
        assert!(s.contains("`x`"), "got: {s}");
        assert!(s.contains("cf.cond_br"), "got: {s}");
    }

    #[test]
    fn cfg_violation_codes_are_unique_and_stable() {
        // Build one of each variant and assert codes are unique.
        let all = vec![
            CfgViolation::EmptyRegion {
                fn_name: "a".into(),
            },
            CfgViolation::OrphanScfYield {
                fn_name: "a".into(),
                parent: "p".into(),
            },
            CfgViolation::UnstructuredCondBr {
                fn_name: "a".into(),
            },
            CfgViolation::UnstructuredBr {
                fn_name: "a".into(),
            },
            CfgViolation::ScfIfWrongRegionCount {
                fn_name: "a".into(),
                actual: 0,
            },
            CfgViolation::LoopWrongRegionCount {
                fn_name: "a".into(),
                op_name: "for".into(),
                actual: 0,
            },
            CfgViolation::ScfRegionMultiBlock {
                fn_name: "a".into(),
                op_name: "if".into(),
                block_count: 0,
            },
            CfgViolation::OrphanScfCondition {
                fn_name: "a".into(),
                parent: "p".into(),
            },
            CfgViolation::UnsupportedBreak {
                fn_name: "a".into(),
            },
            CfgViolation::UnsupportedContinue {
                fn_name: "a".into(),
            },
        ];
        let codes: Vec<&str> = all.iter().map(CfgViolation::code).collect();
        let unique: std::collections::HashSet<&&str> = codes.iter().collect();
        assert_eq!(codes.len(), unique.len(), "duplicate codes : {codes:?}");
        // All codes must start with "CFG" + 4 digits.
        for c in &codes {
            assert!(c.starts_with("CFG"), "non-CFG code : {c}");
            assert_eq!(c.len(), 7, "wrong format : {c}");
        }
    }

    #[test]
    fn validator_recurses_into_arbitrarily_nested_regions() {
        // scf.if > scf.if > scf.if with a cf.br at the deepest leaf.
        // Validator should still find it.
        let mut module = MirModule::new();
        let mut f = well_formed_i32_fn("deep");
        let mut depth_3 = scf_if_with_regions(2);
        if let Some(r) = depth_3.regions.first_mut() {
            if let Some(b) = r.entry_mut() {
                b.push(MirOp::std("cf.br")); // ← the violation, 3 levels deep
                b.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
            }
        }
        if let Some(r) = depth_3.regions.get_mut(1) {
            if let Some(b) = r.entry_mut() {
                b.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
            }
        }
        let mut depth_2 = scf_if_with_regions(2);
        if let Some(r) = depth_2.regions.first_mut() {
            if let Some(b) = r.entry_mut() {
                b.push(depth_3);
                b.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
            }
        }
        if let Some(r) = depth_2.regions.get_mut(1) {
            if let Some(b) = r.entry_mut() {
                b.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
            }
        }
        let mut depth_1 = scf_if_with_regions(2);
        if let Some(r) = depth_1.regions.first_mut() {
            if let Some(b) = r.entry_mut() {
                b.push(depth_2);
                b.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
            }
        }
        if let Some(r) = depth_1.regions.get_mut(1) {
            if let Some(b) = r.entry_mut() {
                b.push(MirOp::std("scf.yield").with_operand(ValueId(0)));
            }
        }
        f.push_op(depth_1);
        module.push_func(f);
        let violations = validate_structured_cfg(&module).unwrap_err();
        assert!(violations.iter().any(|v| v.code() == "CFG0004"));
    }
}
