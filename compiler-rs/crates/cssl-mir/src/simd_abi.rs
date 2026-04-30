//! § W-E5-5 (T11-D288) — `cssl.simd.*` SIMD-intrinsic ABI lowering.
//!
//! § SPEC : `specs/15_MLIR.csl § CSSL-DIALECT-OPS § cssl.simd.*` +
//!          `specs/07_CODEGEN.csl § CPU BACKEND § SIMD TIER` +
//!          `specs/14_BACKEND.csl § STAGE-0 SIMD CONTRACT`.
//! § ROLE : MIR-side helpers that mint the canonical `cssl.simd.*` op
//!          shapes the `body_lower` recognizer arms emit when they see
//!          the corresponding stdlib SIMD intrinsic call. Mirrors the
//!          `string_abi` / `tagged_union_abi` pattern : pure-function
//!          builders, zero allocation outside the explicit `MirOp`
//!          struct, op-name + attribute keys are wire-protocol with
//!          `cssl-cgen-cpu-cranelift::cgen_simd`.
//!
//! § CANONICAL OP SURFACE
//!
//!   - `cssl.simd.v128_load %ptr [, %off] -> v128`
//!     attrs : { lane_width=8|16|32|64, lanes, alignment=16 }
//!   - `cssl.simd.v128_store %v, %ptr [, %off]`
//!     attrs : { lane_width=8, lanes=16, alignment=16 }
//!   - `cssl.simd.v_byte_eq %a, %b -> v128`
//!     attrs : { lane_width=8, lanes=16, op="eq" }
//!     — bytewise equality mask (0xFF on equal lanes / 0x00 otherwise).
//!   - `cssl.simd.v_byte_lt %a, %b -> v128`
//!     attrs : { lane_width=8, lanes=16, op="lt", signed=false }
//!     — bytewise unsigned less-than mask.
//!   - `cssl.simd.v_byte_in_range %v, %lo, %hi -> v128`
//!     attrs : { lane_width=8, lanes=16, inclusive=true }
//!     — composite : `(v >= lo) & (v <= hi)`. Cgen lowers via two
//!     compare-and's. Used by lexer-byte-classify hot path.
//!   - `cssl.simd.v_prefix_sum %v -> v128`
//!     attrs : { lane_width=8, lanes=16, fold="add" }
//!     — in-lane prefix-sum (Hillis/Steele) ; required by the UTF-8
//!     run-length scan.
//!   - `cssl.simd.v_horizontal_sum %v -> i32`
//!     attrs : { lane_width=8, lanes=16, fold="add" }
//!     — collapse all 16 byte lanes into one i32 scalar (`PSADBW` +
//!     two-step add fast-path on x86 / `ADDV` on ARM).
//!
//! § SAWYER-EFFICIENCY
//!   - All builders are pure ; constants resolve at compile time.
//!   - LANE_WIDTH attribute keys are interned `&'static str` ; no
//!     `String::from(...)` allocation outside the unavoidable
//!     `with_attribute("lanes", lanes.to_string())` integer-format.
//!   - Result type-id : `MirType::Opaque("!cssl.v128")` keeps the
//!     stage-0 type-system small ; cgen converts to cranelift's
//!     `I8X16` / `I16X8` / etc. via lane_width attribute lookup.
//!
//! § WIRE-PROTOCOL CONTRACT (lock-step with cgen_simd)
//!   Renaming OP_* / ATTR_* constants requires lock-step changes in
//!   `cssl-cgen-cpu-cranelift/src/cgen_simd.rs § CANONICAL OP-NAMES`.

use crate::block::MirOp;
use crate::value::{IntWidth, MirType, ValueId};

// ─────────────────────────────────────────────────────────────────────────
// § Canonical op-name constants (wire-protocol with cgen_simd).
// ─────────────────────────────────────────────────────────────────────────

/// `cssl.simd.v128_load` — load a 128-bit SIMD register from a typed pointer.
pub const OP_V128_LOAD: &str = "cssl.simd.v128_load";

/// `cssl.simd.v128_store` — store a 128-bit SIMD register to a typed pointer.
pub const OP_V128_STORE: &str = "cssl.simd.v128_store";

/// `cssl.simd.v_byte_eq` — bytewise equality mask producing a v128.
pub const OP_V_BYTE_EQ: &str = "cssl.simd.v_byte_eq";

/// `cssl.simd.v_byte_lt` — bytewise unsigned less-than mask.
pub const OP_V_BYTE_LT: &str = "cssl.simd.v_byte_lt";

/// `cssl.simd.v_byte_in_range` — bytewise inclusive range-check mask.
pub const OP_V_BYTE_IN_RANGE: &str = "cssl.simd.v_byte_in_range";

/// `cssl.simd.v_prefix_sum` — in-lane (Hillis/Steele) prefix-sum.
pub const OP_V_PREFIX_SUM: &str = "cssl.simd.v_prefix_sum";

