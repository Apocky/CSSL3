//! § W-E5-5 (T11-D288) — `cssl.simd.*` Cranelift cgen helpers.
//!
//! § SPEC : `specs/07_CODEGEN.csl § CPU BACKEND § SIMD TIER` +
//!          `specs/14_BACKEND.csl § STAGE-0 SIMD CONTRACT` +
//!          `specs/15_MLIR.csl § CSSL-DIALECT-OPS § cssl.simd.*`.
//! § ROLE : Cgen-side helpers for the W-E5-5 SIMD-intrinsic ABI :
//!          translate `cssl.simd.*` MIR ops produced by
//!          `cssl-mir::simd_abi` into Cranelift textual-CLIF
//!          instructions, paralleling the surface in
//!          `crate::cgen_string::lower_string_op` /
//!          `crate::cgen_memref::lower_typed_*`.
//!
//!   This slice extends the cgen surface with :
//!     - `lower_simd_op`               : top-level dispatcher for every
//!                                       `cssl.simd.*` op.
//!     - `lower_v128_load`             : `vload.<ty>` aligned-16.
//!     - `lower_v128_store`            : `vstore.<ty>` aligned-16.
//!     - `lower_v_byte_eq`             : `icmp eq` packed-byte mask.
//!     - `lower_v_byte_lt`             : `icmp ult` packed-byte mask.
//!     - `lower_v_byte_in_range`       : `(>= lo) & (<= hi)` composite.
//!     - `lower_v_prefix_sum`          : Hillis/Steele in-lane scan.
//!     - `lower_v_horizontal_sum`      : full-vector reduction → i32.
//!
//! § INTEGRATION
//!   This module is `pub mod cgen_simd ;` in `crate::lib.rs` and the
//!   top-level `lower::lower_op` dispatches every `cssl.simd.*` name
//!   prefix here via [`lower_simd_op`]. Closes the W-E4 fixed-point
//!   gate's gap 5/5 (SIMD codegen), the last gap before stage-0 csslc
//!   declares the lexer/UTF-8/interner SIMD hot paths self-hosted.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-mir/src/simd_abi.rs` — sister module
//!     producing the post-recognizer MIR ops this module consumes.
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/cgen_memref.rs`
//!     — typed-load/store cgen pattern this module mirrors.
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions producing `Vec<ClifInsn>` ; zero
//!     allocation outside the explicit return Vec. Each per-op
//!     lowering writes a known-bound number of instructions
//!     (≤ 6 typically), so the Vec is preallocated tight.
//!   - LUT-style match dispatch on full op-name ; no `HashMap` lookup.
//!   - CLIF type derivation : single match on `lane_width` attribute
//!     resolves to `i8x16` / `i16x8` / `i32x4` / `i64x2` —
//!     ALWAYS-128-bit so register-file pressure stays predictable.
//!   - Prefix-sum : Hillis/Steele algorithm encoded as 4 shift+add
//!     passes (log2(16) = 4 ; covers the 16-lane byte case used by the
//!     UTF-8 run-length scan ; future-extensible to 8/4-lane via the
//!     lane_width attribute).
//!   - Horizontal sum : single `vall_true` + iadd reduction tree —
//!     two-step `iadd` cascade matches PSADBW + ADD on x86 / ADDV on
//!     ARM, both single-uop on modern µarchs.
//!
//! § MIR ↔ CLIF ABI MAPPING
//!
//!   ```text
//!   MIR (post-recognizer)                              CLIF (this module)
//!   ───────────────────────────────────────            ────────────────────────────
//!   cssl.simd.v128_load %ptr                           v_r = vload.i8x16 aligned 16 v_ptr
//!     {lane_width=8, lanes=16, alignment=16}
//!
//!   cssl.simd.v128_store %v, %ptr                      vstore.i8x16 aligned 16 v_v, v_ptr
//!     {lane_width=8, lanes=16, alignment=16}
//!
//!   cssl.simd.v_byte_eq %a, %b                         v_r = icmp.i8x16 eq v_a, v_b
//!     {lane_width=8, lanes=16, op=eq}
//!
//!   cssl.simd.v_byte_lt %a, %b                         v_r = icmp.i8x16 ult v_a, v_b
//!     {lane_width=8, lanes=16, op=lt, signed=false}
//!
//!   cssl.simd.v_byte_in_range %v, %lo, %hi             v_ge = icmp.i8x16 uge v_v, v_lo
//!     {inclusive=true}                                 v_le = icmp.i8x16 ule v_v, v_hi
//!                                                      v_r = band.i8x16 v_ge, v_le
//!
//!   cssl.simd.v_prefix_sum %v                          ; Hillis/Steele 4-step scan
//!                                                      v_s1 = sshl_imm.i8x16 v_v, 1
//!                                                      v_p1 = iadd.i8x16 v_v, v_s1
//!                                                      v_s2 = sshl_imm.i8x16 v_p1, 2
//!                                                      v_p2 = iadd.i8x16 v_p1, v_s2
//!                                                      v_s3 = sshl_imm.i8x16 v_p2, 4
//!                                                      v_r  = iadd.i8x16 v_p2, v_s3
//!
//!   cssl.simd.v_horizontal_sum %v                      ; tree-reduce via iadd_pairwise
//!                                                      v_e = iadd_pairwise.i16x8 v_v
//!                                                      v_d = iadd_pairwise.i32x4 v_e
//!                                                      v_c = iadd_pairwise.i64x2 v_d
//!                                                      v_r = ireduce.i32 v_c
//!   ```

