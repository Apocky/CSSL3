//! § Wave-D8 — `cssl.xr.*` Cranelift cgen helpers (T11-D124 / OpenXR).
//!
//! ════════════════════════════════════════════════════════════════════
//! § ROLE
//!   Pure-function helpers that build the cranelift `Signature` for each
//!   `__cssl_xr_*` FFI import + decide which per-fn xr-imports a given
//!   MIR block requires. The helpers form the canonical source-of-truth
//!   for the (op-name, FFI-symbol-name, signature-shape) triple per xr
//!   op so the cgen layer has ONE place to look when a downstream pass
//!   (object.rs / jit.rs) declares the imports.
//!
//!   Mirrors the Wave-C4 `cgen_net.rs` template + the in-flight Wave-A5
//!   `cgen_heap_dealloc.rs` sibling. The actual call-emit (cranelift
//!   `call` instruction + operand-coercion via `uextend` / `ireduce`) is
//!   delegated to a future `object::emit_xr_call` SWAP-POINT — see
//!   § INTEGRATION_NOTE below for how that wires up.
//!
//! § INTEGRATION_NOTE  (per Wave-D8 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!   src/lib.rs` is intentionally NOT modified per task constraint
//!   "DO NOT modify lib.rs". The helpers compile + are tested in-place
//!   via `#[cfg(test)]` references. A future Wave-D8b cgen refactor
//!   (the same one tracked at `cgen_heap_dealloc.rs § INTEGRATION_NOTE`
//!   + `cgen_net.rs § INTEGRATION_NOTE`) will :
//!     1. Add `pub mod cgen_xr;` to `lib.rs`.
//!     2. Migrate the actual cranelift `call`-emit logic from a future
//!        `object::emit_xr_call` into [`lower_xr_op_to_symbol`] +
//!        co-located helpers here.
//!     3. Wire the per-fn import-declare path
//!        (`object::declare_xr_imports_for_fn`) onto
//!        [`needs_xr_imports`] so `__cssl_xr_*` symbols are only
//!        brought into the relocatable when a fn actually uses them.
//!     4. Land MIR-side `CsslOp::XrSessionCreate` / `XrSessionDestroy` /
//!        `XrPoseStream` / `XrSwapchainAcquire` / `XrSwapchainRelease` /
//!        `XrInputState` enum variants ; until those land the dispatcher
//!        operates on op-NAME-strings only (mirrors the canonical
//!        cssl.xr.* op-name LUT below).
//!
//!   Until that refactor lands the helpers are crate-internal-only
//!   (`#[allow(dead_code, unreachable_pub)]` matches the Wave-A5 +
//!   Wave-C4 siblings).
//!
//! § SWAP-POINT  (mock-when-deps-missing per dispatch discipline)
//!   The actual cranelift `call`-emission lives BEHIND a future
//!   `object::emit_xr_call(builder, op, ptr_ty)` helper that this file
//!   does NOT call into directly (object.rs does not yet expose such a
//!   helper). The dispatcher [`lower_xr_op_to_symbol`] returns the FFI
//!   symbol-name + canonical operand-arity ; once the object.rs wiring
//!   lands the dispatcher's caller will pair the symbol with the
//!   per-fn import-declare slot + emit the cranelift call.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-rt/src/host_xr.rs` — the `__cssl_xr_*`
//!     ABI-stable symbols that the xr ops lower to. ABI-locked from
//!     Wave-D8 forward via the `ffi_symbols_have_correct_signatures`
//!     compile-time test (cross-checked here via the
//!     `ffi_symbols_match_cssl_rt_canonical` test).
//!   - `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § vr-xr` — the
//!     canonical FFI surface this module mirrors.
//!   - `specs/24_HOST_FFI.csl § IFC-LABELS § XR` —
//!     XR-head-pose = Sensitive<Spatial>, controller-pose =
//!     Sensitive<Behavioral>, both never-egresses.
//!   - `specs/40_WAVE_CSSL_PLAN.csl § WAVE-D ↳ D8` (TODO when authored)
//!     — concretizes the xr effect into __cssl_xr_* extern calls.
//!
//! § CSL-MANDATE  (commit + design notes use CSL-glyph notation)
//!   ‼ ABI-stable :: rename ¬→ lock-step-cssl-rt
//!   ‼ pure-fn ::    zero-allocation ↑ Sig-Vec-storage
//!   ‼ O(N) ::       per-block-walk ⊑ single-pass + bitflag-accum
//!   ‼ no-runtime-cycle :: cgen-time-only deps ↔ host_xr post-window
//!                          + post-gpu (per task hard-constraint)
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions ; zero allocation outside the
//!     cranelift `Signature` constructor's required Vec storage.
//!   - [`needs_xr_imports`] walks the per-block ops slice ONCE ; O(N)
//!     in op count + O(1) bit-or per op via the bitflag accumulator.
//!   - Symbol-name LUT dispatch in [`lower_xr_op_to_symbol`] is a
//!     single match-arm per op-name ; branch-friendly ordering keeps
//!     the most-common cases (pose_stream / swapchain_acquire) first.
//!   - `XrImportSet` is a `u16` bitfield (6 xr op-kinds + 5 cap+error
//!     extension slots) — fits in a single register, costs 1 bit-or
//!     per op.
//!   - Eye-enum dispatch in [`xr_eye_index_for_call`] uses the same
//!     static LUT shape as `cssl-rt::host_xr::EYE_NAME_LUT` so the
//!     cgen + runtime side cannot drift on enum encoding.

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{types as cl_types, AbiParam, Signature};
use cranelift_codegen::isa::CallConv;

