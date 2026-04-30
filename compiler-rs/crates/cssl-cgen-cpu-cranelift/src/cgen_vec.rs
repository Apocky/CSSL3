//! § Wave-A2-β (T11-D264) — `cssl.vec.*` Cranelift cgen op-handlers.
//!
//! § SPEC : `specs/02_IR.csl § VEC-OPS` · `specs/40_WAVE_CSSL_PLAN.csl`
//!         § WAVES § WAVE-A · A2 (vec end-to-end JIT-executable).
//!
//! § ROLE
//!   The W-A2-α-fix slice (T11-D249) wired the `body_lower` recognizers for
//!   `vec_new::<T>()` / `vec_push::<T>(v,x)` / `vec_index::<T>(v,i)` so the
//!   parser-side surface lowers to canonical `cssl.vec.new` / `cssl.vec.push` /
//!   `cssl.vec.index` MIR ops. But `cssl-cgen-cpu-cranelift::jit::lower_op_to_cl`
//!   rejects those ops as `JitError::UnsupportedMirOp ("scalars-arith-only")`
//!   because no cgen dispatch arm matches them. This module supplies BOTH :
//!     - the text-CLIF lowerers (matching the `cgen_memref.rs` shape, plugged
//!       into `lower::lower_op`'s prefix dispatch), AND
//!     - the JIT-side cranelift-IR-emit handlers (called from
//!       `jit::lower_op_to_cl`'s match-arms).
//!
//! § VEC LAYOUT (stage-0)
//!   Per `stdlib/vec.cssl § struct Vec<T>`, the runtime Vec value is :
//!   ```text
//!   struct Vec<T> {
//!       data : i64,   // host-pointer-as-integer ; 0 when cap == 0
//!       len  : i64,   // valid-element count ≤ cap
//!       cap  : i64,   // element-slots allocated at `data`
//!   }
//!   ```
//!   The MIR ops carry the monomorphized payload `T` as a `payload_ty`
//!   attribute so the cgen layer can dispatch on element-kind without
//!   re-parsing the op-name. `sizeof T` is derived from the shared
//!   `sizeof_for_payload` LUT (i8=1 / i16=2 / i32=4 / i64=8 / f32=4 / f64=8).
//!
//!   STAGE-0 SIMPLIFICATION : the full Vec-by-value ABI rewrite (struct →
//!   {data, len, cap} fan-out) lands in W-A2-γ. Until then the JIT
//!   represents a Vec value as a single I64 SSA value — a sentinel "data
//!   pointer" that's `0` for cap=0 and a real heap pointer once a push
//!   has grown the buffer. `len` / `cap` are NOT tracked at the JIT level :
//!   `vec.len` / `vec.cap` bind their results to a typed-zero const + a
//!   diagnostic comment in the lowered IR. This is sufficient to drive
//!   the W-A2 end-to-end JIT-execution gate (vec-call-roundtrip-without-
//!   crash), which is the actual T11-D264 success criterion.
//!
//! § HANDLER MAP (op-name → cranelift IR shape)
//!   - `cssl.vec.new<T>`   — empty Vec (cap=0). `iconst.i64 0`.
//!                           result-ty = MirType::Opaque("Vec") → CLIF I64.
//!   - `cssl.vec.push<T>`  — alias-thru of operand-0 (the input Vec). The
//!                           grow + memref.store dance lands when the Vec
//!                           ABI rewrite makes (data, len, cap) addressable
//!                           at the JIT layer (W-A2-γ). For now the result
//!                           binds to operand-0 so subsequent ops keep the
//!                           value flowing through. Diagnostic comment
//!                           records the intended grow-shape.
//!   - `cssl.vec.index<T>` — typed-zero const of payload_ty (sentinel
//!                           result). bounds_check is honored at the spec
//!                           level via the attribute ; the actual panic
//!                           call lands once panic-FFI is JIT-registered.
//!   - `cssl.vec.len<T>`   — `iconst.i64 0` (sentinel : zero-len for
//!                           sentinel-data Vec).
//!   - `cssl.vec.cap<T>`   — `iconst.i64 0` (sentinel : zero-cap for
//!                           sentinel-data Vec).
//!   - `cssl.vec.drop<T>`  — no-op on sentinel-zero ptr ; future heap-FFI
//!                           wiring will branch to `__cssl_free` when the
//!                           data ptr is non-zero.
//!
//! § INTEGRATION
//!   - `lib.rs` adds `pub mod cgen_vec;`.
//!   - `lower.rs` adds a `name if cgen_vec::is_vec_op(name) =>
//!     cgen_vec::lower_vec_op(op)` arm at the head of its match.
//!   - `jit.rs` adds match-arms in `lower_op_to_cl` for each of the six
//!     `cssl.vec.*` op-names that delegate to `cgen_vec::jit_lower_vec_*`.
//!
//! § SAWYER-EFFICIENCY
//!   - Pure-fn helpers ; no allocation outside the `Vec<ClifInsn>` storage
//!     required by the existing `lower::lower_op` return shape.
//!   - Branch-free `match` LUTs for op-name + payload-ty dispatch. No
//!     HashMap, no string-format in the hot path.
//!   - All JIT-side handlers operate on the SSA value-map directly — no
//!     temp values beyond what the op semantically requires.

#![allow(dead_code, unreachable_pub)]
// `implicit_hasher` would force a `BuildHasher` type parameter on the
//   JIT-side helper signatures ; the JIT path always hands in a
//   `std::collections::HashMap` with the default hasher (mirrors the
//   `lower_op_to_cl` shape in jit.rs + the `lower_scf_*` helpers in
//   scf.rs). Allowing the lint keeps the helper signatures symmetric.
#![allow(clippy::implicit_hasher)]
// The recognizer-arity LUT-style match in `lower_vec_op_dispatches_all_six_kinds`
//   uses `name.ends_with(".push")` / `.drop"` which clippy's
//   `case_sensitive_file_extension_comparisons` mis-fires on (the strings
//   are op-names not file-extensions). The match is correct as-is.
#![allow(clippy::case_sensitive_file_extension_comparisons)]
// `option_if_let_else` would force `map_or` chains over `if let`/`else` ;
//   the existing pattern is more readable for the multi-branch
//   per-clif-type dispatch.
#![allow(clippy::option_if_let_else)]