/// `cssl.simd.v_horizontal_sum` — collapse all lanes into a scalar.
pub const OP_V_HORIZONTAL_SUM: &str = "cssl.simd.v_horizontal_sum";

// ─────────────────────────────────────────────────────────────────────────
// § Canonical attribute keys.
// ─────────────────────────────────────────────────────────────────────────

/// `lane_width` — width of each SIMD lane in bits (8/16/32/64).
pub const ATTR_LANE_WIDTH: &str = "lane_width";

/// `lanes` — number of lanes in the v128 register (16 for byte ops).
pub const ATTR_LANES: &str = "lanes";

/// `alignment` — operand alignment in bytes (16 for v128 ops).
pub const ATTR_ALIGNMENT: &str = "alignment";

/// `op` — fine-grained sub-operation (eq / lt / etc.).
pub const ATTR_OP: &str = "op";

/// `signed` — signed-vs-unsigned comparison flag.
pub const ATTR_SIGNED: &str = "signed";

/// `inclusive` — whether range-check is inclusive on both ends.
pub const ATTR_INCLUSIVE: &str = "inclusive";

/// `fold` — reduction kind (add / max / min / xor).
pub const ATTR_FOLD: &str = "fold";

// ─────────────────────────────────────────────────────────────────────────
// § Canonical type aliases.
// ─────────────────────────────────────────────────────────────────────────

/// MIR opaque-type token for a 128-bit SIMD register. Cgen maps this to
/// the cranelift IR `I8X16` (default for byte-classify ops) or to the
/// matching `I16X8` / `I32X4` / `I64X2` based on the `lane_width`
/// attribute carried on the producing op.
#[must_use]
pub fn v128_ty() -> MirType {
    MirType::Opaque("!cssl.v128".to_string())
}

// ─────────────────────────────────────────────────────────────────────────
// § Builder helpers — pure-function MirOp factories.
// ─────────────────────────────────────────────────────────────────────────

/// Build `cssl.simd.v128_load %ptr -> v128`.
///
/// `lane_width` selects the underlying cranelift type (8 → I8X16,
/// 16 → I16X8, etc.). The default 16-lane byte-vector covers the hot
/// lexer-byte-classify path ; other widths feed UTF-8-DFA + interner.
#[must_use]
pub fn build_v128_load(ptr: ValueId, result: ValueId, lane_width: u32) -> MirOp {
    let lanes = 128 / lane_width.max(1);
    MirOp::std(OP_V128_LOAD)
        .with_operand(ptr)
        .with_result(result, v128_ty())
        .with_attribute(ATTR_LANE_WIDTH, lane_width.to_string())
        .with_attribute(ATTR_LANES, lanes.to_string())
        .with_attribute(ATTR_ALIGNMENT, "16")
}

/// Build `cssl.simd.v128_store %v, %ptr`.
#[must_use]
pub fn build_v128_store(v: ValueId, ptr: ValueId, lane_width: u32) -> MirOp {
    let lanes = 128 / lane_width.max(1);
    MirOp::std(OP_V128_STORE)
        .with_operand(v)
        .with_operand(ptr)
        .with_attribute(ATTR_LANE_WIDTH, lane_width.to_string())
        .with_attribute(ATTR_LANES, lanes.to_string())
        .with_attribute(ATTR_ALIGNMENT, "16")
}

/// Build `cssl.simd.v_byte_eq %a, %b -> v128`.
#[must_use]
pub fn build_v_byte_eq(a: ValueId, b: ValueId, result: ValueId) -> MirOp {
    MirOp::std(OP_V_BYTE_EQ)
        .with_operand(a)
        .with_operand(b)
        .with_result(result, v128_ty())
        .with_attribute(ATTR_LANE_WIDTH, "8")
        .with_attribute(ATTR_LANES, "16")
        .with_attribute(ATTR_OP, "eq")
}

/// Build `cssl.simd.v_byte_lt %a, %b -> v128` (unsigned).
#[must_use]
pub fn build_v_byte_lt(a: ValueId, b: ValueId, result: ValueId) -> MirOp {
    MirOp::std(OP_V_BYTE_LT)
        .with_operand(a)
        .with_operand(b)
        .with_result(result, v128_ty())
        .with_attribute(ATTR_LANE_WIDTH, "8")
        .with_attribute(ATTR_LANES, "16")
        .with_attribute(ATTR_OP, "lt")
        .with_attribute(ATTR_SIGNED, "false")
}

/// Build `cssl.simd.v_byte_in_range %v, %lo, %hi -> v128` (inclusive).
#[must_use]
pub fn build_v_byte_in_range(v: ValueId, lo: ValueId, hi: ValueId, result: ValueId) -> MirOp {
    MirOp::std(OP_V_BYTE_IN_RANGE)
        .with_operand(v)
        .with_operand(lo)
        .with_operand(hi)
        .with_result(result, v128_ty())
        .with_attribute(ATTR_LANE_WIDTH, "8")
        .with_attribute(ATTR_LANES, "16")
        .with_attribute(ATTR_INCLUSIVE, "true")
}

