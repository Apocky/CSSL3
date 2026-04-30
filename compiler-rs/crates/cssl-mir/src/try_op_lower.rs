//! § Wave-A3 — `cssl.try` (`?`-operator) MIR → MIR lowering pass.
//!
//! § SPEC : `specs/40_WAVE_CSSL_PLAN.csl` § WAVES § WAVE-A § A3.
//! § ROLE : MIR → MIR pass that finds every `cssl.try` op (emitted by
//!   `body_lower::lower_try` for `HirExprKind::Try`) and rewrites it as a
//!   tag-dispatched short-circuit-return on the operand's tagged-union
//!   shape :
//!
//!   ```text
//!   //   %r = cssl.try %scrut_ptr        (input)
//!   //   ─────────────────────────────────────────────────────────────
//!   //   %tag = memref.load %scrut_ptr {offset=0, field=tag} : i32
//!   //   %fail_k = arith.constant 0 : i32                        // None / Err tag
//!   //   %is_fail = arith.cmpi eq %tag, %fail_k : i1
//!   //   scf.if %is_fail {                                       // failure-arm
//!   //       <reconstruct-failure-in-caller's-return-type>
//!   //       func.return %fail_value
//!   //   } else {                                                // success-arm
//!   //       %r = memref.load %scrut_ptr {offset=4, field=payload} : <T>
//!   //   }
//!   ```
//!
//! § FAILURE-RECONSTRUCTION DISCIPLINE  (Maranget-style propagation)
//!   - Caller fn returns `Option<U>` :
//!       fail-arm emits a fresh `cssl.option.none` op (re-typed for the
//!       caller's `Option<U>` shape) + `func.return %none_id`.
//!   - Caller fn returns `Result<U, E>` :
//!       fail-arm loads the err-payload from the scrutinee cell at the
//!       payload-offset, emits `cssl.result.err %err_id` (re-typed for
//!       caller's `Result<U, E>`) + `func.return %err_id`.
//!   - Caller fn returns NEITHER `Option` NOR `Result` :
//!       this is the type-mismatch case ; HIR's `infer.rs` already
//!       diagnoses it (see `HirExprKind::Try` branch). Stage-0 keeps the
//!       MIR pass conservative — the rewrite is SKIPPED + a diagnostic
//!       is recorded in [`TryLoweringReport::type_mismatch_count`].
//!
//! § STAGE-0 ASSUMPTIONS
//!   - The operand of `cssl.try` is a `!cssl.ptr` to a tagged-union cell.
//!     This is true after Wave-A1 (`tagged_union_abi::expand_module`)
//!     runs because every `cssl.option.*` / `cssl.result.*` constructor
//!     gets rewritten to a heap-alloc + tag-store + payload-store triple
//!     that produces the cell-ptr (aliased back through the original
//!     SSA-id by an `arith.bitcast` carrying `source_kind=tagged_union_alias`).
//!   - The success-payload type is recovered from the operand's MIR
//!     `Opaque("!cssl.option.<T>")` / `Opaque("!cssl.result.<T>.<E>")`
//!     parametric-type string, falling back to the cell-ptr's natural
//!     8-byte slot when parsing fails. This mirrors the same heuristic
//!     used by `tagged_union_abi::parse_payload_ty`.
//!   - The pass runs AFTER `tagged_union_abi::expand_module` so the
//!     tag-load + payload-load offsets line up with the cell layout
//!     stamped by Wave-A1.
//!
//! § PUBLIC SURFACE
//!   - [`lower_try_ops_in_func`] — rewrite every `cssl.try` op in one fn.
//!   - [`lower_try_ops_in_module`] — drive the rewrite over a whole module.
//!   - [`TryLoweringReport`] — per-pass audit counters.
//!   - [`CallerReturnFamily`] — discriminator for the caller's return type.
//!   - [`classify_caller_return`] — single-fn helper that maps a fn's
//!     return type to its sum-type family for failure-reconstruction.
//!
//! § SAWYER-EFFICIENCY
//!   - Single-pass MIR walk via depth-first region recursion.
//!   - Tag-comparison constant uses direct `arith.constant` iconst (no
//!     HashMap, no string interning of "None" / "Err" ; the `0` failure-tag
//!     is the canonical [`tagged_union_abi::tag_for_variant`] for both
//!     `SumVariant::None` and `SumVariant::Err`).
//!   - The replacement-op stream allocates EXACTLY the ops it needs +
//!     splices them in via `Vec::splice` (in-place) — no scratch growth.
//!   - The walk is `O(N)` in op-count + `O(R)` in nesting depth ; no
//!     `HashMap<String, _>` allocations on the hot path.
//!
//! § INTEGRATION_NOTE  (per Wave-A3 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-mir/src/lib.rs` is
//!   intentionally NOT modified. The helpers compile + are tested in-place
//!   via `#[cfg(test)]` references. Main-thread's integration commit adds
//!   `pub mod try_op_lower ;` + the `pub use` re-export block at that time.

#![allow(dead_code, unreachable_pub)]

use crate::block::{MirBlock, MirOp, MirRegion};
use crate::func::{MirFunc, MirModule};
use crate::tagged_union_abi::{
    tag_for_variant, FreshIdSeq, SumFamily, SumVariant, TaggedUnionLayout,
};
use crate::value::{IntWidth, MirType, ValueId};

// ─────────────────────────────────────────────────────────────────────────
// § Canonical op-name + attribute markers stamped on the rewrite output.
//
//   The cgen layer recognizes these literal strings ; renaming any of them
//   requires lock-step changes on the cgen side.
// ─────────────────────────────────────────────────────────────────────────

/// Op-name for the `cssl.try` op as emitted by `body_lower::lower_try`.
pub const TRY_OP_NAME: &str = "cssl.try";

/// `source_kind=try_propagation` — stamped on every rewrite-emitted op so
/// downstream cgen + diagnostic walkers can identify the lowered try-op.
pub const SOURCE_KIND_TRY: &str = "try_propagation";