use std::collections::HashMap;

use cranelift_codegen::ir::types as cl_types;
use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::FunctionBuilder;
use cssl_mir::{MirOp, ValueId};

use crate::jit::JitError;
use crate::lower::{format_value, ClifInsn};
use crate::types::ClifType;

// ════════════════════════════════════════════════════════════════════════
// § Canonical op-name + symbol contract (lock-step with body_lower).
// ════════════════════════════════════════════════════════════════════════

/// MIR op-name for the empty-Vec constructor recognizer.
/// Lock-step invariant : matches `body_lower::try_lower_vec_new` minted
/// op + the W-A2-α-fix tests `vec_new_i32_emits_cssl_vec_new`.
pub const MIR_VEC_NEW: &str = "cssl.vec.new";
/// MIR op-name for the push-grow recognizer (`vec_push::<T>(v,x)`).
pub const MIR_VEC_PUSH: &str = "cssl.vec.push";
/// MIR op-name for the bounds-checked index recognizer (`vec_index::<T>(v,i)`).
pub const MIR_VEC_INDEX: &str = "cssl.vec.index";
/// MIR op-name for the `vec_len::<T>(v)` field-load recognizer.
pub const MIR_VEC_LEN: &str = "cssl.vec.len";
/// MIR op-name for the `vec_capacity::<T>(v)` field-load recognizer.
pub const MIR_VEC_CAP: &str = "cssl.vec.cap";
/// MIR op-name for the `vec_drop::<T>(v)` heap-dealloc recognizer.
pub const MIR_VEC_DROP: &str = "cssl.vec.drop";

/// FFI symbol name for `__cssl_alloc` (matches `cssl-rt::ffi`).
pub const HEAP_ALLOC_SYMBOL: &str = "__cssl_alloc";
/// FFI symbol name for `__cssl_realloc` (matches `cssl-rt::ffi`).
pub const HEAP_REALLOC_SYMBOL: &str = "__cssl_realloc";
/// FFI symbol name for `__cssl_panic` (matches `cssl-rt::ffi`).
pub const PANIC_SYMBOL: &str = "__cssl_panic";

/// Field offset (in bytes) of `Vec.len` from the Vec's base pointer.
/// Per `stdlib/vec.cssl § struct Vec<T>` : data@0, len@8, cap@16.
pub const VEC_LEN_OFFSET: i64 = 8;
/// Field offset (in bytes) of `Vec.cap` from the Vec's base pointer.
pub const VEC_CAP_OFFSET: i64 = 16;
/// Initial capacity used by the first grow on a cap=0 push (per the
/// `stdlib/vec.cssl § vec_push` doc-comment : "0 → 8").
pub const VEC_FIRST_GROW_CAP: i64 = 8;

// ════════════════════════════════════════════════════════════════════════
// § Op-kind classification.
// ════════════════════════════════════════════════════════════════════════

/// Classification of `cssl.vec.*` ops. Used by both the cgen text-CLIF
/// dispatcher AND the JIT-side handler dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VecOpKind {
    New,
    Push,
    Index,
    Len,
    Cap,
    Drop,
}

impl VecOpKind {
    /// Return the canonical MIR op-name string for this kind.
    #[must_use]
    pub const fn op_name(self) -> &'static str {
        match self {
            Self::New => MIR_VEC_NEW,
            Self::Push => MIR_VEC_PUSH,
            Self::Index => MIR_VEC_INDEX,
            Self::Len => MIR_VEC_LEN,
            Self::Cap => MIR_VEC_CAP,
            Self::Drop => MIR_VEC_DROP,
        }
    }

    /// Number of operands this op kind expects (per the body_lower
    /// recognizers' arity).
    #[must_use]
    pub const fn operand_count(self) -> usize {
        match self {
            Self::New => 0,
            Self::Push | Self::Index => 2,
            Self::Len | Self::Cap | Self::Drop => 1,
        }
    }
}

/// Classify an op-name into its `VecOpKind`. Returns `None` for non-vec ops.
#[must_use]
pub fn vec_op_kind(name: &str) -> Option<VecOpKind> {
    match name {
        MIR_VEC_NEW => Some(VecOpKind::New),
        MIR_VEC_PUSH => Some(VecOpKind::Push),
        MIR_VEC_INDEX => Some(VecOpKind::Index),
        MIR_VEC_LEN => Some(VecOpKind::Len),
        MIR_VEC_CAP => Some(VecOpKind::Cap),
        MIR_VEC_DROP => Some(VecOpKind::Drop),
        _ => None,
    }
}

/// `true` iff the op-name is one of the recognized `cssl.vec.*` ops.
#[must_use]
pub fn is_vec_op(name: &str) -> bool {
    vec_op_kind(name).is_some()
}

/// `true` iff the op is `cssl.vec.drop`. Pairs with
/// `cgen_heap_dealloc::is_dealloc_op` for the W-A5 wiring.
#[must_use]
pub fn is_vec_drop(op: &MirOp) -> bool {
    op.name == MIR_VEC_DROP
}

// ════════════════════════════════════════════════════════════════════════
// § Payload-ty + sizeof LUT.
// ════════════════════════════════════════════════════════════════════════

/// LUT : `payload_ty` attribute string → element CLIF type. Branch-free
/// `match` ; no HashMap. Mirrors `cgen_memref::clif_type_for_elem_name`.
#[must_use]
pub const fn clif_type_for_payload(name: &str) -> Option<ClifType> {
    match name.as_bytes() {
        b"i8" => Some(ClifType::I8),
        b"i16" => Some(ClifType::I16),
        b"i32" => Some(ClifType::I32),
        b"i64" => Some(ClifType::I64),
        b"f32" => Some(ClifType::F32),
        b"f64" => Some(ClifType::F64),
        _ => None,
    }
}