/// Build `cssl.simd.v_prefix_sum %v -> v128` (in-lane Hillis/Steele add).
#[must_use]
pub fn build_v_prefix_sum(v: ValueId, result: ValueId) -> MirOp {
    MirOp::std(OP_V_PREFIX_SUM)
        .with_operand(v)
        .with_result(result, v128_ty())
        .with_attribute(ATTR_LANE_WIDTH, "8")
        .with_attribute(ATTR_LANES, "16")
        .with_attribute(ATTR_FOLD, "add")
}

/// Build `cssl.simd.v_horizontal_sum %v -> i32`.
#[must_use]
pub fn build_v_horizontal_sum(v: ValueId, result: ValueId) -> MirOp {
    MirOp::std(OP_V_HORIZONTAL_SUM)
        .with_operand(v)
        .with_result(result, MirType::Int(IntWidth::I32))
        .with_attribute(ATTR_LANE_WIDTH, "8")
        .with_attribute(ATTR_LANES, "16")
        .with_attribute(ATTR_FOLD, "add")
}

#[cfg(test)]
mod tests {
    use super::{
        build_v128_load, build_v128_store, build_v_byte_eq, build_v_byte_in_range,
        build_v_byte_lt, build_v_horizontal_sum, build_v_prefix_sum, v128_ty, ATTR_FOLD,
        ATTR_INCLUSIVE, ATTR_LANES, ATTR_LANE_WIDTH, ATTR_SIGNED, OP_V128_LOAD, OP_V128_STORE,
        OP_V_BYTE_EQ, OP_V_BYTE_IN_RANGE, OP_V_BYTE_LT, OP_V_HORIZONTAL_SUM, OP_V_PREFIX_SUM,
    };
    use crate::value::{IntWidth, MirType, ValueId};

    fn vid(n: u32) -> ValueId {
        ValueId(n)
    }

    fn attr<'a>(op: &'a crate::block::MirOp, key: &str) -> Option<&'a str> {
        op.attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn v128_load_default_byte_lane_count() {
        let op = build_v128_load(vid(0), vid(1), 8);
        assert_eq!(op.name, OP_V128_LOAD);
        assert_eq!(op.operands, vec![vid(0)]);
        assert_eq!(op.results.len(), 1);
        assert_eq!(op.results[0].id, vid(1));
        assert_eq!(op.results[0].ty, v128_ty());
        assert_eq!(attr(&op, ATTR_LANE_WIDTH), Some("8"));
        assert_eq!(attr(&op, ATTR_LANES), Some("16"));
    }

    #[test]
    fn v128_load_16bit_lanes_emits_8_lanes() {
        let op = build_v128_load(vid(0), vid(1), 16);
        assert_eq!(attr(&op, ATTR_LANE_WIDTH), Some("16"));
        assert_eq!(attr(&op, ATTR_LANES), Some("8"));
    }

    #[test]
    fn v128_store_emits_two_operands_no_result() {
        let op = build_v128_store(vid(2), vid(3), 8);
        assert_eq!(op.name, OP_V128_STORE);
        assert_eq!(op.operands, vec![vid(2), vid(3)]);
        assert!(op.results.is_empty());
    }

    #[test]
    fn v_byte_eq_marks_op_eq() {
        let op = build_v_byte_eq(vid(4), vid(5), vid(6));
        assert_eq!(op.name, OP_V_BYTE_EQ);
        assert_eq!(op.operands, vec![vid(4), vid(5)]);
        assert_eq!(op.results[0].id, vid(6));
        assert_eq!(attr(&op, "op"), Some("eq"));
    }

    #[test]
    fn v_byte_lt_marks_unsigned_lt() {
        let op = build_v_byte_lt(vid(7), vid(8), vid(9));
        assert_eq!(op.name, OP_V_BYTE_LT);
        assert_eq!(attr(&op, ATTR_SIGNED), Some("false"));
    }

    #[test]
    fn v_byte_in_range_carries_inclusive() {
        let op = build_v_byte_in_range(vid(10), vid(11), vid(12), vid(13));
        assert_eq!(op.name, OP_V_BYTE_IN_RANGE);
        assert_eq!(op.operands.len(), 3);
        assert_eq!(attr(&op, ATTR_INCLUSIVE), Some("true"));
    }

    #[test]
    fn v_prefix_sum_marks_add_fold() {
        let op = build_v_prefix_sum(vid(14), vid(15));
        assert_eq!(op.name, OP_V_PREFIX_SUM);
        assert_eq!(attr(&op, ATTR_FOLD), Some("add"));
    }

    #[test]
    fn v_horizontal_sum_returns_i32() {
        let op = build_v_horizontal_sum(vid(16), vid(17));
        assert_eq!(op.name, OP_V_HORIZONTAL_SUM);
        assert_eq!(op.results[0].ty, MirType::Int(IntWidth::I32));
        assert_eq!(attr(&op, ATTR_FOLD), Some("add"));
    }
}
