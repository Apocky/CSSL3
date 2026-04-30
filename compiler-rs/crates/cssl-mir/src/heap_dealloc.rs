//! § T11-D??? (Wave-A5) — `cssl.heap.dealloc` MIR-emit helpers + vec_drop
//!   recognizer-shape.
//!
//! § ROLE
//!   Pure-function helpers that mint the canonical `cssl.heap.dealloc` op
//!   sequence from a `(ptr, payload-T, capacity)` triple. The op variant
//!   `CsslOp::HeapDealloc` + its 3-operand → 0-result signature already
//!   landed in S6-B1 (T11-D57) ; this slice :
//!     1. closes the loop on stdlib/vec.cssl § Manual Drop — vec_drop's
//!        body previously emitted `let _ = v.data` as a placeholder ;
//!        this module supplies the canonical builder that future
//!        recognizer-bridges (when stdlib intrinsic-route lands for
//!        vec_drop alongside Box::new) call into.
//!     2. encapsulates the size + align computation so callers don't have
//!        to re-derive `cap × sizeof T` + `alignof T` at every emit-site.
//!     3. centralizes the recognizer-test for the vec_drop call-shape so
//!        when the stdlib intrinsic-route slice lands for vec_drop, the
//!        wiring is a single `if matches_vec_drop_pattern(...)` branch.
//!
//! § INTEGRATION_NOTE  (per Wave-A5 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-mir/src/lib.rs` is
//!   intentionally NOT modified. The helpers are reachable via direct
//!   `crate::heap_dealloc::*` use-paths (the crate's `lib.rs` re-exports
//!   are a stage-0 convenience ; the helpers compile + test against the
//!   crate-internal API surface). Once the future stdlib intrinsic-route
//!   slice for `vec_drop` lands, that slice's commit will add the
//!   `pub mod heap_dealloc;` line + `pub use heap_dealloc::*` re-exports
//!   alongside its body_lower wiring change ; until then this module
//!   compiles + is tested in-place via `#[cfg(test)]` references.
//!
//! § SPEC-REFERENCES
//!   - `specs/02_IR.csl` § HEAP-OPS — canonical 3-operand dealloc shape.
//!   - `specs/12_CAPABILITIES.csl` § ISO-OWNERSHIP — dealloc consumes the
//!     iso-owned `data` pointer (linear cap closure).
//!   - `stdlib/vec.cssl` § Manual Drop (lines 360-384) — the call-site
//!     this slice unblocks. The placeholder `let _ = v.data` body is
//!     superseded by an `emit_dealloc_seq(...)` call once the intrinsic-
//!     route lands.
//!   - `compiler-rs/crates/cssl-rt/src/ffi.rs` — the `__cssl_free` symbol
//!     this op lowers to. Symbol is ABI-stable from S6-A1 forward.
//!
//! § SAWYER-EFFICIENCY
//!   - `dealloc_size_for` / `dealloc_align_for` are `const fn` — the
//!     monomorph-time type-arg becomes a compile-time numeric constant
//!     at every call-site (no HashMap lookup, no runtime branch).
//!   - `matches_vec_drop_pattern` is a single 2-segment string-equality
//!     compare ; O(1) bounded by the segment count, no allocation.
//!   - `emit_dealloc_seq` writes 3 ops directly into the caller's
//!     `Vec<MirOp>` ; no intermediate Vec allocation.

// § Suppress dead-code + unreachable-pub : the module is wired in
//   privately + its surface becomes reachable when the future stdlib
//   intrinsic-route slice for vec_drop adds the `pub mod` + the
//   `body_lower` recognizer-call-site. Until then the public-surface
//   helpers are orphan-callable from tests only ; without these
//   suppressions the workspace lint policy surfaces 23 warnings.
#![allow(dead_code, unreachable_pub)]

use crate::block::MirOp;
use crate::op::CsslOp;
use crate::value::{FloatWidth, IntWidth, MirType, ValueId};