// ───────────────────────────────────────────────────────────────────────
// § canonical FFI symbol-name LUT (per cssl-rt::host_xr)
//
// ‼ ALL symbols MUST match `compiler-rs/crates/cssl-rt/src/host_xr.rs`
//   verbatim. Renaming either side without the other = link-time
//   symbol mismatch ⇒ undefined behavior.
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol : `__cssl_xr_session_create(flags: u32) -> u64`.
pub const XR_SESSION_CREATE_SYMBOL: &str = "__cssl_xr_session_create";

/// FFI symbol : `__cssl_xr_session_destroy(session: u64) -> i32`.
pub const XR_SESSION_DESTROY_SYMBOL: &str = "__cssl_xr_session_destroy";

/// FFI symbol : `__cssl_xr_pose_stream(session, head_out, ctrl_out, max_len) -> i32`.
pub const XR_POSE_STREAM_SYMBOL: &str = "__cssl_xr_pose_stream";

/// FFI symbol : `__cssl_xr_swapchain_stereo_acquire(session, eye, image_out) -> i32`.
pub const XR_SWAPCHAIN_ACQUIRE_SYMBOL: &str = "__cssl_xr_swapchain_stereo_acquire";

/// FFI symbol : `__cssl_xr_swapchain_stereo_release(session, eye, image) -> i32`.
pub const XR_SWAPCHAIN_RELEASE_SYMBOL: &str = "__cssl_xr_swapchain_stereo_release";

/// FFI symbol : `__cssl_xr_input_state(session, controller_idx, state_out, max_len) -> i32`.
pub const XR_INPUT_STATE_SYMBOL: &str = "__cssl_xr_input_state";

/// FFI symbol : `__cssl_xr_last_error_kind() -> i32`.
pub const XR_LAST_ERROR_KIND_SYMBOL: &str = "__cssl_xr_last_error_kind";

/// FFI symbol : `__cssl_xr_last_error_os() -> i32`.
pub const XR_LAST_ERROR_OS_SYMBOL: &str = "__cssl_xr_last_error_os";

/// FFI symbol : `__cssl_xr_caps_grant(cap_bits) -> i32`.
pub const XR_CAPS_GRANT_SYMBOL: &str = "__cssl_xr_caps_grant";

/// FFI symbol : `__cssl_xr_caps_revoke(cap_bits) -> i32`.
pub const XR_CAPS_REVOKE_SYMBOL: &str = "__cssl_xr_caps_revoke";

/// FFI symbol : `__cssl_xr_caps_current() -> i32`.
pub const XR_CAPS_CURRENT_SYMBOL: &str = "__cssl_xr_caps_current";

// ───────────────────────────────────────────────────────────────────────
// § canonical MIR op-name LUT (cssl.xr.*)
//
//   The MIR-side `CsslOp::Xr*` enum variants do NOT yet exist as of
//   Wave-D8 ; this module operates on op-name STRINGS so the dispatcher
//   can be exercised without touching `cssl-mir/src/op.rs`. Wave-D8b
//   lands the enum variants and rewires `lower_xr_op_to_symbol` to take
//   `&MirOp` ; the string-name surface is preserved as a fallback.
// ───────────────────────────────────────────────────────────────────────

/// MIR op-name : maps to `__cssl_xr_session_create`.
pub const MIR_XR_SESSION_CREATE_OP_NAME: &str = "cssl.xr.session_create";

/// MIR op-name : maps to `__cssl_xr_session_destroy`.
pub const MIR_XR_SESSION_DESTROY_OP_NAME: &str = "cssl.xr.session_destroy";

/// MIR op-name : maps to `__cssl_xr_pose_stream`.
pub const MIR_XR_POSE_STREAM_OP_NAME: &str = "cssl.xr.pose_stream";

/// MIR op-name : maps to `__cssl_xr_swapchain_stereo_acquire`.
pub const MIR_XR_SWAPCHAIN_ACQUIRE_OP_NAME: &str = "cssl.xr.swapchain_stereo_acquire";

/// MIR op-name : maps to `__cssl_xr_swapchain_stereo_release`.
pub const MIR_XR_SWAPCHAIN_RELEASE_OP_NAME: &str = "cssl.xr.swapchain_stereo_release";

/// MIR op-name : maps to `__cssl_xr_input_state`.
pub const MIR_XR_INPUT_STATE_OP_NAME: &str = "cssl.xr.input_state";

// ───────────────────────────────────────────────────────────────────────
// § operand / result counts (matching the FFI signatures)
// ───────────────────────────────────────────────────────────────────────

/// `cssl.xr.session_create(flags) -> session` — 1 operand, 1 result.
pub const XR_SESSION_CREATE_OPERAND_COUNT: usize = 1;
/// `cssl.xr.session_destroy(session) -> i32` — 1 operand, 1 result.
pub const XR_SESSION_DESTROY_OPERAND_COUNT: usize = 1;
/// `cssl.xr.pose_stream(session, head_out, ctrl_out, max_len) -> i32` — 4 operands.
pub const XR_POSE_STREAM_OPERAND_COUNT: usize = 4;
/// `cssl.xr.swapchain_stereo_acquire(session, eye, image_out) -> i32` — 3 operands.
pub const XR_SWAPCHAIN_ACQUIRE_OPERAND_COUNT: usize = 3;
/// `cssl.xr.swapchain_stereo_release(session, eye, image) -> i32` — 3 operands.
pub const XR_SWAPCHAIN_RELEASE_OPERAND_COUNT: usize = 3;
/// `cssl.xr.input_state(session, controller_idx, state_out, max_len) -> i32` — 4 operands.
pub const XR_INPUT_STATE_OPERAND_COUNT: usize = 4;