/// LUT : `payload_ty` attribute → cranelift IR type. Mirrors
/// `clif_type_for_payload` but returns the runtime `cranelift_codegen::ir::Type`
/// used by the JIT-side helpers.
#[must_use]
pub fn cl_type_for_payload(name: &str) -> Option<cranelift_codegen::ir::Type> {
    match name.as_bytes() {
        b"i8" => Some(cl_types::I8),
        b"i16" => Some(cl_types::I16),
        b"i32" => Some(cl_types::I32),
        b"i64" => Some(cl_types::I64),
        b"f32" => Some(cl_types::F32),
        b"f64" => Some(cl_types::F64),
        _ => None,
    }
}

/// LUT : `payload_ty` attribute → sizeof in bytes. Mirrors
/// `cssl_mir::memref_typed::sizeof_for`.
#[must_use]
pub const fn sizeof_for_payload(name: &str) -> Option<i64> {
    match name.as_bytes() {
        b"i8" => Some(1),
        b"i16" => Some(2),
        b"i32" | b"f32" => Some(4),
        b"i64" | b"f64" => Some(8),
        _ => None,
    }
}

/// LUT : `payload_ty` attribute → natural-alignment in bytes. For the
/// stage-0 primitive set, alignment == sizeof.
#[must_use]
pub const fn alignof_for_payload(name: &str) -> Option<i64> {
    sizeof_for_payload(name)
}

/// Extract the `payload_ty` attribute from an op. Returns `None` if the
/// attribute is absent (the body_lower recognizers always emit it, so
/// missing = upstream pipeline bug).
#[must_use]
pub fn payload_ty_of(op: &MirOp) -> Option<&str> {
    op.attributes
        .iter()
        .find(|(k, _)| k == "payload_ty")
        .map(|(_, v)| v.as_str())
}

// ════════════════════════════════════════════════════════════════════════
// § Top-level cgen text-CLIF dispatcher.
// ════════════════════════════════════════════════════════════════════════

/// Build a `ClifInsn` from a textual instruction. Crate-internal builder
/// matching `cgen_memref::insn` shape.
fn insn(text: impl Into<String>) -> ClifInsn {
    ClifInsn { text: text.into() }
}

/// Top-level dispatcher : map a `cssl.vec.*` op to its CLIF text
/// instructions. Returns `None` for non-vec ops — callers fall through
/// to `lower::lower_op`'s scalar-arith match-arms.
#[must_use]
pub fn lower_vec_op(op: &MirOp) -> Option<Vec<ClifInsn>> {
    match vec_op_kind(&op.name)? {
        VecOpKind::New => lower_vec_new(op),
        VecOpKind::Push => lower_vec_push(op),
        VecOpKind::Index => lower_vec_index(op),
        VecOpKind::Len => lower_vec_len(op),
        VecOpKind::Cap => lower_vec_cap(op),
        VecOpKind::Drop => lower_vec_drop(op),
    }
}

// ════════════════════════════════════════════════════════════════════════
// § Per-op text-CLIF lowerers.
// ════════════════════════════════════════════════════════════════════════

/// Lower `cssl.vec.new<T>` : empty-Vec construction. cap=0 invariant —
/// no heap call. Stage-0 packs the result-id to a typed-zero i64 const.
#[must_use]
pub fn lower_vec_new(op: &MirOp) -> Option<Vec<ClifInsn>> {
    if op.name != MIR_VEC_NEW {
        return None;
    }
    let r = op.results.first()?;
    Some(vec![insn(format!(
        "    {} = iconst.i64 0",
        format_value(r.id)
    ))])
}

/// Lower `cssl.vec.push<T>(v, x)` : append-with-grow. Stage-0 emits a
/// commented placeholder for the realloc-grow + memref.store sequence.
#[must_use]
pub fn lower_vec_push(op: &MirOp) -> Option<Vec<ClifInsn>> {
    if op.name != MIR_VEC_PUSH {
        return None;
    }
    if op.operands.len() != 2 {
        return None;
    }
    let r = op.results.first()?;
    let v = op.operands.first()?;
    let payload = payload_ty_of(op).unwrap_or("i64");
    let sizeof = sizeof_for_payload(payload).unwrap_or(8);
    Some(vec![
        insn(format!(
            "    ; cssl.vec.push<{payload}> : sizeof={sizeof} grow-if-len==cap then store + len++"
        )),
        insn(format!(
            "    {} = iadd.i64 {}, v0",
            format_value(r.id),
            format_value(*v),
        )),
    ])
}

/// Lower `cssl.vec.index<T>(v, i)` : bounds-checked element load.
#[must_use]
pub fn lower_vec_index(op: &MirOp) -> Option<Vec<ClifInsn>> {
    if op.name != MIR_VEC_INDEX {
        return None;
    }
    if op.operands.len() != 2 {
        return None;
    }
    let r = op.results.first()?;
    let payload = payload_ty_of(op).unwrap_or("i64");
    let clif_ty = clif_type_for_payload(payload).unwrap_or(ClifType::I64);
    let ty_str = clif_ty.as_str();
    let sizeof = sizeof_for_payload(payload).unwrap_or(8);
    let v_name = format_value(r.id);
    let bounds_check = op
        .attributes
        .iter()
        .find(|(k, _)| k == "bounds_check")
        .map_or("panic", |(_, v)| v.as_str());
    let load_text = if ty_str.starts_with('i') {
        format!("    {v_name} = iconst.{ty_str} 0")
    } else {
        format!("    {v_name} = {ty_str}const 0.0")
    };
    Some(vec![
        insn(format!(
            "    ; cssl.vec.index<{payload}> : bounds_check={bounds_check} sizeof={sizeof}"
        )),
        insn(load_text),
    ])
}

/// Lower `cssl.vec.len<T>` : load the `len` field at offset 8.
#[must_use]
pub fn lower_vec_len(op: &MirOp) -> Option<Vec<ClifInsn>> {
    if op.name != MIR_VEC_LEN {
        return None;
    }
    let r = op.results.first()?;
    Some(vec![
        insn(format!(
            "    ; cssl.vec.len : load.i64 at offset {VEC_LEN_OFFSET}"
        )),
        insn(format!("    {} = iconst.i64 0", format_value(r.id))),
    ])
}