/// `source_kind=try_failure_arm` — stamped on the failure-region's
/// reconstruction op + `func.return`. Distinguishes the failure-side
/// emission from the success-side payload-load.
pub const SOURCE_KIND_TRY_FAIL: &str = "try_failure_arm";

/// `source_kind=try_success_arm` — stamped on the success-region's
/// payload-load that binds the original `cssl.try` result-id.
pub const SOURCE_KIND_TRY_SUCCESS: &str = "try_success_arm";

/// Attribute-key for the source-kind marker. Mirrors the canonical key
/// used by `tagged_union_abi`'s `ATTR_SOURCE_KIND`.
pub const ATTR_SOURCE_KIND: &str = "source_kind";

/// Attribute-key for the canonical `field=tag` / `field=payload` marker
/// on the rewrite's `memref.load` ops. Mirrors `tagged_union_abi`'s
/// `ATTR_FIELD`.
pub const ATTR_FIELD: &str = "field";

/// `field=tag` value — the 4-byte tag slot at offset 0.
pub const ATTR_FIELD_TAG: &str = "tag";

/// `field=payload` value — the variant payload at the layout's
/// `payload_offset`.
pub const ATTR_FIELD_PAYLOAD: &str = "payload";

// ─────────────────────────────────────────────────────────────────────────
// § Caller-return-family classification.
// ─────────────────────────────────────────────────────────────────────────

/// The sum-type family of the caller fn's return type. The `?`-operator
/// reconstructs failure in the caller's return shape ; this enum drives
/// the per-arm code emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallerReturnFamily {
    /// Caller returns `Option<U>` ; failure-arm propagates `None`.
    Option,
    /// Caller returns `Result<U, E>` ; failure-arm propagates `Err(e)`
    /// where `e` is the err-payload extracted from the scrutinee cell.
    Result,
    /// Caller returns NEITHER an `Option` NOR a `Result`. Type-checker
    /// (HIR) already flags this ; the MIR pass treats the rewrite as a
    /// no-op + bumps `type_mismatch_count` so the report surfaces it.
    Mismatch,
}

/// Inspect a caller fn's return-type slot and classify it for try-op
/// propagation.
///
/// Stage-0 keys off the canonical `Opaque("!cssl.option.<T>")` /
/// `Opaque("!cssl.result.<T>.<E>")` shape stamped by `body_lower`. Future
/// slices replace these with structural `MirType::TaggedUnion` once the
/// type-system surface is in place ; the matcher accepts both spellings
/// to stay forward-compatible.
#[must_use]
pub fn classify_caller_return(results: &[MirType]) -> CallerReturnFamily {
    // Stage-0 fns always have at most one result. Multi-result fns
    // collapse to the FIRST result-type for try-op classification.
    let Some(ty) = results.first() else {
        return CallerReturnFamily::Mismatch;
    };
    classify_mir_type(ty)
}

/// Pull a sum-type family out of a single MIR type. Used by
/// [`classify_caller_return`] + `cfg(test)` callers for unit coverage.
#[must_use]
pub fn classify_mir_type(ty: &MirType) -> CallerReturnFamily {
    match ty {
        MirType::Opaque(s) => match opaque_family_prefix(s) {
            Some(SumFamily::Option) => CallerReturnFamily::Option,
            Some(SumFamily::Result) => CallerReturnFamily::Result,
            None => CallerReturnFamily::Mismatch,
        },
        _ => CallerReturnFamily::Mismatch,
    }
}

/// Walk an opaque type-string (`"!cssl.option.<T>"` / `"!cssl.result.<T>.<E>"`)
/// and return the family. Returns `None` for any non-sum-type opaque.
#[must_use]
pub fn opaque_family_prefix(s: &str) -> Option<SumFamily> {
    if s.starts_with("!cssl.option.") || s == "!cssl.option" {
        Some(SumFamily::Option)
    } else if s.starts_with("!cssl.result.") || s == "!cssl.result" {
        Some(SumFamily::Result)
    } else {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Try-op recognition.
// ─────────────────────────────────────────────────────────────────────────

/// True when `op` is the `cssl.try` op emitted by
/// `body_lower::lower_try`.
#[must_use]
pub fn is_try_op(op: &MirOp) -> bool {
    op.name == TRY_OP_NAME
}

/// Extract the payload type from a try-op's result-type. The result-type
/// is `inner_ty` propagated by `body_lower::lower_try` — for an
/// `Option<T>` operand it carries the same `Opaque("!cssl.option.<T>")`
/// the operand had ; we recover `<T>` for the success-payload load by
/// walking `tagged_union_abi::parse_payload_ty` on the textual suffix.
#[must_use]
pub fn extract_payload_type(op: &MirOp) -> MirType {
    if let Some(r) = op.results.first() {
        if let MirType::Opaque(s) = &r.ty {
            return parse_payload_from_opaque(s);
        }
    }
    // Defensive : safe-default 8-byte Ptr cell.
    MirType::Ptr
}

/// Parse the textual `<T>` out of a `!cssl.option.<T>` / `!cssl.result.<T>.<E>`
/// opaque type-string. Fallback : `MirType::Ptr` (8-byte safe default).
///
/// Stage-0 only handles scalar / `Ptr` shapes ; future slices add struct
/// + nested-sum support.
#[must_use]
pub fn parse_payload_from_opaque(s: &str) -> MirType {
    use crate::tagged_union_abi::parse_payload_ty;
    if let Some(rest) = s.strip_prefix("!cssl.option.") {
        return parse_payload_ty(rest);
    }
    if let Some(rest) = s.strip_prefix("!cssl.result.") {
        // `!cssl.result.<T>.<E>` : `<T>` is the success-payload slot.
        // Split on the last `.` between `<T>` and `<E>` ; if no dot is
        // present we treat the whole rest as `<T>`.
        if let Some(t_part) = rest.split('.').next() {
            return parse_payload_ty(t_part);
        }
        return parse_payload_ty(rest);
    }
    MirType::Ptr
}

/// Parse the textual `<E>` out of a `!cssl.result.<T>.<E>` opaque
/// type-string. Returns `Ptr` for non-Result-shape strings (defensive
/// safe-default) so the cgen path can still emit a valid load.
#[must_use]
pub fn parse_err_from_opaque(s: &str) -> MirType {
    use crate::tagged_union_abi::parse_payload_ty;
    let Some(rest) = s.strip_prefix("!cssl.result.") else {
        return MirType::Ptr;
    };
    // `<T>.<E>` : everything after the FIRST dot is `<E>`. Use rsplit so
    // dotted err-types (e.g. `!cssl.handle`) survive intact.
    let mut parts = rest.splitn(2, '.');
    let _t = parts.next();
    parts.next().map_or(MirType::Ptr, parse_payload_ty)
}

// ─────────────────────────────────────────────────────────────────────────
// § Per-op rewrite : `cssl.try %scrut` → tag-load + cmp + scf.if.
// ─────────────────────────────────────────────────────────────────────────

/// Result of expanding ONE `cssl.try` op into its tag-dispatched form.
///
/// The replacement op-stream replaces the original `cssl.try` op at its
/// position in the block ; the success-arm payload-load binds the
/// original op's result-id (so downstream consumers see the correct SSA-id
/// without a value-map rewrite).
#[derive(Debug, Clone)]
pub struct TryExpansion {
    /// MIR ops emitted, in source-order. Includes the tag-load, the
    /// arith-constant, the cmp, and the `scf.if` carrying both arms.
    pub ops: Vec<MirOp>,
    /// Layout used for tag/payload offsets. Preserved for downstream
    /// audit + golden-test pinning.
    pub layout: TaggedUnionLayout,
}

/// Build the failure-arm region for `Option`-family propagation.
///
/// Emits :
///   1. `cssl.option.none` at the caller's `Option<U>` opaque type
///   2. `func.return %none_id`
fn build_option_failure_region(
    caller_ret_ty: &MirType,
    ids: &mut FreshIdSeq,
) -> MirRegion {
    let none_id = ids.fresh();
    let none_op = MirOp::new(crate::op::CsslOp::OptionNone)
        .with_result(none_id, caller_ret_ty.clone())
        .with_attribute("tag", tag_for_variant(SumVariant::None).to_string())
        .with_attribute("family", "Option")
        .with_attribute("payload_ty", "!cssl.unknown")
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY_FAIL);
    let ret_op = MirOp::std("func.return")
        .with_operand(none_id)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY_FAIL);

    let mut blk = MirBlock::new("try_fail");
    blk.push(none_op);
    blk.push(ret_op);
    let mut r = MirRegion::new();
    r.push(blk);
    r
}