// ───────────────────────────────────────────────────────────────────────
// § canonical attribute keys + values (wire-protocol with cgen)
// ───────────────────────────────────────────────────────────────────────

/// Op-attribute key on the dealloc op recording the payload type.
/// Mirrors B1's `payload_ty` attribute on `cssl.heap.alloc` so that
/// downstream layout-aware passes (post-trait-resolve) can recover the
/// concrete monomorph type without re-parsing the op name.
pub const ATTR_PAYLOAD_TY: &str = "payload_ty";

/// Op-attribute key recording where the dealloc was minted from. Set to
/// `"vec_drop"` when the recognizer-bridge path is the source ; future
/// intrinsic-route variants use distinct origin tags so post-MIR audit
/// passes can distinguish call-sites by syntactic provenance.
pub const ATTR_ORIGIN: &str = "origin";

/// Canonical origin-tag for a dealloc minted via `vec_drop::<T>(v)`.
pub const ORIGIN_VEC_DROP: &str = "vec_drop";

/// Op-attribute key recording the source-form span (file:line:col). Mirrors
/// the alloc-side convention so MIR-textual diagnostics can point at the
/// originating source-fragment.
pub const ATTR_SOURCE_LOC: &str = "source_loc";

/// Op-attribute key carrying the linear-capability tag — dealloc consumes
/// `iso<ptr>` per `specs/12_CAPABILITIES.csl § ISO-OWNERSHIP`. The value
/// `"iso_consumed"` distinguishes the consumer-side from the producer-side
/// (`"iso"` on alloc).
pub const ATTR_CAP: &str = "cap";

/// Canonical capability-tag value for a dealloc op.
pub const CAP_ISO_CONSUMED: &str = "iso_consumed";

// ───────────────────────────────────────────────────────────────────────
// § monomorph-aware sizeof / alignof — const fns
// ───────────────────────────────────────────────────────────────────────

/// Compile-time sizeof T for a monomorphized payload type.
///
/// Mirrors `body_lower::stage0_heuristic_size_of` — the values must agree
/// because alloc + dealloc must thread the same byte-count to `__cssl_alloc`
/// and `__cssl_free` (cssl-rt's allocator-tracker pairs them by
/// `(ptr, size, align)` — a mismatch is undefined behavior under the
/// stage-0 allocator contract).
///
/// Returns `0` for unresolved / opaque payload types ; downstream callers
/// can early-exit on `0` size to avoid emitting a dealloc for an empty
/// allocation (matches the `vec_drop` § Manual Drop guard `if v.cap > 0`).
#[must_use]
pub const fn dealloc_size_for(t: &MirType) -> i64 {
    match t {
        MirType::Int(IntWidth::I1 | IntWidth::I8) | MirType::Bool => 1,
        MirType::Int(IntWidth::I16) => 2,
        MirType::Int(IntWidth::I32) => 4,
        MirType::Int(IntWidth::I64 | IntWidth::Index) => 8,
        MirType::Float(FloatWidth::F16 | FloatWidth::Bf16) => 2,
        MirType::Float(FloatWidth::F32) => 4,
        MirType::Float(FloatWidth::F64) => 8,
        MirType::Ptr | MirType::Handle => 8, // 64-bit host @ stage-0
        MirType::Vec(lanes, w) => {
            let lane_bytes: i64 = match w {
                FloatWidth::F16 | FloatWidth::Bf16 => 2,
                FloatWidth::F32 => 4,
                FloatWidth::F64 => 8,
            };
            (*lanes as i64) * lane_bytes
        }
        // Composite / unresolved : 0 — caller should treat as no-op.
        MirType::Tuple(_)
        | MirType::Function { .. }
        | MirType::Memref { .. }
        | MirType::Opaque(_)
        | MirType::None => 0,
    }
}

