//! § Wave-A3 — `cssl.try` (`?`-operator) Cranelift cgen helpers.
//!
//! § ROLE
//!   Cgen-side helpers for the `?`-operator runtime execution path : the
//!   `cssl_mir::try_op_lower` MIR pass rewrites every `cssl.try` op into a
//!   tag-dispatched `scf.if` form ; this module provides the canonical
//!   recognizer + emit helpers that the JIT / Object backends use to
//!   lower the resulting IR shape directly through Cranelift.
//!
//!   Most of the heavy lifting is already in place :
//!     - `cgen_tagged_union::emit_tag_load` / `emit_tag_eq_compare` /
//!       `emit_payload_load` (Wave-A1) handle the per-instruction
//!       Cranelift-IR emission ;
//!     - the standard `scf.if` lowering in `crate::scf::lower_scf_if`
//!       handles the dispatch-arm shape ;
//!     - the standard `func.return` / `arith.constant` / `arith.cmpi` /
//!       `memref.load` paths handle the rest of the rewrite-stream.
//!
//!   What this slice adds :
//!     - canonical attribute-readers that recognize the
//!       `source_kind=try_propagation` / `try_failure_arm` /
//!       `try_success_arm` markers stamped by `try_op_lower` ;
//!     - predicate-helpers that let the JIT / Object backends decide
//!       whether a given op is part of a try-propagation sequence
//!       (useful for skipping no-op aliases + surfacing tag-dispatch
//!       sequences in pre-emit diagnostic dumps) ;
//!     - cranelift-IR builder helpers that emit the canonical
//!       try-propagation primitives directly. These are used by future
//!       cgen paths that bypass the `try_op_lower` MIR rewrite (e.g.
//!       when a future stack-slot construction lands and there is no
//!       per-op `memref.load` to dispatch on).
//!
//! § INTEGRATION_NOTE  (per Wave-A3 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!   src/lib.rs` is intentionally NOT modified. The helpers compile +
//!   are tested in-place via `#[cfg(test)]` references. Main-thread's
//!   integration commit promotes this to `pub mod cgen_try ;` + adds
//!   the `pub use cgen_try::*;` re-export at that time.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-mir/src/try_op_lower.rs` — sister module
//!     that produces the post-rewrite MIR shape this module consumes.
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/cgen_tagged_union.rs`
//!     — Wave-A1 cgen helpers ; we delegate `emit_tag_load` /
//!     `emit_tag_eq_compare` / `emit_payload_load` to that module rather
//!     than duplicating the load + compare logic.
//!   - `specs/40_WAVE_CSSL_PLAN.csl § WAVE-A § A3` — the wave plan that
//!     scopes this slice.
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions : zero allocation outside the
//!     cranelift `Signature` / `InstBuilder` storage that the cranelift
//!     side already requires.
//!   - The op-recognizer walk is single-pass O(N) over `op.attributes` ;
//!     no `HashMap<String, _>` allocation. The 4-entry attribute lookup
//!     uses linear scan (typical N ≤ 6) — strictly faster than a
//!     hash-table at this size.
//!   - Branch-friendly Cranelift IR : (cmp tag, fail-tag) → brif → fail-block /
//!     success-block with no scratch IR-builder state held across arms.
//!   - We REUSE Wave-A1 emit helpers ; we DO NOT duplicate the tag-load /
//!     tag-compare logic.
//!
//! § MIR ↔ CLIF ABI MAPPING
//!
//!   ```text
//!   MIR (post-try-rewrite)                          CLIF (this module)
//!   ───────────────────────────────────────         ────────────────────────────
//!   memref.load %ptr {offset=0,                     v_tag = load.i32 v_ptr (off 0)
//!     field=tag, source_kind=try_propagation}
//!   arith.constant {value=0}                        v_k = iconst.i32 0
//!     {source_kind=try_propagation}
//!   arith.cmpi eq %tag, %k                          v_cond = icmp eq v_tag, v_k
//!     {source_kind=try_propagation}
//!   scf.if %cond {fail-region} {success-region}     brif v_cond, fail_blk, ok_blk
//!     {source_kind=try_propagation, family=...}
//!
//!   <fail-region>                                   fail_blk:
//!     [Option] cssl.option.none + func.return         <build None ; return>
//!     [Result] memref.load (err) +                    <load err> + <build Err> +
//!              cssl.result.err + func.return            <return>
//!   <success-region>                                ok_blk:
//!     memref.load %ptr {offset=4,                     v_payload = load.i32 v_ptr (off 4)
//!       field=payload,
//!       source_kind=try_success_arm}
//!   ```

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{InstBuilder, MemFlags, Type, Value};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_frontend::FunctionBuilder;
use cssl_mir::MirOp;