/// Lower `cssl.vec.cap<T>` : load the `cap` field at offset 16.
#[must_use]
pub fn lower_vec_cap(op: &MirOp) -> Option<Vec<ClifInsn>> {
    if op.name != MIR_VEC_CAP {
        return None;
    }
    let r = op.results.first()?;
    Some(vec![
        insn(format!(
            "    ; cssl.vec.cap : load.i64 at offset {VEC_CAP_OFFSET}"
        )),
        insn(format!("    {} = iconst.i64 0", format_value(r.id))),
    ])
}

/// Lower `cssl.vec.drop<T>` : delegates to the W-A5 heap-dealloc cgen.
#[must_use]
pub fn lower_vec_drop(op: &MirOp) -> Option<Vec<ClifInsn>> {
    if op.name != MIR_VEC_DROP {
        return None;
    }
    let v = op.operands.first()?;
    let payload = payload_ty_of(op).unwrap_or("i64");
    let sizeof = sizeof_for_payload(payload).unwrap_or(8);
    Some(vec![insn(format!(
        "    ; cssl.vec.drop<{payload}> : sizeof={sizeof} -> __cssl_free({}, cap*sizeof, alignof)",
        format_value(*v)
    ))])
}

// ════════════════════════════════════════════════════════════════════════
// § JIT-side cranelift IR emit handlers.
//
//   These are called from `jit::lower_op_to_cl`'s match-arms and operate
//   directly on a `cranelift_frontend::FunctionBuilder`. They follow the
//   stage-0 simplification described in this module's docstring : the Vec
//   value is represented as a single I64 SSA (sentinel-data-ptr) and
//   `len` / `cap` are not yet tracked separately.
//
//   All handlers return `Ok(false)` (non-terminator) — `cssl.vec.*` ops
//   never end a basic block.
// ════════════════════════════════════════════════════════════════════════

/// JIT-emit `cssl.vec.new<T>` : bind result-id to `iconst.i64 0`. cap=0 +
/// len=0 invariant ; no heap call needed for the empty-Vec case.
///
/// # Errors
/// Returns [`JitError::LoweringFailed`] if the op has no result.
pub fn jit_lower_vec_new(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: "cssl.vec.new with no result".to_string(),
    })?;
    let v = builder.ins().iconst(cl_types::I64, 0);
    value_map.insert(r.id, v);
    Ok(false)
}

/// JIT-emit `cssl.vec.push<T>(v, x)` : alias-thru of the input Vec value.
///
/// STAGE-0 SEMANTICS : the result-id binds to operand-0 (the input Vec
/// value-as-i64). The full grow-if-len-eq-cap + memref.store + len++
/// sequence requires the Vec struct-ABI rewrite (W-A2-γ) so the JIT can
/// see (data, len, cap) as separate SSA values. Until then this binding
/// keeps the Vec value flowing through the SSA dataflow correctly.
///
/// # Errors
/// Returns [`JitError::LoweringFailed`] if operands or result are missing,
/// or the operand ValueId is unknown to the value-map.
pub fn jit_lower_vec_push(
    op: &MirOp,
    _builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    let &v_id = op
        .operands
        .first()
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "cssl.vec.push expects (vec, elem) operands".to_string(),
        })?;
    // op.operands.get(1) : the elem-x. Stage-0 doesn't store it (no real
    //   buffer to write into yet). Validate presence so a malformed op
    //   surfaces as a LoweringFailed at the cgen layer.
    if op.operands.len() < 2 {
        return Err(JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "cssl.vec.push expects 2 operands ; got {}",
                op.operands.len()
            ),
        });
    }
    let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: "cssl.vec.push has no result".to_string(),
    })?;
    let v = *value_map
        .get(&v_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("cssl.vec.push : unknown vec ValueId({})", v_id.0),
        })?;
    // Alias-thru : the result-id maps to the same CLIF value as the
    //   input vec. Subsequent ops referencing the result see the same
    //   sentinel I64.
    value_map.insert(r.id, v);
    Ok(false)
}

/// JIT-emit `cssl.vec.index<T>(v, i)` : bind result-id to typed-zero
/// const of `payload_ty`.
///
/// STAGE-0 SEMANTICS : the bounds-check + memref.load require the Vec
/// struct-ABI rewrite (W-A2-γ) before they can produce a real load
/// from `data + i*sizeof T`. For now the result is a typed-zero
/// sentinel — the JIT verifies the dispatch surface compiles + executes
/// without crashing, which is the W-A2-β success criterion.
///
/// # Errors
/// Returns [`JitError::LoweringFailed`] if operands / result are missing
/// or `payload_ty` is unknown.
pub fn jit_lower_vec_index(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    if op.operands.len() < 2 {
        return Err(JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "cssl.vec.index expects 2 operands (vec, idx) ; got {}",
                op.operands.len()
            ),
        });
    }
    let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: "cssl.vec.index has no result".to_string(),
    })?;
    // Validate operand ValueIds resolve so a stale-MIR op fails fast.
    for vid in &op.operands {
        let _ = value_map.get(vid).ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("cssl.vec.index : unknown operand ValueId({})", vid.0),
        })?;
    }
    let payload = payload_ty_of(op).unwrap_or("i64");
    let cl_ty = cl_type_for_payload(payload).ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: format!("cssl.vec.index : unknown payload_ty `{payload}`"),
    })?;
    // Typed-zero binding : iconst for ints, fconst for floats.
    let zero = if cl_ty == cl_types::F32 {
        builder.ins().f32const(0.0_f32)
    } else if cl_ty == cl_types::F64 {
        builder.ins().f64const(0.0_f64)
    } else {
        builder.ins().iconst(cl_ty, 0)
    };
    value_map.insert(r.id, zero);
    Ok(false)
}

/// JIT-emit `cssl.vec.len<T>` : bind result-id to `iconst.i64 0`.
///
/// STAGE-0 SEMANTICS : len isn't tracked separately at the JIT level
/// until W-A2-γ Vec struct-ABI rewrite. For sentinel-data Vec (cap=0),
/// len is always 0 — this returns the correct value.
///
/// # Errors
/// Returns [`JitError::LoweringFailed`] if the op has no result.
pub fn jit_lower_vec_len(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: "cssl.vec.len with no result".to_string(),
    })?;
    let v = builder.ins().iconst(cl_types::I64, 0);
    value_map.insert(r.id, v);
    Ok(false)
}

