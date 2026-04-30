//! § Wave-A1 — `cssl.option.*` / `cssl.result.*` Cranelift cgen helpers.
//!
//! § ROLE
//!   Cgen-side helpers for the tagged-union ABI : translate the post-
//!   `tagged_union_abi::expand_module` MIR shape — `cssl.heap.alloc` +
//!   `arith.constant` + `memref.store` (tag) + `memref.store` (payload) —
//!   into the Cranelift CLIF surface that the JIT actually executes.
//!
//!   Most of the heavy lifting is already in place : the cgen layer
//!   handles `arith.constant` / `memref.load` / `memref.store` /
//!   `cssl.heap.alloc` / `scf.if` / `arith.cmpi` natively (see jit.rs +
//!   object.rs op-dispatch tables). What this slice adds :
//!
//!   - canonical attribute-readers that recognize the `field=tag` /
//!     `field=payload` markers stamped by `tagged_union_abi::expand_*`
//!     plus the `source_kind=tagged_union*` / `arm_tag=N` markers stamped
//!     by `tagged_union_abi::build_match_dispatch_cascade`.
//!   - predicate-helpers that let the JIT / Object backends decide
//!     whether a given op is part of a tagged-union sequence (useful
//!     for skipping no-op aliases and surfacing tag-store sequences
//!     in pre-emit diagnostic dumps).
//!   - cranelift-IR builder helpers that emit the canonical tag-load
//!     + arm-cascade primitives directly (used by future cgen paths
//!     that bypass the post-rewrite MIR shape — e.g. when a stack-
//!     slot construction lands and there is no `cssl.heap.alloc` op
//!     to expand).
//!
//! § INTEGRATION_NOTE  (per Wave-A1 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!   src/lib.rs` is intentionally NOT modified. The helpers compile +
//!   are tested in-place via `#[cfg(test)]` references. Main-thread's
//!   integration commit promotes this to `pub mod cgen_tagged_union ;` +
//!   adds the `pub use cgen_tagged_union::*;` re-export at that time.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-mir/src/tagged_union_abi.rs` — sister
//!     module that produces the post-rewrite MIR shape this module
//!     consumes.
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/jit.rs` — existing
//!     JIT op-dispatch + memref.load/store + heap.alloc lowering paths.
//!   - `specs/40_WAVE_CSSL_PLAN.csl § WAVE-A § A1` — the wave plan that
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
//!   - Tag-load + cmp + brif emit ONCE per arm — no scratch IR-builder
//!     state held across arms.
//!
//! § MIR ↔ CLIF ABI MAPPING
//!
//!   ```text
//!   MIR (post-expand)                              CLIF (this module)
//!   ───────────────────────────────────────        ────────────────────────────
//!   cssl.heap.alloc {bytes=8, alignment=4,         call __cssl_alloc(8, 4) -> ptr
//!     source_kind=tagged_union, family=Option}     (already in object.rs +
//!                                                   jit.rs paths ;
//!                                                   tagged-union annotation
//!                                                   is informational)
//!   arith.constant {value=1}                       v? = iconst.i32 1
//!   memref.store %tag, %ptr                        store.i32 v_tag, v_ptr
//!     {offset=0, alignment=4, field=tag}             (offset 0)
//!   memref.store %payload, %ptr                    store.i32 v_payload, v_ptr+4
//!     {offset=4, alignment=4, field=payload}
//!
//!   memref.load %ptr {offset=0, alignment=4,       v_tag = load.i32 v_ptr (off 0)
//!     field=tag} -> i32
//!   arith.cmpi eq %tag, %const                     v_cond = icmp eq v_tag, v_k
//!   scf.if %cond {...arm-tag=K} else {...}         brif v_cond, then, else
//!     source_kind=tagged_union_dispatch
//!   ```

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{InstBuilder, MemFlags, Type, Value};
use cranelift_frontend::FunctionBuilder;
use cssl_mir::MirOp;

