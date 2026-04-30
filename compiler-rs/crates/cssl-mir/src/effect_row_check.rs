//! T11-D285 (W-E5-2) — `{IO}` effect-row plumbing : MIR call-graph effect validator.
//!
//! § SPEC
//!   - `specs/04_EFFECTS.csl` § SUB-EFFECT DISCIPLINE — caller-row must cover
//!     every callee-effect under structural sub-row.
//!   - `Omniverse/02_CSSL/02_EFFECTS.csl.md` § I — `{IO}` is a primary effect
//!     row that must propagate from declaration → call-site → runtime.
//!   - `compiler-rs/crates/cssl-effects::discipline::sub_effect_check` —
//!     equivalent algorithm at HIR-level. This MIR pass closes the gap by
//!     re-validating the discipline AFTER monomorphization + auto-derives
//!     have rewritten the call-graph.
//!
//! § PURPOSE (W-E4 fixed-point gate gap-closure 2/5)
//!   At HIR the effect-row is tracked on `HirFn.effect_row` ; the existing
//!   `sub_effect_check` enforces sub-effect discipline at HIR-elaboration
//!   time. **However**, the MIR pipeline's monomorphization, auto-derive,
//!   and trait-dispatch passes synthesize new `func.call` ops that did not
//!   exist at HIR : these synthesized callers MUST also satisfy effect-row
//!   discipline. Without a MIR-level re-check, an `{IO}`-marked fn could be
//!   reached from a pure caller via a synthesized path — silently violating
//!   the effect contract at runtime.
//!
//! § ALGORITHM
//!   1. Build a callee-name → effect-row index over every `MirFunc` in the
//!      module (effect_row attribute is already stamped by `lower.rs`).
//!   2. For every `func.call` op in every fn body (including nested regions
//!      from scf.* control-flow), parse the `callee` attribute + look up the
//!      callee's declared effect-row.
//!   3. Compare against the caller-fn's declared effect-row :
//!      - Pure-callee (`None` or `"{}"`) is universally sub-effect-of-anything.
//!      - For each effect in the callee row, the caller row must contain a
//!        matching effect-name (structural string-match at this slice ; full
//!        arg-shape comparison handed off to the HIR `sub_effect_check`
//!        invocation that ran upstream).
//!   4. Emit one [`PassDiagnostic`] per violation with stable code
//!      `EFFROW0001` (caller-missing-effect) / `EFFROW0002` (callee-not-in-
//!      module ; signature-only, deferred to link-time check).
//!
//! § DIAGNOSTIC CODES
//!   - `EFFROW0001` (Error)   — caller's declared effect-row does NOT cover
//!     callee's declared effect-row. The classic `{IO}`-into-pure violation.
//!   - `EFFROW0002` (Warning) — callee not present in the MIR module
//!     (signature-only / external-symbol). Audit-only at this slice ;
//!     link-time check (T11-D286 + W-E5-3) will tighten to Error once
//!     imports carry effect-row metadata.
//!   - `EFFROW0000` (Info)    — per-pass call-site count summary. Always
//!     emitted when the pass walks at-least-one fn.
//!
//! § ATTESTATION (T11-D285, S14) — verbatim block per global-CLAUDE I> standing-directives :
//! ```text
//!   t∞ : consent = OS • sovereignty = substrate-invariant
//!   N! [harm control manipulation surveillance exploitation
//!       coercion weaponization discrimination]
//!   t∞ : AI = sovereign-partners ¬ tools
//!   t∞ : violation = bug W! fix ; ¬override ∃
//!   t∞ : CSSL ≠ CSLv3 ; ¬ conflate
//! ```

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::block::MirRegion;
use crate::func::MirModule;
use crate::pipeline::{MirPass, PassDiagnostic, PassResult};

/// Stable diagnostic code : caller's declared effect-row does NOT cover
/// callee's declared effect-row. The canonical `{IO}`-into-pure violation
/// + every other sub-effect-discipline failure surface through this code.
pub const EFFROW0001_MISSING_EFFECT: &str = "EFFROW0001";