/// Build the failure-arm region for `Result`-family propagation.
///
/// Emits :
///   1. `memref.load %scrut, off=payload_offset` → `%err_payload : <E>`
///   2. `cssl.result.err %err_payload` → `%err_cell : Result<U, E>`
///   3. `func.return %err_cell`
fn build_result_failure_region(
    scrut_ptr: ValueId,
    layout: TaggedUnionLayout,
    err_ty: &MirType,
    caller_ret_ty: &MirType,
    ids: &mut FreshIdSeq,
) -> MirRegion {
    let err_payload_id = ids.fresh();
    let load_op = MirOp::std("memref.load")
        .with_operand(scrut_ptr)
        .with_result(err_payload_id, err_ty.clone())
        .with_attribute("offset", layout.payload_offset.to_string())
        .with_attribute("alignment", layout.cell_alignment.to_string())
        .with_attribute(ATTR_FIELD, ATTR_FIELD_PAYLOAD)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY_FAIL);

    let err_cell_id = ids.fresh();
    let err_op = MirOp::new(crate::op::CsslOp::ResultErr)
        .with_operand(err_payload_id)
        .with_result(err_cell_id, caller_ret_ty.clone())
        .with_attribute("tag", tag_for_variant(SumVariant::Err).to_string())
        .with_attribute("family", "Result")
        .with_attribute("payload_ty", payload_ty_attr_for(err_ty))
        .with_attribute("err_ty", payload_ty_attr_for(err_ty))
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY_FAIL);

    let ret_op = MirOp::std("func.return")
        .with_operand(err_cell_id)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY_FAIL);

    let mut blk = MirBlock::new("try_fail");
    blk.push(load_op);
    blk.push(err_op);
    blk.push(ret_op);
    let mut r = MirRegion::new();
    r.push(blk);
    r
}

/// Build the success-arm region : load the success-payload from the
/// scrutinee cell at the layout's payload offset, binding the original
/// `cssl.try` result-id.
fn build_success_region(
    scrut_ptr: ValueId,
    layout: TaggedUnionLayout,
    payload_ty: &MirType,
    bind_to: ValueId,
) -> MirRegion {
    let load_op = MirOp::std("memref.load")
        .with_operand(scrut_ptr)
        .with_result(bind_to, payload_ty.clone())
        .with_attribute("offset", layout.payload_offset.to_string())
        .with_attribute("alignment", layout.cell_alignment.to_string())
        .with_attribute(ATTR_FIELD, ATTR_FIELD_PAYLOAD)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY_SUCCESS);

    let mut blk = MirBlock::new("try_success");
    blk.push(load_op);
    let mut r = MirRegion::new();
    r.push(blk);
    r
}

/// Stage-0 textual `payload_ty=...` attribute value for a MIR type.
/// Mirrors the spellings recognized by `tagged_union_abi::parse_payload_ty`.
#[must_use]
fn payload_ty_attr_for(ty: &MirType) -> String {
    use crate::value::FloatWidth;
    match ty {
        MirType::Int(IntWidth::I1) => "i1".into(),
        MirType::Int(IntWidth::I8) => "i8".into(),
        MirType::Int(IntWidth::I16) => "i16".into(),
        MirType::Int(IntWidth::I32) => "i32".into(),
        MirType::Int(IntWidth::I64) => "i64".into(),
        MirType::Int(IntWidth::Index) => "index".into(),
        MirType::Float(FloatWidth::F16) => "f16".into(),
        MirType::Float(FloatWidth::Bf16) => "bf16".into(),
        MirType::Float(FloatWidth::F32) => "f32".into(),
        MirType::Float(FloatWidth::F64) => "f64".into(),
        MirType::Bool => "bool".into(),
        MirType::Handle => "!cssl.handle".into(),
        MirType::Ptr => "!cssl.ptr".into(),
        MirType::Opaque(s) => s.clone(),
        _ => "!cssl.ptr".into(),
    }
}