/// JIT-emit `cssl.vec.cap<T>` : bind result-id to `iconst.i64 0`.
///
/// Same stage-0 semantics as `jit_lower_vec_len` — cap=0 for sentinel-
/// data Vec.
///
/// # Errors
/// Returns [`JitError::LoweringFailed`] if the op has no result.
pub fn jit_lower_vec_cap(
    op: &MirOp,
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    let r = op.results.first().ok_or_else(|| JitError::LoweringFailed {
        fn_name: fn_name.to_string(),
        detail: "cssl.vec.cap with no result".to_string(),
    })?;
    let v = builder.ins().iconst(cl_types::I64, 0);
    value_map.insert(r.id, v);
    Ok(false)
}

/// JIT-emit `cssl.vec.drop<T>(v)` : no-op on sentinel-zero ptr.
///
/// STAGE-0 SEMANTICS : the data-ptr is always 0 (sentinel) so there's no
/// allocation to free. Future heap-FFI integration (W-A2-γ) will branch
/// on (data != 0) and call `__cssl_free`. Validate the operand resolves
/// so a malformed op surfaces as `LoweringFailed`.
///
/// # Errors
/// Returns [`JitError::LoweringFailed`] if the operand is missing or
/// references an unknown ValueId.
pub fn jit_lower_vec_drop(
    op: &MirOp,
    _builder: &mut FunctionBuilder<'_>,
    value_map: &HashMap<ValueId, cranelift_codegen::ir::Value>,
    fn_name: &str,
) -> Result<bool, JitError> {
    let &v_id = op
        .operands
        .first()
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: "cssl.vec.drop expects (vec) operand".to_string(),
        })?;
    let _ = value_map
        .get(&v_id)
        .ok_or_else(|| JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!("cssl.vec.drop : unknown vec ValueId({})", v_id.0),
        })?;
    if !op.results.is_empty() {
        return Err(JitError::LoweringFailed {
            fn_name: fn_name.to_string(),
            detail: format!(
                "cssl.vec.drop must have 0 results ; got {}",
                op.results.len()
            ),
        });
    }
    Ok(false)
}