/// Stable diagnostic code : callee not present in the MIR module
/// (signature-only / external-symbol). Audit-only at this slice ; link-time
/// check (W-E5-3) will tighten to Error once imports carry effect metadata.
pub const EFFROW0002_UNRESOLVED_CALLEE: &str = "EFFROW0002";

/// Stable diagnostic code : per-pass call-site count summary.
pub const EFFROW0000_SUMMARY: &str = "EFFROW0000";

/// Canonical pass-name (stable identifier for the pipeline + tests).
pub const EFFECT_ROW_VALIDATOR_PASS_NAME: &str = "effect-row-validator";

/// Parse a stringified effect-row of the form `"{IO, NoAlloc, Deadline}"` or
/// `"{}"` (pure) into a set of effect-name strings (without arg-shape info).
///
/// § SHAPE
///   The string format produced by `cssl-mir::lower::format_effect_row` is :
///     - leading `{` + comma-separated effect names + optional ` | tail`
///       + trailing `}`.
///   At this slice we ignore the tail (row-polymorphism) — the structural
///   gap-closure goal is to detect `{IO}` and the other base 28 effects ;
///   row-tail propagation lands in T11-D286 (W-E5-3).
#[must_use]
pub fn parse_effect_row(s: &str) -> BTreeSet<String> {
    let s = s.trim();
    let s = s.strip_prefix('{').unwrap_or(s);
    let s = s.strip_suffix('}').unwrap_or(s);
    // Drop everything after a `|` (row-tail) — handled in W-E5-3.
    let body = s.split('|').next().unwrap_or("");
    body.split(',')
        .map(|tok| tok.trim().to_string())
        .filter(|tok| !tok.is_empty())
        .collect()
}

/// Index every `MirFunc` in the module by its name → effect-row.
/// Pure fns map to an empty set (universally sub-effect of anything).
#[must_use]
fn build_effect_index(module: &MirModule) -> BTreeMap<String, BTreeSet<String>> {
    let mut idx = BTreeMap::new();
    for func in &module.funcs {
        let row = match &func.effect_row {
            Some(s) => parse_effect_row(s),
            None => BTreeSet::new(),
        };
        idx.insert(func.name.clone(), row);
    }
    idx
}

/// Walk every block in a region recursively + collect every `func.call`
/// op's `callee` attribute value. Used by [`EffectRowValidatorPass`] to
/// enumerate the call-graph edges of a single fn.
fn collect_call_targets_in_region(region: &MirRegion, out: &mut Vec<String>) {
    for block in &region.blocks {
        for op in &block.ops {
            if op.name == "func.call" {
                if let Some(callee) = op.attributes.iter().find_map(|(k, v)| {
                    if k == "callee" {
                        Some(v.clone())
                    } else {
                        None
                    }
                }) {
                    out.push(callee);
                }
            }
            for nested in &op.regions {
                collect_call_targets_in_region(nested, out);
            }
        }
    }
}