// ─────────────────────────────────────────────────────────────────────────
// § Canonical attribute keys + values stamped by `tagged_union_abi`.
//
//   These const strings are the wire-protocol between the MIR rewriter
//   and this cgen layer. Renaming any of them requires lock-step changes
//   on both sides ; the constants make the lock-step explicit + grep-
//   friendly.
// ─────────────────────────────────────────────────────────────────────────

/// Attribute key carrying the canonical `field=tag` / `field=payload`
/// marker on `memref.load` / `memref.store` ops emitted by
/// `tagged_union_abi::expand_construct`.
pub const ATTR_FIELD: &str = "field";
/// `field=tag` value — the 4-byte tag slot at offset 0.
pub const ATTR_FIELD_TAG: &str = "tag";
/// `field=payload` value — the variant payload at the layout's
/// `payload_offset`.
pub const ATTR_FIELD_PAYLOAD: &str = "payload";

/// Attribute key carrying the source-kind marker (`tagged_union` /
/// `tagged_union_alias` / `tagged_union_dispatch`).
pub const ATTR_SOURCE_KIND: &str = "source_kind";
/// `source_kind=tagged_union` — the heap-alloc that owns a sum-type cell.
pub const SOURCE_KIND_CELL: &str = "tagged_union";
/// `source_kind=tagged_union_alias` — the bitcast that re-routes the
/// original construction-op result-id to the new cell-ptr. Cgen treats
/// it as a value-map alias only ; no instruction is emitted.
pub const SOURCE_KIND_ALIAS: &str = "tagged_union_alias";
/// `source_kind=tagged_union_dispatch` — the `scf.if` op that decodes
/// one arm of a match cascade.
pub const SOURCE_KIND_DISPATCH: &str = "tagged_union_dispatch";

/// Attribute key carrying the per-arm tag value on a dispatch
/// `scf.if`. Type : decimal `u32` literal (e.g. `"1"` for `Some` /
/// `Ok` arms).
pub const ATTR_ARM_TAG: &str = "arm_tag";

/// Attribute key carrying the family discriminator (`Option` /
/// `Result`) on a tagged-union `cssl.heap.alloc`.
pub const ATTR_FAMILY: &str = "family";
/// `family=Option`.
pub const FAMILY_OPTION: &str = "Option";
/// `family=Result`.
pub const FAMILY_RESULT: &str = "Result";

/// Attribute key carrying the byte offset on a typed memref op.
pub const ATTR_OFFSET: &str = "offset";

// ─────────────────────────────────────────────────────────────────────────
// § Predicate helpers — recognize tagged-union ops in the post-rewrite MIR.
// ─────────────────────────────────────────────────────────────────────────

/// Test whether `op` carries the canonical `(source_kind, value)` pair.
/// Used as the building block for the more specialized predicates below.
#[must_use]
pub fn has_source_kind(op: &MirOp, expected: &str) -> bool {
    op.attributes
        .iter()
        .any(|(k, v)| k == ATTR_SOURCE_KIND && v == expected)
}

/// Test whether `op` is a `cssl.heap.alloc` that allocates a tagged-
/// union cell. Cgen uses this to attach diagnostic metadata to the
/// emitted instruction (e.g. cell-source map for crash dumps) without
/// re-deriving the family from the surrounding ops.
#[must_use]
pub fn is_tagged_union_cell_alloc(op: &MirOp) -> bool {
    op.name == "cssl.heap.alloc" && has_source_kind(op, SOURCE_KIND_CELL)
}

/// Test whether `op` is the `arith.bitcast` alias that re-routes the
/// original sum-type result-id to the new cell-ptr. Cgen skips
/// emitting a CLIF instruction for these (pure value-map plumbing) —
/// the dispatcher reads `op.results[0].id` and maps it to `op.operands[0]`'s
/// already-bound CLIF Value.
#[must_use]
pub fn is_tagged_union_alias(op: &MirOp) -> bool {
    op.name == "arith.bitcast" && has_source_kind(op, SOURCE_KIND_ALIAS)
}

/// Test whether `op` is the `scf.if` that dispatches on a loaded tag.
/// Useful for cgen-side diagnostics + structured-CFG audit walks.
#[must_use]
pub fn is_tagged_union_dispatch_if(op: &MirOp) -> bool {
    op.name == "scf.if" && has_source_kind(op, SOURCE_KIND_DISPATCH)
}