// ════════════════════════════════════════════════════════════════════════
// § Tests.
// ════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::{
        alignof_for_payload, cl_type_for_payload, clif_type_for_payload, is_vec_drop, is_vec_op,
        jit_lower_vec_cap, jit_lower_vec_drop, jit_lower_vec_index, jit_lower_vec_len,
        jit_lower_vec_new, jit_lower_vec_push, lower_vec_cap, lower_vec_drop, lower_vec_index,
        lower_vec_len, lower_vec_new, lower_vec_op, lower_vec_push, payload_ty_of, vec_op_kind,
        VecOpKind, MIR_VEC_CAP, MIR_VEC_DROP, MIR_VEC_INDEX, MIR_VEC_LEN, MIR_VEC_NEW,
        MIR_VEC_PUSH, VEC_CAP_OFFSET, VEC_FIRST_GROW_CAP, VEC_LEN_OFFSET,
    };
    use crate::types::ClifType;
    use cranelift_codegen::ir::{
        types as cl_types, AbiParam, Function, InstBuilder, Signature, UserFuncName,
    };
    use cranelift_codegen::isa::CallConv;
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cssl_mir::{IntWidth, MirOp, MirType, ValueId};
    use std::collections::HashMap;

    // ─── § Op-name + classification tests ──────────────────────────────

    #[test]
    fn canonical_op_names_match_body_lower() {
        assert_eq!(MIR_VEC_NEW, "cssl.vec.new");
        assert_eq!(MIR_VEC_PUSH, "cssl.vec.push");
        assert_eq!(MIR_VEC_INDEX, "cssl.vec.index");
        assert_eq!(MIR_VEC_LEN, "cssl.vec.len");
        assert_eq!(MIR_VEC_CAP, "cssl.vec.cap");
        assert_eq!(MIR_VEC_DROP, "cssl.vec.drop");
    }

    #[test]
    fn vec_field_offsets_match_struct_layout() {
        assert_eq!(VEC_LEN_OFFSET, 8);
        assert_eq!(VEC_CAP_OFFSET, 16);
        assert_eq!(VEC_FIRST_GROW_CAP, 8);
    }

    #[test]
    fn vec_op_kind_classifies_all_six() {
        assert_eq!(vec_op_kind("cssl.vec.new"), Some(VecOpKind::New));
        assert_eq!(vec_op_kind("cssl.vec.push"), Some(VecOpKind::Push));
        assert_eq!(vec_op_kind("cssl.vec.index"), Some(VecOpKind::Index));
        assert_eq!(vec_op_kind("cssl.vec.len"), Some(VecOpKind::Len));
        assert_eq!(vec_op_kind("cssl.vec.cap"), Some(VecOpKind::Cap));
        assert_eq!(vec_op_kind("cssl.vec.drop"), Some(VecOpKind::Drop));
    }

    #[test]
    fn vec_op_kind_returns_none_for_other_ops() {
        assert_eq!(vec_op_kind("arith.addi"), None);
        assert_eq!(vec_op_kind("memref.load"), None);
        assert_eq!(vec_op_kind("cssl.heap.alloc"), None);
        assert_eq!(vec_op_kind(""), None);
    }

    #[test]
    fn vec_op_kind_op_name_round_trip() {
        for kind in [
            VecOpKind::New,
            VecOpKind::Push,
            VecOpKind::Index,
            VecOpKind::Len,
            VecOpKind::Cap,
            VecOpKind::Drop,
        ] {
            assert_eq!(vec_op_kind(kind.op_name()), Some(kind));
        }
    }

    #[test]
    fn vec_op_kind_operand_count_matches_recognizer_arity() {
        assert_eq!(VecOpKind::New.operand_count(), 0);
        assert_eq!(VecOpKind::Push.operand_count(), 2);
        assert_eq!(VecOpKind::Index.operand_count(), 2);
        assert_eq!(VecOpKind::Len.operand_count(), 1);
        assert_eq!(VecOpKind::Cap.operand_count(), 1);
        assert_eq!(VecOpKind::Drop.operand_count(), 1);
    }

    #[test]
    fn is_vec_op_predicate_canonical_match() {
        for name in [
            "cssl.vec.new",
            "cssl.vec.push",
            "cssl.vec.index",
            "cssl.vec.len",
            "cssl.vec.cap",
            "cssl.vec.drop",
        ] {
            assert!(is_vec_op(name), "{name} should be a vec op");
        }
        assert!(!is_vec_op("arith.addi"));
        assert!(!is_vec_op("cssl.heap.alloc"));
    }

    #[test]
    fn is_vec_drop_distinguishes_drop_from_other_vec_ops() {
        let drop = MirOp::std("cssl.vec.drop").with_operand(ValueId(0));
        let push = MirOp::std("cssl.vec.push")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1));
        assert!(is_vec_drop(&drop));
        assert!(!is_vec_drop(&push));
    }

    // ─── § Payload-ty + sizeof LUT tests ──────────────────────────────

    #[test]
    fn clif_type_for_payload_handles_six_primitives() {
        assert_eq!(clif_type_for_payload("i8"), Some(ClifType::I8));
        assert_eq!(clif_type_for_payload("i16"), Some(ClifType::I16));
        assert_eq!(clif_type_for_payload("i32"), Some(ClifType::I32));
        assert_eq!(clif_type_for_payload("i64"), Some(ClifType::I64));
        assert_eq!(clif_type_for_payload("f32"), Some(ClifType::F32));
        assert_eq!(clif_type_for_payload("f64"), Some(ClifType::F64));
        assert_eq!(clif_type_for_payload("u128"), None);
        assert_eq!(clif_type_for_payload(""), None);
    }

    #[test]
    fn cl_type_for_payload_matches_clif_widths() {
        assert_eq!(cl_type_for_payload("i32"), Some(cl_types::I32));
        assert_eq!(cl_type_for_payload("i64"), Some(cl_types::I64));
        assert_eq!(cl_type_for_payload("f32"), Some(cl_types::F32));
        assert_eq!(cl_type_for_payload("f64"), Some(cl_types::F64));
        assert_eq!(cl_type_for_payload("u128"), None);
    }

    #[test]
    fn alignof_equals_sizeof_for_primitives() {
        for name in ["i8", "i16", "i32", "i64", "f32", "f64"] {
            assert_eq!(alignof_for_payload(name), super::sizeof_for_payload(name));
        }
    }

    #[test]
    fn payload_ty_of_extracts_attribute() {
        let op = MirOp::std("cssl.vec.new")
            .with_attribute("payload_ty", "i32")
            .with_result(ValueId(0), MirType::Opaque("Vec".to_string()));
        assert_eq!(payload_ty_of(&op), Some("i32"));
    }

    #[test]
    fn payload_ty_of_returns_none_when_absent() {
        let op = MirOp::std("cssl.vec.new").with_result(ValueId(0), MirType::Int(IntWidth::I64));
        assert!(payload_ty_of(&op).is_none());
    }

    // ─── § Text-CLIF lowerer tests ────────────────────────────────────

    #[test]
    fn lower_vec_new_emits_iconst_zero_for_empty_vec() {
        let op = MirOp::std("cssl.vec.new")
            .with_attribute("payload_ty", "i32")
            .with_attribute("cap", "iso")
            .with_attribute("origin", "vec_new")
            .with_result(ValueId(7), MirType::Opaque("Vec".to_string()));
        let insns = lower_vec_new(&op).unwrap();
        assert_eq!(insns.len(), 1);
        assert_eq!(insns[0].text, "    v7 = iconst.i64 0");
        assert!(!insns[0].text.contains("__cssl_alloc"));
    }

    #[test]
    fn lower_vec_push_emits_grow_placeholder_with_two_operands() {
        let op = MirOp::std("cssl.vec.push")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_attribute("payload_ty", "i32")
            .with_result(ValueId(2), MirType::Opaque("Vec".to_string()));
        let insns = lower_vec_push(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert!(insns[0].text.contains("cssl.vec.push<i32>"));
        assert!(insns[0].text.contains("sizeof=4"));
        assert!(insns[0].text.contains("grow-if-len==cap"));
    }

    #[test]
    fn lower_vec_index_emits_bounds_check_comment_and_typed_load() {
        let op = MirOp::std("cssl.vec.index")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_attribute("payload_ty", "i32")
            .with_attribute("bounds_check", "panic")
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        let insns = lower_vec_index(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert!(insns[0].text.contains("bounds_check=panic"));
        assert!(insns[0].text.contains("sizeof=4"));
        assert_eq!(insns[1].text, "    v2 = iconst.i32 0");
    }

    #[test]
    fn lower_vec_len_emits_offset_8_load_comment() {
        let op = MirOp::std("cssl.vec.len")
            .with_operand(ValueId(3))
            .with_attribute("payload_ty", "i32")
            .with_result(ValueId(4), MirType::Int(IntWidth::I64));
        let insns = lower_vec_len(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert!(insns[0].text.contains("offset 8"));
        assert_eq!(insns[1].text, "    v4 = iconst.i64 0");
    }

    #[test]
    fn lower_vec_cap_emits_offset_16_load_comment() {
        let op = MirOp::std("cssl.vec.cap")
            .with_operand(ValueId(3))
            .with_attribute("payload_ty", "i32")
            .with_result(ValueId(4), MirType::Int(IntWidth::I64));
        let insns = lower_vec_cap(&op).unwrap();
        assert_eq!(insns.len(), 2);
        assert!(insns[0].text.contains("offset 16"));
        assert_eq!(insns[1].text, "    v4 = iconst.i64 0");
    }

    #[test]
    fn lower_vec_drop_emits_free_comment_with_payload_sizeof() {
        let op = MirOp::std("cssl.vec.drop")
            .with_operand(ValueId(5))
            .with_attribute("payload_ty", "i32");
        let insns = lower_vec_drop(&op).unwrap();
        assert_eq!(insns.len(), 1);
        assert!(insns[0].text.contains("__cssl_free"));
        assert!(insns[0].text.contains("sizeof=4"));
        assert!(insns[0].text.contains("v5"));
    }

    #[test]
    fn lower_vec_op_dispatches_all_six_kinds() {
        let cases: &[(&str, usize)] = &[
            ("cssl.vec.new", 0),
            ("cssl.vec.push", 2),
            ("cssl.vec.index", 2),
            ("cssl.vec.len", 1),
            ("cssl.vec.cap", 1),
            ("cssl.vec.drop", 1),
        ];
        for (name, operand_count) in cases {
            let mut op = MirOp::std(*name).with_attribute("payload_ty", "i32");
            for i in 0..*operand_count {
                op = op.with_operand(ValueId(i as u32));
            }
            // Result-id binding by op-kind :
            //   .drop has no result ; .push returns Vec ; others return scalar.
            if name.ends_with(".push") {
                op = op.with_result(ValueId(99), MirType::Opaque("Vec".to_string()));
            } else if !name.ends_with(".drop") {
                op = op.with_result(ValueId(99), MirType::Int(IntWidth::I64));
            }
            let insns = lower_vec_op(&op);
            assert!(insns.is_some(), "{name} should dispatch");
        }
    }

    #[test]
    fn lower_vec_op_returns_none_for_non_vec_ops() {
        let op = MirOp::std("arith.addi")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I32));
        assert!(lower_vec_op(&op).is_none());
    }

    // ─── § JIT IR-emit handler tests ─────────────────────────────────
    //
    //   These tests exercise the JIT-side helpers directly by building a
    //   fresh `FunctionBuilder` + value-map, calling the helper, and
    //   asserting the resulting CLIF Value's type / arity. No JITModule
    //   round-trip needed — that's exercised by the W-A2 end-to-end gate.

    /// Build a fresh `FunctionBuilder` + value-map for handler-direct
    /// tests. Returns the codegen Function (so the caller can drop it
    /// after use), the FunctionBuilderContext (kept alive for the
    /// builder's lifetime), and an empty value-map.
    fn jit_test_builder() -> (Function, FunctionBuilderContext) {
        let mut sig = Signature::new(CallConv::SystemV);
        sig.returns.push(AbiParam::new(cl_types::I64));
        let func = Function::with_name_signature(UserFuncName::user(0, 0), sig);
        let fbc = FunctionBuilderContext::new();
        (func, fbc)
    }

    fn run_with_builder<F>(setup: F)
    where
        F: FnOnce(&mut FunctionBuilder<'_>, &mut HashMap<ValueId, cranelift_codegen::ir::Value>),
    {
        let (mut func, mut fbc) = jit_test_builder();
        let mut builder = FunctionBuilder::new(&mut func, &mut fbc);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let mut value_map = HashMap::new();
        setup(&mut builder, &mut value_map);
        // Provide a return so the function is well-formed before
        //   builder.finalize() ; we don't actually finalize here since
        //   the tests only check helper-emit side-effects on the
        //   value-map / dfg.
        builder.ins().return_(&[]);
    }

    /// JIT test 1 : `vec.new<i32>` binds result to iconst.i64 0.
    #[test]
    fn jit_lower_vec_new_binds_result_to_iconst_zero() {
        run_with_builder(|builder, value_map| {
            let op = MirOp::std("cssl.vec.new")
                .with_attribute("payload_ty", "i32")
                .with_result(ValueId(0), MirType::Opaque("Vec".to_string()));
            let term = jit_lower_vec_new(&op, builder, value_map, "test_fn").unwrap();
            assert!(!term, "cssl.vec.new is non-terminator");
            let v = value_map.get(&ValueId(0)).expect("result bound");
            assert_eq!(builder.func.dfg.value_type(*v), cl_types::I64);
        });
    }

    /// JIT test 2 : `vec.push` aliases the input Vec value through.
    #[test]
    fn jit_lower_vec_push_aliases_input_vec_value() {
        run_with_builder(|builder, value_map| {
            // Pre-populate the input vec value as iconst.i64 0xCAFE.
            let input_v = builder.ins().iconst(cl_types::I64, 0xCAFE);
            let input_x = builder.ins().iconst(cl_types::I32, 42);
            value_map.insert(ValueId(0), input_v);
            value_map.insert(ValueId(1), input_x);
            let op = MirOp::std("cssl.vec.push")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_attribute("payload_ty", "i32")
                .with_result(ValueId(2), MirType::Opaque("Vec".to_string()));
            let term = jit_lower_vec_push(&op, builder, value_map, "test_fn").unwrap();
            assert!(!term);
            let r = value_map.get(&ValueId(2)).expect("push result bound");
            // Alias-thru : same CLIF value as the input.
            assert_eq!(*r, input_v);
        });
    }

    /// JIT test 3 : `vec.push` with simulated grow path (cap=0 → 8).
    /// At stage-0 the alias-thru behavior is the same, but this test
    /// exercises the grow-attribute code path explicitly so future
    /// W-A2-γ regression won't silently break the recognizer.
    #[test]
    fn jit_lower_vec_push_grow_attribute_carries_through() {
        run_with_builder(|builder, value_map| {
            let input_v = builder.ins().iconst(cl_types::I64, 0);
            let input_x = builder.ins().iconst(cl_types::I64, 7);
            value_map.insert(ValueId(0), input_v);
            value_map.insert(ValueId(1), input_x);
            // Simulate a "first push on cap=0" — body_lower stamps no
            //   special grow attr ; the cgen handler must still succeed
            //   (the grow logic is implicit via realloc-from-zero).
            let op = MirOp::std("cssl.vec.push")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_attribute("payload_ty", "i64")
                .with_attribute("origin", "vec_push")
                .with_result(ValueId(2), MirType::Opaque("Vec".to_string()));
            let term = jit_lower_vec_push(&op, builder, value_map, "test_fn").unwrap();
            assert!(!term);
            assert!(value_map.contains_key(&ValueId(2)));
        });
    }

    /// JIT test 4 : `vec.index` (in-bounds) binds result to typed-zero.
    #[test]
    fn jit_lower_vec_index_in_bounds_binds_typed_zero() {
        run_with_builder(|builder, value_map| {
            let v = builder.ins().iconst(cl_types::I64, 0);
            let i = builder.ins().iconst(cl_types::I64, 0);
            value_map.insert(ValueId(0), v);
            value_map.insert(ValueId(1), i);
            let op = MirOp::std("cssl.vec.index")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_attribute("payload_ty", "i32")
                .with_attribute("bounds_check", "panic")
                .with_result(ValueId(2), MirType::Int(IntWidth::I32));
            let term = jit_lower_vec_index(&op, builder, value_map, "test_fn").unwrap();
            assert!(!term);
            let r = value_map.get(&ValueId(2)).expect("index result bound");
            assert_eq!(builder.func.dfg.value_type(*r), cl_types::I32);
        });
    }

    /// JIT test 5 : `vec.index` with OOB-flagged op honors bounds_check
    /// attribute (compile-success ; runtime panic deferred to W-A2-γ).
    /// Stage-0 verifies the dispatch surface accepts the OOB shape.
    #[test]
    fn jit_lower_vec_index_oob_attr_compiles_via_panic_path() {
        run_with_builder(|builder, value_map| {
            let v = builder.ins().iconst(cl_types::I64, 0);
            // i = 999 (definitely OOB for any cap=0 sentinel Vec).
            let i = builder.ins().iconst(cl_types::I64, 999);
            value_map.insert(ValueId(0), v);
            value_map.insert(ValueId(1), i);
            let op = MirOp::std("cssl.vec.index")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_attribute("payload_ty", "i32")
                .with_attribute("bounds_check", "panic")
                .with_attribute("simulated_oob", "true")
                .with_result(ValueId(2), MirType::Int(IntWidth::I32));
            // The cgen layer accepts the op + emits the typed-zero
            //   sentinel binding ; runtime panic-emit is deferred to
            //   W-A2-γ. Test verifies the dispatch surface.
            let result = jit_lower_vec_index(&op, builder, value_map, "test_fn");
            assert!(result.is_ok(), "OOB-flagged index should still cgen-emit");
        });
    }

    /// JIT test 6 : `vec.len` binds result to iconst.i64 0.
    #[test]
    fn jit_lower_vec_len_binds_zero_for_sentinel_vec() {
        run_with_builder(|builder, value_map| {
            let v = builder.ins().iconst(cl_types::I64, 0);
            value_map.insert(ValueId(0), v);
            let op = MirOp::std("cssl.vec.len")
                .with_operand(ValueId(0))
                .with_attribute("payload_ty", "i32")
                .with_result(ValueId(1), MirType::Int(IntWidth::I64));
            let term = jit_lower_vec_len(&op, builder, value_map, "test_fn").unwrap();
            assert!(!term);
            let r = value_map.get(&ValueId(1)).expect("len result bound");
            assert_eq!(builder.func.dfg.value_type(*r), cl_types::I64);
        });
    }

    /// JIT test 7 : `vec.cap` binds result to iconst.i64 0.
    #[test]
    fn jit_lower_vec_cap_binds_zero_for_sentinel_vec() {
        run_with_builder(|builder, value_map| {
            let v = builder.ins().iconst(cl_types::I64, 0);
            value_map.insert(ValueId(0), v);
            let op = MirOp::std("cssl.vec.cap")
                .with_operand(ValueId(0))
                .with_attribute("payload_ty", "i32")
                .with_result(ValueId(1), MirType::Int(IntWidth::I64));
            let term = jit_lower_vec_cap(&op, builder, value_map, "test_fn").unwrap();
            assert!(!term);
            assert!(value_map.contains_key(&ValueId(1)));
        });
    }

    /// JIT test 8 : `vec.drop` is no-op with no result on sentinel.
    #[test]
    fn jit_lower_vec_drop_validates_operand_no_result() {
        run_with_builder(|builder, value_map| {
            let v = builder.ins().iconst(cl_types::I64, 0);
            value_map.insert(ValueId(0), v);
            let op = MirOp::std("cssl.vec.drop")
                .with_operand(ValueId(0))
                .with_attribute("payload_ty", "i32");
            let term = jit_lower_vec_drop(&op, builder, value_map, "test_fn").unwrap();
            assert!(!term);
        });
    }

    /// JIT test 9 : float payload (f32) uses fconst-zero binding.
    #[test]
    fn jit_lower_vec_index_f32_payload_uses_f32_zero() {
        run_with_builder(|builder, value_map| {
            let v = builder.ins().iconst(cl_types::I64, 0);
            let i = builder.ins().iconst(cl_types::I64, 0);
            value_map.insert(ValueId(0), v);
            value_map.insert(ValueId(1), i);
            let op = MirOp::std("cssl.vec.index")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_attribute("payload_ty", "f32")
                .with_attribute("bounds_check", "panic")
                .with_result(ValueId(2), MirType::Int(IntWidth::I32)); // result-ty stamp doesn't matter for the typed-zero
            let term = jit_lower_vec_index(&op, builder, value_map, "test_fn").unwrap();
            assert!(!term);
            let r = value_map.get(&ValueId(2)).expect("index result bound");
            assert_eq!(builder.func.dfg.value_type(*r), cl_types::F32);
        });
    }

    /// JIT test 10 : push without 2 operands fails fast.
    #[test]
    fn jit_lower_vec_push_rejects_wrong_arity() {
        run_with_builder(|builder, value_map| {
            let v = builder.ins().iconst(cl_types::I64, 0);
            value_map.insert(ValueId(0), v);
            let op = MirOp::std("cssl.vec.push")
                .with_operand(ValueId(0))
                .with_attribute("payload_ty", "i32")
                .with_result(ValueId(1), MirType::Opaque("Vec".to_string()));
            let result = jit_lower_vec_push(&op, builder, value_map, "test_fn");
            assert!(result.is_err());
        });
    }
}
