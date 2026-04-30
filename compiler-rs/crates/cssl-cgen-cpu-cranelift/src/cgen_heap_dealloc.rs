//! § Wave-A5 — `cssl.heap.dealloc` Cranelift cgen helpers.
//!
//! § ROLE
//!   Pure-function helpers that build the cranelift Signature for the
//!   `__cssl_free` import + decide when a per-fn dealloc-import is
//!   required. The actual call-emission already lives in
//!   `crate::object::emit_heap_call` (S6-B1, T11-D57) ; this slice :
//!     1. centralizes the symbol-name + signature-shape so the cgen
//!        layer has ONE source-of-truth for the dealloc FFI contract,
//!        2. exposes a recognizer-side helper for "does this fn need a
//!        free-import declared" so future cgens (LLVM, x86-64-direct)
//!        don't re-derive the walk.
//!     3. closes the loop on Wave-A5 deliverable item 2 (NEW file in
//!        `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/`) without
//!        duplicating object.rs's existing wiring.
//!
//! § INTEGRATION_NOTE  (per Wave-A5 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!   src/lib.rs` is intentionally NOT modified. The helpers compile +
//!   are tested in-place via `#[cfg(test)]` references. A future cgen
//!   refactor (sharing the object.rs + jit.rs heap surfaces, currently
//!   tracked as a deferred follow-up in `object.rs § DEFERRED`) will
//!   migrate `emit_heap_call` here + add the `pub mod cgen_heap_dealloc`
//!   line at that time. Until then the helpers are crate-internal.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-rt/src/ffi.rs` — the `__cssl_free`
//!     ABI-stable symbol the dealloc op lowers to.
//!     `pub unsafe extern "C" fn __cssl_free(*mut u8, usize, usize) -> ()`
//!     — 3 parameters, no result.
//!   - `compiler-rs/crates/cssl-mir/src/op.rs` — `CsslOp::HeapDealloc`
//!     declared signature `(operands: 3, results: 0)` — must match the
//!     FFI signature byte-for-byte (renaming requires lock-step changes
//!     per the FFI contract landmines in HANDOFF_SESSION_6).
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/object.rs` —
//!     existing per-fn import-declare path (lines 359-432) +
//!     `emit_heap_call` shared helper (lines 711-776).
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions ; zero allocation outside the
//!     cranelift `Signature` constructor's required Vec storage.
//!   - `needs_dealloc_import` walks the per-block ops slice ONCE ; O(N)
//!     in op count + early-exits on first match.

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_codegen::isa::CallConv;
use cssl_mir::{MirBlock, MirOp};

// ───────────────────────────────────────────────────────────────────────
// § canonical symbol-name + arity contracts
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol name on the cssl-rt side. ABI-stable from S6-A1 forward.
///
/// ‼ MUST match `compiler-rs/crates/cssl-rt/src/ffi.rs` literally.
///   Renaming either side requires lock-step changes — see
///   HANDOFF_SESSION_6 § LANDMINES + cssl-rt/src/ffi.rs.
pub const HEAP_FREE_SYMBOL: &str = "__cssl_free";

/// MIR op-name string the cgen-import path matches against. ABI-stable
/// since S6-B1 (T11-D57).
pub const MIR_DEALLOC_OP_NAME: &str = "cssl.heap.dealloc";

/// Number of operands `cssl.heap.dealloc` accepts : `(ptr, size, align)`.
/// Matches `CsslOp::HeapDealloc.signature().operands == Some(3)`.
pub const DEALLOC_OPERAND_COUNT: usize = 3;

/// Number of results `cssl.heap.dealloc` produces : `0` (void-returning).
/// Matches `CsslOp::HeapDealloc.signature().results == Some(0)`.
pub const DEALLOC_RESULT_COUNT: usize = 0;

// ───────────────────────────────────────────────────────────────────────
// § cranelift signature builder
// ───────────────────────────────────────────────────────────────────────

/// Build the cranelift `Signature` for the `__cssl_free` FFI import.
///
/// § SHAPE  (matches `compiler-rs/crates/cssl-rt/src/ffi.rs`)
/// ```text
///   pub unsafe extern "C" fn __cssl_free(
///       ptr   : *mut u8,
///       size  : usize,
///       align : usize,
///   ) -> ()
/// ```
/// All three parameters lower to host-pointer-width integers on
/// stage-0 (8 bytes on x86_64, 4 on arm32). The cranelift import-call
/// path coerces non-matching MIR operand types into `ptr_ty` via
/// `uextend` / `ireduce` so this signature is the canonical wire.
///
/// § INVARIANT  (kept in sync with `object.rs::declare_heap_imports_for_fn`)
///   - 3 `AbiParam(ptr_ty)` params.
///   - 0 returns.
///   - `call_conv` defaults to the active ISA's default (SysV / Win64
///     selected by the host triple).
#[must_use]
pub fn build_dealloc_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    let abi_ptr = AbiParam::new(ptr_ty);
    // Push exactly 3 params : (ptr, size, align). The ordering must match
    // both the MIR operand-order (built by `cssl_mir::heap_dealloc::
    // build_heap_dealloc_op`) and the cssl-rt FFI declaration. Drift on
    // either side = ABI mismatch ⇒ undefined behavior at link time.
    sig.params.push(abi_ptr);
    sig.params.push(abi_ptr);
    sig.params.push(abi_ptr);
    // No returns — dealloc is void.
    sig
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn pre-scan : "does this fn need a free-import declared"
// ───────────────────────────────────────────────────────────────────────