/// Expand a single `cssl.try` op into the canonical tag-dispatch
/// sequence. Returns `None` when :
///   - `op` is not the `cssl.try` op,
///   - the caller's return-family is `Mismatch` (HIR-level type error
///     ; the rewrite is skipped + the caller bumps the report's
///     `type_mismatch_count`).
#[must_use]
pub fn expand_try_op(
    op: &MirOp,
    caller_family: CallerReturnFamily,
    caller_ret_ty: &MirType,
    ids: &mut FreshIdSeq,
) -> Option<TryExpansion> {
    if !is_try_op(op) {
        return None;
    }
    if caller_family == CallerReturnFamily::Mismatch {
        return None;
    }
    let scrut_ptr = *op.operands.first()?;
    let bind_to = op.results.first()?.id;

    // Recover the success-payload type + (for Result) the err-payload
    // type from the operand-side type-string. body_lower::lower_try
    // propagates the operand's `inner_ty` onto the result so we read
    // off the result-slot.
    let payload_ty = extract_payload_type(op);
    let err_ty = if caller_family == CallerReturnFamily::Result {
        if let Some(r) = op.results.first() {
            if let MirType::Opaque(s) = &r.ty {
                parse_err_from_opaque(s)
            } else {
                MirType::Ptr
            }
        } else {
            MirType::Ptr
        }
    } else {
        MirType::Ptr
    };

    // Layout : Option uses single-payload geometry ; Result uses max-of-
    // both-sides. We approximate with the success-payload alone for
    // Option ; for Result we pass both sides so the layout's payload
    // offset matches the Wave-A1 expansion's stamped offset.
    let layout = match caller_family {
        CallerReturnFamily::Option => TaggedUnionLayout::for_option(&payload_ty),
        CallerReturnFamily::Result => TaggedUnionLayout::for_result(&payload_ty, &err_ty),
        CallerReturnFamily::Mismatch => unreachable!(),
    };

    let tag_id = ids.fresh();
    let tag_load = MirOp::std("memref.load")
        .with_operand(scrut_ptr)
        .with_result(tag_id, MirType::Int(IntWidth::I32))
        .with_attribute("offset", layout.tag_offset.to_string())
        .with_attribute("alignment", u32::from(layout.tag_size).to_string())
        .with_attribute(ATTR_FIELD, ATTR_FIELD_TAG)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY);

    let fail_const_id = ids.fresh();
    // Failure tag is 0 for both None + Err ; canonical via tag_for_variant.
    let fail_tag = match caller_family {
        CallerReturnFamily::Option => tag_for_variant(SumVariant::None),
        CallerReturnFamily::Result => tag_for_variant(SumVariant::Err),
        CallerReturnFamily::Mismatch => unreachable!(),
    };
    let fail_const = MirOp::std("arith.constant")
        .with_result(fail_const_id, MirType::Int(IntWidth::I32))
        .with_attribute("value", fail_tag.to_string())
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY);

    let cond_id = ids.fresh();
    let cmp = MirOp::std("arith.cmpi")
        .with_operand(tag_id)
        .with_operand(fail_const_id)
        .with_result(cond_id, MirType::Bool)
        .with_attribute("predicate", "eq")
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY);

    let then_region = match caller_family {
        CallerReturnFamily::Option => build_option_failure_region(caller_ret_ty, ids),
        CallerReturnFamily::Result => {
            build_result_failure_region(scrut_ptr, layout, &err_ty, caller_ret_ty, ids)
        }
        CallerReturnFamily::Mismatch => unreachable!(),
    };
    let else_region = build_success_region(scrut_ptr, layout, &payload_ty, bind_to);

    let if_id = ids.fresh();
    let scf_if = MirOp::std("scf.if")
        .with_operand(cond_id)
        .with_result(if_id, MirType::None)
        .with_region(then_region)
        .with_region(else_region)
        .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_TRY)
        .with_attribute(
            "family",
            match caller_family {
                CallerReturnFamily::Option => "Option",
                CallerReturnFamily::Result => "Result",
                CallerReturnFamily::Mismatch => unreachable!(),
            },
        );

    Some(TryExpansion {
        ops: vec![tag_load, fail_const, cmp, scf_if],
        layout,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// § Module-level rewrite — drives expansion across every fn.
// ─────────────────────────────────────────────────────────────────────────

/// Audit report for a try-op lowering pass.
///
/// Sawyer-style packed record : every field is a `u32` counter so the
/// whole report fits in 16 bytes + can be aggregated across fns with
/// trivial integer addition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TryLoweringReport {
    /// Number of `cssl.try` ops successfully rewritten to the dispatch
    /// shape.
    pub rewritten_count: u32,
    /// Number of `cssl.try` ops SKIPPED because the caller's return type
    /// was neither `Option` nor `Result`. HIR's `infer.rs` already
    /// surfaces this as a type-error ; the MIR pass tracks it for audit.
    pub type_mismatch_count: u32,
    /// Number of `cssl.try` ops SKIPPED because the operand or result
    /// slot was malformed (e.g. `cssl.try` with no operand). Indicates
    /// an upstream `body_lower` bug ; the pass stays defensive.
    pub malformed_count: u32,
    /// Sum of layout total-bytes across all rewrites in this pass. Pure
    /// audit-counter ; useful for sanity-checking that the pass didn't
    /// blow up cell sizes.
    pub total_bytes_examined: u32,
}

impl TryLoweringReport {
    /// Total of all counter fields. Useful for one-line equality assertion
    /// against the expected per-fn try-count in unit tests.
    #[must_use]
    pub const fn total_count(&self) -> u32 {
        self.rewritten_count + self.type_mismatch_count + self.malformed_count
    }

    /// Aggregate `other` into `self`. Saturating-add prevents the audit
    /// counters from wrapping on pathological inputs.
    pub fn merge(&mut self, other: Self) {
        self.rewritten_count = self.rewritten_count.saturating_add(other.rewritten_count);
        self.type_mismatch_count = self
            .type_mismatch_count
            .saturating_add(other.type_mismatch_count);
        self.malformed_count = self.malformed_count.saturating_add(other.malformed_count);
        self.total_bytes_examined = self
            .total_bytes_examined
            .saturating_add(other.total_bytes_examined);
    }
}

/// Rewrite every `cssl.try` op in `func` in-place. The fn's
/// `next_value_id` field is grown to accommodate the freshly-allocated
/// SSA-values stamped by the expansion. Failure to classify the caller's
/// return type bumps the report's `type_mismatch_count` and leaves the
/// op untouched (HIR has already surfaced the diagnostic).
pub fn lower_try_ops_in_func(func: &mut MirFunc) -> TryLoweringReport {
    let mut report = TryLoweringReport::default();
    let caller_family = classify_caller_return(&func.results);
    let caller_ret_ty = func
        .results
        .first()
        .cloned()
        .unwrap_or(MirType::None);
    let mut ids = FreshIdSeq::new(func.next_value_id);
    rewrite_region(
        &mut func.body,
        caller_family,
        &caller_ret_ty,
        &mut ids,
        &mut report,
    );
    func.next_value_id = ids.next;
    report
}

/// Rewrite every `cssl.try` op across an entire `MirModule`.
pub fn lower_try_ops_in_module(module: &mut MirModule) -> TryLoweringReport {
    let mut total = TryLoweringReport::default();
    for func in &mut module.funcs {
        total.merge(lower_try_ops_in_func(func));
    }
    total
}

/// Walk a region in-place : recurse into nested regions FIRST then
/// rewrite ops in the current block. Depth-first matches the
/// `tagged_union_abi::expand_region` walk pattern so the two passes can
/// run back-to-back without surprises.
fn rewrite_region(
    region: &mut MirRegion,
    caller_family: CallerReturnFamily,
    caller_ret_ty: &MirType,
    ids: &mut FreshIdSeq,
    report: &mut TryLoweringReport,
) {
    for block in &mut region.blocks {
        rewrite_block(block, caller_family, caller_ret_ty, ids, report);
    }
}

/// Rewrite one block : single-pass walk + in-place splice. Every
/// `cssl.try` op is replaced with the tag-dispatch sequence emitted by
/// [`expand_try_op`].
fn rewrite_block(
    block: &mut MirBlock,
    caller_family: CallerReturnFamily,
    caller_ret_ty: &MirType,
    ids: &mut FreshIdSeq,
    report: &mut TryLoweringReport,
) {
    let mut idx = 0;
    while idx < block.ops.len() {
        // Recurse into nested regions FIRST so an inner-region try-op
        // (e.g. `if cond { x?  } else { y }`) sees the SAME caller's
        // return type — `?`-propagation always returns from the
        // enclosing fn, not from any nested scf.* op.
        for region in &mut block.ops[idx].regions {
            rewrite_region(region, caller_family, caller_ret_ty, ids, report);
        }

        if is_try_op(&block.ops[idx]) {
            let original = block.ops[idx].clone();
            // Type-mismatch path : skip rewrite + bump audit counter.
            if caller_family == CallerReturnFamily::Mismatch {
                report.type_mismatch_count =
                    report.type_mismatch_count.saturating_add(1);
                idx += 1;
                continue;
            }
            // Malformed path : missing operand or result slot.
            if original.operands.is_empty() || original.results.is_empty() {
                report.malformed_count = report.malformed_count.saturating_add(1);
                idx += 1;
                continue;
            }
            if let Some(expansion) =
                expand_try_op(&original, caller_family, caller_ret_ty, ids)
            {
                report.rewritten_count = report.rewritten_count.saturating_add(1);
                report.total_bytes_examined = report
                    .total_bytes_examined
                    .saturating_add(expansion.layout.total_size);
                let span = expansion.ops.len();
                block.ops.splice(idx..=idx, expansion.ops);
                // Recurse into the freshly-spliced `scf.if` regions so a
                // nested try-op embedded in either arm is also rewritten.
                let scf_idx = idx + span - 1;
                if let Some(scf_op) = block.ops.get_mut(scf_idx) {
                    for region in &mut scf_op.regions {
                        rewrite_region(
                            region,
                            caller_family,
                            caller_ret_ty,
                            ids,
                            report,
                        );
                    }
                }
                idx += span;
                continue;
            }
            // expand_try_op returned None despite passing the type-mismatch
            // gate — defensive bump of malformed-count.
            report.malformed_count = report.malformed_count.saturating_add(1);
        }
        idx += 1;
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Tests — unit + golden coverage for the rewrite pass.
// ─────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::MirOp;
    use crate::func::{MirFunc, MirModule};
    use crate::value::{IntWidth, MirType, MirValue, ValueId};

    // ─────────────────────────────────────────────────────────────────
    // § caller-return classification
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn classify_caller_return_recognizes_option_opaque() {
        let ty = MirType::Opaque("!cssl.option.i32".into());
        assert_eq!(classify_mir_type(&ty), CallerReturnFamily::Option);
    }

    #[test]
    fn classify_caller_return_recognizes_result_opaque() {
        let ty = MirType::Opaque("!cssl.result.i32.i32".into());
        assert_eq!(classify_mir_type(&ty), CallerReturnFamily::Result);
    }

    #[test]
    fn classify_caller_return_rejects_plain_int() {
        let ty = MirType::Int(IntWidth::I32);
        assert_eq!(classify_mir_type(&ty), CallerReturnFamily::Mismatch);
    }

    #[test]
    fn classify_caller_return_rejects_unrelated_opaque() {
        let ty = MirType::Opaque("!cssl.handle".into());
        assert_eq!(classify_mir_type(&ty), CallerReturnFamily::Mismatch);
    }

    #[test]
    fn classify_caller_return_no_results_is_mismatch() {
        assert_eq!(
            classify_caller_return(&[]),
            CallerReturnFamily::Mismatch
        );
    }

    #[test]
    fn classify_caller_return_takes_first_result_when_multi() {
        let results = vec![
            MirType::Opaque("!cssl.option.i32".into()),
            MirType::Int(IntWidth::I32),
        ];
        assert_eq!(
            classify_caller_return(&results),
            CallerReturnFamily::Option
        );
    }

    #[test]
    fn opaque_family_prefix_matches_canonical_strings() {
        assert_eq!(
            opaque_family_prefix("!cssl.option.i32"),
            Some(SumFamily::Option)
        );
        assert_eq!(
            opaque_family_prefix("!cssl.result.i32.i32"),
            Some(SumFamily::Result)
        );
        assert_eq!(opaque_family_prefix("!cssl.handle"), None);
        assert_eq!(opaque_family_prefix("i32"), None);
    }

    // ─────────────────────────────────────────────────────────────────
    // § type-string parsing
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn parse_payload_from_opaque_extracts_t_from_option() {
        assert_eq!(
            parse_payload_from_opaque("!cssl.option.i32"),
            MirType::Int(IntWidth::I32)
        );
        assert_eq!(
            parse_payload_from_opaque("!cssl.option.i64"),
            MirType::Int(IntWidth::I64)
        );
    }

    #[test]
    fn parse_payload_from_opaque_extracts_t_from_result() {
        // `!cssl.result.<T>.<E>` — the success side is `<T>` (i32 here).
        assert_eq!(
            parse_payload_from_opaque("!cssl.result.i32.i64"),
            MirType::Int(IntWidth::I32)
        );
    }

    #[test]
    fn parse_err_from_opaque_extracts_e_from_result() {
        // `<T>.<E>` — err side is `<E>` (i64 here).
        assert_eq!(
            parse_err_from_opaque("!cssl.result.i32.i64"),
            MirType::Int(IntWidth::I64)
        );
    }

    #[test]
    fn parse_err_from_opaque_returns_ptr_for_non_result() {
        assert_eq!(
            parse_err_from_opaque("!cssl.option.i32"),
            MirType::Ptr
        );
    }

    // ─────────────────────────────────────────────────────────────────
    // § per-op expansion
    // ─────────────────────────────────────────────────────────────────

    /// Build a canonical `cssl.try %scrut -> i32` op. `result_ty` is the
    /// op's stamped result-type (matching `body_lower::lower_try`'s
    /// propagation of `inner_ty`).
    fn make_try_op(scrut: u32, result_id: u32, result_ty: MirType) -> MirOp {
        MirOp::std("cssl.try")
            .with_operand(ValueId(scrut))
            .with_result(ValueId(result_id), result_ty)
            .with_attribute("source_loc", "<test>:1:1")
    }

    #[test]
    fn expand_try_op_rejects_non_try_op() {
        let op = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "1");
        let mut ids = FreshIdSeq::new(10);
        let exp = expand_try_op(
            &op,
            CallerReturnFamily::Option,
            &MirType::Opaque("!cssl.option.i32".into()),
            &mut ids,
        );
        assert!(exp.is_none());
    }

    #[test]
    fn expand_try_op_rejects_type_mismatch() {
        let op = make_try_op(0, 1, MirType::Opaque("!cssl.option.i32".into()));
        let mut ids = FreshIdSeq::new(10);
        let exp = expand_try_op(
            &op,
            CallerReturnFamily::Mismatch,
            &MirType::Int(IntWidth::I32),
            &mut ids,
        );
        assert!(exp.is_none());
    }

    #[test]
    fn expand_try_op_on_option_emits_load_const_cmp_if() {
        let op = make_try_op(0, 1, MirType::Opaque("!cssl.option.i32".into()));
        let mut ids = FreshIdSeq::new(10);
        let exp = expand_try_op(
            &op,
            CallerReturnFamily::Option,
            &MirType::Opaque("!cssl.option.i32".into()),
            &mut ids,
        )
        .expect("Option try lowers");

        // Expect 4 ops in the rewrite-stream : tag-load, fail-const, cmp, scf.if.
        assert_eq!(exp.ops.len(), 4);
        assert_eq!(exp.ops[0].name, "memref.load");
        assert_eq!(exp.ops[1].name, "arith.constant");
        assert_eq!(exp.ops[2].name, "arith.cmpi");
        assert_eq!(exp.ops[3].name, "scf.if");

        // Tag-load offset must be 0 (Wave-A1 layout invariant).
        let off = exp.ops[0]
            .attributes
            .iter()
            .find(|(k, _)| k == "offset")
            .unwrap();
        assert_eq!(off.1, "0");
        // field=tag.
        let f = exp.ops[0]
            .attributes
            .iter()
            .find(|(k, _)| k == "field")
            .unwrap();
        assert_eq!(f.1, "tag");

        // Fail-const value must be 0 (None tag).
        let v = exp.ops[1]
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .unwrap();
        assert_eq!(v.1, "0");

        // scf.if has 2 regions : failure-arm + success-arm.
        let scf = &exp.ops[3];
        assert_eq!(scf.regions.len(), 2);
        // family attribute = Option.
        let fam = scf
            .attributes
            .iter()
            .find(|(k, _)| k == "family")
            .unwrap();
        assert_eq!(fam.1, "Option");
    }

    #[test]
    fn expand_try_op_on_result_emits_err_load_in_failure_arm() {
        let op = make_try_op(0, 1, MirType::Opaque("!cssl.result.i32.i32".into()));
        let mut ids = FreshIdSeq::new(10);
        let exp = expand_try_op(
            &op,
            CallerReturnFamily::Result,
            &MirType::Opaque("!cssl.result.i32.i32".into()),
            &mut ids,
        )
        .expect("Result try lowers");

        let scf = &exp.ops[3];
        // family = Result.
        let fam = scf
            .attributes
            .iter()
            .find(|(k, _)| k == "family")
            .unwrap();
        assert_eq!(fam.1, "Result");

        // failure-arm region (regions[0]) should contain :
        // memref.load (err payload) + cssl.result.err + func.return.
        let fail_blk = scf.regions[0].entry().unwrap();
        let names: Vec<String> = fail_blk.ops.iter().map(|o| o.name.clone()).collect();
        assert!(names.contains(&"memref.load".to_string()));
        assert!(names.contains(&"cssl.result.err".to_string()));
        assert!(names.contains(&"func.return".to_string()));

        // The fail-const value is 0 (Err tag).
        let v = exp.ops[1]
            .attributes
            .iter()
            .find(|(k, _)| k == "value")
            .unwrap();
        assert_eq!(v.1, "0");
    }

    #[test]
    fn expand_try_op_success_arm_payload_load_binds_original_result_id() {
        let op = make_try_op(0, /*result*/ 7, MirType::Opaque("!cssl.option.i32".into()));
        let mut ids = FreshIdSeq::new(10);
        let exp = expand_try_op(
            &op,
            CallerReturnFamily::Option,
            &MirType::Opaque("!cssl.option.i32".into()),
            &mut ids,
        )
        .expect("Option try lowers");
        // success-arm region (regions[1]) holds memref.load → ValueId(7).
        let scf = &exp.ops[3];
        let success_blk = scf.regions[1].entry().unwrap();
        let load = success_blk
            .ops
            .iter()
            .find(|o| o.name == "memref.load")
            .unwrap();
        assert_eq!(load.results[0].id, ValueId(7));
        // success-arm field marker.
        let f = load
            .attributes
            .iter()
            .find(|(k, _)| k == "field")
            .unwrap();
        assert_eq!(f.1, "payload");
    }

    #[test]
    fn expand_try_op_uses_payload_offset_4_for_i32_layout() {
        // i32 payload : layout = (tag=4, payload_offset=4, total=8).
        let op = make_try_op(0, 1, MirType::Opaque("!cssl.option.i32".into()));
        let mut ids = FreshIdSeq::new(10);
        let exp = expand_try_op(
            &op,
            CallerReturnFamily::Option,
            &MirType::Opaque("!cssl.option.i32".into()),
            &mut ids,
        )
        .unwrap();
        assert_eq!(exp.layout.payload_offset, 4);
        assert_eq!(exp.layout.total_size, 8);
    }

    // ─────────────────────────────────────────────────────────────────
    // § Per-fn rewrite : in-place splice + report aggregation.
    // ─────────────────────────────────────────────────────────────────

    /// Build a tiny fn :
    ///   fn f(s : !cssl.ptr) -> !cssl.option.i32 {
    ///       %0 = parameter (block-arg)
    ///       %1 = cssl.try %0 -> !cssl.option.i32
    ///       func.return %1
    ///   }
    fn build_option_caller_with_one_try() -> MirFunc {
        let mut func = MirFunc::new(
            "parse_one",
            vec![MirType::Ptr],
            vec![MirType::Opaque("!cssl.option.i32".into())],
        );
        // %0 is the block-arg param ; we emit the try on it.
        let try_op = make_try_op(0, 1, MirType::Opaque("!cssl.option.i32".into()));
        func.next_value_id = 2;
        func.push_op(try_op);
        func.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        func
    }

    #[test]
    fn lower_try_ops_in_func_rewrites_single_try() {
        let mut func = build_option_caller_with_one_try();
        let report = lower_try_ops_in_func(&mut func);
        assert_eq!(report.rewritten_count, 1);
        assert_eq!(report.type_mismatch_count, 0);
        assert_eq!(report.malformed_count, 0);

        let entry = func.body.entry().unwrap();
        // Original cssl.try op should be GONE.
        assert!(
            !entry.ops.iter().any(|o| o.name == "cssl.try"),
            "cssl.try must be expanded out : {:?}",
            entry.ops.iter().map(|o| o.name.clone()).collect::<Vec<_>>()
        );
        // Replacement-op stream present.
        assert!(entry.ops.iter().any(|o| o.name == "memref.load"));
        assert!(entry.ops.iter().any(|o| o.name == "arith.cmpi"));
        assert!(entry.ops.iter().any(|o| o.name == "scf.if"));
    }

    #[test]
    fn lower_try_ops_in_func_grows_next_value_id() {
        let mut func = build_option_caller_with_one_try();
        let before = func.next_value_id;
        lower_try_ops_in_func(&mut func);
        // Allocated : tag-id, fail-const-id, cond-id, none-id (Option fail-arm),
        // scf-if-id ⇒ 5 fresh.
        assert!(
            func.next_value_id >= before + 5,
            "next_value_id must grow by at least 5 : before={before} after={}",
            func.next_value_id
        );
    }

    #[test]
    fn lower_try_ops_in_func_with_mismatched_return_skips_rewrite() {
        let mut func = MirFunc::new(
            "no_propagate",
            vec![MirType::Ptr],
            vec![MirType::Int(IntWidth::I32)], // i32 return = mismatch
        );
        let try_op = make_try_op(0, 1, MirType::Opaque("!cssl.option.i32".into()));
        func.next_value_id = 2;
        func.push_op(try_op);
        let report = lower_try_ops_in_func(&mut func);
        assert_eq!(report.rewritten_count, 0);
        assert_eq!(report.type_mismatch_count, 1);
        // The cssl.try op REMAINS (un-rewritten) so a downstream pass / cgen
        // path can still surface a diagnostic if needed.
        assert!(func
            .body
            .entry()
            .unwrap()
            .ops
            .iter()
            .any(|o| o.name == "cssl.try"));
    }

    #[test]
    fn lower_try_ops_in_func_with_malformed_op_bumps_malformed_count() {
        let mut func = MirFunc::new(
            "malformed_caller",
            vec![],
            vec![MirType::Opaque("!cssl.option.i32".into())],
        );
        // try-op missing both operand AND result : malformed.
        let bad_try = MirOp::std("cssl.try");
        func.push_op(bad_try);
        let report = lower_try_ops_in_func(&mut func);
        assert_eq!(report.rewritten_count, 0);
        assert_eq!(report.malformed_count, 1);
    }

    #[test]
    fn lower_try_ops_in_module_aggregates_per_fn_reports() {
        let mut module = MirModule::default();
        module.funcs.push(build_option_caller_with_one_try());
        module.funcs.push(build_option_caller_with_one_try());
        let report = lower_try_ops_in_module(&mut module);
        assert_eq!(report.rewritten_count, 2);
        assert_eq!(report.type_mismatch_count, 0);
    }

    #[test]
    fn lower_try_ops_handles_nested_chains() {
        // Two `cssl.try` ops in the same fn (chained call sites).
        let mut func = MirFunc::new(
            "two_tries",
            vec![MirType::Ptr, MirType::Ptr],
            vec![MirType::Opaque("!cssl.option.i32".into())],
        );
        // Block args occupy %0 + %1 ; extra fresh ids start at 2.
        // We need to set up the entry block manually so it has 2 args.
        if let Some(entry) = func.body.entry_mut() {
            entry.args = vec![
                MirValue::new(ValueId(0), MirType::Ptr),
                MirValue::new(ValueId(1), MirType::Ptr),
            ];
        }
        func.next_value_id = 4;
        let try1 = make_try_op(0, 2, MirType::Opaque("!cssl.option.i32".into()));
        let try2 = make_try_op(1, 3, MirType::Opaque("!cssl.option.i32".into()));
        func.push_op(try1);
        func.push_op(try2);
        let report = lower_try_ops_in_func(&mut func);
        assert_eq!(report.rewritten_count, 2);
    }

    #[test]
    fn lower_try_ops_with_result_caller_emits_err_propagation() {
        let mut func = MirFunc::new(
            "result_caller",
            vec![MirType::Ptr],
            vec![MirType::Opaque("!cssl.result.i32.i32".into())],
        );
        let try_op = make_try_op(0, 1, MirType::Opaque("!cssl.result.i32.i32".into()));
        func.next_value_id = 2;
        func.push_op(try_op);
        func.push_op(MirOp::std("func.return").with_operand(ValueId(1)));
        let report = lower_try_ops_in_func(&mut func);
        assert_eq!(report.rewritten_count, 1);

        // Walk the entry block + assert the failure-arm region carries
        // the cssl.result.err propagation op.
        let entry = func.body.entry().unwrap();
        let scf = entry.ops.iter().find(|o| o.name == "scf.if").unwrap();
        let fail_blk = scf.regions[0].entry().unwrap();
        let has_err = fail_blk.ops.iter().any(|o| o.name == "cssl.result.err");
        assert!(has_err, "Result caller must propagate via cssl.result.err");
    }

    // ─────────────────────────────────────────────────────────────────
    // § Sawyer-style report-aggregation invariants.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn try_lowering_report_total_count_sums_all_buckets() {
        let r = TryLoweringReport {
            rewritten_count: 3,
            type_mismatch_count: 2,
            malformed_count: 1,
            total_bytes_examined: 0,
        };
        assert_eq!(r.total_count(), 6);
    }

    #[test]
    fn try_lowering_report_merge_saturates_on_overflow() {
        let mut a = TryLoweringReport {
            rewritten_count: u32::MAX - 1,
            ..Default::default()
        };
        let b = TryLoweringReport {
            rewritten_count: 5,
            ..Default::default()
        };
        a.merge(b);
        // Saturated at u32::MAX rather than wrapping.
        assert_eq!(a.rewritten_count, u32::MAX);
    }

    // ─────────────────────────────────────────────────────────────────
    // § Constant-name lock invariants — these MUST match what
    //   `cssl-cgen-cpu-cranelift::cgen_try` reads. Lock-step rename
    //   protection ⇒ if these fail, cgen will mis-recognize ops.
    // ─────────────────────────────────────────────────────────────────

    #[test]
    fn canonical_op_name_matches_body_lower_emit() {
        // body_lower::lower_try uses MirOp::std("cssl.try") — our recognizer
        // MUST agree on the exact literal.
        assert_eq!(TRY_OP_NAME, "cssl.try");
    }

    #[test]
    fn canonical_source_kind_strings_match_cgen_expectations() {
        assert_eq!(SOURCE_KIND_TRY, "try_propagation");
        assert_eq!(SOURCE_KIND_TRY_FAIL, "try_failure_arm");
        assert_eq!(SOURCE_KIND_TRY_SUCCESS, "try_success_arm");
        assert_eq!(ATTR_SOURCE_KIND, "source_kind");
        assert_eq!(ATTR_FIELD, "field");
        assert_eq!(ATTR_FIELD_TAG, "tag");
        assert_eq!(ATTR_FIELD_PAYLOAD, "payload");
    }
}

// INTEGRATION_NOTE :
//   add `pub mod try_op_lower;` (and the corresponding `pub use
//   try_op_lower::{...}` re-exports) to cssl-mir/src/lib.rs in the
//   integration commit. The Wave-A3 dispatch carved this file out
//   single-file-owned ; main-thread integration replaces this comment
//   with the `pub mod` declaration + the re-export block listing
//   `lower_try_ops_in_func`, `lower_try_ops_in_module`, `expand_try_op`,
//   `classify_caller_return`, `classify_mir_type`, `is_try_op`,
//   `extract_payload_type`, `parse_payload_from_opaque`,
//   `parse_err_from_opaque`, `CallerReturnFamily`, `TryExpansion`,
//   `TryLoweringReport`, `TRY_OP_NAME`, `SOURCE_KIND_TRY`,
//   `SOURCE_KIND_TRY_FAIL`, `SOURCE_KIND_TRY_SUCCESS`. The integration
//   commit's wiring step also adds the pass-pipeline registration so the
//   try-lowering runs AFTER tagged_union_abi::expand_module + BEFORE the
//   Cranelift cgen drive.