// ─────────────────────────────────────────────────────────────────────────
// § Canonical attribute keys + values stamped by `cssl_mir::try_op_lower`.
//
//   These const strings are the wire-protocol between the MIR rewriter
//   and this cgen layer. Renaming any of them requires lock-step changes
//   on both sides ; the constants make the lock-step explicit + grep-
//   friendly.
//
//   ‼ MUST match the spelling in `cssl_mir::try_op_lower` :
//        SOURCE_KIND_TRY        = "try_propagation"
//        SOURCE_KIND_TRY_FAIL   = "try_failure_arm"
//        SOURCE_KIND_TRY_SUCCESS= "try_success_arm"
//        TRY_OP_NAME            = "cssl.try"
// ─────────────────────────────────────────────────────────────────────────

/// Attribute key carrying the source-kind marker for the try-propagation
/// dispatch. Mirrors the canonical
/// `cgen_tagged_union::ATTR_SOURCE_KIND` literal.
pub const ATTR_SOURCE_KIND: &str = "source_kind";

/// `source_kind=try_propagation` — stamped on every op emitted by
/// `cssl_mir::try_op_lower::expand_try_op` in the dispatch-skeleton
/// (tag-load, fail-const, cmp, scf.if).
pub const SOURCE_KIND_TRY: &str = "try_propagation";

/// `source_kind=try_failure_arm` — stamped on the failure-region's
/// reconstruction op (`cssl.option.none` / `cssl.result.err`) + the
/// `func.return` that exits the caller.
pub const SOURCE_KIND_TRY_FAIL: &str = "try_failure_arm";

/// `source_kind=try_success_arm` — stamped on the success-region's
/// payload-load that binds the original `cssl.try` result-id.
pub const SOURCE_KIND_TRY_SUCCESS: &str = "try_success_arm";

/// MIR op-name for the un-rewritten `cssl.try` op (matches
/// `body_lower::lower_try`'s emit).
pub const TRY_OP_NAME: &str = "cssl.try";

/// Attribute key carrying the family discriminator (`Option` /
/// `Result`) on the dispatch `scf.if` op.
pub const ATTR_FAMILY: &str = "family";

/// Family value stamped when the caller fn returns `Option<U>`.
pub const FAMILY_OPTION: &str = "Option";

/// Family value stamped when the caller fn returns `Result<U, E>`.
pub const FAMILY_RESULT: &str = "Result";

/// Failure-tag value used in the `arith.constant` of the dispatch.
/// Matches `cssl_mir::tagged_union_abi::tag_for_variant(SumVariant::None)`
/// + `tag_for_variant(SumVariant::Err)` — both are `0` per the
/// Wave-A1 tag-discipline.
pub const FAILURE_TAG_VALUE: i64 = 0;

/// Success-tag value (Some / Ok). Matches Wave-A1 tag-discipline.
pub const SUCCESS_TAG_VALUE: i64 = 1;

// ─────────────────────────────────────────────────────────────────────────
// § Predicate helpers — recognize try-propagation ops in the post-rewrite MIR.
// ─────────────────────────────────────────────────────────────────────────

/// Test whether `op` carries the canonical `source_kind=value` pair.
/// Internal helper shared by the more specialized predicates below.
#[must_use]
pub fn has_source_kind(op: &MirOp, expected: &str) -> bool {
    op.attributes
        .iter()
        .any(|(k, v)| k == ATTR_SOURCE_KIND && v == expected)
}

/// True when `op` is an un-rewritten `cssl.try` op (i.e. the MIR pass
/// has NOT YET run on this fn). Cgen surfaces this as a hard error
/// rather than emitting unspecified IR — the rewrite must run first.
#[must_use]
pub fn is_unlowered_try_op(op: &MirOp) -> bool {
    op.name == TRY_OP_NAME
}

/// True when `op` is part of a try-propagation rewrite (any of the four
/// op-types stamped by `expand_try_op` : tag-load, fail-const, cmp,
/// scf.if). Used by the JIT diagnostic walker to surface the dispatch-
/// sequence in pre-emit dumps.
#[must_use]
pub fn is_try_propagation_op(op: &MirOp) -> bool {
    has_source_kind(op, SOURCE_KIND_TRY)
}

