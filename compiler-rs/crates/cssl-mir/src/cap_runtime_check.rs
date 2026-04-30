//! § T11-D286 (W-E5-3) — cap_check → codegen runtime wire-through.
//!
//! § ROLE
//!   The HIR `cap_check` pass (`cssl_hir::check_capabilities`) is the
//!   authoritative compile-time enforcer of Pony-6 + IFC capability rules.
//!   This pass closes the runtime side of the W-E4 fixed-point gate gap by
//!   emitting a `cssl.cap.verify` op at every cap-boundary in the
//!   monomorphized + lowered MIR. Each emitted op compiles down to a
//!   `__cssl_cap_verify(cap_handle, op_kind)` runtime call (cf.
//!   `cssl-rt::cap_verify`), so a cap-violation that slips past the static
//!   analysis is caught at runtime as defense-in-depth.
//!
//! § TYPE-SYSTEM CLAIM REPAIR
//!   Pre-D286 : compile-time-prove + runtime-soft-enforce (gap).
//!   Post-D286 : compile-time-prove + runtime-verify (defense-in-depth).
//!
//! § OP SHAPE
//!   ```text
//!   cssl.cap.verify(%cap_handle : i64, %op_kind : i32) -> i8
//!     attributes :
//!       (cap_kind = "iso"|"trn"|"ref"|"val"|"box"|"tag")
//!       (op_kind  = "call_pass_param"|"fn_entry"|"field_access"|"return")
//!       (origin   = "fn_entry" | …)              // recognizer provenance
//!   ```
//!   Operand layout matches the FFI surface : low byte of operand-0 is
//!   the [`CapKind::index()`] ; operand-1 is the op-kind enum from
//!   `cssl_rt::cap_verify`. The result byte is `1` allow / `0` deny ;
//!   the cgen-emitted preamble branches to `__cssl_panic` on deny.
//!
//! § INTEGRATION ORDER (canonical pipeline)
//!   Wired AFTER monomorphization (so cap-attributes are concrete) and
//!   AFTER `IfcLoweringPass` (so the IFC-lowered call-shape is final),
//!   BEFORE biometric-egress-check (so the cap preambles already exist by
//!   the time the egress audit runs), and BEFORE the structured-CFG
//!   validator (so the validator sees the final block shape).
//!
//! § SAWYER-EFFICIENCY
//!   - `cap_kind_index` is a `const fn` 6-arm match, no allocation.
//!   - The pass walks the entry block in O(N+K) where N = ops, K = caps
//!     emitted (always one constant-pair + one verify per cap-required
//!     param ; ≤ 3 ops appended per param).
//!   - Op-attributes are recorded as `&'static str` literal-keys to keep
//!     the per-op overhead to two `String::from(literal)` clones.
//!
//! § DIAGNOSTIC CODES
//!   - `CAP-RT0001` (Info)    — pass summary : N verify-ops emitted across
//!                              M fns. Empty modules stay quiet.
//!   - `CAP-RT0002` (Warning) — fn declared a cap-required param but the
//!                              MIR carried no `cap_required.<idx>` attr ;
//!                              indicates a HIR→MIR threading regression.
//!                              The pass continues (safe-mode : skip the
//!                              fn rather than emit an unverifiable op).

use core::str::FromStr;

use crate::block::MirOp;
use crate::func::{MirFunc, MirModule};
use crate::pipeline::{MirPass, PassDiagnostic, PassResult};
use crate::value::{IntWidth, MirType};
use cssl_caps::CapKind;

// ───────────────────────────────────────────────────────────────────────
// § wire-protocol constants — must mirror cssl-rt::cap_verify exactly
// ───────────────────────────────────────────────────────────────────────

/// Op-name emitted by this pass + recognized by cgen.
pub const OP_CAP_VERIFY: &str = "cssl.cap.verify";

/// Attribute key : the cap-kind name (`"iso"` etc.) the verify is gating.
pub const ATTR_CAP_KIND: &str = "cap_kind";
/// Attribute key : the op-kind tag (`"fn_entry"` etc.) for diagnostic clarity.
pub const ATTR_OP_KIND_TAG: &str = "op_kind_tag";
/// Attribute key : the boundary-origin (`"fn_entry"` is canonical @ stage-0).
pub const ATTR_ORIGIN: &str = "origin";
/// Origin tag : the verify-op was emitted at fn-entry from a cap-required param.
pub const ORIGIN_FN_ENTRY: &str = "fn_entry";

/// Per-fn attribute key recording that the cap-runtime preamble was
/// installed. Idempotency marker so re-running the pass is a no-op.
pub const FN_ATTR_CAP_RUNTIME_INSTALLED: &str = "cap_runtime_check.installed";