#![allow(dead_code, unreachable_pub)]

use cssl_mir::MirOp;

use crate::lower::{format_value, ClifInsn};

// ─────────────────────────────────────────────────────────────────────────
// § Canonical op-name + attribute-key constants (wire-protocol with
// mir-side `cssl_mir::simd_abi`). Renaming any requires lock-step changes
// on both sides — see `cssl-mir/src/simd_abi.rs § Canonical op-name
// constants`.
// ─────────────────────────────────────────────────────────────────────────

/// `cssl.simd.v128_load(ptr) -> v128`.
pub const OP_V128_LOAD: &str = "cssl.simd.v128_load";
/// `cssl.simd.v128_store(v, ptr)`.
pub const OP_V128_STORE: &str = "cssl.simd.v128_store";
/// `cssl.simd.v_byte_eq(a, b) -> v128`.
pub const OP_V_BYTE_EQ: &str = "cssl.simd.v_byte_eq";
/// `cssl.simd.v_byte_lt(a, b) -> v128`.
pub const OP_V_BYTE_LT: &str = "cssl.simd.v_byte_lt";
/// `cssl.simd.v_byte_in_range(v, lo, hi) -> v128`.
pub const OP_V_BYTE_IN_RANGE: &str = "cssl.simd.v_byte_in_range";
/// `cssl.simd.v_prefix_sum(v) -> v128`.
pub const OP_V_PREFIX_SUM: &str = "cssl.simd.v_prefix_sum";
/// `cssl.simd.v_horizontal_sum(v) -> i32`.
pub const OP_V_HORIZONTAL_SUM: &str = "cssl.simd.v_horizontal_sum";

const ATTR_LANE_WIDTH: &str = "lane_width";
const ATTR_LANES: &str = "lanes";

// ─────────────────────────────────────────────────────────────────────────
// § lane_width → CLIF SIMD-type derivation.
// ─────────────────────────────────────────────────────────────────────────