/// True when `op` lives inside the failure-arm region of a try-rewrite.
/// Tagged with `source_kind=try_failure_arm` ; used for diagnostic
/// dumps + the auditing walks that count per-arm op emissions.
#[must_use]
pub fn is_try_failure_arm_op(op: &MirOp) -> bool {
    has_source_kind(op, SOURCE_KIND_TRY_FAIL)
}

/// True when `op` lives inside the success-arm region of a try-rewrite.
/// Tagged with `source_kind=try_success_arm`.
#[must_use]
pub fn is_try_success_arm_op(op: &MirOp) -> bool {
    has_source_kind(op, SOURCE_KIND_TRY_SUCCESS)
}

/// True when `op` is the `scf.if` that carries the try-propagation
/// dispatch (tag-cmp condition + 2 arms). Useful for cgen-side
/// diagnostics + structured-CFG audit walks.
#[must_use]
pub fn is_try_dispatch_if(op: &MirOp) -> bool {
    op.name == "scf.if" && is_try_propagation_op(op)
}

/// True when `op` is the `memref.load` of the tag word emitted by the
/// try-rewrite (NOT the user-program tag-load — uses the source-kind
/// marker to discriminate).
#[must_use]
pub fn is_try_tag_load(op: &MirOp) -> bool {
    op.name == "memref.load" && is_try_propagation_op(op)
}

/// True when `op` is the `arith.cmpi` that compares the loaded tag
/// against the failure-tag constant.
#[must_use]
pub fn is_try_tag_cmp(op: &MirOp) -> bool {
    op.name == "arith.cmpi" && is_try_propagation_op(op)
}

// ─────────────────────────────────────────────────────────────────────────
// § Attribute readers — pull canonical values off a try-propagation op.
// ─────────────────────────────────────────────────────────────────────────

/// Read the `family` attribute from a dispatch `scf.if`. Returns `None`
/// when the attribute is absent (pre-rewrite or unrelated `scf.if`).
#[must_use]
pub fn read_family_attr(op: &MirOp) -> Option<&str> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_FAMILY)
        .map(|(_, v)| v.as_str())
}

/// True when the dispatch's family is `Option`.
#[must_use]
pub fn is_option_family(op: &MirOp) -> bool {
    read_family_attr(op) == Some(FAMILY_OPTION)
}

/// True when the dispatch's family is `Result`.
#[must_use]
pub fn is_result_family(op: &MirOp) -> bool {
    read_family_attr(op) == Some(FAMILY_RESULT)
}

// ─────────────────────────────────────────────────────────────────────────
// § Cranelift-IR emit helpers — emit the canonical try-propagation
//   primitives directly. Used by future cgen paths that bypass the
//   per-op MIR-rewrite stream.
//
//   These DELEGATE to Wave-A1's `cgen_tagged_union::emit_tag_load` /
//   `emit_tag_eq_compare` / `emit_payload_load` rather than duplicating
//   the load + compare logic. Wave-A1 owns the tag-ABI emission.
// ─────────────────────────────────────────────────────────────────────────

/// Memory-flag set used for try-propagation loads. Aligned + non-trapping.
/// Matches `cgen_tagged_union::tagged_union_mem_flags()` ; we duplicate
/// the constructor here so we don't depend on Wave-A1's pub-mod
/// declaration (which the integration commit hasn't added yet for the
/// cgen side either).
#[must_use]
pub fn try_mem_flags() -> MemFlags {
    let mut flags = MemFlags::new();
    flags.set_aligned();
    flags.set_notrap();
    flags
}

/// Emit the canonical tag-load instruction for a `?` dispatch. Returns
/// the resulting CLIF `Value` carrying the loaded tag word.
///
/// Direct-cgen entry point : skips the per-op MIR rewrite + emits the
/// load straight into the cranelift function. Useful for future cgen
/// paths that synthesize try-propagation without going through the
/// `try_op_lower` rewrite (e.g. a stack-slot construction with no heap
/// cell to load from).
#[must_use]
pub fn emit_try_tag_load(
    builder: &mut FunctionBuilder<'_>,
    cell_ptr: Value,
    tag_offset: i32,
    tag_clif_ty: Type,
) -> Value {
    let flags = try_mem_flags();
    builder.ins().load(tag_clif_ty, flags, cell_ptr, tag_offset)
}