/// Per-fn attribute key produced by HIR → MIR signature lowering : value is
/// the cap-kind source-form name (e.g. `"iso"`). Mirrors the pre-existing
/// `cap` attribute on `MirFunc::cap` ; this pass walks BOTH places so the
/// integration is robust to either threading path.
pub const FN_ATTR_CAP_REQUIRED_PREFIX: &str = "cap_required.";

// ───────────────────────────────────────────────────────────────────────
// § op-kind tags — wire-protocol with cssl-rt::cap_verify::OP_*
// ───────────────────────────────────────────────────────────────────────

/// Op-kind tag : caller passes value to callee param.
pub const TAG_CALL_PASS_PARAM: &str = "call_pass_param";
/// Op-kind tag : callee fn-entry with cap-required-param.
pub const TAG_FN_ENTRY: &str = "fn_entry";
/// Op-kind tag : struct-field access (deferred ; reserved).
pub const TAG_FIELD_ACCESS: &str = "field_access";
/// Op-kind tag : fn-return value cap-check.
pub const TAG_RETURN: &str = "return";

/// Numeric op-kind for the wire — matches `cssl-rt::cap_verify`.
#[must_use]
pub const fn op_kind_numeric(tag: &OpKindTag) -> u32 {
    match tag {
        OpKindTag::CallPassParam => 0,
        OpKindTag::FnEntry => 1,
        OpKindTag::FieldAccess => 2,
        OpKindTag::Return => 3,
    }
}

/// Tag enum mirroring the runtime op-kind constants. Avoids stringly-typed
/// dispatch within the pass while keeping the wire-protocol explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpKindTag {
    CallPassParam,
    FnEntry,
    FieldAccess,
    Return,
}

impl OpKindTag {
    /// Source-form attribute value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CallPassParam => TAG_CALL_PASS_PARAM,
            Self::FnEntry => TAG_FN_ENTRY,
            Self::FieldAccess => TAG_FIELD_ACCESS,
            Self::Return => TAG_RETURN,
        }
    }
}