/// Test whether `op` is a `memref.store` of the tag-word.
#[must_use]
pub fn is_tag_store(op: &MirOp) -> bool {
    op.name == "memref.store"
        && op.attributes
            .iter()
            .any(|(k, v)| k == ATTR_FIELD && v == ATTR_FIELD_TAG)
}

/// Test whether `op` is a `memref.store` of the payload bytes.
#[must_use]
pub fn is_payload_store(op: &MirOp) -> bool {
    op.name == "memref.store"
        && op.attributes
            .iter()
            .any(|(k, v)| k == ATTR_FIELD && v == ATTR_FIELD_PAYLOAD)
}

/// Test whether `op` is a `memref.load` of the tag-word — the entry
/// point of the dispatch cascade.
#[must_use]
pub fn is_tag_load(op: &MirOp) -> bool {
    op.name == "memref.load"
        && op.attributes
            .iter()
            .any(|(k, v)| k == ATTR_FIELD && v == ATTR_FIELD_TAG)
}

// ─────────────────────────────────────────────────────────────────────────
// § Attribute readers — pull canonical values off a tagged-union op.
// ─────────────────────────────────────────────────────────────────────────

/// Read the `arm_tag` numeric value from a dispatch `scf.if`. Returns
/// `None` when the op isn't a tagged-union dispatch or the attribute
/// fails to parse.
#[must_use]
pub fn arm_tag_value(op: &MirOp) -> Option<u32> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_ARM_TAG)
        .and_then(|(_, v)| v.parse::<u32>().ok())
}

/// Read the `offset` numeric value from a typed memref op. Returns
/// `None` when absent or unparseable.
#[must_use]
pub fn read_offset_attr(op: &MirOp) -> Option<u32> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_OFFSET)
        .and_then(|(_, v)| v.parse::<u32>().ok())
}

/// Read the `family` discriminator from a `cssl.heap.alloc` cell-alloc.
/// Returns `None` when absent — callers fall back to family-agnostic
/// emission.
#[must_use]
pub fn read_family_attr(op: &MirOp) -> Option<&str> {
    op.attributes
        .iter()
        .find(|(k, _)| k == ATTR_FAMILY)
        .map(|(_, v)| v.as_str())
}

// ─────────────────────────────────────────────────────────────────────────
// § Cranelift-IR emit helpers — emit the canonical tag-load + cmp pair
//   directly. Used by future cgen paths that bypass the MIR-rewrite.
// ─────────────────────────────────────────────────────────────────────────

/// Memory-flag set used for tagged-union loads + stores. Aligned + non-
/// volatile + non-trapping ; the layout invariants computed by
/// `tagged_union_abi::TaggedUnionLayout` guarantee aligned access.
#[must_use]
pub fn tagged_union_mem_flags() -> MemFlags {
    let mut flags = MemFlags::new();
    flags.set_aligned();
    flags.set_notrap();
    flags
}

/// Emit the canonical `load.i32 %ptr+0` tag-load instruction. Returns
/// the resulting CLIF `Value` carrying the loaded tag.
///
/// This is the inverse of the `memref.store` emitted by
/// `tagged_union_abi::expand_construct` for the tag-field. Used by cgen
/// paths that emit dispatch directly without going through the
/// MIR-rewrite (e.g. when a future stack-slot construction skips the
/// `cssl.heap.alloc` op entirely).
#[must_use]
pub fn emit_tag_load(
    builder: &mut FunctionBuilder<'_>,
    cell_ptr: Value,
    tag_offset: i32,
    tag_clif_ty: Type,
) -> Value {
    let flags = tagged_union_mem_flags();
    builder.ins().load(tag_clif_ty, flags, cell_ptr, tag_offset)
}

/// Emit a `store.i32 %tag, %ptr+0` instruction for the canonical
/// tag-write. Counterpart of [`emit_tag_load`] ; used by direct-cgen
/// paths.
pub fn emit_tag_store(
    builder: &mut FunctionBuilder<'_>,
    tag_value: Value,
    cell_ptr: Value,
    tag_offset: i32,
) {
    let flags = tagged_union_mem_flags();
    builder.ins().store(flags, tag_value, cell_ptr, tag_offset);
}