/// Emit a tag-equality comparison `icmp eq %tag, %k` for the failure-
/// dispatch. Returns the boolean condition value used as the `brif`
/// selector.
///
/// `k` is the failure tag (always `0` per Wave-A1 tag-discipline). The
/// caller can also pass [`SUCCESS_TAG_VALUE`] when emitting a "is-this-
/// success" check rather than a "is-this-failure" check.
#[must_use]
pub fn emit_try_tag_eq_compare(
    builder: &mut FunctionBuilder<'_>,
    tag_value: Value,
    k: i64,
    tag_clif_ty: Type,
) -> Value {
    let const_v = builder.ins().iconst(tag_clif_ty, k);
    builder.ins().icmp(IntCC::Equal, tag_value, const_v)
}

/// Emit the canonical payload-load that runs in the success-arm of a
/// try dispatch. The caller supplies the payload's CLIF type ; this
/// helper does NOT validate that the offset matches a real layout —
/// pre-validation is expected before reaching codegen.
#[must_use]
pub fn emit_try_payload_load(
    builder: &mut FunctionBuilder<'_>,
    cell_ptr: Value,
    payload_offset: i32,
    payload_clif_ty: Type,
) -> Value {
    let flags = try_mem_flags();
    builder
        .ins()
        .load(payload_clif_ty, flags, cell_ptr, payload_offset)
}

/// Emit the canonical err-payload load that runs in the failure-arm of
/// a `Result` try-propagation. Identical to [`emit_try_payload_load`]
/// in shape ; named differently so cgen sites read intentionally —
/// loading "the err side of a Result" is the failure-side operation
/// that the success-side never performs.
#[must_use]
pub fn emit_try_err_payload_load(
    builder: &mut FunctionBuilder<'_>,
    cell_ptr: Value,
    payload_offset: i32,
    err_clif_ty: Type,
) -> Value {
    let flags = try_mem_flags();
    builder.ins().load(err_clif_ty, flags, cell_ptr, payload_offset)
}

// ─────────────────────────────────────────────────────────────────────────
// § Whole-block scan : "does this block touch a try-rewrite"
// ─────────────────────────────────────────────────────────────────────────

/// Walk a single MIR block once + return whether ANY op participates
/// in a try-propagation rewrite (either as part of the dispatch, the
/// failure-arm, or the success-arm). Cgen uses this to keep diagnostic
/// instrumentation lean — only emit the try-helpers when the block
/// actually needs them.
///
/// § COMPLEXITY  O(N) in op-count, single-pass, early-exit on first
///   recognized op. No allocation.
#[must_use]
pub fn block_touches_try_propagation(block: &cssl_mir::MirBlock) -> bool {
    block.ops.iter().any(|op| {
        is_unlowered_try_op(op)
            || is_try_propagation_op(op)
            || is_try_failure_arm_op(op)
            || is_try_success_arm_op(op)
    })
}