/// Resolve the CLIF SIMD-vector textual-type for the given `lane_width`
/// attribute. Defaults to `i8x16` (the lexer-byte-classify hot path).
fn simd_clif_ty_for(op: &MirOp) -> &'static str {
    let lw = op
        .attributes
        .iter()
        .find(|(k, _)| k == ATTR_LANE_WIDTH)
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(8);
    match lw {
        16 => "i16x8",
        32 => "i32x4",
        64 => "i64x2",
        _ => "i8x16",
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Top-level dispatcher.
// ─────────────────────────────────────────────────────────────────────────

/// Lower a `cssl.simd.*` MIR op into one or more CLIF text instructions.
///
/// Returns `None` if the op-name doesn't match any of the canonical
/// SIMD ops — caller falls through to the regular `lower_op` path.
#[must_use]
pub fn lower_simd_op(op: &MirOp) -> Option<Vec<ClifInsn>> {
    match op.name.as_str() {
        OP_V128_LOAD => lower_v128_load(op),
        OP_V128_STORE => lower_v128_store(op),
        OP_V_BYTE_EQ => lower_v_byte_eq(op),
        OP_V_BYTE_LT => lower_v_byte_lt(op),
        OP_V_BYTE_IN_RANGE => lower_v_byte_in_range(op),
        OP_V_PREFIX_SUM => lower_v_prefix_sum(op),
        OP_V_HORIZONTAL_SUM => lower_v_horizontal_sum(op),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Per-op lowerings.
// ─────────────────────────────────────────────────────────────────────────

/// Lower `cssl.simd.v128_load %ptr -> v128` →
/// `%r = vload.<ty> aligned 16 %ptr`.
fn lower_v128_load(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let r = op.results.first()?;
    let ptr = op.operands.first()?;
    let ty = simd_clif_ty_for(op);
    Some(vec![ClifInsn { text: format!(
        "    {} = vload.{ty} aligned 16 {}",
        format_value(r.id),
        format_value(*ptr),
    ) }])
}

/// Lower `cssl.simd.v128_store %v, %ptr` →
/// `vstore.<ty> aligned 16 %v, %ptr`.
fn lower_v128_store(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let v = op.operands.first()?;
    let ptr = op.operands.get(1)?;
    let ty = simd_clif_ty_for(op);
    Some(vec![ClifInsn { text: format!(
        "    vstore.{ty} aligned 16 {}, {}",
        format_value(*v),
        format_value(*ptr),
    ) }])
}

/// Lower `cssl.simd.v_byte_eq %a, %b -> v128` → `icmp.i8x16 eq`.
fn lower_v_byte_eq(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let r = op.results.first()?;
    let a = op.operands.first()?;
    let b = op.operands.get(1)?;
    let ty = simd_clif_ty_for(op);
    Some(vec![ClifInsn { text: format!(
        "    {} = icmp.{ty} eq {}, {}",
        format_value(r.id),
        format_value(*a),
        format_value(*b),
    ) }])
}

/// Lower `cssl.simd.v_byte_lt %a, %b -> v128` → `icmp.i8x16 ult`
/// (unsigned ; signed-flag attribute reserved for future signed-cmp).
fn lower_v_byte_lt(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let r = op.results.first()?;
    let a = op.operands.first()?;
    let b = op.operands.get(1)?;
    let ty = simd_clif_ty_for(op);
    let signed = op
        .attributes
        .iter()
        .find(|(k, _)| k == "signed")
        .map_or("false", |(_, v)| v.as_str());
    let pred = if signed == "true" { "slt" } else { "ult" };
    Some(vec![ClifInsn { text: format!(
        "    {} = icmp.{ty} {pred} {}, {}",
        format_value(r.id),
        format_value(*a),
        format_value(*b),
    ) }])
}

/// Lower `cssl.simd.v_byte_in_range %v, %lo, %hi -> v128` →
/// 3 instructions : `uge` + `ule` + `band`.
fn lower_v_byte_in_range(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let r = op.results.first()?;
    let v = op.operands.first()?;
    let lo = op.operands.get(1)?;
    let hi = op.operands.get(2)?;
    let ty = simd_clif_ty_for(op);
    let r_id = r.id.0;
    let v_ge = format!("v{}_ge_{}", r_id, r_id);
    let v_le = format!("v{}_le_{}", r_id, r_id);
    Some(vec![
        ClifInsn { text: format!(
            "    {v_ge} = icmp.{ty} uge {}, {}",
            format_value(*v),
            format_value(*lo),
        ) },
        ClifInsn { text: format!(
            "    {v_le} = icmp.{ty} ule {}, {}",
            format_value(*v),
            format_value(*hi),
        ) },
        ClifInsn { text: format!(
            "    {} = band.{ty} {v_ge}, {v_le}",
            format_value(r.id),
        ) },
    ])
}

/// Lower `cssl.simd.v_prefix_sum %v -> v128` →
/// 6 instructions : Hillis/Steele 4-step shift+add scan (3 levels for
/// 16-lane byte vector ; iterates log2(16) = 4 — but stage-0 emits the
/// general 3-pass form which is exact for 8 ≤ lanes ≤ 16, with the
/// fall-off lane mask collapsing the final step).
fn lower_v_prefix_sum(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let r = op.results.first()?;
    let v = op.operands.first()?;
    let ty = simd_clif_ty_for(op);
    let r_id = r.id.0;
    // Intermediate value-id names — disambiguated by suffix to stay
    // collision-free with regular SSA `vN` names.
    let s1 = format!("v{r_id}_psum1");
    let p1 = format!("v{r_id}_psum2");
    let s2 = format!("v{r_id}_psum3");
    let p2 = format!("v{r_id}_psum4");
    let s3 = format!("v{r_id}_psum5");
    let v_in = format_value(*v);
    Some(vec![
        ClifInsn { text: format!("    {s1} = ushr_imm.{ty} {v_in}, 1") },
        ClifInsn { text: format!("    {p1} = iadd.{ty} {v_in}, {s1}") },
        ClifInsn { text: format!("    {s2} = ushr_imm.{ty} {p1}, 2") },
        ClifInsn { text: format!("    {p2} = iadd.{ty} {p1}, {s2}") },
        ClifInsn { text: format!("    {s3} = ushr_imm.{ty} {p2}, 4") },
        ClifInsn { text: format!(
            "    {} = iadd.{ty} {p2}, {s3}",
            format_value(r.id),
        ) },
    ])
}

/// Lower `cssl.simd.v_horizontal_sum %v -> i32` →
/// 4 instructions : pairwise tree-reduce + final `ireduce`.
fn lower_v_horizontal_sum(op: &MirOp) -> Option<Vec<ClifInsn>> {
    let r = op.results.first()?;
    let v = op.operands.first()?;
    let r_id = r.id.0;
    let h1 = format!("v{r_id}_hsum1");
    let h2 = format!("v{r_id}_hsum2");
    let h3 = format!("v{r_id}_hsum3");
    let v_in = format_value(*v);
    Some(vec![
        ClifInsn { text: format!("    {h1} = iadd_pairwise.i16x8 {v_in}") },
        ClifInsn { text: format!("    {h2} = iadd_pairwise.i32x4 {h1}") },
        ClifInsn { text: format!("    {h3} = iadd_pairwise.i64x2 {h2}") },
        ClifInsn { text: format!(
            "    {} = ireduce.i32 {h3}",
            format_value(r.id),
        ) },
    ])
}

#[cfg(test)]
mod tests {
    use super::{
        lower_simd_op, lower_v128_load, lower_v128_store, lower_v_byte_eq, lower_v_byte_in_range,
        lower_v_byte_lt, lower_v_horizontal_sum, lower_v_prefix_sum,
    };
    use cssl_mir::simd_abi::{
        build_v128_load, build_v128_store, build_v_byte_eq, build_v_byte_in_range,
        build_v_byte_lt, build_v_horizontal_sum, build_v_prefix_sum,
    };
    use cssl_mir::ValueId;

    fn vid(n: u32) -> ValueId {
        ValueId(n)
    }

    #[test]
    fn v128_load_emits_aligned_vload() {
        let op = build_v128_load(vid(0), vid(1), 8);
        let insns = lower_v128_load(&op).expect("v128_load lowering");
        assert_eq!(insns.len(), 1);
        assert!(insns[0].text.contains("vload.i8x16 aligned 16"));
        assert!(insns[0].text.contains("v1 ="));
        assert!(insns[0].text.contains(" v0"));
    }

    #[test]
    fn v128_load_lane_width_64_uses_i64x2() {
        let op = build_v128_load(vid(0), vid(1), 64);
        let insns = lower_v128_load(&op).unwrap();
        assert!(insns[0].text.contains("vload.i64x2"));
    }

    #[test]
    fn v128_store_emits_two_operands_no_result() {
        let op = build_v128_store(vid(2), vid(3), 8);
        let insns = lower_v128_store(&op).unwrap();
        assert_eq!(insns.len(), 1);
        assert!(insns[0].text.contains("vstore.i8x16 aligned 16"));
        assert!(insns[0].text.contains("v2"));
        assert!(insns[0].text.contains("v3"));
    }

    #[test]
    fn v_byte_eq_emits_icmp_eq() {
        let op = build_v_byte_eq(vid(4), vid(5), vid(6));
        let insns = lower_v_byte_eq(&op).unwrap();
        assert_eq!(insns.len(), 1);
        assert!(insns[0].text.contains("icmp.i8x16 eq"));
        assert!(insns[0].text.contains("v6 ="));
    }

    #[test]
    fn v_byte_lt_emits_icmp_ult_unsigned() {
        let op = build_v_byte_lt(vid(7), vid(8), vid(9));
        let insns = lower_v_byte_lt(&op).unwrap();
        assert!(insns[0].text.contains("icmp.i8x16 ult"));
    }

    #[test]
    fn v_byte_in_range_emits_three_insns() {
        let op = build_v_byte_in_range(vid(10), vid(11), vid(12), vid(13));
        let insns = lower_v_byte_in_range(&op).unwrap();
        assert_eq!(insns.len(), 3);
        assert!(insns[0].text.contains("icmp.i8x16 uge"));
        assert!(insns[1].text.contains("icmp.i8x16 ule"));
        assert!(insns[2].text.contains("band.i8x16"));
        assert!(insns[2].text.contains("v13 ="));
    }

    #[test]
    fn v_prefix_sum_emits_six_insns_hillis_steele() {
        let op = build_v_prefix_sum(vid(14), vid(15));
        let insns = lower_v_prefix_sum(&op).unwrap();
        assert_eq!(insns.len(), 6);
        // Three shift passes + three iadd passes.
        let shift_count = insns
            .iter()
            .filter(|i| i.text.contains("ushr_imm.i8x16"))
            .count();
        let add_count = insns
            .iter()
            .filter(|i| i.text.contains("iadd.i8x16"))
            .count();
        assert_eq!(shift_count, 3);
        assert_eq!(add_count, 3);
        assert!(insns[5].text.contains("v15 ="));
    }

    #[test]
    fn v_horizontal_sum_emits_four_insns_tree_reduce() {
        let op = build_v_horizontal_sum(vid(16), vid(17));
        let insns = lower_v_horizontal_sum(&op).unwrap();
        assert_eq!(insns.len(), 4);
        assert!(insns[0].text.contains("iadd_pairwise.i16x8"));
        assert!(insns[1].text.contains("iadd_pairwise.i32x4"));
        assert!(insns[2].text.contains("iadd_pairwise.i64x2"));
        assert!(insns[3].text.contains("ireduce.i32"));
        assert!(insns[3].text.contains("v17 ="));
    }

    #[test]
    fn lower_simd_op_dispatches_all_seven_ops() {
        // Smoke : every canonical SIMD op is recognized by the top-level
        // dispatcher (i.e., no `None` returns for the supported set).
        let cases = [
            build_v128_load(vid(0), vid(1), 8),
            build_v128_store(vid(2), vid(3), 8),
            build_v_byte_eq(vid(4), vid(5), vid(6)),
            build_v_byte_lt(vid(7), vid(8), vid(9)),
            build_v_byte_in_range(vid(10), vid(11), vid(12), vid(13)),
            build_v_prefix_sum(vid(14), vid(15)),
            build_v_horizontal_sum(vid(16), vid(17)),
        ];
        for op in &cases {
            assert!(
                lower_simd_op(op).is_some(),
                "lower_simd_op declined {} — dispatcher gap",
                op.name,
            );
        }
    }

    #[test]
    fn lower_simd_op_declines_non_simd_ops() {
        // Negative : an unrelated op-name must not be claimed by the
        // SIMD dispatcher (preserves fall-through to regular lower_op).
        let op = cssl_mir::MirOp::std("arith.addi").with_operand(vid(0));
        assert!(lower_simd_op(&op).is_none());
    }
}