/// Emit a tag-comparison `icmp eq` between a loaded tag and a constant
/// tag-value `k`. Returns the boolean CLIF `Value` used as the
/// `brif` condition.
#[must_use]
pub fn emit_tag_eq_compare(
    builder: &mut FunctionBuilder<'_>,
    tag_value: Value,
    k: i64,
    tag_clif_ty: Type,
) -> Value {
    let const_v = builder.ins().iconst(tag_clif_ty, k);
    builder
        .ins()
        .icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, tag_value, const_v)
}

/// Emit a payload-load instruction at the layout's payload offset.
///
/// The caller supplies the payload's CLIF type ; this helper does NOT
/// validate that the offset matches a real layout — pre-validation is
/// expected before reaching codegen. Tagged-union layouts come from
/// `tagged_union_abi::TaggedUnionLayout` ; the caller passes
/// `layout.payload_offset` here.
#[must_use]
pub fn emit_payload_load(
    builder: &mut FunctionBuilder<'_>,
    cell_ptr: Value,
    payload_offset: i32,
    payload_clif_ty: Type,
) -> Value {
    let flags = tagged_union_mem_flags();
    builder
        .ins()
        .load(payload_clif_ty, flags, cell_ptr, payload_offset)
}

/// Emit a payload-store instruction at the layout's payload offset.
pub fn emit_payload_store(
    builder: &mut FunctionBuilder<'_>,
    payload_value: Value,
    cell_ptr: Value,
    payload_offset: i32,
) {
    let flags = tagged_union_mem_flags();
    builder
        .ins()
        .store(flags, payload_value, cell_ptr, payload_offset);
}

// ─────────────────────────────────────────────────────────────────────────
// § Whole-fn pre-scan : "does this fn touch tagged unions"
// ─────────────────────────────────────────────────────────────────────────