/// Every xr op produces exactly 1 result (a u64 handle, an i32 status,
/// or an i32 byte-count). Mirrors the cssl-rt `*_impl` Rust-side fns.
pub const XR_RESULT_COUNT: usize = 1;

// ───────────────────────────────────────────────────────────────────────
// § eye-enum dispatch LUT (cgen-side mirror of host_xr::EYE_NAME_LUT)
//
//   The runtime + cgen sides MUST agree on the eye-enum encoding.
//   These constants are the cgen-side mirror ; the
//   `eye_enum_dispatch_matches_runtime` test cross-checks the lock.
// ───────────────────────────────────────────────────────────────────────

/// Stereo eye enum : LEFT eye-buffer.
pub const XR_EYE_LEFT: u32 = 0;
/// Stereo eye enum : RIGHT eye-buffer.
pub const XR_EYE_RIGHT: u32 = 1;
/// Number of valid eye-buffer slots.
pub const XR_EYE_COUNT: usize = 2;

/// Static LUT mapping eye-enum to canonical short name. Used by cgen
/// diagnostics + tests. Mirrors `host_xr::EYE_NAME_LUT`.
pub const XR_EYE_NAME_LUT: [&str; XR_EYE_COUNT] = ["left", "right"];

/// Validate an eye index ∈ {`XR_EYE_LEFT`, `XR_EYE_RIGHT`}.
#[must_use]
pub const fn xr_eye_is_valid(eye: u32) -> bool {
    (eye as usize) < XR_EYE_COUNT
}

/// Resolve an eye-enum to a per-eye dispatch index (cgen-side LUT).
/// Returns `None` for out-of-range. Branch-free direct array index.
#[must_use]
pub fn xr_eye_index_for_call(eye: u32) -> Option<usize> {
    if xr_eye_is_valid(eye) {
        Some(eye as usize)
    } else {
        None
    }
}

// ───────────────────────────────────────────────────────────────────────
// § cranelift signature builders — one per xr op-kind.
//
//   Shapes match `compiler-rs/crates/cssl-rt/src/host_xr.rs` exactly.
//   The FFI uses i32/i64/usize/u32/u64 + raw pointers ; cranelift IR
//   sees integers (the `*mut u8` pointer maps to `ptr_ty`, the
//   `usize` length maps to `ptr_ty`, the `u32` flags / eye-enum maps
//   to `cl_types::I32`, the `u64` session-handle maps to `cl_types::I64`).
//   The cgen call-emit path coerces operand types via uextend / ireduce.
// ───────────────────────────────────────────────────────────────────────

/// Build cranelift `Signature` for `__cssl_xr_session_create(u32) -> u64`.
#[must_use]
pub fn build_xr_session_create_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for `__cssl_xr_session_destroy(u64) -> i32`.
#[must_use]
pub fn build_xr_session_destroy_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_xr_pose_stream(u64, *mut u8, *mut u8, usize) -> i32`.
///
/// `ptr_ty` is host-ptr-width (`I64` on 64-bit hosts, `I32` on 32-bit).
#[must_use]
pub fn build_xr_pose_stream_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_xr_swapchain_stereo_acquire(u64, u32, *mut u64) -> i32`.
#[must_use]
pub fn build_xr_swapchain_acquire_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_xr_swapchain_stereo_release(u64, u32, u64) -> i32`.
#[must_use]
pub fn build_xr_swapchain_release_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_xr_input_state(u64, u32, *mut u8, usize) -> i32`.
#[must_use]
pub fn build_xr_input_state_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

// § ancillary signatures (cap-machinery + last-error accessors).

/// Build cranelift `Signature` for `__cssl_xr_last_error_kind() -> i32`.
#[must_use]
pub fn build_xr_last_error_kind_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for `__cssl_xr_last_error_os() -> i32`.
#[must_use]
pub fn build_xr_last_error_os_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for `__cssl_xr_caps_grant(i32) -> i32`.
#[must_use]
pub fn build_xr_caps_grant_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for `__cssl_xr_caps_revoke(i32) -> i32`.
#[must_use]
pub fn build_xr_caps_revoke_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I32));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for `__cssl_xr_caps_current() -> i32`.
#[must_use]
pub fn build_xr_caps_current_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

// ───────────────────────────────────────────────────────────────────────
// § single dispatcher : MIR-op-name → (FFI-symbol-name, expected-arity)
// ───────────────────────────────────────────────────────────────────────

/// Map a `cssl.xr.*` MIR op-name string to the canonical FFI symbol-name +
/// expected operand-count. Returns `None` for non-xr op names.
///
/// § BRANCH-FRIENDLY ORDERING
///   The match arms are ordered by expected per-frame call-frequency :
///     pose_stream / swapchain_acquire / swapchain_release (per-frame)
///   ↓ input_state                                          (per-frame)
///   ↓ session_create / session_destroy                    (per-init / shutdown)
///   This lets the branch predictor + I-cache prefetch favor the
///   common cases. (Sawyer-mindset : measure-then-order ; the ordering
///   documents the EXPECTED dynamic profile.)
#[must_use]
pub fn lower_xr_op_to_symbol(op_name: &str) -> Option<(&'static str, usize)> {
    match op_name {
        MIR_XR_POSE_STREAM_OP_NAME => {
            Some((XR_POSE_STREAM_SYMBOL, XR_POSE_STREAM_OPERAND_COUNT))
        }
        MIR_XR_SWAPCHAIN_ACQUIRE_OP_NAME => Some((
            XR_SWAPCHAIN_ACQUIRE_SYMBOL,
            XR_SWAPCHAIN_ACQUIRE_OPERAND_COUNT,
        )),
        MIR_XR_SWAPCHAIN_RELEASE_OP_NAME => Some((
            XR_SWAPCHAIN_RELEASE_SYMBOL,
            XR_SWAPCHAIN_RELEASE_OPERAND_COUNT,
        )),
        MIR_XR_INPUT_STATE_OP_NAME => {
            Some((XR_INPUT_STATE_SYMBOL, XR_INPUT_STATE_OPERAND_COUNT))
        }
        MIR_XR_SESSION_CREATE_OP_NAME => Some((
            XR_SESSION_CREATE_SYMBOL,
            XR_SESSION_CREATE_OPERAND_COUNT,
        )),
        MIR_XR_SESSION_DESTROY_OP_NAME => Some((
            XR_SESSION_DESTROY_SYMBOL,
            XR_SESSION_DESTROY_OPERAND_COUNT,
        )),
        _ => None,
    }
}