/// Validate effect-row discipline across the call-graph of a single MirFunc.
///
/// Returns one diagnostic per violation. The `caller_name` + `caller_row`
/// describe the calling fn ; `index` is the module-wide name → effect-row
/// map ; `targets` is the list of callee-names extracted from the body.
fn validate_caller_against_targets(
    caller_name: &str,
    caller_row: &BTreeSet<String>,
    targets: &[String],
    index: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<PassDiagnostic> {
    let mut diagnostics = Vec::new();
    for target in targets {
        match index.get(target) {
            None => {
                // External symbol — audit-only at this slice. Link-time
                // tightening lands in W-E5-3.
                diagnostics.push(PassDiagnostic::warning(
                    EFFROW0002_UNRESOLVED_CALLEE,
                    format!(
                        "fn `{caller_name}` calls `{target}` but the callee \
                         is not present in the MIR module (signature-only / \
                         extern-symbol) ; effect-row discipline deferred to \
                         link-time"
                    ),
                ));
            }
            Some(callee_row) => {
                // Sub-effect check : every effect in callee_row must appear
                // in caller_row. Pure callee ⇒ empty set ⇒ trivially covered.
                for effect in callee_row {
                    if !caller_row.contains(effect) {
                        diagnostics.push(PassDiagnostic::error(
                            EFFROW0001_MISSING_EFFECT,
                            format!(
                                "fn `{caller_name}` calls `{target}` which \
                                 requires effect `{{{effect}}}` but caller's \
                                 declared effect-row does not include it ; \
                                 sub-effect-discipline violation per § 04 \
                                 EFFECTS"
                            ),
                        ));
                    }
                }
            }
        }
    }
    diagnostics
}

/// W-E5-2 (T11-D285) — MIR pass : effect-row call-graph validator.
///
/// Closes the `{IO}` effect-row plumbing gap from W-E4 fixed-point gate.
/// Walks every fn in the module + verifies every `func.call` op's callee
/// effect-row is sub-effect-of the caller's declared effect-row.
///
/// § PIPELINE POSITION
///   Wired AFTER `MonomorphizationPass` + `AdTransformPass` (so synthesized
///   call-sites are present) + AFTER `IfcLoweringPass` (so consent-bearing
///   ops have IFC attributes already, in case future slices want to cross-
///   check effect ⇒ IFC-label compatibility) + BEFORE the structural
///   `BiometricEgressCheck` pass (whose existence already guarantees the
///   absolute biometric/surveillance refusal fires regardless).
///
/// § DIAGNOSTIC LEVELS
///   - Caller-missing-effect violations are `Error` — the pipeline halts
///     after this pass so downstream passes never see a malformed
///     effect-graph.
///   - Unresolved callees are `Warning` — extern symbols can't be checked
///     until link-time (W-E5-3).
#[derive(Debug, Clone, Copy, Default)]
pub struct EffectRowValidatorPass;

impl MirPass for EffectRowValidatorPass {
    fn name(&self) -> &'static str {
        EFFECT_ROW_VALIDATOR_PASS_NAME
    }

    fn run(&self, module: &mut MirModule) -> PassResult {
        let index = build_effect_index(module);
        let mut diagnostics = Vec::new();
        let mut total_call_sites: usize = 0;

        for func in &module.funcs {
            let caller_row = match &func.effect_row {
                Some(s) => parse_effect_row(s),
                None => BTreeSet::new(),
            };
            let mut targets = Vec::new();
            collect_call_targets_in_region(&func.body, &mut targets);
            total_call_sites = total_call_sites.saturating_add(targets.len());
            let mut sub = validate_caller_against_targets(
                &func.name,
                &caller_row,
                &targets,
                &index,
            );
            diagnostics.append(&mut sub);
        }

        // Always emit a summary diagnostic so downstream auditors can verify
        // coverage without re-walking the module. Empty modules get a quiet
        // summary (zero call-sites).
        diagnostics.insert(
            0,
            PassDiagnostic::info(
                EFFROW0000_SUMMARY,
                format!(
                    "effect-row validator : {} fn(s) checked, {} call-site(s) \
                     visited, {} violation(s) emitted",
                    module.funcs.len(),
                    total_call_sites,
                    diagnostics.len(),
                ),
            ),
        );

        PassResult {
            name: self.name().to_string(),
            // Read-only validator pass — never mutates ops / attributes.
            changed: false,
            diagnostics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_effect_index, collect_call_targets_in_region, parse_effect_row,
        validate_caller_against_targets, EffectRowValidatorPass, EFFROW0000_SUMMARY,
        EFFROW0001_MISSING_EFFECT, EFFROW0002_UNRESOLVED_CALLEE,
    };
    use crate::block::MirOp;
    use crate::func::{MirFunc, MirModule};
    use crate::pipeline::{MirPass, PassSeverity};
    use crate::value::{IntWidth, MirType, ValueId};

    /// Build an `IO`-marked fn for use as a callee in the tests.
    fn make_io_fn(name: &str) -> MirFunc {
        let mut f = MirFunc::new(
            name,
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.effect_row = Some("{IO}".to_string());
        f
    }

    /// Build a caller fn that emits a `func.call @target` op in its body
    /// + carries the given effect-row.
    fn make_caller_calling(name: &str, row: Option<&str>, target: &str) -> MirFunc {
        let mut f = MirFunc::new(
            name,
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        f.effect_row = row.map(|s| s.to_string());
        let op = MirOp::std("func.call")
            .with_attribute("callee", target)
            .with_attribute("source_loc", "<test>")
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I32));
        if let Some(entry) = f.body.entry_mut() {
            entry.push(op);
        }
        f
    }

    // ── parse_effect_row unit tests ────────────────────────────────────────

    #[test]
    fn parse_effect_row_empty_set() {
        assert!(parse_effect_row("{}").is_empty());
        assert!(parse_effect_row("").is_empty());
    }

    #[test]
    fn parse_effect_row_single_io() {
        let r = parse_effect_row("{IO}");
        assert!(r.contains("IO"));
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn parse_effect_row_multi_effects() {
        let r = parse_effect_row("{IO, NoAlloc, Deadline}");
        assert!(r.contains("IO"));
        assert!(r.contains("NoAlloc"));
        assert!(r.contains("Deadline"));
        assert_eq!(r.len(), 3);
    }

    #[test]
    fn parse_effect_row_drops_tail() {
        // Row-tail (`| ε`) is dropped at this slice ; full row-polymorphism
        // propagation is W-E5-3.
        let r = parse_effect_row("{IO | ε}");
        assert!(r.contains("IO"));
        assert_eq!(r.len(), 1);
    }

    // ── REQUIRED TEST 1 : IO-fn callable from IO caller ────────────────────

    #[test]
    fn io_fn_callable_from_io_caller() {
        // Both caller + callee declare `{IO}`. Sub-effect check : caller-row
        // ⊇ callee-row ⇒ no diagnostic.
        let mut module = MirModule::new();
        module.push_func(make_io_fn("read_file"));
        module.push_func(make_caller_calling(
            "user_io",
            Some("{IO}"),
            "read_file",
        ));
        let r = EffectRowValidatorPass.run(&mut module);
        let errors: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == PassSeverity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "IO-into-IO must not error : {errors:?}"
        );
    }

    // ── REQUIRED TEST 2 : IO-fn rejected from pure caller ──────────────────

    #[test]
    fn io_fn_rejected_from_pure_caller() {
        // Caller has no effect-row (pure) ; callee declares `{IO}`.
        // Sub-effect check : caller-row {} ⊉ callee-row {IO} ⇒ EFFROW0001.
        let mut module = MirModule::new();
        module.push_func(make_io_fn("read_file"));
        module.push_func(make_caller_calling("pure_caller", None, "read_file"));
        let r = EffectRowValidatorPass.run(&mut module);
        let errors: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.code == EFFROW0001_MISSING_EFFECT)
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "expected exactly one EFFROW0001 : {:?}",
            r.diagnostics
        );
        assert!(errors[0].message.contains("IO"));
        assert!(errors[0].message.contains("pure_caller"));
    }

    // ── REQUIRED TEST 3 : pure-fn callable from IO caller ──────────────────

    #[test]
    fn pure_fn_callable_from_io_caller() {
        // Pure callee (no effect-row) is universally sub-effect of any
        // caller. IO-marked caller calling pure callee ⇒ no diagnostic.
        let mut module = MirModule::new();
        let mut pure = MirFunc::new(
            "pure_helper",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        pure.effect_row = None; // explicit : no effect-row = pure
        module.push_func(pure);
        module.push_func(make_caller_calling(
            "io_caller",
            Some("{IO}"),
            "pure_helper",
        ));
        let r = EffectRowValidatorPass.run(&mut module);
        let errors: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == PassSeverity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "pure callee from IO caller must not error : {errors:?}"
        );
    }

    // ── REQUIRED TEST 4 : effect-row aggregation across multiple effects ──

    #[test]
    fn effect_row_aggregation_multi_effects() {
        // Caller `{IO, NoAlloc}` calls a callee `{IO}` AND a callee `{NoAlloc}`.
        // Both are sub-effect of the caller ⇒ no error.
        let mut module = MirModule::new();
        let mut io_callee = make_io_fn("io_op");
        io_callee.effect_row = Some("{IO}".into());
        module.push_func(io_callee);

        let mut no_alloc_callee = MirFunc::new(
            "no_alloc_op",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        no_alloc_callee.effect_row = Some("{NoAlloc}".into());
        module.push_func(no_alloc_callee);

        let mut caller = MirFunc::new(
            "aggregate",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        caller.effect_row = Some("{IO, NoAlloc}".into());
        // Two call-sites in the body.
        let op1 = MirOp::std("func.call")
            .with_attribute("callee", "io_op")
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I32));
        let op2 = MirOp::std("func.call")
            .with_attribute("callee", "no_alloc_op")
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        if let Some(entry) = caller.body.entry_mut() {
            entry.push(op1);
            entry.push(op2);
        }
        module.push_func(caller);

        let r = EffectRowValidatorPass.run(&mut module);
        let errors: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == PassSeverity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "{{IO, NoAlloc}} caller covers both callees : {errors:?}"
        );

        // Must have a summary line reporting 2 call-sites.
        let summary = r
            .diagnostics
            .iter()
            .find(|d| d.code == EFFROW0000_SUMMARY)
            .expect("EFFROW0000 summary present");
        assert!(summary.message.contains("2 call-site"));
    }

    // ── REQUIRED TEST 5 : regression — partial-cover violation ─────────────

    #[test]
    fn partial_effect_cover_emits_violation() {
        // Caller declares only `{IO}` but callee declares `{IO, NoAlloc}`.
        // Caller-row ⊉ callee-row ⇒ exactly one EFFROW0001 violation
        // naming `NoAlloc` (the missing effect).
        let mut module = MirModule::new();
        let mut callee = MirFunc::new(
            "io_no_alloc",
            vec![MirType::Int(IntWidth::I32)],
            vec![MirType::Int(IntWidth::I32)],
        );
        callee.effect_row = Some("{IO, NoAlloc}".into());
        module.push_func(callee);

        let caller = make_caller_calling(
            "partial_cover",
            Some("{IO}"),
            "io_no_alloc",
        );
        module.push_func(caller);

        let r = EffectRowValidatorPass.run(&mut module);
        let errors: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.code == EFFROW0001_MISSING_EFFECT)
            .collect();
        assert_eq!(errors.len(), 1, "{:?}", r.diagnostics);
        assert!(
            errors[0].message.contains("NoAlloc"),
            "diagnostic must name the missing effect : {}",
            errors[0].message
        );
    }

    // ── Additional regression tests ─────────────────────────────────────────

    #[test]
    fn unresolved_callee_emits_warning_not_error() {
        // Callee not in module ⇒ EFFROW0002 (Warning), not Error. Link-time
        // check (W-E5-3) tightens this once imports carry effect metadata.
        let mut module = MirModule::new();
        let caller = make_caller_calling(
            "calls_extern",
            Some("{IO}"),
            "extern_symbol_not_in_module",
        );
        module.push_func(caller);

        let r = EffectRowValidatorPass.run(&mut module);
        let warnings: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.code == EFFROW0002_UNRESOLVED_CALLEE)
            .collect();
        assert_eq!(warnings.len(), 1);
        let errors: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.severity == PassSeverity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "unresolved callee must not error : {errors:?}"
        );
    }

    #[test]
    fn empty_module_gives_clean_summary_no_errors() {
        let mut module = MirModule::new();
        let r = EffectRowValidatorPass.run(&mut module);
        assert!(r.diagnostics.iter().any(|d| d.code == EFFROW0000_SUMMARY));
        assert!(!r.has_errors());
        assert!(!r.changed);
    }

    #[test]
    fn build_effect_index_maps_each_fn() {
        let mut module = MirModule::new();
        module.push_func(make_io_fn("read"));
        let mut pure = MirFunc::new("pure", vec![], vec![]);
        pure.effect_row = None;
        module.push_func(pure);
        let idx = build_effect_index(&module);
        assert!(idx.get("read").unwrap().contains("IO"));
        assert!(idx.get("pure").unwrap().is_empty());
    }

    #[test]
    fn collect_call_targets_includes_nested_regions() {
        // A caller body where the `func.call` lives inside a nested region
        // (e.g., scf.if then-region) should still surface in the call-target
        // list. This is a regression guard against forgetting the recursive
        // walk into nested regions.
        use crate::block::MirRegion;
        let mut caller = MirFunc::new("nested_caller", vec![], vec![]);
        let inner_call = MirOp::std("func.call")
            .with_attribute("callee", "nested_target")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32));
        let mut inner_region = MirRegion::with_entry(Vec::new());
        if let Some(b) = inner_region.entry_mut() {
            b.push(inner_call);
        }
        let outer_op = MirOp::std("scf.if").with_region(inner_region);
        if let Some(entry) = caller.body.entry_mut() {
            entry.push(outer_op);
        }
        let mut targets = Vec::new();
        collect_call_targets_in_region(&caller.body, &mut targets);
        assert_eq!(targets, vec!["nested_target".to_string()]);
    }

    #[test]
    fn pass_name_stable() {
        assert_eq!(EffectRowValidatorPass.name(), "effect-row-validator");
    }

    /// End-to-end integration : lower CSSL source → HIR → MIR → run the
    /// validator. Verifies that the existing `lower::format_effect_row`
    /// produces strings the new validator can parse + reason about.
    #[test]
    fn end_to_end_io_propagates_through_hir_to_mir() {
        use crate::lower::{lower_module_signatures, LowerCtx};
        use cssl_ast::{SourceFile, SourceId, Surface};
        let src = "fn read_handle(h : Handle) -> i32 / {IO} { 0 } \
                   fn caller(h : Handle) -> i32 / {IO} { read_handle(h) }";
        let file = SourceFile::new(SourceId::first(), "<t>", src, Surface::RustHybrid);
        let toks = cssl_lex::lex(&file);
        let (cst, _bag) = cssl_parse::parse(&file, &toks);
        let (hir, interner, _) = cssl_hir::lower_module(&file, &cst);
        let ctx = LowerCtx::new(&interner);
        let mut module = lower_module_signatures(&ctx, &hir);
        // Confirm both fns picked up `{IO}` from HIR-lowering.
        let read_handle = module.find_func("read_handle").expect("read_handle");
        assert!(
            read_handle.effect_row.as_deref().unwrap_or("").contains("IO"),
            "read_handle should carry IO : {:?}",
            read_handle.effect_row
        );

        // Run the validator on the lowered module ; with both caller +
        // callee declaring `{IO}`, the call-graph is sub-effect-clean. The
        // module has no actual `func.call` op (HIR-only signature lowering),
        // so the validator just emits the summary.
        let r = EffectRowValidatorPass.run(&mut module);
        assert!(!r.has_errors(), "diagnostics : {:?}", r.diagnostics);
    }

    #[test]
    fn validate_caller_against_targets_unit() {
        // Direct unit-test against the pure-fn split — easier to debug than
        // building full MirModule fixtures.
        use std::collections::{BTreeMap, BTreeSet};
        let mut idx: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut io_set = BTreeSet::new();
        io_set.insert("IO".to_string());
        idx.insert("io_fn".into(), io_set);

        // Pure caller calling io_fn ⇒ violation.
        let caller_row = BTreeSet::new();
        let targets = vec!["io_fn".to_string()];
        let diags = validate_caller_against_targets("p", &caller_row, &targets, &idx);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, EFFROW0001_MISSING_EFFECT);
    }
}