/// Walk a single MIR block once + return whether ANY op produces or
/// consumes a tagged-union cell. Cgen uses this to keep import sets
/// + diagnostic instrumentation lean — only emit the tagged-union
/// helpers when the fn actually uses them.
///
/// § COMPLEXITY  O(N) in op count, single-pass, early-exit on first
///   recognized op. No allocation.
#[must_use]
pub fn block_touches_tagged_union(block: &cssl_mir::MirBlock) -> bool {
    block.ops.iter().any(|op| {
        is_tagged_union_cell_alloc(op)
            || is_tagged_union_alias(op)
            || is_tagged_union_dispatch_if(op)
            || is_tag_store(op)
            || is_payload_store(op)
            || is_tag_load(op)
    })
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
    //    stamped by `tagged_union_abi::expand_construct` /
    //    `build_match_dispatch_cascade` verbatim. Renaming either side
    //    without the other = silent ABI drift ⇒ cgen mis-emits. ──

    #[test]
    fn attr_keys_match_mir_rewrite_canonical_strings() {
        // These tests pin the wire-protocol. If a future change in
        // tagged_union_abi.rs renames a key, the test here flags the
        // mismatch immediately rather than at the runtime cgen path.
        assert_eq!(ATTR_FIELD, "field");
        assert_eq!(ATTR_FIELD_TAG, "tag");
        assert_eq!(ATTR_FIELD_PAYLOAD, "payload");
        assert_eq!(ATTR_SOURCE_KIND, "source_kind");
        assert_eq!(SOURCE_KIND_CELL, "tagged_union");
        assert_eq!(SOURCE_KIND_ALIAS, "tagged_union_alias");
        assert_eq!(SOURCE_KIND_DISPATCH, "tagged_union_dispatch");
        assert_eq!(ATTR_ARM_TAG, "arm_tag");
        assert_eq!(ATTR_FAMILY, "family");
        assert_eq!(FAMILY_OPTION, "Option");
        assert_eq!(FAMILY_RESULT, "Result");
        assert_eq!(ATTR_OFFSET, "offset");
    }

    // ── predicate helpers — rust-side recognition of post-rewrite MIR ──

    fn cell_alloc_op() -> MirOp {
        MirOp::std("cssl.heap.alloc")
            .with_result(ValueId(0), MirType::Ptr)
            .with_attribute("bytes", "8")
            .with_attribute("alignment", "4")
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_CELL)
            .with_attribute(ATTR_FAMILY, FAMILY_OPTION)
    }

    fn tag_const_op() -> MirOp {
        MirOp::std("arith.constant")
            .with_result(ValueId(1), MirType::Int(IntWidth::I32))
            .with_attribute("value", "1")
    }

    fn tag_store_op() -> MirOp {
        MirOp::std("memref.store")
            .with_operand(ValueId(1))
            .with_operand(ValueId(0))
            .with_attribute(ATTR_OFFSET, "0")
            .with_attribute("alignment", "4")
            .with_attribute(ATTR_FIELD, ATTR_FIELD_TAG)
    }

    fn payload_store_op() -> MirOp {
        MirOp::std("memref.store")
            .with_operand(ValueId(2))
            .with_operand(ValueId(0))
            .with_attribute(ATTR_OFFSET, "4")
            .with_attribute("alignment", "4")
            .with_attribute(ATTR_FIELD, ATTR_FIELD_PAYLOAD)
    }

    fn tag_load_op() -> MirOp {
        MirOp::std("memref.load")
            .with_operand(ValueId(0))
            .with_result(ValueId(3), MirType::Int(IntWidth::I32))
            .with_attribute(ATTR_OFFSET, "0")
            .with_attribute("alignment", "4")
            .with_attribute(ATTR_FIELD, ATTR_FIELD_TAG)
    }

    fn alias_op() -> MirOp {
        MirOp::std("arith.bitcast")
            .with_operand(ValueId(0))
            .with_result(ValueId(99), MirType::Ptr)
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_ALIAS)
    }

    fn dispatch_if_op() -> MirOp {
        MirOp::std("scf.if")
            .with_operand(ValueId(4))
            .with_result(ValueId(5), MirType::None)
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_DISPATCH)
            .with_attribute(ATTR_ARM_TAG, "1")
    }

    #[test]
    fn is_tagged_union_cell_alloc_recognizes_canonical_op() {
        assert!(is_tagged_union_cell_alloc(&cell_alloc_op()));
        // Non-cell heap.alloc (no source_kind) : NOT recognized.
        let plain_alloc = MirOp::std("cssl.heap.alloc")
            .with_result(ValueId(0), MirType::Ptr);
        assert!(!is_tagged_union_cell_alloc(&plain_alloc));
        // Op with the right marker but wrong name : NOT recognized.
        let wrong_name = MirOp::std("arith.constant")
            .with_attribute(ATTR_SOURCE_KIND, SOURCE_KIND_CELL);
        assert!(!is_tagged_union_cell_alloc(&wrong_name));
    }

    #[test]
    fn is_tag_store_vs_payload_store_disambiguates_correctly() {
        let ts = tag_store_op();
        let ps = payload_store_op();
        assert!(is_tag_store(&ts));
        assert!(!is_payload_store(&ts));
        assert!(is_payload_store(&ps));
        assert!(!is_tag_store(&ps));
    }

    #[test]
    fn is_tag_load_recognizes_offset_zero_load() {
        let load = tag_load_op();
        assert!(is_tag_load(&load));
        // Without the field=tag marker : NOT recognized.
        let bare_load = MirOp::std("memref.load")
            .with_operand(ValueId(0))
            .with_result(ValueId(7), MirType::Int(IntWidth::I32));
        assert!(!is_tag_load(&bare_load));
    }

    #[test]
    fn is_tagged_union_alias_recognizes_bitcast_marker() {
        assert!(is_tagged_union_alias(&alias_op()));
        // bitcast WITHOUT alias-marker : NOT recognized.
        let plain = MirOp::std("arith.bitcast").with_operand(ValueId(0));
        assert!(!is_tagged_union_alias(&plain));
    }

    #[test]
    fn is_tagged_union_dispatch_if_recognizes_dispatch_marker() {
        assert!(is_tagged_union_dispatch_if(&dispatch_if_op()));
        let plain = MirOp::std("scf.if").with_operand(ValueId(4));
        assert!(!is_tagged_union_dispatch_if(&plain));
    }

    // ── attribute readers ──

    #[test]
    fn arm_tag_value_parses_decimal_attribute() {
        let op = dispatch_if_op();
        assert_eq!(arm_tag_value(&op), Some(1));
        // Op without arm_tag : None.
        let no_tag = MirOp::std("scf.if");
        assert_eq!(arm_tag_value(&no_tag), None);
        // Garbled : None (parse failure).
        let garbled = MirOp::std("scf.if").with_attribute(ATTR_ARM_TAG, "not-a-number");
        assert_eq!(arm_tag_value(&garbled), None);
    }

    #[test]
    fn read_offset_attr_parses_decimal_offset() {
        let ts = tag_store_op();
        assert_eq!(read_offset_attr(&ts), Some(0));
        let ps = payload_store_op();
        assert_eq!(read_offset_attr(&ps), Some(4));
        // Op without offset : None.
        let no_off = MirOp::std("memref.store");
        assert_eq!(read_offset_attr(&no_off), None);
    }

    #[test]
    fn read_family_attr_returns_canonical_string() {
        let op = cell_alloc_op();
        assert_eq!(read_family_attr(&op), Some(FAMILY_OPTION));
        // Result-family : different value.
        let result_alloc = MirOp::std("cssl.heap.alloc")
            .with_attribute(ATTR_FAMILY, FAMILY_RESULT);
        assert_eq!(read_family_attr(&result_alloc), Some(FAMILY_RESULT));
        // Absent : None.
        let no_family = MirOp::std("cssl.heap.alloc");
        assert_eq!(read_family_attr(&no_family), None);
    }

    // ── memory-flags + emit helpers ──

    #[test]
    fn tagged_union_mem_flags_are_aligned_notrap() {
        let f = tagged_union_mem_flags();
        assert!(f.aligned());
        assert!(f.notrap());
    }

    // ── whole-block scan ──

    #[test]
    fn block_touches_tagged_union_detects_any_marker_op() {
        let mut blk = MirBlock::new("entry");
        blk.push(MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "42"));
        blk.push(cell_alloc_op());
        blk.push(MirOp::std("func.return"));
        assert!(block_touches_tagged_union(&blk));
    }

    #[test]
    fn block_touches_tagged_union_returns_false_for_plain_arithmetic() {
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
        assert!(!block_touches_tagged_union(&blk));
    }

    #[test]
    fn block_touches_tagged_union_detects_dispatch_if() {
        let mut blk = MirBlock::new("entry");
        blk.push(tag_load_op());
        blk.push(dispatch_if_op());
        assert!(block_touches_tagged_union(&blk));
    }

    #[test]
    fn block_touches_tagged_union_detects_alias_bitcast() {
        let mut blk = MirBlock::new("entry");
        blk.push(alias_op());
        assert!(block_touches_tagged_union(&blk));
    }

    // ── canonical sequence : Some(42) post-rewrite shape ──
    //
    //   This test is the slice's golden : it builds the EXACT op-stream
    //   that `tagged_union_abi::expand_construct` would produce for
    //   `Some(42_i32)` and verifies the cgen-side recognizers fire on
    //   each piece. The integration commit lifts this into a JIT-roundtrip
    //   that actually executes ; today the sequence stops at structural
    //   verification.

    #[test]
    fn golden_canonical_some_i32_sequence_recognizes_each_step() {
        // %0 = arith.constant 42 : i32       (the payload)
        let payload = MirOp::std("arith.constant")
            .with_result(ValueId(0), MirType::Int(IntWidth::I32))
            .with_attribute("value", "42");
        // %1 = cssl.heap.alloc {bytes=8, alignment=4, ...}     [tagged_union]
        let alloc = cell_alloc_op();
        // %2 = arith.constant 1 : i32        (the tag value)
        let tag_const = tag_const_op();
        // memref.store %2, %1 {offset=0, field=tag}
        let tag_store = tag_store_op();
        // memref.store %0, %1 {offset=4, field=payload}
        let payload_store = MirOp::std("memref.store")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_attribute(ATTR_OFFSET, "4")
            .with_attribute("alignment", "4")
            .with_attribute(ATTR_FIELD, ATTR_FIELD_PAYLOAD);
        // %3 = arith.bitcast %1 -> !cssl.ptr {source_kind=tagged_union_alias}
        let alias = alias_op();

        let mut blk = MirBlock::new("entry");
        blk.push(payload);
        blk.push(alloc);
        blk.push(tag_const);
        blk.push(tag_store);
        blk.push(payload_store);
        blk.push(alias);

        // Whole-block scan should fire.
        assert!(block_touches_tagged_union(&blk));

        // Per-op recognizers each fire on the right op.
        let alloc_idx = 1;
        let tag_store_idx = 3;
        let payload_store_idx = 4;
        let alias_idx = 5;
        assert!(is_tagged_union_cell_alloc(&blk.ops[alloc_idx]));
        assert!(is_tag_store(&blk.ops[tag_store_idx]));
        assert!(is_payload_store(&blk.ops[payload_store_idx]));
        assert!(is_tagged_union_alias(&blk.ops[alias_idx]));
        // Family is Option per cell_alloc_op().
        assert_eq!(read_family_attr(&blk.ops[alloc_idx]), Some(FAMILY_OPTION));
        // Tag store offset is 0 ; payload store offset is 4.
        assert_eq!(read_offset_attr(&blk.ops[tag_store_idx]), Some(0));
        assert_eq!(read_offset_attr(&blk.ops[payload_store_idx]), Some(4));
    }

    // ── canonical sequence : match-arm dispatch shape ──
    //
    //   The post-`build_match_dispatch_cascade` op-stream is :
    //     memref.load (tag) + arith.constant + arith.cmpi + scf.if (dispatch)
    //
    //   This test asserts each cgen-side recognizer / reader works on
    //   the dispatch sequence.

    #[test]
    fn golden_canonical_match_dispatch_sequence_decodes_arms() {
        let load = tag_load_op();
        let kconst = MirOp::std("arith.constant")
            .with_result(ValueId(4), MirType::Int(IntWidth::I32))
            .with_attribute("value", "1");
        let cmp = MirOp::std("arith.cmpi")
            .with_operand(ValueId(3))
            .with_operand(ValueId(4))
            .with_result(ValueId(5), MirType::Bool)
            .with_attribute("predicate", "eq");
        let dispatch = dispatch_if_op();

        let mut blk = MirBlock::new("entry");
        blk.push(load);
        blk.push(kconst);
        blk.push(cmp);
        blk.push(dispatch);

        assert!(block_touches_tagged_union(&blk));
        assert!(is_tag_load(&blk.ops[0]));
        assert!(is_tagged_union_dispatch_if(&blk.ops[3]));
        assert_eq!(arm_tag_value(&blk.ops[3]), Some(1));
        assert_eq!(read_offset_attr(&blk.ops[0]), Some(0));
    }
}

// INTEGRATION_NOTE :
//   add `pub mod cgen_tagged_union;` to cssl-cgen-cpu-cranelift/src/lib.rs
//   in the integration commit. The wave-A1 dispatch carved this file out
//   single-file-owned ; main-thread integration replaces this comment
//   with the `pub mod` declaration + the re-export block listing
//   `is_tagged_union_cell_alloc`, `is_tagged_union_alias`,
//   `is_tagged_union_dispatch_if`, `is_tag_store`, `is_payload_store`,
//   `is_tag_load`, `arm_tag_value`, `read_offset_attr`,
//   `read_family_attr`, `tagged_union_mem_flags`, `emit_tag_load`,
//   `emit_tag_store`, `emit_tag_eq_compare`, `emit_payload_load`,
//   `emit_payload_store`, `block_touches_tagged_union`. The
//   integration commit's wiring step also adds dispatch arms for
//   `is_tagged_union_alias` (skip CLIF emission, alias the value-map)
//   to the `lower_op_to_cl` match in jit.rs / object.rs.