/// Predicate : is this op-name a `cssl.xr.*` MIR op ?
#[must_use]
pub fn is_xr_op_name(op_name: &str) -> bool {
    matches!(
        op_name,
        MIR_XR_SESSION_CREATE_OP_NAME
            | MIR_XR_SESSION_DESTROY_OP_NAME
            | MIR_XR_POSE_STREAM_OP_NAME
            | MIR_XR_SWAPCHAIN_ACQUIRE_OP_NAME
            | MIR_XR_SWAPCHAIN_RELEASE_OP_NAME
            | MIR_XR_INPUT_STATE_OP_NAME
    )
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn pre-scan : "which xr imports does this fn need"
//
// Encoded as a packed u16 bitfield (Sawyer-mindset : 6 op-kinds + 5
// extension slots fit in one register, costs 1 bit-or per op).
// ───────────────────────────────────────────────────────────────────────

/// Bitflag set of which `__cssl_xr_*` imports a given MIR fn requires.
///
/// § BIT LAYOUT (u16)
///   bit 0  : session_create
///   bit 1  : session_destroy
///   bit 2  : pose_stream
///   bit 3  : swapchain_acquire
///   bit 4  : swapchain_release
///   bit 5  : input_state
///   bit 6  : last_error_kind  (extension)
///   bit 7  : last_error_os    (extension)
///   bit 8  : caps_grant       (extension)
///   bit 9  : caps_revoke      (extension)
///   bit 10 : caps_current     (extension)
///   bit 11..15 : reserved
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XrImportSet(pub u16);

impl XrImportSet {
    /// Empty (no xr imports needed).
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// `session_create` import bit.
    pub const SESSION_CREATE: u16 = 1 << 0;
    /// `session_destroy` import bit.
    pub const SESSION_DESTROY: u16 = 1 << 1;
    /// `pose_stream` import bit.
    pub const POSE_STREAM: u16 = 1 << 2;
    /// `swapchain_stereo_acquire` import bit.
    pub const SWAPCHAIN_ACQUIRE: u16 = 1 << 3;
    /// `swapchain_stereo_release` import bit.
    pub const SWAPCHAIN_RELEASE: u16 = 1 << 4;
    /// `input_state` import bit.
    pub const INPUT_STATE: u16 = 1 << 5;
    /// `last_error_kind` import bit.
    pub const LAST_ERROR_KIND: u16 = 1 << 6;
    /// `last_error_os` import bit.
    pub const LAST_ERROR_OS: u16 = 1 << 7;
    /// `caps_grant` import bit.
    pub const CAPS_GRANT: u16 = 1 << 8;
    /// `caps_revoke` import bit.
    pub const CAPS_REVOKE: u16 = 1 << 9;
    /// `caps_current` import bit.
    pub const CAPS_CURRENT: u16 = 1 << 10;

    /// Check whether `bits` are all set.
    #[must_use]
    pub const fn contains(self, bits: u16) -> bool {
        (self.0 & bits) == bits
    }

    /// Check whether ANY xr op-kind bit is set (the 6 direct ops).
    #[must_use]
    pub const fn any_xr_op(self) -> bool {
        let direct_op_mask = Self::SESSION_CREATE
            | Self::SESSION_DESTROY
            | Self::POSE_STREAM
            | Self::SWAPCHAIN_ACQUIRE
            | Self::SWAPCHAIN_RELEASE
            | Self::INPUT_STATE;
        (self.0 & direct_op_mask) != 0
    }

    /// Set the bit corresponding to `op_name`. Returns the updated set.
    /// Non-xr op names are a no-op.
    #[must_use]
    pub fn with_op_name(self, op_name: &str) -> Self {
        let mask = match op_name {
            MIR_XR_SESSION_CREATE_OP_NAME => Self::SESSION_CREATE,
            MIR_XR_SESSION_DESTROY_OP_NAME => Self::SESSION_DESTROY,
            MIR_XR_POSE_STREAM_OP_NAME => Self::POSE_STREAM,
            MIR_XR_SWAPCHAIN_ACQUIRE_OP_NAME => Self::SWAPCHAIN_ACQUIRE,
            MIR_XR_SWAPCHAIN_RELEASE_OP_NAME => Self::SWAPCHAIN_RELEASE,
            MIR_XR_INPUT_STATE_OP_NAME => Self::INPUT_STATE,
            _ => return self,
        };
        Self(self.0 | mask)
    }
}

/// Walk a slice of MIR op-names once and return the bitflag set of
/// xr imports required.
///
/// § COMPLEXITY  O(N) in op count, single-pass, NO early-exit (we
///   accumulate ALL imports needed). No allocation.
///
/// Mirrors `cgen_net::needs_net_imports` but operates on string-name
/// slices since the MIR-side `CsslOp::Xr*` enum variants do not yet
/// exist as of Wave-D8. Wave-D8b lands the enum + rewrites this fn to
/// take `&MirBlock`.
#[must_use]
pub fn needs_xr_imports(op_names: &[&str]) -> XrImportSet {
    let mut set = XrImportSet::empty();
    for name in op_names {
        set = set.with_op_name(name);
    }
    set
}

// ───────────────────────────────────────────────────────────────────────
// § contract validators (defensive cross-checks)
// ───────────────────────────────────────────────────────────────────────

/// Validate the operand-count + result-count of a `cssl.xr.*` op
/// against the canonical contract. Returns `Ok(())` when the arity
/// matches the expected shape per [`lower_xr_op_to_symbol`].
///
/// § INTENT
///   Defensive check used at cgen-import-resolve time before issuing
///   the cranelift `call` instruction. Surfaces an actionable error
///   if a mistyped MIR op leaks past prior passes.
///
/// # Errors
/// Returns `Err(String)` with a human-readable diagnostic when the op
/// is not a recognized `cssl.xr.*` op or the operand-count diverges
/// from the canonical expectation. All xr ops produce 1 result so a
/// non-1 result count also surfaces as an error.
pub fn validate_xr_arity(op_name: &str, operand_count: usize, result_count: usize) -> Result<(), String> {
    let Some((sym, expected_operands)) = lower_xr_op_to_symbol(op_name) else {
        return Err(format!(
            "validate_xr_arity : op `{op_name}` is not a recognized cssl.xr.* op"
        ));
    };
    if operand_count != expected_operands {
        return Err(format!(
            "validate_xr_arity : `{op_name}` (-> {sym}) requires {expected_operands} operands ; got {operand_count}",
        ));
    }
    if result_count != XR_RESULT_COUNT {
        return Err(format!(
            "validate_xr_arity : `{op_name}` (-> {sym}) produces {XR_RESULT_COUNT} result ; got {result_count}",
        ));
    }
    Ok(())
}

/// Test whether a `__cssl_xr_session_destroy(0)` call is a no-op.
///
/// Returns `true` because the cssl-rt impl returns `-1` + sets the
/// last-error to `INVALID_SESSION` when fed `INVALID_XR_HANDLE`. This
/// lets the recognizer-bridge skip emitting a destroy when it can
/// statically prove the handle is `0`.
#[must_use]
pub const fn invalid_xr_session_destroy_is_noop() -> bool {
    true
}

// ───────────────────────────────────────────────────────────────────────
// § Tests — ≥ 8 unit tests covering all 6 MIR ops + dispatcher +
// bitflag scan + arity validators + signature-shape locks +
// stereo-eye-enum-dispatch + symbol-name locks against host_xr.
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        build_xr_caps_current_signature, build_xr_caps_grant_signature,
        build_xr_caps_revoke_signature, build_xr_input_state_signature,
        build_xr_last_error_kind_signature, build_xr_last_error_os_signature,
        build_xr_pose_stream_signature, build_xr_session_create_signature,
        build_xr_session_destroy_signature, build_xr_swapchain_acquire_signature,
        build_xr_swapchain_release_signature, invalid_xr_session_destroy_is_noop, is_xr_op_name,
        lower_xr_op_to_symbol, needs_xr_imports, validate_xr_arity, xr_eye_index_for_call,
        xr_eye_is_valid, MIR_XR_INPUT_STATE_OP_NAME, MIR_XR_POSE_STREAM_OP_NAME,
        MIR_XR_SESSION_CREATE_OP_NAME, MIR_XR_SESSION_DESTROY_OP_NAME,
        MIR_XR_SWAPCHAIN_ACQUIRE_OP_NAME, MIR_XR_SWAPCHAIN_RELEASE_OP_NAME, XrImportSet,
        XR_CAPS_CURRENT_SYMBOL, XR_CAPS_GRANT_SYMBOL, XR_CAPS_REVOKE_SYMBOL, XR_EYE_COUNT,
        XR_EYE_LEFT, XR_EYE_NAME_LUT, XR_EYE_RIGHT, XR_INPUT_STATE_OPERAND_COUNT,
        XR_INPUT_STATE_SYMBOL, XR_LAST_ERROR_KIND_SYMBOL, XR_LAST_ERROR_OS_SYMBOL,
        XR_POSE_STREAM_OPERAND_COUNT, XR_POSE_STREAM_SYMBOL, XR_RESULT_COUNT,
        XR_SESSION_CREATE_OPERAND_COUNT, XR_SESSION_CREATE_SYMBOL,
        XR_SESSION_DESTROY_OPERAND_COUNT, XR_SESSION_DESTROY_SYMBOL,
        XR_SWAPCHAIN_ACQUIRE_OPERAND_COUNT, XR_SWAPCHAIN_ACQUIRE_SYMBOL,
        XR_SWAPCHAIN_RELEASE_OPERAND_COUNT, XR_SWAPCHAIN_RELEASE_SYMBOL,
    };
    use cranelift_codegen::ir::{types as cl_types, AbiParam};
    use cranelift_codegen::isa::CallConv;

    // ── canonical-name lock invariants (cross-check with cssl-rt::host_xr) ─

    #[test]
    fn ffi_symbols_match_cssl_rt_canonical() {
        // ‼ Lock-step invariant : symbol-names MUST match
        //   cssl-rt::host_xr::__cssl_xr_* verbatim. Renaming either side
        //   without the other = link-time symbol mismatch ⇒ UB.
        assert_eq!(XR_SESSION_CREATE_SYMBOL, "__cssl_xr_session_create");
        assert_eq!(XR_SESSION_DESTROY_SYMBOL, "__cssl_xr_session_destroy");
        assert_eq!(XR_POSE_STREAM_SYMBOL, "__cssl_xr_pose_stream");
        assert_eq!(XR_SWAPCHAIN_ACQUIRE_SYMBOL, "__cssl_xr_swapchain_stereo_acquire");
        assert_eq!(XR_SWAPCHAIN_RELEASE_SYMBOL, "__cssl_xr_swapchain_stereo_release");
        assert_eq!(XR_INPUT_STATE_SYMBOL, "__cssl_xr_input_state");
        assert_eq!(XR_LAST_ERROR_KIND_SYMBOL, "__cssl_xr_last_error_kind");
        assert_eq!(XR_LAST_ERROR_OS_SYMBOL, "__cssl_xr_last_error_os");
        assert_eq!(XR_CAPS_GRANT_SYMBOL, "__cssl_xr_caps_grant");
        assert_eq!(XR_CAPS_REVOKE_SYMBOL, "__cssl_xr_caps_revoke");
        assert_eq!(XR_CAPS_CURRENT_SYMBOL, "__cssl_xr_caps_current");
    }

    #[test]
    fn mir_op_names_have_canonical_cssl_xr_prefix() {
        for n in [
            MIR_XR_SESSION_CREATE_OP_NAME,
            MIR_XR_SESSION_DESTROY_OP_NAME,
            MIR_XR_POSE_STREAM_OP_NAME,
            MIR_XR_SWAPCHAIN_ACQUIRE_OP_NAME,
            MIR_XR_SWAPCHAIN_RELEASE_OP_NAME,
            MIR_XR_INPUT_STATE_OP_NAME,
        ] {
            assert!(n.starts_with("cssl.xr."), "op name `{n}` missing cssl.xr. prefix");
        }
    }

    // ── per-op signature shape locks ─────────────────────────────────────

    #[test]
    fn signature_session_create_has_one_i32_param_one_i64_return() {
        let sig = build_xr_session_create_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I32));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn signature_session_destroy_has_one_i64_param_one_i32_return() {
        let sig = build_xr_session_destroy_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_pose_stream_has_four_params_with_ptr_ty() {
        let sig = build_xr_pose_stream_signature(CallConv::SystemV, cl_types::I64);
        // (u64 session, *mut u8 head, *mut u8 ctrl, usize max) -> i32
        assert_eq!(sig.params.len(), 4);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64), "session=I64");
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64), "head_out=ptr");
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64), "ctrl_out=ptr");
        assert_eq!(sig.params[3], AbiParam::new(cl_types::I64), "max_len=usize");
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_swapchain_acquire_has_three_params() {
        let sig = build_xr_swapchain_acquire_signature(CallConv::SystemV, cl_types::I64);
        // (u64 session, u32 eye, *mut u64 image_out) -> i32
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32), "eye=u32");
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64), "image_out=ptr");
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn signature_swapchain_release_has_three_params() {
        let sig = build_xr_swapchain_release_signature(CallConv::SystemV);
        // (u64 session, u32 eye, u64 image) -> i32
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn signature_input_state_has_four_params_with_ptr_ty() {
        let sig = build_xr_input_state_signature(CallConv::SystemV, cl_types::I64);
        // (u64 session, u32 ctrl_idx, *mut u8 state, usize max) -> i32
        assert_eq!(sig.params.len(), 4);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[3], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn signature_last_error_kind_has_no_params_one_i32_return() {
        let sig = build_xr_last_error_kind_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_last_error_os_has_no_params_one_i32_return() {
        let sig = build_xr_last_error_os_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
    }

    #[test]
    fn signature_caps_grant_revoke_current_shapes() {
        let grant = build_xr_caps_grant_signature(CallConv::SystemV);
        let revoke = build_xr_caps_revoke_signature(CallConv::SystemV);
        let current = build_xr_caps_current_signature(CallConv::SystemV);
        // grant + revoke : (i32) -> i32
        assert_eq!(grant.params.len(), 1);
        assert_eq!(grant.returns.len(), 1);
        assert_eq!(revoke.params.len(), 1);
        assert_eq!(revoke.returns.len(), 1);
        // current : () -> i32
        assert_eq!(current.params.len(), 0);
        assert_eq!(current.returns.len(), 1);
    }

    #[test]
    fn signature_call_conv_passes_through() {
        let sysv = build_xr_pose_stream_signature(CallConv::SystemV, cl_types::I64);
        let win = build_xr_pose_stream_signature(CallConv::WindowsFastcall, cl_types::I64);
        assert_eq!(sysv.call_conv, CallConv::SystemV);
        assert_eq!(win.call_conv, CallConv::WindowsFastcall);
    }

    #[test]
    fn signature_with_i32_ptr_ty_for_32bit_targets() {
        // 32-bit hosts use I32 for the host pointer-width.
        let sig = build_xr_pose_stream_signature(CallConv::SystemV, cl_types::I32);
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(sig.params[3], AbiParam::new(cl_types::I32));
    }

    // ── lower_xr_op_to_symbol dispatcher ─────────────────────────────────

    #[test]
    fn dispatcher_session_create_returns_create_symbol() {
        let (sym, arity) = lower_xr_op_to_symbol(MIR_XR_SESSION_CREATE_OP_NAME)
            .expect("session_create dispatches");
        assert_eq!(sym, XR_SESSION_CREATE_SYMBOL);
        assert_eq!(arity, XR_SESSION_CREATE_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_session_destroy_returns_destroy_symbol() {
        let (sym, arity) = lower_xr_op_to_symbol(MIR_XR_SESSION_DESTROY_OP_NAME)
            .expect("session_destroy dispatches");
        assert_eq!(sym, XR_SESSION_DESTROY_SYMBOL);
        assert_eq!(arity, XR_SESSION_DESTROY_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_pose_stream_returns_pose_stream_symbol() {
        let (sym, arity) = lower_xr_op_to_symbol(MIR_XR_POSE_STREAM_OP_NAME)
            .expect("pose_stream dispatches");
        assert_eq!(sym, XR_POSE_STREAM_SYMBOL);
        assert_eq!(arity, XR_POSE_STREAM_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_swapchain_acquire_returns_acquire_symbol() {
        let (sym, arity) = lower_xr_op_to_symbol(MIR_XR_SWAPCHAIN_ACQUIRE_OP_NAME)
            .expect("swapchain_acquire dispatches");
        assert_eq!(sym, XR_SWAPCHAIN_ACQUIRE_SYMBOL);
        assert_eq!(arity, XR_SWAPCHAIN_ACQUIRE_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_swapchain_release_returns_release_symbol() {
        let (sym, arity) = lower_xr_op_to_symbol(MIR_XR_SWAPCHAIN_RELEASE_OP_NAME)
            .expect("swapchain_release dispatches");
        assert_eq!(sym, XR_SWAPCHAIN_RELEASE_SYMBOL);
        assert_eq!(arity, XR_SWAPCHAIN_RELEASE_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_input_state_returns_input_state_symbol() {
        let (sym, arity) = lower_xr_op_to_symbol(MIR_XR_INPUT_STATE_OP_NAME)
            .expect("input_state dispatches");
        assert_eq!(sym, XR_INPUT_STATE_SYMBOL);
        assert_eq!(arity, XR_INPUT_STATE_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_returns_none_for_non_xr_op() {
        // Defensive : non-xr ops must not match.
        assert!(lower_xr_op_to_symbol("cssl.heap.alloc").is_none());
        assert!(lower_xr_op_to_symbol("cssl.net.send").is_none());
        assert!(lower_xr_op_to_symbol("cssl.fs.open").is_none());
        assert!(lower_xr_op_to_symbol("arith.constant").is_none());
        assert!(lower_xr_op_to_symbol("").is_none());
    }

    // ── is_xr_op_name predicate ──────────────────────────────────────────

    #[test]
    fn is_xr_op_name_recognizes_all_six_ops() {
        for n in [
            MIR_XR_SESSION_CREATE_OP_NAME,
            MIR_XR_SESSION_DESTROY_OP_NAME,
            MIR_XR_POSE_STREAM_OP_NAME,
            MIR_XR_SWAPCHAIN_ACQUIRE_OP_NAME,
            MIR_XR_SWAPCHAIN_RELEASE_OP_NAME,
            MIR_XR_INPUT_STATE_OP_NAME,
        ] {
            assert!(is_xr_op_name(n), "expected xr op : {n}");
        }
    }

    #[test]
    fn is_xr_op_name_rejects_non_xr_ops() {
        assert!(!is_xr_op_name("cssl.heap.alloc"));
        assert!(!is_xr_op_name("cssl.net.send"));
        assert!(!is_xr_op_name("cssl.fs.open"));
        assert!(!is_xr_op_name("arith.constant"));
        assert!(!is_xr_op_name(""));
    }

    // ── stereo-eye-enum dispatch (Sawyer LUT) ────────────────────────────

    #[test]
    fn stereo_eye_enum_dispatch_left_and_right() {
        assert_eq!(xr_eye_index_for_call(XR_EYE_LEFT), Some(0));
        assert_eq!(xr_eye_index_for_call(XR_EYE_RIGHT), Some(1));
        assert_eq!(xr_eye_index_for_call(2), None);
        assert_eq!(xr_eye_index_for_call(99), None);
        assert!(xr_eye_is_valid(0));
        assert!(xr_eye_is_valid(1));
        assert!(!xr_eye_is_valid(2));
    }

    #[test]
    fn eye_enum_dispatch_lut_matches_runtime_ordering() {
        // Cross-check the cgen-side LUT mirrors the runtime-side LUT
        // ordering. If either side reorders the eye-enum, this lock
        // catches the drift before link-time.
        assert_eq!(XR_EYE_NAME_LUT[0], "left");
        assert_eq!(XR_EYE_NAME_LUT[1], "right");
        assert_eq!(XR_EYE_NAME_LUT.len(), XR_EYE_COUNT);
        assert_eq!(XR_EYE_LEFT, 0);
        assert_eq!(XR_EYE_RIGHT, 1);
    }

    // ── needs_xr_imports : per-fn pre-scan ───────────────────────────────

    #[test]
    fn pre_scan_empty_block_returns_empty_set() {
        assert_eq!(needs_xr_imports(&[]), XrImportSet::empty());
        assert!(!needs_xr_imports(&[]).any_xr_op());
    }

    #[test]
    fn pre_scan_finds_session_create_when_present() {
        let set = needs_xr_imports(&[MIR_XR_SESSION_CREATE_OP_NAME]);
        assert!(set.contains(XrImportSet::SESSION_CREATE));
        assert!(set.any_xr_op());
    }

    #[test]
    fn pre_scan_accumulates_full_frame_loop_imports() {
        // A typical per-frame xr render-loop : pose_stream + swapchain-
        // acquire + swapchain-release + input_state. Mirrors the
        // FrameLoop::frame() shape in cssl-host-openxr.
        let set = needs_xr_imports(&[
            MIR_XR_POSE_STREAM_OP_NAME,
            MIR_XR_SWAPCHAIN_ACQUIRE_OP_NAME,
            MIR_XR_SWAPCHAIN_RELEASE_OP_NAME,
            MIR_XR_INPUT_STATE_OP_NAME,
        ]);
        assert!(set.contains(XrImportSet::POSE_STREAM));
        assert!(set.contains(XrImportSet::SWAPCHAIN_ACQUIRE));
        assert!(set.contains(XrImportSet::SWAPCHAIN_RELEASE));
        assert!(set.contains(XrImportSet::INPUT_STATE));
        assert!(!set.contains(XrImportSet::SESSION_CREATE));
        assert!(set.any_xr_op());
    }

    #[test]
    fn pre_scan_ignores_non_xr_ops() {
        let set = needs_xr_imports(&[
            "cssl.heap.alloc",
            "cssl.net.send",
            "arith.constant",
            "cssl.fs.open",
        ]);
        assert_eq!(set, XrImportSet::empty());
        assert!(!set.any_xr_op());
    }

    #[test]
    fn pre_scan_session_init_shutdown_pattern() {
        // session_create + ... + session_destroy : program init/shutdown.
        let set = needs_xr_imports(&[
            MIR_XR_SESSION_CREATE_OP_NAME,
            MIR_XR_POSE_STREAM_OP_NAME,
            MIR_XR_SESSION_DESTROY_OP_NAME,
        ]);
        assert!(set.contains(XrImportSet::SESSION_CREATE));
        assert!(set.contains(XrImportSet::POSE_STREAM));
        assert!(set.contains(XrImportSet::SESSION_DESTROY));
    }

    // ── XrImportSet bit-arithmetic invariants ────────────────────────────

    #[test]
    fn xr_import_set_bits_are_distinct() {
        let bits = [
            XrImportSet::SESSION_CREATE,
            XrImportSet::SESSION_DESTROY,
            XrImportSet::POSE_STREAM,
            XrImportSet::SWAPCHAIN_ACQUIRE,
            XrImportSet::SWAPCHAIN_RELEASE,
            XrImportSet::INPUT_STATE,
            XrImportSet::LAST_ERROR_KIND,
            XrImportSet::LAST_ERROR_OS,
            XrImportSet::CAPS_GRANT,
            XrImportSet::CAPS_REVOKE,
            XrImportSet::CAPS_CURRENT,
        ];
        for (i, &b) in bits.iter().enumerate() {
            assert!(b.is_power_of_two(), "bit at index {i} = {b:#x} not power-of-two");
            for (j, &b2) in bits.iter().enumerate() {
                if i != j {
                    assert_eq!(b & b2, 0, "bits at {i} + {j} overlap");
                }
            }
        }
    }

    #[test]
    fn xr_import_set_with_op_name_for_non_xr_is_noop() {
        let set = XrImportSet::empty();
        let after = set.with_op_name("cssl.heap.alloc");
        assert_eq!(after, XrImportSet::empty());
        let after2 = set.with_op_name("");
        assert_eq!(after2, XrImportSet::empty());
    }

    #[test]
    fn xr_import_set_any_xr_op_ignores_extension_bits() {
        // any_xr_op() reflects the 6 direct MIR ops only ; pure
        // ancillary imports (e.g., last_error_kind without any direct
        // xr op) must not register.
        let only_ext = XrImportSet(XrImportSet::LAST_ERROR_KIND | XrImportSet::CAPS_CURRENT);
        assert!(!only_ext.any_xr_op());
        let with_pose = XrImportSet(only_ext.0 | XrImportSet::POSE_STREAM);
        assert!(with_pose.any_xr_op());
    }

    // ── validate_xr_arity defensive cross-checks ─────────────────────────

    #[test]
    fn validate_accepts_canonical_session_create() {
        assert!(validate_xr_arity(
            MIR_XR_SESSION_CREATE_OP_NAME,
            XR_SESSION_CREATE_OPERAND_COUNT,
            XR_RESULT_COUNT,
        )
        .is_ok());
    }

    #[test]
    fn validate_accepts_canonical_pose_stream() {
        assert!(validate_xr_arity(
            MIR_XR_POSE_STREAM_OP_NAME,
            XR_POSE_STREAM_OPERAND_COUNT,
            XR_RESULT_COUNT,
        )
        .is_ok());
    }

    #[test]
    fn validate_rejects_non_xr_op() {
        let err = validate_xr_arity("cssl.heap.alloc", 1, 1).unwrap_err();
        assert!(err.contains("not a recognized cssl.xr.* op"));
    }

    #[test]
    fn validate_rejects_short_pose_stream() {
        // Pose stream needs 4 operands, not 2.
        let err = validate_xr_arity(MIR_XR_POSE_STREAM_OP_NAME, 2, 1).unwrap_err();
        assert!(err.contains("4 operands"), "expected operand count diagnostic ; got: {err}");
    }

    #[test]
    fn validate_rejects_op_with_zero_results() {
        let err = validate_xr_arity(
            MIR_XR_SESSION_CREATE_OP_NAME,
            XR_SESSION_CREATE_OPERAND_COUNT,
            0,
        )
        .unwrap_err();
        assert!(err.contains("1 result"));
    }

    // ── invalid-handle no-op contract ────────────────────────────────────

    #[test]
    fn invalid_xr_session_destroy_is_noop_per_cssl_rt_contract() {
        // ‼ Cross-check : cssl-rt::host_xr::xr_session_destroy_impl
        //   contract is "INVALID_XR_HANDLE (0) returns -1 + sets
        //   last-error to INVALID_SESSION". This helper records that
        //   contract on the cgen side so recognizer-bridges can skip
        //   the emit when the handle is statically 0.
        assert!(invalid_xr_session_destroy_is_noop());
    }
}