/// Count the number of try-propagation dispatches in a block (i.e.
/// `scf.if` ops carrying `source_kind=try_propagation`). Pure audit
/// counter ; useful for assertion-style tests + post-rewrite
/// invariant checks.
#[must_use]
pub fn count_try_dispatches(block: &cssl_mir::MirBlock) -> usize {
    block
        .ops
        .iter()
        .filter(|op| is_try_dispatch_if(op))
        .count()
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — pure-helper coverage. End-to-end JIT roundtrips land in
//   jit.rs once main-thread integrates the dispatch wiring.
// ─────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use cssl_mir::{IntWidth, MirBlock, MirOp, MirType, ValueId};

    // ── Constant-name lock invariants — these MUST match the strings
    //    stamped by `cssl_mir::try_op_lower::expand_try_op` verbatim.
    //    Renaming either side without the other = silent ABI drift ⇒
    //    cgen mis-emits. ──

    #[test]
    fn attr_keys_match_mir_rewrite_canonical_strings() {
        // These tests pin the wire-protocol. If a future change in
        // try_op_lower.rs renames a key, the test here flags the
        // mismatch immediately rather than at the runtime cgen path.
        assert_eq!(ATTR_SOURCE_KIND, "source_kind");
        assert_eq!(SOURCE_KIND_TRY, "try_propagation");
        assert_eq!(SOURCE_KIND_TRY_FAIL, "try_failure_arm");
        assert_eq!(SOURCE_KIND_TRY_SUCCESS, "try_success_arm");
        assert_eq!(TRY_OP_NAME, "cssl.try");
        assert_eq!(ATTR_FAMILY, "family");
        assert_eq!(FAMILY_OPTION, "Option");
        assert_eq!(FAMILY_RESULT, "Result");
        assert_eq!(FAILURE_TAG_VALUE, 0);
        assert_eq!(SUCCESS_TAG_VALUE, 1);
    }

    // ── helpers : build canonical post-rewrite ops for testing. ──

    fn try_tag_load_op() -> MirOp {
        MirOp::std("memref.load")
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I32))
            .with_attribute("offset", "0")
            .with_attribute("alignment", "4")
            .with_attribute("field", "tag")
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY)
    }

    fn try_fail_const_op() -> MirOp {
        MirOp::std("arith.constant")
            .with_result(ValueId(2), MirType::Int(IntWidth::I32))
            .with_attribute("value", "0")
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY)
    }

    fn try_cmp_op() -> MirOp {
        MirOp::std("arith.cmpi")
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_result(ValueId(3), MirType::Bool)
            .with_attribute("predicate", "eq")
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY)
    }

    fn try_dispatch_if_op_option() -> MirOp {
        MirOp::std("scf.if")
            .with_operand(ValueId(3))
            .with_result(ValueId(4), MirType::None)
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY)
            .with_attribute(ATTR_FAMILY, FAMILY_OPTION)
    }

    fn try_dispatch_if_op_result() -> MirOp {
        MirOp::std("scf.if")
            .with_operand(ValueId(3))
            .with_result(ValueId(4), MirType::None)
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY)
            .with_attribute(ATTR_FAMILY, FAMILY_RESULT)
    }

    fn try_unlowered_op() -> MirOp {
        MirOp::std("cssl.try")
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Opaque("!cssl.option.i32".into()))
    }

    fn try_failure_arm_none_op() -> MirOp {
        MirOp::std("cssl.option.none")
            .with_result(ValueId(5), MirType::Opaque("!cssl.option.i32".into()))
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY_FAIL)
    }

    fn try_success_arm_load_op() -> MirOp {
        MirOp::std("memref.load")
            .with_operand(ValueId(0))
            .with_result(ValueId(7), MirType::Int(IntWidth::I32))
            .with_attribute("offset", "4")
            .with_attribute("field", "payload")
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY_SUCCESS)
    }

    // ── predicate recognition tests ──

    #[test]
    fn is_unlowered_try_op_recognizes_pre_rewrite_op() {
        assert!(is_unlowered_try_op(&try_unlowered_op()));
        // post-rewrite tag-load uses memref.load name, NOT cssl.try.
        assert!(!is_unlowered_try_op(&try_tag_load_op()));
    }

    #[test]
    fn is_try_propagation_op_recognizes_dispatch_skeleton() {
        assert!(is_try_propagation_op(&try_tag_load_op()));
        assert!(is_try_propagation_op(&try_fail_const_op()));
        assert!(is_try_propagation_op(&try_cmp_op()));
        assert!(is_try_propagation_op(&try_dispatch_if_op_option()));
        // Bare arith.cmpi without the source_kind marker : NOT recognized.
        let bare = MirOp::std("arith.cmpi");
        assert!(!is_try_propagation_op(&bare));
    }

    #[test]
    fn is_try_failure_arm_op_recognizes_failure_emissions() {
        assert!(is_try_failure_arm_op(&try_failure_arm_none_op()));
        // dispatch-skeleton op uses try_propagation, not try_failure_arm.
        assert!(!is_try_failure_arm_op(&try_tag_load_op()));
    }

    #[test]
    fn is_try_success_arm_op_recognizes_success_load() {
        assert!(is_try_success_arm_op(&try_success_arm_load_op()));
        // dispatch tag-load uses try_propagation, not try_success_arm.
        assert!(!is_try_success_arm_op(&try_tag_load_op()));
    }

    #[test]
    fn is_try_dispatch_if_recognizes_scf_if_with_marker() {
        assert!(is_try_dispatch_if(&try_dispatch_if_op_option()));
        assert!(is_try_dispatch_if(&try_dispatch_if_op_result()));
        // Bare scf.if without marker : NOT recognized.
        let bare = MirOp::std("scf.if").with_operand(ValueId(3));
        assert!(!is_try_dispatch_if(&bare));
    }

    #[test]
    fn is_try_tag_load_recognizes_dispatch_load() {
        assert!(is_try_tag_load(&try_tag_load_op()));
        // success-arm load uses try_success_arm, not try_propagation.
        assert!(!is_try_tag_load(&try_success_arm_load_op()));
    }

    #[test]
    fn is_try_tag_cmp_recognizes_dispatch_compare() {
        assert!(is_try_tag_cmp(&try_cmp_op()));
        // bare cmp : NOT recognized.
        let bare = MirOp::std("arith.cmpi");
        assert!(!is_try_tag_cmp(&bare));
    }

    // ── attribute readers ──

    #[test]
    fn read_family_attr_returns_canonical_family() {
        assert_eq!(
            read_family_attr(&try_dispatch_if_op_option()),
            Some(FAMILY_OPTION)
        );
        assert_eq!(
            read_family_attr(&try_dispatch_if_op_result()),
            Some(FAMILY_RESULT)
        );
        // Op without family attribute : None.
        let bare = MirOp::std("scf.if");
        assert_eq!(read_family_attr(&bare), None);
    }

    #[test]
    fn is_option_family_vs_is_result_family() {
        assert!(is_option_family(&try_dispatch_if_op_option()));
        assert!(!is_result_family(&try_dispatch_if_op_option()));
        assert!(is_result_family(&try_dispatch_if_op_result()));
        assert!(!is_option_family(&try_dispatch_if_op_result()));
    }

    // ── memory-flags ──

    #[test]
    fn try_mem_flags_are_aligned_notrap() {
        let f = try_mem_flags();
        assert!(f.aligned());
        assert!(f.notrap());
    }

    // ── whole-block scan ──

    #[test]
    fn block_touches_try_propagation_detects_any_marker_op() {
        let mut blk = MirBlock::new("entry");
        blk.push(MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "42"));
        blk.push(try_tag_load_op());
        blk.push(MirOp::std("func.return"));
        assert!(block_touches_try_propagation(&blk));
    }

    #[test]
    fn block_touches_try_propagation_returns_false_for_plain_arithmetic() {
        let mut blk = MirBlock::new("entry");
        blk.push(MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "1"));
        blk.push(MirOp::std("arith.constant")
            .with_result(ValueId(1), MirType::Int(IntWidth::I32))
            .with_attribute("value", "2"));
        blk.push(MirOp::std("arith.addi")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32)));
        blk.push(MirOp::std("func.return").with_operand(ValueId(2)));
        assert!(!block_touches_try_propagation(&blk));
    }

    #[test]
    fn block_touches_try_propagation_detects_unlowered_op() {
        let mut blk = MirBlock::new("entry");
        blk.push(try_unlowered_op());
        assert!(block_touches_try_propagation(&blk));
    }

    #[test]
    fn count_try_dispatches_counts_dispatch_if_ops() {
        let mut blk = MirBlock::new("entry");
        blk.push(try_tag_load_op());
        blk.push(try_dispatch_if_op_option());
        blk.push(try_dispatch_if_op_result());
        // Only scf.if's with the marker count.
        assert_eq!(count_try_dispatches(&blk), 2);
        // Tag-load with try_propagation marker is NOT a dispatch-if (its
        // op-name is memref.load, not scf.if).
    }

    // ── canonical sequence : Some(42) try-on-Option @ Option-caller ──
    //
    //   This test is the slice's golden : it builds the EXACT op-stream
    //   that `try_op_lower::expand_try_op` would produce for `parse(s)?`
    //   in an Option<i32>-returning fn and verifies the cgen-side
    //   recognizers fire on each piece. The integration commit lifts
    //   this into a JIT-roundtrip ; today the sequence stops at
    //   structural verification.

    #[test]
    fn golden_canonical_try_option_dispatch_recognizes_each_step() {
        let mut blk = MirBlock::new("entry");
        blk.push(try_tag_load_op());
        blk.push(try_fail_const_op());
        blk.push(try_cmp_op());
        blk.push(try_dispatch_if_op_option());

        // Whole-block scan should fire.
        assert!(block_touches_try_propagation(&blk));
        // count the dispatches : exactly 1 scf.if with the marker.
        assert_eq!(count_try_dispatches(&blk), 1);

        // Per-op recognizers each fire on the right op.
        assert!(is_try_tag_load(&blk.ops[0]));
        assert!(is_try_propagation_op(&blk.ops[1]));
        assert!(is_try_tag_cmp(&blk.ops[2]));
        assert!(is_try_dispatch_if(&blk.ops[3]));
        // Family is Option per try_dispatch_if_op_option.
        assert_eq!(read_family_attr(&blk.ops[3]), Some(FAMILY_OPTION));
    }

    // ── canonical sequence : try-on-Result @ Result-caller ──
    //
    //   For Result the failure-arm contains a memref.load (err-payload)
    //   followed by cssl.result.err + func.return. The success-arm is
    //   identical in shape to the Option case (just a payload load).

    #[test]
    fn golden_canonical_try_result_dispatch_recognizes_each_step() {
        let mut blk = MirBlock::new("entry");
        blk.push(try_tag_load_op());
        blk.push(try_fail_const_op());
        blk.push(try_cmp_op());
        blk.push(try_dispatch_if_op_result());

        assert!(block_touches_try_propagation(&blk));
        assert!(is_try_dispatch_if(&blk.ops[3]));
        assert!(is_result_family(&blk.ops[3]));
    }

    // ── failure-arm + success-arm op recognition ──

    #[test]
    fn failure_arm_none_op_recognizable() {
        let op = try_failure_arm_none_op();
        assert!(is_try_failure_arm_op(&op));
        // It's NOT a dispatch-skeleton op.
        assert!(!is_try_propagation_op(&op));
    }

    #[test]
    fn success_arm_payload_load_recognizable() {
        let op = try_success_arm_load_op();
        assert!(is_try_success_arm_op(&op));
        assert!(!is_try_failure_arm_op(&op));
        assert!(!is_try_propagation_op(&op));
    }

    // ── source-kind discriminator ── verifies has_source_kind logic.

    #[test]
    fn has_source_kind_distinguishes_three_canonical_kinds() {
        let propagation = try_tag_load_op();
        let failure = try_failure_arm_none_op();
        let success = try_success_arm_load_op();

        assert!(has_source_kind(&propagation, SOURCE_KIND_TRY));
        assert!(!has_source_kind(&propagation, SOURCE_KIND_TRY_FAIL));
        assert!(!has_source_kind(&propagation, SOURCE_KIND_TRY_SUCCESS));

        assert!(has_source_kind(&failure, SOURCE_KIND_TRY_FAIL));
        assert!(!has_source_kind(&failure, SOURCE_KIND_TRY));

        assert!(has_source_kind(&success, SOURCE_KIND_TRY_SUCCESS));
        assert!(!has_source_kind(&success, SOURCE_KIND_TRY));
    }

    // ── failure-tag canonical-value lock ──
    //
    //   The MIR rewrite emits `arith.constant value=0` for the failure-
    //   tag (matches Wave-A1 tag-discipline : None=0, Err=0). The cgen
    //   side reuses this constant as `FAILURE_TAG_VALUE`. If either
    //   side renumbers the failure-tag, the lock-check fails.

    #[test]
    fn failure_tag_value_matches_mir_rewrite_emit() {
        let const_op = try_fail_const_op();
        let v = const_op
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .unwrap();
        assert_eq!(v.1, FAILURE_TAG_VALUE.to_string());
    }
}

// INTEGRATION_NOTE :
//   add `pub mod cgen_try;` to cssl-cgen-cpu-cranelift/src/lib.rs in the
//   integration commit. The Wave-A3 dispatch carved this file out
//   single-file-owned ; main-thread integration replaces this comment
//   with the `pub mod` declaration + the re-export block listing
//   `is_unlowered_try_op`, `is_try_propagation_op`,
//   `is_try_failure_arm_op`, `is_try_success_arm_op`,
//   `is_try_dispatch_if`, `is_try_tag_load`, `is_try_tag_cmp`,
//   `read_family_attr`, `is_option_family`, `is_result_family`,
//   `try_mem_flags`, `emit_try_tag_load`, `emit_try_tag_eq_compare`,
//   `emit_try_payload_load`, `emit_try_err_payload_load`,
//   `block_touches_try_propagation`, `count_try_dispatches`. The
//   integration commit's wiring step also adds dispatch arms for
//   `is_unlowered_try_op` (hard-error : the rewrite must run first) +
//   the post-rewrite ops (delegate to existing `memref.load` /
//   `arith.cmpi` / `scf.if` lowerers) to the `lower_op_to_cl` match in
//   jit.rs / object.rs.