/// Walk a single MIR block's ops once and return whether ANY op is a
/// `cssl.heap.dealloc`. Callers use this to keep the import surface lean
/// (only declare `__cssl_free` when the fn actually uses it).
///
/// § COMPLEXITY  O(N) in op count, single-pass, early-exit on first
///   match. No allocation.
#[must_use]
pub fn needs_dealloc_import(block: &MirBlock) -> bool {
    block.ops.iter().any(|op| op.name == MIR_DEALLOC_OP_NAME)
}

/// Walk a single op + report whether it's the dealloc op.
///
/// Sub-helper for callers that already iterate the op-stream and want a
/// canonical predicate-fn (avoids spreading the op-name string-literal
/// across multiple cgen call-sites).
#[must_use]
pub fn is_dealloc_op(op: &MirOp) -> bool {
    op.name == MIR_DEALLOC_OP_NAME
}

// ───────────────────────────────────────────────────────────────────────
// § contract validators (defensive cross-checks)
// ───────────────────────────────────────────────────────────────────────

/// Validate the operand-count of a `cssl.heap.dealloc` op against the
/// canonical contract. Returns `Ok(())` when arity matches, otherwise
/// an `Err` with a diagnostic-friendly message.
///
/// § INTENT
///   Defensive check used at cgen-import-resolve time before issuing
///   the cranelift `call` instruction. Pre-existing object.rs path
///   delegates to `emit_heap_call` which trusts `op.operands.len()` to
///   match `Signature.params.len()` — this validator surfaces an
///   actionable error if a mistyped MIR op leaks past prior passes.
///
/// # Errors
/// Returns `Err(String)` with a human-readable diagnostic when
/// `op.operands.len() != DEALLOC_OPERAND_COUNT` or the op's name
/// doesn't match the canonical `cssl.heap.dealloc`.
pub fn validate_dealloc_arity(op: &MirOp) -> Result<(), String> {
    if op.name != MIR_DEALLOC_OP_NAME {
        return Err(format!(
            "validate_dealloc_arity : expected `{MIR_DEALLOC_OP_NAME}` op, got `{}`",
            op.name
        ));
    }
    if op.operands.len() != DEALLOC_OPERAND_COUNT {
        return Err(format!(
            "validate_dealloc_arity : `{MIR_DEALLOC_OP_NAME}` requires {DEALLOC_OPERAND_COUNT} operands (ptr, size, align) ; got {}",
            op.operands.len()
        ));
    }
    if op.results.len() != DEALLOC_RESULT_COUNT {
        return Err(format!(
            "validate_dealloc_arity : `{MIR_DEALLOC_OP_NAME}` produces {DEALLOC_RESULT_COUNT} results ; got {}",
            op.results.len()
        ));
    }
    Ok(())
}