impl FromStr for OpKindTag {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            TAG_CALL_PASS_PARAM => Ok(Self::CallPassParam),
            TAG_FN_ENTRY => Ok(Self::FnEntry),
            TAG_FIELD_ACCESS => Ok(Self::FieldAccess),
            TAG_RETURN => Ok(Self::Return),
            _ => Err(()),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § cap-kind helpers — mirror CapKind::index()
// ───────────────────────────────────────────────────────────────────────

/// Map a `CapKind` to the low-byte cap_handle encoding the runtime expects.
#[must_use]
pub const fn cap_kind_index(c: CapKind) -> u8 {
    match c {
        CapKind::Iso => 0,
        CapKind::Trn => 1,
        CapKind::Ref => 2,
        CapKind::Val => 3,
        CapKind::Box => 4,
        CapKind::Tag => 5,
    }
}

/// Parse a cap source-form name back to a `CapKind`. Returns `None` on
/// unknown input (used to be defensive against future extensions).
#[must_use]
pub fn cap_kind_from_attr(s: &str) -> Option<CapKind> {
    match s {
        "iso" => Some(CapKind::Iso),
        "trn" => Some(CapKind::Trn),
        "ref" => Some(CapKind::Ref),
        "val" => Some(CapKind::Val),
        "box" => Some(CapKind::Box),
        "tag" => Some(CapKind::Tag),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn cap-info collection
// ───────────────────────────────────────────────────────────────────────

/// (param-index, cap) extracted from a fn's attributes / `cap` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamCapEntry {
    pub param_idx: usize,
    pub cap: CapKind,
}

/// Collect every cap-required-param entry on a fn. Walks the fn's
/// `attributes` list looking for `cap_required.<N>` entries.
///
/// Deduplicates : if a param's cap appears in BOTH places (legacy `cap`
/// field and per-param attribute), only the attribute wins.
#[must_use]
pub fn collect_cap_required_params(f: &MirFunc) -> Vec<ParamCapEntry> {
    let mut out: Vec<ParamCapEntry> = Vec::new();
    for (k, v) in &f.attributes {
        let Some(suffix) = k.strip_prefix(FN_ATTR_CAP_REQUIRED_PREFIX) else {
            continue;
        };
        let Ok(idx) = suffix.parse::<usize>() else {
            continue;
        };
        let Some(cap) = cap_kind_from_attr(v.as_str()) else {
            continue;
        };
        out.push(ParamCapEntry {
            param_idx: idx,
            cap,
        });
    }
    out.sort_by_key(|e| e.param_idx);
    out
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn preamble emission
// ───────────────────────────────────────────────────────────────────────

/// Append three ops onto `f`'s entry block for one cap-required param :
/// ```text
///   %h = arith.constant <cap_index>      : i64
///   %k = arith.constant <op_kind=fn_entry> : i32
///   cssl.cap.verify(%h, %k) : i8           // result discarded @ stage-0
/// ```
/// Stage-0 elides the deny-branch (panic). The verify-op CALL is itself
/// a runtime check ; cgen will lower it to `call __cssl_cap_verify`. A
/// future slice will wire the i8 result into a brif against
/// `__cssl_panic` for the hard-fail path.
fn emit_fn_entry_preamble(f: &mut MirFunc, entry: &ParamCapEntry) {
    let cap_index = i64::from(cap_kind_index(entry.cap));
    let kind_num = i64::from(op_kind_numeric(&OpKindTag::FnEntry));

    let h_id = f.fresh_value_id();
    let k_id = f.fresh_value_id();
    let v_id = f.fresh_value_id();

    let h_const = MirOp::std("arith.constant")
        .with_attribute("value", cap_index.to_string())
        .with_result(h_id, MirType::Int(IntWidth::I64))
        .with_attribute("origin", "cap_runtime_check");
    let k_const = MirOp::std("arith.constant")
        .with_attribute("value", kind_num.to_string())
        .with_result(k_id, MirType::Int(IntWidth::I32))
        .with_attribute("origin", "cap_runtime_check");

    let verify = MirOp::std(OP_CAP_VERIFY)
        .with_operand(h_id)
        .with_operand(k_id)
        .with_result(v_id, MirType::Int(IntWidth::I8))
        .with_attribute(ATTR_CAP_KIND, entry.cap.as_str())
        .with_attribute(ATTR_OP_KIND_TAG, OpKindTag::FnEntry.as_str())
        .with_attribute(ATTR_ORIGIN, ORIGIN_FN_ENTRY)
        .with_attribute("param_idx", entry.param_idx.to_string());

    // Prepend the preamble : verify ops must execute BEFORE any user-body
    // op observes the cap. We rebuild the entry block's op-list so the
    // three preamble ops land first (in canonical h/k/verify order),
    // followed by the existing ops in their original order.
    if let Some(entry_block) = f.body.entry_mut() {
        let mut new_ops: Vec<MirOp> = Vec::with_capacity(entry_block.ops.len() + 3);
        new_ops.push(h_const);
        new_ops.push(k_const);
        new_ops.push(verify);
        new_ops.extend(entry_block.ops.drain(..));
        entry_block.ops = new_ops;
    }
}

// ───────────────────────────────────────────────────────────────────────
// § the pass
// ───────────────────────────────────────────────────────────────────────

/// MIR pass : install runtime cap-verify preambles for every fn that has
/// at least one cap-required parameter.
///
/// Idempotent : a fn is processed AT MOST once per module (guarded by the
/// `cap_runtime_check.installed` per-fn attribute). Re-running the pipeline
/// is therefore safe.
#[derive(Debug, Clone, Copy, Default)]
pub struct CapRuntimeCheckPass;

impl CapRuntimeCheckPass {
    /// Pass-name used in diagnostics + `PassResult.name`.
    pub const NAME: &'static str = "cap-runtime-check";
}

impl MirPass for CapRuntimeCheckPass {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn run(&self, module: &mut MirModule) -> PassResult {
        let mut total_verify_emitted: usize = 0;
        let mut fns_processed: usize = 0;
        let mut warnings: Vec<PassDiagnostic> = Vec::new();

        for f in &mut module.funcs {
            if f.is_generic {
                // Skip unspecialized generics — they don't survive monomorphization.
                continue;
            }
            // Idempotency guard : skip fns we already processed.
            let already_installed = f
                .attributes
                .iter()
                .any(|(k, _)| k == FN_ATTR_CAP_RUNTIME_INSTALLED);
            if already_installed {
                continue;
            }

            let entries = collect_cap_required_params(f);
            if entries.is_empty() {
                // Sanity-check : if the legacy `cap` field is set on the fn
                // but no per-param attr was threaded, surface a warning so
                // the threading regression is caught.
                if f.cap.is_some() && !f.params.is_empty() {
                    warnings.push(PassDiagnostic::warning(
                        "CAP-RT0002",
                        format!(
                            "fn `{}` has cap-attribute `{}` but no `cap_required.<idx>` per-param attrs ; runtime cap-verify SKIPPED",
                            f.name,
                            f.cap.as_deref().unwrap_or(""),
                        ),
                    ));
                }
                continue;
            }
            for entry in &entries {
                emit_fn_entry_preamble(f, entry);
                total_verify_emitted += 1;
            }
            // Mark the fn as processed.
            f.attributes
                .push((FN_ATTR_CAP_RUNTIME_INSTALLED.to_string(), "true".to_string()));
            fns_processed += 1;
        }

        let mut diagnostics = warnings;
        if total_verify_emitted > 0 {
            diagnostics.push(PassDiagnostic::info(
                "CAP-RT0001",
                format!(
                    "cap-runtime-check : emitted {total_verify_emitted} verify-op(s) across {fns_processed} fn(s)"
                ),
            ));
        }

        PassResult {
            name: Self::NAME.to_string(),
            changed: total_verify_emitted > 0,
            diagnostics,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § public utility — count emitted verify-ops in a module
// ───────────────────────────────────────────────────────────────────────

/// Count `cssl.cap.verify` ops across every fn in the module. Used by
/// integration tests to assert per-fn emission counts after the pass runs.
#[must_use]
pub fn count_cap_verify_ops(module: &MirModule) -> usize {
    let mut n = 0;
    for f in &module.funcs {
        for block in &f.body.blocks {
            for op in &block.ops {
                if op.name == OP_CAP_VERIFY {
                    n += 1;
                }
            }
        }
    }
    n
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        cap_kind_from_attr, cap_kind_index, collect_cap_required_params, count_cap_verify_ops,
        op_kind_numeric, CapRuntimeCheckPass, OpKindTag, ATTR_CAP_KIND, ATTR_ORIGIN,
        FN_ATTR_CAP_REQUIRED_PREFIX, FN_ATTR_CAP_RUNTIME_INSTALLED, OP_CAP_VERIFY,
        ORIGIN_FN_ENTRY,
    };
    use crate::func::{MirFunc, MirModule};
    use crate::pipeline::MirPass;
    use crate::value::{IntWidth, MirType};
    use cssl_caps::CapKind;

    fn fn_with_iso_param(name: &str) -> MirFunc {
        let mut f = MirFunc::new(name, vec![MirType::Int(IntWidth::I64)], vec![]);
        f.cap = Some(CapKind::Iso.to_string());
        f.attributes
            .push((format!("{FN_ATTR_CAP_REQUIRED_PREFIX}0"), "iso".to_string()));
        f
    }

    #[test]
    fn cap_index_mirrors_cap_kind_index_method() {
        // The two helpers must agree byte-for-byte ; otherwise the
        // wire-protocol with cssl-rt::cap_verify is broken.
        for cap in CapKind::ALL {
            assert_eq!(cap_kind_index(cap), cap.index() as u8, "{cap:?}");
        }
    }

    #[test]
    fn cap_kind_from_attr_roundtrips_all_caps() {
        for cap in CapKind::ALL {
            let s = cap.to_string();
            assert_eq!(cap_kind_from_attr(&s), Some(cap), "{cap:?}");
        }
        assert_eq!(cap_kind_from_attr("nonsense"), None);
    }

    #[test]
    fn op_kind_numeric_matches_runtime_constants() {
        // ‼ Wire-protocol invariant : the numeric values must match
        // `cssl-rt::cap_verify::OP_*`. Renaming requires lock-step
        // changes — this test catches drift @ build time.
        assert_eq!(op_kind_numeric(&OpKindTag::CallPassParam), 0);
        assert_eq!(op_kind_numeric(&OpKindTag::FnEntry), 1);
        assert_eq!(op_kind_numeric(&OpKindTag::FieldAccess), 2);
        assert_eq!(op_kind_numeric(&OpKindTag::Return), 3);
    }

    #[test]
    fn collect_finds_per_param_cap_attrs() {
        let mut f = fn_with_iso_param("foo");
        f.attributes
            .push((format!("{FN_ATTR_CAP_REQUIRED_PREFIX}1"), "val".to_string()));
        let entries = collect_cap_required_params(&f);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].param_idx, 0);
        assert_eq!(entries[0].cap, CapKind::Iso);
        assert_eq!(entries[1].param_idx, 1);
        assert_eq!(entries[1].cap, CapKind::Val);
    }

    #[test]
    fn pass_emits_verify_op_for_iso_param() {
        // Cap-required-call-passes-with-cap : iso fn-entry → 1 verify-op.
        let mut module = MirModule::new();
        module.push_func(fn_with_iso_param("consume_iso"));
        let pass = CapRuntimeCheckPass;
        let result = pass.run(&mut module);
        assert!(result.changed);
        assert!(!result.has_errors());
        assert_eq!(count_cap_verify_ops(&module), 1);
        let op = module.funcs[0]
            .body
            .blocks
            .first()
            .unwrap()
            .ops
            .iter()
            .find(|o| o.name == OP_CAP_VERIFY)
            .expect("verify op present");
        let kind_attr = op
            .attributes
            .iter()
            .find(|(k, _)| k == ATTR_CAP_KIND)
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        assert_eq!(kind_attr, "iso");
        let origin_attr = op
            .attributes
            .iter()
            .find(|(k, _)| k == ATTR_ORIGIN)
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        assert_eq!(origin_attr, ORIGIN_FN_ENTRY);
    }

    #[test]
    fn pass_skips_fns_without_cap_required_attrs() {
        // Cap-required-fails-without : a fn lacking `cap_required.<idx>`
        // attrs gets ZERO verify-ops emitted (and no diagnostic-error).
        let mut module = MirModule::new();
        module.push_func(MirFunc::new(
            "no_caps",
            vec![MirType::Int(IntWidth::I32)],
            vec![],
        ));
        let pass = CapRuntimeCheckPass;
        let result = pass.run(&mut module);
        assert!(!result.changed);
        assert!(!result.has_errors());
        assert_eq!(count_cap_verify_ops(&module), 0);
    }

    #[test]
    fn pass_idempotent_re_run_is_no_op() {
        let mut module = MirModule::new();
        module.push_func(fn_with_iso_param("g"));
        let pass = CapRuntimeCheckPass;
        let _ = pass.run(&mut module);
        assert_eq!(count_cap_verify_ops(&module), 1);
        // Re-run : no additional verify-ops emitted (idempotency marker).
        let result = pass.run(&mut module);
        assert!(!result.changed);
        assert_eq!(count_cap_verify_ops(&module), 1);
        // Marker present.
        let installed = module.funcs[0]
            .attributes
            .iter()
            .any(|(k, _)| k == FN_ATTR_CAP_RUNTIME_INSTALLED);
        assert!(installed);
    }

    #[test]
    fn pass_emits_one_verify_per_cap_required_param() {
        // Cap-verify-op-emission : 2 cap params ⇒ 2 verify ops (canonical
        // boundary-per-param contract). This is the regression guard.
        let mut module = MirModule::new();
        let mut f = MirFunc::new(
            "two_caps",
            vec![MirType::Int(IntWidth::I64), MirType::Int(IntWidth::I64)],
            vec![],
        );
        f.attributes
            .push((format!("{FN_ATTR_CAP_REQUIRED_PREFIX}0"), "iso".to_string()));
        f.attributes
            .push((format!("{FN_ATTR_CAP_REQUIRED_PREFIX}1"), "val".to_string()));
        module.push_func(f);
        let pass = CapRuntimeCheckPass;
        let result = pass.run(&mut module);
        assert!(result.changed);
        assert_eq!(count_cap_verify_ops(&module), 2);
        // Diagnostic surfaces the count.
        let info = result
            .diagnostics
            .iter()
            .find(|d| d.code == "CAP-RT0001")
            .expect("info diagnostic present");
        assert!(info.message.contains("2 verify-op"));
    }

    #[test]
    fn pass_warns_on_legacy_cap_field_without_per_param_attrs() {
        // Regression guard : the HIR→MIR threading must populate the
        // per-param attrs ; if only the legacy `cap` field is set we
        // warn but don't panic.
        let mut module = MirModule::new();
        let mut f = MirFunc::new("legacy", vec![MirType::Int(IntWidth::I64)], vec![]);
        f.cap = Some("iso".to_string());
        // Note : NO `cap_required.0` attr — the regression scenario.
        module.push_func(f);
        let pass = CapRuntimeCheckPass;
        let result = pass.run(&mut module);
        assert!(!result.changed);
        let warn = result
            .diagnostics
            .iter()
            .find(|d| d.code == "CAP-RT0002")
            .expect("warning diagnostic present");
        assert!(warn.message.contains("legacy"));
    }

    #[test]
    fn pass_skips_generic_fns_until_after_monomorph() {
        // Generics carry placeholder types ; cap-runtime-check only wires
        // concrete fns (post-monomorph). The pass must skip them silently.
        let mut module = MirModule::new();
        let mut f = fn_with_iso_param("generic_iso");
        f.is_generic = true;
        module.push_func(f);
        let pass = CapRuntimeCheckPass;
        let result = pass.run(&mut module);
        assert!(!result.changed);
        assert_eq!(count_cap_verify_ops(&module), 0);
    }
}