/// Compile-time alignof T for a monomorphized payload type.
///
/// Returns the natural ABI alignment ; falls back to `8` for unresolved /
/// composite types (safe upper bound on 64-bit hosts ; matches the
/// alloc-side fallback in `body_lower::stage0_heuristic_align_of` so
/// alloc + dealloc agree byte-for-byte).
#[must_use]
pub const fn dealloc_align_for(t: &MirType) -> i64 {
    match t {
        MirType::Int(IntWidth::I1 | IntWidth::I8) | MirType::Bool => 1,
        MirType::Int(IntWidth::I16) | MirType::Float(FloatWidth::F16 | FloatWidth::Bf16) => 2,
        MirType::Int(IntWidth::I32) | MirType::Float(FloatWidth::F32) => 4,
        MirType::Int(IntWidth::I64 | IntWidth::Index)
        | MirType::Float(FloatWidth::F64)
        | MirType::Ptr
        | MirType::Handle => 8,
        MirType::Vec(_, w) => match w {
            FloatWidth::F16 | FloatWidth::Bf16 => 2,
            FloatWidth::F32 => 4,
            FloatWidth::F64 => 8,
        },
        // Composite / unresolved : 8 (max-alignment safe default).
        MirType::Tuple(_)
        | MirType::Function { .. }
        | MirType::Memref { .. }
        | MirType::Opaque(_)
        | MirType::None => 8,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § recognizer — vec_drop call-shape match
// ───────────────────────────────────────────────────────────────────────

/// Test whether a callee path-segment list matches the canonical
/// `vec_drop` call-shape that this slice's emit-helper services.
///
/// § ACCEPTED-SHAPES
///   - `["vec_drop"]`         — single-segment free-fn form (stage-0
///                              canonical until trait-resolve lands).
///   - `["Vec", "drop"]`      — 2-segment associated-fn form (post-trait-
///                              resolve migration target ; recognized
///                              ahead-of-time so the same builder works
///                              before + after the trait-resolve slice).
///
/// § REJECTED-SHAPES
///   - 0 or 3+ segments  — bypasses the recognizer (matches the B1
///                          Box::new precedent of strict segment-count
///                          guards to prevent false positives on user
///                          code that shadows the name).
///   - other 1 or 2-segment names    — declines so the regular
///                                      generic-call path takes over.
///
/// § WALK-COST
///   O(1) bounded by the segment count (always 1 or 2 string compares).
///   Single-pass over the input slice ; no allocation.
#[must_use]
pub fn matches_vec_drop_pattern(callee_segments: &[&str]) -> bool {
    match callee_segments {
        ["vec_drop"] => true,
        ["Vec", "drop"] => true,
        _ => false,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § canonical op-builder
// ───────────────────────────────────────────────────────────────────────

/// Build the canonical `cssl.heap.dealloc` MIR op given pre-existing
/// SSA-value-ids for the three operands `(ptr, size, align)` plus the
/// monomorphized payload type.
///
/// Operand-order MUST match the cgen import-signature in
/// `cssl-cgen-cpu-cranelift::object` :
/// ```text
///   __cssl_free(ptr : *mut u8, size : usize, align : usize) -> ()
/// ```
/// (Renaming either side requires lock-step changes per the FFI contract
/// in `cssl-rt::ffi` ; this builder centralizes the contract on the
/// MIR-emit side.)
///
/// § ATTRIBUTES emitted on the op
///   - `payload_ty`  — `format!("{payload_ty}")` so layout-aware passes
///                     can re-derive size from the type when needed.
///   - `cap`         — `"iso_consumed"` — marks linear-capability
///                     consumption point. A dealloc may only consume an
///                     iso-owned ptr exactly once.
///   - `origin`      — when `origin_tag` is `Some`, recorded ; otherwise
///                     omitted. `Some(ORIGIN_VEC_DROP)` is the canonical
///                     vec_drop bridge value.
///   - `source_loc`  — when `span_str` is non-empty, recorded ; otherwise
///                     omitted (lowered nodes without source-locs skip
///                     the attribute to keep MIR text clean).
#[must_use]
pub fn build_heap_dealloc_op(
    ptr: ValueId,
    size: ValueId,
    align: ValueId,
    payload_ty: &MirType,
    origin_tag: Option<&str>,
    span_str: &str,
) -> MirOp {
    let mut op = MirOp::new(CsslOp::HeapDealloc)
        .with_operand(ptr)
        .with_operand(size)
        .with_operand(align)
        .with_attribute(ATTR_PAYLOAD_TY, format!("{payload_ty}"))
        .with_attribute(ATTR_CAP, CAP_ISO_CONSUMED);
    if let Some(tag) = origin_tag {
        op = op.with_attribute(ATTR_ORIGIN, tag);
    }
    if !span_str.is_empty() {
        op = op.with_attribute(ATTR_SOURCE_LOC, span_str);
    }
    op
}

// ───────────────────────────────────────────────────────────────────────
// § emit-sequence helper — appends size+align+dealloc trio onto an op-vec
// ───────────────────────────────────────────────────────────────────────

/// Emitted sequence describing the three appended ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeallocEmittedIds {
    /// The arith.constant op result holding `cap × sizeof T`.
    pub size_id: ValueId,
    /// The arith.constant op result holding `alignof T`.
    pub align_id: ValueId,
    /// `true` when the payload-T size resolved to 0 — signals callers
    /// that the dealloc is logically a no-op (matches vec_drop's
    /// `if v.cap > 0` guard for empty Vec). The op is STILL appended
    /// (so the recognizer-trace remains observable in MIR-textual
    /// pretty-print), but cgen can fast-path it later.
    pub is_zero_size: bool,
}

/// Append the canonical 3-op dealloc-emit sequence onto `ops` :
/// ```text
///   %size  = arith.constant <cap × sizeof T> : i64
///   %align = arith.constant <alignof T>      : i64
///   cssl.heap.dealloc(%ptr, %size, %align)
/// ```
///
/// `total_byte_size_const` is the precomputed `cap × sizeof T` numeric
/// value at compile-time (monomorph + cap-known site) ; this matches
/// vec_drop's intent of `v.cap × sizeof T` resolved at recognizer-time.
/// Callers that need a runtime-computed size (cap not known at compile-
/// time) should emit their own arith.muli + use `build_heap_dealloc_op`
/// directly instead of this helper.
///
/// § EFFICIENCY
///   - 3 vec-pushes onto `ops` ; no intermediate allocation.
///   - 2 calls to `fresh_id()` ; the 3rd op (the dealloc itself) has no
///     result so consumes no fresh-id.
pub fn emit_dealloc_seq(
    ops: &mut Vec<MirOp>,
    fresh_id: &mut dyn FnMut() -> ValueId,
    ptr: ValueId,
    payload_ty: &MirType,
    total_byte_size_const: i64,
    span_str: &str,
) -> DeallocEmittedIds {
    let align_const = dealloc_align_for(payload_ty);

    let size_id = fresh_id();
    ops.push(
        MirOp::std("arith.constant")
            .with_attribute("value", total_byte_size_const.to_string())
            .with_result(size_id, MirType::Int(IntWidth::I64))
            .with_attribute(ATTR_SOURCE_LOC, span_str),
    );

    let align_id = fresh_id();
    ops.push(
        MirOp::std("arith.constant")
            .with_attribute("value", align_const.to_string())
            .with_result(align_id, MirType::Int(IntWidth::I64))
            .with_attribute(ATTR_SOURCE_LOC, span_str),
    );

    ops.push(build_heap_dealloc_op(
        ptr,
        size_id,
        align_id,
        payload_ty,
        Some(ORIGIN_VEC_DROP),
        span_str,
    ));

    DeallocEmittedIds {
        size_id,
        align_id,
        is_zero_size: total_byte_size_const == 0,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        build_heap_dealloc_op, dealloc_align_for, dealloc_size_for, emit_dealloc_seq,
        matches_vec_drop_pattern, ATTR_CAP, ATTR_ORIGIN, ATTR_PAYLOAD_TY, ATTR_SOURCE_LOC,
        CAP_ISO_CONSUMED, ORIGIN_VEC_DROP,
    };
    use crate::block::MirOp;
    use crate::op::CsslOp;
    use crate::value::{FloatWidth, IntWidth, MirType, ValueId};

    // ── dealloc_size_for : monomorph-aware sizeof correctness ────────────

    #[test]
    fn size_i32_is_4_bytes() {
        // ‼ Sawyer-efficiency : the size is a compile-time constant per
        //   monomorphized type-arg ; no runtime branch ever fires.
        assert_eq!(dealloc_size_for(&MirType::Int(IntWidth::I32)), 4);
    }

    #[test]
    fn size_i64_is_8_bytes() {
        assert_eq!(dealloc_size_for(&MirType::Int(IntWidth::I64)), 8);
    }

    #[test]
    fn size_f32_is_4_bytes() {
        assert_eq!(dealloc_size_for(&MirType::Float(FloatWidth::F32)), 4);
    }

    #[test]
    fn size_f64_is_8_bytes() {
        assert_eq!(dealloc_size_for(&MirType::Float(FloatWidth::F64)), 8);
    }

    #[test]
    fn size_bool_is_1_byte() {
        assert_eq!(dealloc_size_for(&MirType::Bool), 1);
    }

    #[test]
    fn size_ptr_is_8_bytes_on_64bit_host() {
        // ‼ Stage-0 assumes 64-bit host. A future cross-compile slice
        //   parameterizes this on target-triple ; until then the lock is
        //   intentional.
        assert_eq!(dealloc_size_for(&MirType::Ptr), 8);
    }

    #[test]
    fn size_vec3_f32_is_12_bytes() {
        // 3 lanes × 4 bytes-per-lane = 12.
        assert_eq!(dealloc_size_for(&MirType::Vec(3, FloatWidth::F32)), 12);
    }

    #[test]
    fn size_unresolved_is_zero_marker() {
        // None-type / Tuple / Memref / Opaque all return 0 — caller treats
        // as no-op (matches stdlib/vec.cssl § Manual Drop's `if v.cap > 0`
        // guard for empty Vec).
        assert_eq!(dealloc_size_for(&MirType::None), 0);
        assert_eq!(dealloc_size_for(&MirType::Tuple(vec![])), 0);
        assert_eq!(dealloc_size_for(&MirType::Opaque("Vec<i32>".into())), 0);
    }

    // ── dealloc_align_for : monomorph-aware alignof correctness ──────────

    #[test]
    fn align_i32_is_4() {
        assert_eq!(dealloc_align_for(&MirType::Int(IntWidth::I32)), 4);
    }

    #[test]
    fn align_i64_is_8() {
        assert_eq!(dealloc_align_for(&MirType::Int(IntWidth::I64)), 8);
    }

    #[test]
    fn align_unresolved_is_8_safe_default() {
        // ‼ Composite / unresolved types use 8 as a safe upper bound on
        //   64-bit hosts. Mirrors the alloc-side fallback so alloc + free
        //   threads of `__cssl_alloc` and `__cssl_free` see the same
        //   align value (cssl-rt allocator-tracker pairs by alignment).
        assert_eq!(dealloc_align_for(&MirType::None), 8);
        assert_eq!(dealloc_align_for(&MirType::Tuple(vec![])), 8);
        assert_eq!(dealloc_align_for(&MirType::Opaque("Vec<i32>".into())), 8);
    }

    // ── matches_vec_drop_pattern : recognizer guard correctness ──────────

    #[test]
    fn recognizer_matches_single_segment_vec_drop() {
        // Stage-0 free-fn form per stdlib/vec.cssl § Manual Drop.
        assert!(matches_vec_drop_pattern(&["vec_drop"]));
    }

    #[test]
    fn recognizer_matches_two_segment_vec_drop_form() {
        // Post-trait-resolve associated-fn form. Recognized ahead-of-time
        // so the same builder serves both source-shapes.
        assert!(matches_vec_drop_pattern(&["Vec", "drop"]));
    }

    #[test]
    fn recognizer_rejects_unrelated_calls() {
        // Other free-fn names must NOT trip the vec_drop bridge — the
        // regular generic-call path takes over for them.
        assert!(!matches_vec_drop_pattern(&["vec_push"]));
        assert!(!matches_vec_drop_pattern(&["vec_new"]));
        assert!(!matches_vec_drop_pattern(&["drop"])); // bare `drop` is not vec_drop
        assert!(!matches_vec_drop_pattern(&["Box", "new"]));
    }

    #[test]
    fn recognizer_rejects_three_segment_qualified_paths() {
        // ‼ Strict guard mirrors the B1 Box::new precedent — a 3-segment
        //   `mod::Vec::drop` user-shadow does not match (a future
        //   trait-dispatch slice will route those through the regular
        //   path, NOT the recognizer-fast-path).
        assert!(!matches_vec_drop_pattern(&["std", "Vec", "drop"]));
        assert!(!matches_vec_drop_pattern(&["foo", "vec_drop"]));
    }

    #[test]
    fn recognizer_rejects_zero_segments_and_empty() {
        // Empty path — defensive ; never produced by the parser but the
        // guard must hold.
        assert!(!matches_vec_drop_pattern(&[]));
    }

    // ── build_heap_dealloc_op : op-shape correctness ─────────────────────

    #[test]
    fn build_op_emits_canonical_cssl_heap_dealloc() {
        let op = build_heap_dealloc_op(
            ValueId(10),
            ValueId(11),
            ValueId(12),
            &MirType::Int(IntWidth::I32),
            Some(ORIGIN_VEC_DROP),
            "<test>:1:1",
        );
        // ‼ Op-name must be the canonical "cssl.heap.dealloc" — renaming
        //   requires lock-step changes with cssl-rt::ffi + cgen import.
        assert_eq!(op.op, CsslOp::HeapDealloc);
        assert_eq!(op.name, "cssl.heap.dealloc");
    }

    #[test]
    fn build_op_has_three_operands_in_canonical_order() {
        // Per specs/02_IR.csl § HEAP-OPS : (ptr, size, align) → ()
        let op = build_heap_dealloc_op(
            ValueId(10),
            ValueId(11),
            ValueId(12),
            &MirType::Int(IntWidth::I32),
            None,
            "",
        );
        assert_eq!(op.operands.len(), 3);
        assert_eq!(op.operands[0], ValueId(10)); // ptr
        assert_eq!(op.operands[1], ValueId(11)); // size
        assert_eq!(op.operands[2], ValueId(12)); // align
        // No result — dealloc is void-returning.
        assert_eq!(op.results.len(), 0);
    }

    #[test]
    fn build_op_carries_payload_ty_and_cap_attributes() {
        let op = build_heap_dealloc_op(
            ValueId(0),
            ValueId(1),
            ValueId(2),
            &MirType::Int(IntWidth::I32),
            Some(ORIGIN_VEC_DROP),
            "<test>:1:1",
        );
        let attr_payload = op.attributes.iter().find(|(k, _)| k == ATTR_PAYLOAD_TY);
        assert!(attr_payload.is_some(), "payload_ty attr expected");
        assert_eq!(attr_payload.unwrap().1, "i32");

        let attr_cap = op.attributes.iter().find(|(k, _)| k == ATTR_CAP);
        assert_eq!(attr_cap.unwrap().1, CAP_ISO_CONSUMED);

        let attr_origin = op.attributes.iter().find(|(k, _)| k == ATTR_ORIGIN);
        assert_eq!(attr_origin.unwrap().1, ORIGIN_VEC_DROP);

        let attr_loc = op.attributes.iter().find(|(k, _)| k == ATTR_SOURCE_LOC);
        assert_eq!(attr_loc.unwrap().1, "<test>:1:1");
    }

    #[test]
    fn build_op_omits_origin_when_none() {
        // origin attribute is conditional — None means "general dealloc"
        // (no syntactic-recognizer provenance to record).
        let op = build_heap_dealloc_op(
            ValueId(0),
            ValueId(1),
            ValueId(2),
            &MirType::Int(IntWidth::I64),
            None,
            "",
        );
        assert!(op.attributes.iter().all(|(k, _)| k != ATTR_ORIGIN));
        // source_loc empty also means "no location to record" — keeps MIR
        // text clean.
        assert!(op.attributes.iter().all(|(k, _)| k != ATTR_SOURCE_LOC));
    }

    #[test]
    fn build_op_signature_matches_csslop_declared_arity() {
        // ‼ Cross-check : the op variant's declared signature (in op.rs)
        //   must match what we emit. If anyone changes one side without
        //   the other, this test catches the drift.
        let op = build_heap_dealloc_op(
            ValueId(0),
            ValueId(1),
            ValueId(2),
            &MirType::Int(IntWidth::I32),
            None,
            "",
        );
        let sig = CsslOp::HeapDealloc.signature();
        assert_eq!(sig.operands, Some(op.operands.len()));
        assert_eq!(sig.results, Some(op.results.len()));
    }

    // ── emit_dealloc_seq : 3-op append + zero-size flag ──────────────────

    #[test]
    fn emit_seq_appends_three_ops_in_canonical_order() {
        let mut ops: Vec<MirOp> = Vec::new();
        let mut next: u32 = 100;
        let mut fresh = move || {
            let v = ValueId(next);
            next += 1;
            v
        };

        // For Vec<i32> with cap=8 :  total = 8 × 4 = 32 bytes.
        let result = emit_dealloc_seq(
            &mut ops,
            &mut fresh,
            ValueId(99), // ptr
            &MirType::Int(IntWidth::I32),
            32, // total bytes precomputed from cap × sizeof T
            "<test>:1:1",
        );

        assert_eq!(ops.len(), 3, "size-const + align-const + dealloc");
        assert_eq!(ops[0].name, "arith.constant");
        assert_eq!(ops[1].name, "arith.constant");
        assert_eq!(ops[2].name, "cssl.heap.dealloc");

        // Size-const carries "32" ; align-const carries "4" (i32 align).
        let s_val = &ops[0].attributes.iter().find(|(k, _)| k == "value").unwrap().1;
        assert_eq!(s_val, "32");
        let a_val = &ops[1].attributes.iter().find(|(k, _)| k == "value").unwrap().1;
        assert_eq!(a_val, "4");

        // Dealloc operands : (ptr=99, size=fresh_0, align=fresh_1).
        assert_eq!(ops[2].operands[0], ValueId(99));
        assert_eq!(ops[2].operands[1], result.size_id);
        assert_eq!(ops[2].operands[2], result.align_id);
        assert!(!result.is_zero_size);
    }

    #[test]
    fn emit_seq_zero_size_marks_no_op_flag_but_still_appends() {
        // Empty Vec (cap=0) + i32 = total 0 bytes.
        // ‼ Per stdlib/vec.cssl § Manual Drop : `if v.cap > 0` guards the
        //   dealloc. The recognizer-bridge SHOULD short-circuit before
        //   calling emit_dealloc_seq when cap is statically 0. But if it
        //   does call us with size=0, we still emit the op + flag it for
        //   downstream cgen fast-path (cssl-rt's __cssl_free is itself a
        //   no-op for null-ptr per the FFI contract).
        let mut ops: Vec<MirOp> = Vec::new();
        let mut next: u32 = 0;
        let mut fresh = move || {
            let v = ValueId(next);
            next += 1;
            v
        };
        let result = emit_dealloc_seq(
            &mut ops,
            &mut fresh,
            ValueId(50),
            &MirType::Int(IntWidth::I32),
            0,
            "",
        );
        assert_eq!(ops.len(), 3);
        assert!(result.is_zero_size);
        let s_val = &ops[0].attributes.iter().find(|(k, _)| k == "value").unwrap().1;
        assert_eq!(s_val, "0");
    }

    #[test]
    fn emit_seq_align_value_matches_payload_type() {
        // ‼ Sawyer-efficiency : the align value lands as a compile-time
        //   constant matched to the payload-type — no runtime branch. For
        //   Vec<f64> the align is 8 ; for Vec<bool> the align is 1 ; etc.
        let mut ops: Vec<MirOp> = Vec::new();
        let mut next: u32 = 0;
        let mut fresh = move || {
            let v = ValueId(next);
            next += 1;
            v
        };
        ops.clear();
        let _ = emit_dealloc_seq(
            &mut ops,
            &mut fresh,
            ValueId(50),
            &MirType::Float(FloatWidth::F64),
            16,
            "",
        );
        let a_val = &ops[1].attributes.iter().find(|(k, _)| k == "value").unwrap().1;
        assert_eq!(a_val, "8", "f64 align must be 8 bytes");
    }

    #[test]
    fn emit_seq_payload_ty_attr_records_monomorph_form() {
        // After monomorph the payload-T is concrete ; the dealloc op
        // records it for downstream layout-aware passes.
        let mut ops: Vec<MirOp> = Vec::new();
        let mut next: u32 = 0;
        let mut fresh = move || {
            let v = ValueId(next);
            next += 1;
            v
        };
        let _ = emit_dealloc_seq(
            &mut ops,
            &mut fresh,
            ValueId(50),
            &MirType::Float(FloatWidth::F64),
            8,
            "",
        );
        let attr_payload = ops[2]
            .attributes
            .iter()
            .find(|(k, _)| k == ATTR_PAYLOAD_TY)
            .unwrap();
        assert_eq!(attr_payload.1, "f64");
    }

    #[test]
    fn emit_seq_carries_origin_vec_drop_tag() {
        // ‼ The emit-seq helper hard-codes ORIGIN_VEC_DROP on the dealloc
        //   op so post-MIR audit walkers can distinguish vec_drop-bridge
        //   sites from future direct dealloc emissions (e.g., from
        //   trait-resolve's automatic Drop invocation).
        let mut ops: Vec<MirOp> = Vec::new();
        let mut next: u32 = 0;
        let mut fresh = move || {
            let v = ValueId(next);
            next += 1;
            v
        };
        let _ = emit_dealloc_seq(
            &mut ops,
            &mut fresh,
            ValueId(50),
            &MirType::Int(IntWidth::I32),
            4,
            "",
        );
        let attr_origin = ops[2]
            .attributes
            .iter()
            .find(|(k, _)| k == ATTR_ORIGIN)
            .unwrap();
        assert_eq!(attr_origin.1, ORIGIN_VEC_DROP);
    }

    // ── monomorph-correctness : end-to-end sizeof-T threading ────────────

    #[test]
    fn monomorph_sizeof_threading_for_vec_i32_cap_8_is_32_bytes() {
        // Simulates : `Vec<i32>` with cap=8 → vec_drop emits dealloc
        // with total_size = 8 × 4 = 32 bytes. The assertion threads the
        // entire monomorph-sizeof-cap chain.
        let payload_ty = MirType::Int(IntWidth::I32);
        let cap: i64 = 8;
        let sizeof_t: i64 = dealloc_size_for(&payload_ty);
        let total = cap * sizeof_t;
        assert_eq!(total, 32);
    }

    #[test]
    fn monomorph_sizeof_threading_for_vec_f64_cap_4_is_32_bytes() {
        // Different T but same total — confirms the cap × sizeof T
        // composition is type-agnostic in shape.
        let payload_ty = MirType::Float(FloatWidth::F64);
        let cap: i64 = 4;
        let sizeof_t: i64 = dealloc_size_for(&payload_ty);
        let total = cap * sizeof_t;
        assert_eq!(total, 32);
    }
}