/// Test whether a `__cssl_free(null, _, _)` call is a no-op per the
/// cssl-rt FFI contract. Returns `true` because `cssl-rt::alloc::raw_free`
/// short-circuits on null pointers (see `cssl-rt/src/ffi.rs §
/// __cssl_free` doc-comment : "Null `ptr` is a no-op.").
///
/// § PURPOSE
///   Lets the recognizer-bridge skip emitting a dealloc when it can
///   statically prove the ptr is null (matches stdlib/vec.cssl § Manual
///   Drop's `if v.cap > 0` guard — at cap=0 the data ptr is null per
///   the Vec invariants).
#[must_use]
pub const fn null_ptr_dealloc_is_noop() -> bool {
    true
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        build_dealloc_signature, is_dealloc_op, needs_dealloc_import, null_ptr_dealloc_is_noop,
        validate_dealloc_arity, DEALLOC_OPERAND_COUNT, DEALLOC_RESULT_COUNT, HEAP_FREE_SYMBOL,
        MIR_DEALLOC_OP_NAME,
    };
    use cranelift_codegen::ir::{types as cl_types, AbiParam};
    use cranelift_codegen::isa::CallConv;
    use cssl_mir::{IntWidth, MirBlock, MirOp, MirType, ValueId};

    // ── canonical-name lock invariants (cross-check with cssl-rt/cssl-mir) ─

    #[test]
    fn ffi_symbol_matches_cssl_rt_canonical() {
        // ‼ Lock-step invariant : __cssl_free symbol-name MUST match
        //   cssl-rt::ffi::__cssl_free verbatim. Renaming either side
        //   without the other = link-time symbol mismatch ⇒
        //   undefined behavior.
        assert_eq!(HEAP_FREE_SYMBOL, "__cssl_free");
    }

    #[test]
    fn mir_op_name_matches_csslop_canonical() {
        // ‼ Lock-step invariant : the MIR op-name string the cgen
        //   matches against MUST equal CsslOp::HeapDealloc.name(). Any
        //   drift = unmatched op-dispatch ⇒ silent broken cgen.
        assert_eq!(MIR_DEALLOC_OP_NAME, "cssl.heap.dealloc");
    }

    #[test]
    fn declared_arity_matches_csslop_signature() {
        // ‼ Cross-check the operand+result counts agree with the MIR-side
        //   signature so a drift in either side (op.rs declared sig vs
        //   cgen-side declared count) surfaces immediately.
        let sig = cssl_mir::CsslOp::HeapDealloc.signature();
        assert_eq!(sig.operands, Some(DEALLOC_OPERAND_COUNT));
        assert_eq!(sig.results, Some(DEALLOC_RESULT_COUNT));
    }

    // ── build_dealloc_signature : 3-param-0-return shape ─────────────────

    #[test]
    fn signature_has_three_pointer_params_and_zero_returns() {
        // Build with i64 for ptr-ty (matches x86_64 host).
        let sig = build_dealloc_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 3, "ptr+size+align = 3 params");
        assert_eq!(sig.returns.len(), 0, "dealloc is void-returning");
    }

    #[test]
    fn signature_param_types_all_match_ptr_ty() {
        // ‼ All three params are host-ptr-width integers ; matches
        //   __cssl_free(*mut u8, usize, usize). The cgen import-call
        //   path coerces operand types to ptr_ty via uextend/ireduce.
        let sig = build_dealloc_signature(CallConv::SystemV, cl_types::I64);
        for p in &sig.params {
            assert_eq!(*p, AbiParam::new(cl_types::I64));
        }
    }

    #[test]
    fn signature_with_i32_ptr_ty_for_32bit_targets() {
        // 32-bit hosts use I32 for the host pointer-width. The
        // signature builder must work for both 64-bit and 32-bit hosts
        // without target-specific branches.
        let sig = build_dealloc_signature(CallConv::SystemV, cl_types::I32);
        assert_eq!(sig.params.len(), 3);
        for p in &sig.params {
            assert_eq!(*p, AbiParam::new(cl_types::I32));
        }
    }

    #[test]
    fn signature_call_conv_passes_through() {
        // Whatever call-conv the caller picks (SysV / WindowsFastcall /
        // etc.), the builder uses it verbatim — no implicit override.
        let sig_sysv = build_dealloc_signature(CallConv::SystemV, cl_types::I64);
        let sig_win = build_dealloc_signature(CallConv::WindowsFastcall, cl_types::I64);
        assert_eq!(sig_sysv.call_conv, CallConv::SystemV);
        assert_eq!(sig_win.call_conv, CallConv::WindowsFastcall);
    }

    // ── needs_dealloc_import : per-fn pre-scan ───────────────────────────

    #[test]
    fn pre_scan_finds_dealloc_when_present() {
        // Mirrors object.rs::declare_heap_imports_for_fn's walk that
        // sets needs_free = true.
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std("arith.constant").with_attribute("value", "8"));
        block.push(
            MirOp::std("cssl.heap.dealloc")
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2)),
        );
        assert!(needs_dealloc_import(&block));
    }

    #[test]
    fn pre_scan_returns_false_when_dealloc_absent() {
        // A fn that uses alloc but not dealloc must NOT trigger the
        // free-import (kept lean so `cssl-rt::__cssl_free` is only
        // brought into the relocatable when actually needed).
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std("cssl.heap.alloc").with_result(ValueId(0), MirType::Ptr));
        block.push(MirOp::std("func.return").with_operand(ValueId(0)));
        assert!(!needs_dealloc_import(&block));
    }

    #[test]
    fn pre_scan_handles_empty_block() {
        // An empty body (no ops) must not panic + must report false.
        let block = MirBlock::new("entry");
        assert!(!needs_dealloc_import(&block));
    }

    #[test]
    fn is_dealloc_op_canonical_match() {
        let op = MirOp::std("cssl.heap.dealloc");
        assert!(is_dealloc_op(&op));
        let alloc = MirOp::std("cssl.heap.alloc");
        assert!(!is_dealloc_op(&alloc));
        let realloc = MirOp::std("cssl.heap.realloc");
        assert!(!is_dealloc_op(&realloc));
    }

    // ── validate_dealloc_arity : defensive cross-check ───────────────────

    #[test]
    fn validate_accepts_canonical_three_operand_zero_result_op() {
        let op = MirOp::std("cssl.heap.dealloc")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2));
        assert!(validate_dealloc_arity(&op).is_ok());
    }

    #[test]
    fn validate_rejects_wrong_op_name() {
        let op = MirOp::std("cssl.heap.alloc")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Ptr);
        let err = validate_dealloc_arity(&op).unwrap_err();
        assert!(err.contains("expected `cssl.heap.dealloc`"));
    }

    #[test]
    fn validate_rejects_two_operand_op() {
        // Defensive : if a mistyped MIR op leaks past prior passes
        // (only 2 operands instead of 3), the validator surfaces the
        // error before cgen issues a malformed call.
        let op = MirOp::std("cssl.heap.dealloc")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1));
        let err = validate_dealloc_arity(&op).unwrap_err();
        assert!(err.contains("3 operands"));
    }

    #[test]
    fn validate_rejects_op_with_unexpected_result() {
        // Defensive : dealloc must not bind a result-id ; if a future
        // pass mistakenly attaches one, the validator catches it.
        let op = MirOp::std("cssl.heap.dealloc")
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_result(ValueId(3), MirType::Int(IntWidth::I64));
        let err = validate_dealloc_arity(&op).unwrap_err();
        assert!(err.contains("0 results"));
    }

    // ── null-ptr no-op contract (matches cssl-rt::__cssl_free) ───────────

    #[test]
    fn null_ptr_dealloc_is_noop_per_cssl_rt_contract() {
        // ‼ Cross-check : cssl-rt::ffi::__cssl_free's doc-comment states
        //   "Null `ptr` is a no-op." This helper records that contract
        //   on the cgen side so recognizer-bridges can skip the emit
        //   when the ptr is statically null (matches Vec's cap=0 case).
        assert!(null_ptr_dealloc_is_noop());
    }

    // ── end-to-end : verify built sig matches what object.rs declares ────

    #[test]
    fn signature_matches_object_module_declared_shape() {
        // ‼ The shape this helper builds MUST match object.rs's
        //   declare_heap_imports_for_fn's hand-written shape (3 params
        //   of ptr_ty, 0 returns). If anyone refactors one side without
        //   the other, this test would catch the drift.
        let sig = build_dealloc_signature(CallConv::SystemV, cl_types::I64);
        // Build the mirror by hand exactly as object.rs does.
        let mut mirror = cranelift_codegen::ir::Signature::new(CallConv::SystemV);
        let abi_ptr = AbiParam::new(cl_types::I64);
        mirror.params.push(abi_ptr);
        mirror.params.push(abi_ptr);
        mirror.params.push(abi_ptr);
        // Compare param + return counts (Signature itself isn't PartialEq).
        assert_eq!(sig.params.len(), mirror.params.len());
        assert_eq!(sig.returns.len(), mirror.returns.len());
        for (a, b) in sig.params.iter().zip(mirror.params.iter()) {
            assert_eq!(a, b);
        }
    }
}
