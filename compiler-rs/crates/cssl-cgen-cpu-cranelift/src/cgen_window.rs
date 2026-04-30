//! § Wave-D3 — `__cssl_window_*` Cranelift cgen helpers.
//! ════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § window`.
//! Plan reference     : `specs/40_WAVE_CSSL_PLAN.csl § WAVE-D ↳ D3`.
//!
//! § ROLE
//!   Pure-function helpers that build the cranelift `Signature` for each
//!   `__cssl_window_*` FFI import + carry the canonical symbol-name +
//!   ABI-locked event-record layout constants. The helpers form the
//!   canonical source-of-truth for the (FFI-symbol-name, signature-shape)
//!   pair per window op so the cgen layer has ONE place to look when a
//!   downstream pass (object.rs / jit.rs) declares the imports.
//!
//!   Mirrors `cgen_net.rs` (Wave-C4, T11-D82) + `cgen_heap_dealloc.rs`
//!   (Wave-A5) sibling pattern. The actual call-emit (cranelift `call`
//!   instruction + operand-coercion via `uextend` / `ireduce`) is
//!   delegated to the existing `object::emit_window_call` SWAP-POINT —
//!   see § INTEGRATION_NOTE below for how that wires up.
//!
//! § ABI-LOCKED CONSTANTS
//!   ‼ The values pinned below MUST match
//!     `compiler-rs/crates/cssl-rt/src/host_window.rs` verbatim. Drift
//!     between the two sides = silent ABI mismatch ⇒ link-time symbol
//!     wiring or runtime mis-decode of event-records.
//!   ‼ Symbol-renames are major-version-bump events ; both sides update
//!     in lock-step.
//!
//! § INTEGRATION_NOTE  (per Wave-D3 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!   src/lib.rs` is intentionally NOT modified per task constraint
//!   "Touch ONLY : cssl-rt/src/host_window.rs (NEW), cssl-cgen-cpu-
//!   cranelift/src/cgen_window.rs (NEW)". The helpers compile + are
//!   tested in-place via `#[cfg(test)]` references.
//!
//!   ⌈ Main-thread integration follow-up ⌋ :
//!     1. Add `pub mod cgen_window;` to `cssl-cgen-cpu-cranelift/src/lib.rs`.
//!     2. Once cssl-mir grows `CsslOp::Window*` variants (deferred to a
//!        future MIR-side wave — neither D3 nor D4 ship MIR ops since the
//!        FFI surface is the lower layer that source-code calls into via
//!        stdlib helpers), wire `lower_window_op_to_symbol` (a future
//!        addition mirroring `cgen_net::lower_net_op_to_symbol`) +
//!        [`needs_window_imports`] into the existing
//!        `object::declare_*_imports_for_fn` machinery so
//!        `__cssl_window_*` symbols are only brought into the relocatable
//!        when a fn actually uses them.
//!     3. Migrate the actual cranelift `call`-emit logic from a future
//!        `object::emit_window_call` into a co-located helper here once
//!        the MIR-side ops land.
//!
//!   Until that follow-up lands the helpers are crate-internal-only
//!   (`#[allow(dead_code, unreachable_pub)]` matches the cgen_net /
//!   cgen_heap_dealloc sibling pattern).
//!
//! § SWAP-POINT  (mock-when-deps-missing per dispatch discipline)
//!   - The actual cranelift `call`-emission lives BEHIND a future
//!     `object::emit_window_call(builder, op, ptr_ty)` helper that this
//!     file does NOT call into directly (object.rs does not yet expose
//!     such a helper). The dispatcher [`window_symbol_for`] returns the
//!     FFI symbol-name + canonical signature ; once the object.rs wiring
//!     lands the dispatcher's caller will pair the symbol with the per-fn
//!     import-declare slot + emit the cranelift call. Until then the
//!     helpers compile + test in-place without touching the existing
//!     object.rs / jit.rs surface.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-rt/src/host_window.rs` — the
//!     `__cssl_window_*` ABI-stable symbols + event-record layout.
//!   - `compiler-rs/crates/cssl-host-window/src/` — the underlying Rust
//!     window-surface that cssl-rt::host_window's `_impl` fns delegate
//!     to (post Wave-D3 main-thread integration).
//!   - `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § window` — the
//!     authoritative ABI-lock for the six symbols.
//!   - `specs/40_WAVE_CSSL_PLAN.csl § WAVE-D ↳ D3` — wave-context.
//!
//! § CSL-MANDATE  (commit + design notes use CSL-glyph notation)
//!   ‼ ABI-stable :: rename ¬→ lock-step-cssl-rt::host_window
//!   ‼ pure-fn ::    zero-allocation ↑ Sig-Vec-storage
//!   ‼ event-rec :: 32-byte fixed-width · LE-order · cgen+rt agree
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions ; zero allocation outside the
//!     cranelift `Signature` constructor's required Vec storage.
//!   - Symbol-name LUT dispatch in [`window_symbol_for`] is a single
//!     match-arm per op-kind ; branch-friendly ordering keeps the
//!     most-common cases (pump / spawn / destroy) first.
//!   - `WindowImportSet` is a `u8` bitfield — 6 op-kinds fit in one byte
//!     with 2 reserved slots ; costs 1 bit-or per op (matches the
//!     `cgen_net::NetImportSet` pattern but compresses further since 6
//!     fits in u8 vs 9+ for net).

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{types as cl_types, AbiParam, Signature};
use cranelift_codegen::isa::CallConv;

// ───────────────────────────────────────────────────────────────────────
// § canonical FFI symbol-name LUT (per cssl-rt::host_window)
//
// ‼ ALL symbols MUST match `compiler-rs/crates/cssl-rt/src/host_window.rs`
//   verbatim. Renaming either side without the other = link-time
//   symbol mismatch ⇒ undefined behavior.
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol : `__cssl_window_spawn(title_ptr, title_len, w, h, flags) -> u64`.
pub const WINDOW_SPAWN_SYMBOL: &str = "__cssl_window_spawn";

/// FFI symbol : `__cssl_window_pump(handle, events_out, max_events) -> i64`.
pub const WINDOW_PUMP_SYMBOL: &str = "__cssl_window_pump";

/// FFI symbol : `__cssl_window_request_close(handle) -> i32`.
pub const WINDOW_REQUEST_CLOSE_SYMBOL: &str = "__cssl_window_request_close";

/// FFI symbol : `__cssl_window_destroy(handle) -> i32`.
pub const WINDOW_DESTROY_SYMBOL: &str = "__cssl_window_destroy";

/// FFI symbol : `__cssl_window_raw_handle(handle, out, max_len) -> i32`.
pub const WINDOW_RAW_HANDLE_SYMBOL: &str = "__cssl_window_raw_handle";

/// FFI symbol : `__cssl_window_get_dims(handle, w_out, h_out) -> i32`.
pub const WINDOW_GET_DIMS_SYMBOL: &str = "__cssl_window_get_dims";

// ───────────────────────────────────────────────────────────────────────
// § operand counts per FFI signature (matches cssl-rt::host_window arity)
// ───────────────────────────────────────────────────────────────────────

/// `__cssl_window_spawn(title_ptr, title_len, w, h, flags)` — 5 operands.
pub const WINDOW_SPAWN_OPERAND_COUNT: usize = 5;
/// `__cssl_window_pump(handle, events_out, max_events)` — 3 operands.
pub const WINDOW_PUMP_OPERAND_COUNT: usize = 3;
/// `__cssl_window_request_close(handle)` — 1 operand.
pub const WINDOW_REQUEST_CLOSE_OPERAND_COUNT: usize = 1;
/// `__cssl_window_destroy(handle)` — 1 operand.
pub const WINDOW_DESTROY_OPERAND_COUNT: usize = 1;
/// `__cssl_window_raw_handle(handle, out, max_len)` — 3 operands.
pub const WINDOW_RAW_HANDLE_OPERAND_COUNT: usize = 3;
/// `__cssl_window_get_dims(handle, w_out, h_out)` — 3 operands.
pub const WINDOW_GET_DIMS_OPERAND_COUNT: usize = 3;

// ───────────────────────────────────────────────────────────────────────
// § ABI-locked event-record layout constants
//
// ‼ Drift here vs cssl-rt::host_window = silent decode mismatch.
// ───────────────────────────────────────────────────────────────────────

/// Fixed size of one packed event record in bytes. Mirrors
/// `cssl_rt::host_window::EVENT_RECORD_SIZE`.
pub const EVENT_RECORD_SIZE: usize = 32;

/// Maximum bytes the raw-handle blob occupies on Win32 64-bit
/// `(HWND, HINSTANCE)` pair = 2 × usize. Mirrors
/// `cssl_rt::host_window::RAW_HANDLE_MAX_BYTES_WIN32`.
pub const RAW_HANDLE_MAX_BYTES_WIN32_64: usize = 16;

/// Sentinel : invalid window-handle is `0`. Mirrors
/// `cssl_rt::host_window::INVALID_WINDOW_HANDLE`.
pub const INVALID_WINDOW_HANDLE: u64 = 0;

// ── event-kind discriminants (ABI-locked u16 ; cssl-rt::host_window mirrors)

/// Sentinel ; not emitted.
pub const EVENT_KIND_NONE: u16 = 0;
/// User requested window close.
pub const EVENT_KIND_CLOSE: u16 = 1;
/// Window resized.
pub const EVENT_KIND_RESIZE: u16 = 2;
/// Window gained focus.
pub const EVENT_KIND_FOCUS_GAIN: u16 = 3;
/// Window lost focus.
pub const EVENT_KIND_FOCUS_LOSS: u16 = 4;
/// Keyboard key pressed.
pub const EVENT_KIND_KEY_DOWN: u16 = 5;
/// Keyboard key released.
pub const EVENT_KIND_KEY_UP: u16 = 6;
/// Mouse cursor moved.
pub const EVENT_KIND_MOUSE_MOVE: u16 = 7;
/// Mouse button pressed.
pub const EVENT_KIND_MOUSE_DOWN: u16 = 8;
/// Mouse button released.
pub const EVENT_KIND_MOUSE_UP: u16 = 9;
/// Mouse wheel scrolled.
pub const EVENT_KIND_SCROLL: u16 = 10;
/// DPI change.
pub const EVENT_KIND_DPI_CHANGE: u16 = 11;

// ── spawn-flag bitset (matches cssl-rt::host_window)

/// Window is user-resizable.
pub const SPAWN_FLAG_RESIZABLE: u32 = 1 << 0;
/// Window is fullscreen on primary monitor.
pub const SPAWN_FLAG_FULLSCREEN: u32 = 1 << 1;
/// Window is per-monitor-v2 DPI-aware on Win32.
pub const SPAWN_FLAG_DPI_AWARE: u32 = 1 << 2;
/// Window is borderless (no title-bar / window-edges).
pub const SPAWN_FLAG_BORDERLESS: u32 = 1 << 3;

/// Mask of recognized spawn-flag bits ; any other bit is rejected by the runtime.
pub const SPAWN_FLAG_MASK: u32 = SPAWN_FLAG_RESIZABLE
    | SPAWN_FLAG_FULLSCREEN
    | SPAWN_FLAG_DPI_AWARE
    | SPAWN_FLAG_BORDERLESS;

// ── pump return-code domain (matches cssl-rt::host_window)

/// Pump error : bad window handle.
pub const PUMP_ERR_BAD_HANDLE: i64 = -1;
/// Pump error : null events_out buffer with max_events > 0.
pub const PUMP_ERR_NULL_BUF: i64 = -2;
/// Pump error : window destroyed.
pub const PUMP_ERR_DESTROYED: i64 = -3;

// ───────────────────────────────────────────────────────────────────────
// § cranelift signature builders — one per FFI symbol
//
// Shapes match `compiler-rs/crates/cssl-rt/src/host_window.rs` exactly.
// The FFI uses i32/i64/u32/u64/usize + raw pointers ; cranelift IR sees
// integers (the `*const u8` / `*mut u8` pointer maps to `ptr_ty`, the
// `usize` length maps to `ptr_ty`, the `u32` width/height/flags maps to
// `cl_types::I32`, the `u64` handle maps to `cl_types::I64`).
// ───────────────────────────────────────────────────────────────────────

/// Build cranelift `Signature` for
/// `__cssl_window_spawn(*const u8, usize, u32, u32, u32) -> u64`.
///
/// Param layout : (title_ptr, title_len, width, height, flags).
/// `ptr_ty` is the host-pointer-width (`I64` on 64-bit hosts, `I32` on
/// 32-bit hosts) — `usize` collapses to the same type.
#[must_use]
pub fn build_window_spawn_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty)); // title_ptr
    sig.params.push(AbiParam::new(ptr_ty)); // title_len (usize)
    sig.params.push(AbiParam::new(cl_types::I32)); // width
    sig.params.push(AbiParam::new(cl_types::I32)); // height
    sig.params.push(AbiParam::new(cl_types::I32)); // flags
    sig.returns.push(AbiParam::new(cl_types::I64)); // u64 handle
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_window_pump(u64, *mut u8, usize) -> i64`.
///
/// Param layout : (handle, events_out, max_events).
#[must_use]
pub fn build_window_pump_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64)); // handle
    sig.params.push(AbiParam::new(ptr_ty)); // events_out
    sig.params.push(AbiParam::new(ptr_ty)); // max_events (usize)
    sig.returns.push(AbiParam::new(cl_types::I64)); // i64 count-or-errno
    sig
}

/// Build cranelift `Signature` for `__cssl_window_request_close(u64) -> i32`.
#[must_use]
pub fn build_window_request_close_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for `__cssl_window_destroy(u64) -> i32`.
#[must_use]
pub fn build_window_destroy_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_window_raw_handle(u64, *mut u8, usize) -> i32`.
#[must_use]
pub fn build_window_raw_handle_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64)); // handle
    sig.params.push(AbiParam::new(ptr_ty)); // out
    sig.params.push(AbiParam::new(ptr_ty)); // max_len (usize)
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_window_get_dims(u64, *mut u32, *mut u32) -> i32`.
#[must_use]
pub fn build_window_get_dims_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64)); // handle
    sig.params.push(AbiParam::new(ptr_ty)); // w_out
    sig.params.push(AbiParam::new(ptr_ty)); // h_out
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

// ───────────────────────────────────────────────────────────────────────
// § single dispatcher : window-op-tag → (FFI-symbol-name, expected-arity)
// ───────────────────────────────────────────────────────────────────────

/// Logical window-op categorical tag for cgen-side dispatch.
///
/// Pre Wave-D-MIR : there are no `cssl.window.*` MIR ops yet (the FFI
/// surface is the lower layer that source-code calls into via stdlib
/// helpers). When MIR-side ops land in a future wave the dispatcher will
/// switch to `cssl_mir::CsslOp::Window*` matching ; this tag keeps the
/// helper testable today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowOpTag {
    Spawn,
    Pump,
    RequestClose,
    Destroy,
    RawHandle,
    GetDims,
}

/// Map a [`WindowOpTag`] to the canonical FFI symbol-name + expected
/// operand-count.
///
/// § BRANCH-FRIENDLY ORDERING
///   The match arms are ordered by expected call-frequency :
///     pump (per-frame hot loop)
///   ↓ get_dims     (per-frame layout query)
///   ↓ raw_handle   (per-swapchain-create cold)
///   ↓ request_close + destroy (per-window cleanup)
///   ↓ spawn        (per-window setup, called once).
///   This lets the branch predictor + I-cache prefetch favor the common
///   per-frame cases.
#[must_use]
pub const fn window_symbol_for(tag: WindowOpTag) -> (&'static str, usize) {
    match tag {
        WindowOpTag::Pump => (WINDOW_PUMP_SYMBOL, WINDOW_PUMP_OPERAND_COUNT),
        WindowOpTag::GetDims => (WINDOW_GET_DIMS_SYMBOL, WINDOW_GET_DIMS_OPERAND_COUNT),
        WindowOpTag::RawHandle => (WINDOW_RAW_HANDLE_SYMBOL, WINDOW_RAW_HANDLE_OPERAND_COUNT),
        WindowOpTag::RequestClose => (
            WINDOW_REQUEST_CLOSE_SYMBOL,
            WINDOW_REQUEST_CLOSE_OPERAND_COUNT,
        ),
        WindowOpTag::Destroy => (WINDOW_DESTROY_SYMBOL, WINDOW_DESTROY_OPERAND_COUNT),
        WindowOpTag::Spawn => (WINDOW_SPAWN_SYMBOL, WINDOW_SPAWN_OPERAND_COUNT),
    }
}

/// Build the cranelift signature for the given window-op tag.
///
/// § PURPOSE
///   One-shot helper for cgen-paths that have a tag in hand + want the
///   matching `Signature` without re-deriving the per-symbol builder
///   table. Mirrors `cgen_net::lower_net_op_to_symbol` + per-op signature
///   builders at the call-site.
#[must_use]
pub fn build_window_signature(
    tag: WindowOpTag,
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    match tag {
        WindowOpTag::Spawn => build_window_spawn_signature(call_conv, ptr_ty),
        WindowOpTag::Pump => build_window_pump_signature(call_conv, ptr_ty),
        WindowOpTag::RequestClose => build_window_request_close_signature(call_conv),
        WindowOpTag::Destroy => build_window_destroy_signature(call_conv),
        WindowOpTag::RawHandle => build_window_raw_handle_signature(call_conv, ptr_ty),
        WindowOpTag::GetDims => build_window_get_dims_signature(call_conv, ptr_ty),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn pre-scan : "which window imports does this fn need"
//
// Encoded as a packed u8 bitfield (Sawyer-mindset : 6 op-kinds + 2
// reserved slots fit in one byte, costs 1 bit-or per op).
// ───────────────────────────────────────────────────────────────────────

/// Bitflag set of which `__cssl_window_*` imports a given MIR fn requires.
///
/// § BIT LAYOUT (u8)
///   bit 0 : spawn
///   bit 1 : pump
///   bit 2 : request_close
///   bit 3 : destroy
///   bit 4 : raw_handle
///   bit 5 : get_dims
///   bit 6 : reserved
///   bit 7 : reserved
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowImportSet(pub u8);

impl WindowImportSet {
    /// Empty (no window imports needed).
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// `spawn` import bit.
    pub const SPAWN: u8 = 1 << 0;
    /// `pump` import bit.
    pub const PUMP: u8 = 1 << 1;
    /// `request_close` import bit.
    pub const REQUEST_CLOSE: u8 = 1 << 2;
    /// `destroy` import bit.
    pub const DESTROY: u8 = 1 << 3;
    /// `raw_handle` import bit.
    pub const RAW_HANDLE: u8 = 1 << 4;
    /// `get_dims` import bit.
    pub const GET_DIMS: u8 = 1 << 5;

    /// Check whether `bits` are all set.
    #[must_use]
    pub const fn contains(self, bits: u8) -> bool {
        (self.0 & bits) == bits
    }

    /// Check whether ANY window-op-kind bit is set (the 6 direct ops).
    #[must_use]
    pub const fn any_window_op(self) -> bool {
        let direct_op_mask = Self::SPAWN
            | Self::PUMP
            | Self::REQUEST_CLOSE
            | Self::DESTROY
            | Self::RAW_HANDLE
            | Self::GET_DIMS;
        (self.0 & direct_op_mask) != 0
    }

    /// Set the bit corresponding to `tag`. Returns the updated set.
    #[must_use]
    pub const fn with_tag(self, tag: WindowOpTag) -> Self {
        let mask = match tag {
            WindowOpTag::Spawn => Self::SPAWN,
            WindowOpTag::Pump => Self::PUMP,
            WindowOpTag::RequestClose => Self::REQUEST_CLOSE,
            WindowOpTag::Destroy => Self::DESTROY,
            WindowOpTag::RawHandle => Self::RAW_HANDLE,
            WindowOpTag::GetDims => Self::GET_DIMS,
        };
        Self(self.0 | mask)
    }
}

/// Walk a slice of [`WindowOpTag`]s once + return the bitflag set.
///
/// § COMPLEXITY  O(N) in tag count, single-pass, NO early-exit (we
///   accumulate ALL imports needed). No allocation.
///
/// § NOTE  Once cssl-mir grows `CsslOp::Window*` variants the per-fn
///   pre-scan signature will be `&MirBlock` mirroring
///   `cgen_net::needs_net_imports` ; this helper is the testable
///   stand-in until then.
#[must_use]
pub fn needs_window_imports(tags: &[WindowOpTag]) -> WindowImportSet {
    let mut set = WindowImportSet::empty();
    for &tag in tags {
        set = set.with_tag(tag);
    }
    set
}

// ───────────────────────────────────────────────────────────────────────
// § contract validators (defensive cross-checks)
// ───────────────────────────────────────────────────────────────────────

/// Validate the operand count for the given tag.
///
/// Returns `Ok(())` when `n == operand-count(tag)` ; `Err(String)` with
/// a human-readable diagnostic otherwise. Used at cgen-import-resolve
/// time before issuing the cranelift `call` instruction so a mistyped
/// MIR op (once Window MIR-ops land) surfaces immediately.
pub fn validate_window_arity(tag: WindowOpTag, n: usize) -> Result<(), String> {
    let (sym, expected) = window_symbol_for(tag);
    if n != expected {
        return Err(format!(
            "validate_window_arity : {sym} requires {expected} operands ; got {n}",
        ));
    }
    Ok(())
}

/// Compute the events-buffer byte-length for `max_events` events.
///
/// Helper for source-level CSSLv3 stdlib helpers that need to size a
/// stack buffer before calling `__cssl_window_pump`. Matches
/// `EVENT_RECORD_SIZE * max_events` via const-arithmetic.
#[must_use]
pub const fn events_buffer_bytes(max_events: usize) -> usize {
    EVENT_RECORD_SIZE * max_events
}

/// Test : is `code` a known pump-error sentinel ?
#[must_use]
pub const fn is_pump_error(code: i64) -> bool {
    code == PUMP_ERR_BAD_HANDLE || code == PUMP_ERR_NULL_BUF || code == PUMP_ERR_DESTROYED
}

// ───────────────────────────────────────────────────────────────────────
// § tests — ≥ 12 unit tests covering all 6 ops + dispatcher + bitflag
// scan + arity validators + signature-shape locks
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::types as cl_types;
    use cranelift_codegen::isa::CallConv;

    // ── canonical-name lock invariants (cross-check with cssl-rt::host_window) ─

    #[test]
    fn ffi_symbols_match_cssl_rt_canonical() {
        // ‼ Lock-step invariant : symbol-names MUST match
        //   cssl-rt::host_window::__cssl_window_* verbatim. Renaming
        //   either side without the other = link-time symbol mismatch ⇒ UB.
        assert_eq!(WINDOW_SPAWN_SYMBOL, "__cssl_window_spawn");
        assert_eq!(WINDOW_PUMP_SYMBOL, "__cssl_window_pump");
        assert_eq!(WINDOW_REQUEST_CLOSE_SYMBOL, "__cssl_window_request_close");
        assert_eq!(WINDOW_DESTROY_SYMBOL, "__cssl_window_destroy");
        assert_eq!(WINDOW_RAW_HANDLE_SYMBOL, "__cssl_window_raw_handle");
        assert_eq!(WINDOW_GET_DIMS_SYMBOL, "__cssl_window_get_dims");
    }

    #[test]
    fn event_record_size_matches_runtime() {
        // ‼ ABI lock : drift here = silent decode mismatch.
        assert_eq!(EVENT_RECORD_SIZE, 32);
    }

    #[test]
    fn event_kind_discriminants_are_pinned() {
        // ‼ ABI lock : event-kind ordinals match cssl-rt::host_window.
        assert_eq!(EVENT_KIND_NONE, 0);
        assert_eq!(EVENT_KIND_CLOSE, 1);
        assert_eq!(EVENT_KIND_RESIZE, 2);
        assert_eq!(EVENT_KIND_FOCUS_GAIN, 3);
        assert_eq!(EVENT_KIND_FOCUS_LOSS, 4);
        assert_eq!(EVENT_KIND_KEY_DOWN, 5);
        assert_eq!(EVENT_KIND_KEY_UP, 6);
        assert_eq!(EVENT_KIND_MOUSE_MOVE, 7);
        assert_eq!(EVENT_KIND_MOUSE_DOWN, 8);
        assert_eq!(EVENT_KIND_MOUSE_UP, 9);
        assert_eq!(EVENT_KIND_SCROLL, 10);
        assert_eq!(EVENT_KIND_DPI_CHANGE, 11);
    }

    #[test]
    fn spawn_flag_constants_are_pinned() {
        assert_eq!(SPAWN_FLAG_RESIZABLE, 1);
        assert_eq!(SPAWN_FLAG_FULLSCREEN, 2);
        assert_eq!(SPAWN_FLAG_DPI_AWARE, 4);
        assert_eq!(SPAWN_FLAG_BORDERLESS, 8);
        assert_eq!(SPAWN_FLAG_MASK, 0b1111);
    }

    #[test]
    fn pump_error_constants_are_negative_and_distinct() {
        // Use assert_eq!/assert_ne! rather than `assert!(c < 0)` so clippy
        // doesn't const-fold the comparison into `assert!(true)` and emit
        // the optimize-out warning.
        assert_eq!(PUMP_ERR_BAD_HANDLE, -1);
        assert_eq!(PUMP_ERR_NULL_BUF, -2);
        assert_eq!(PUMP_ERR_DESTROYED, -3);
        assert_ne!(PUMP_ERR_BAD_HANDLE, PUMP_ERR_NULL_BUF);
        assert_ne!(PUMP_ERR_NULL_BUF, PUMP_ERR_DESTROYED);
        assert!(is_pump_error(PUMP_ERR_BAD_HANDLE));
        assert!(is_pump_error(PUMP_ERR_NULL_BUF));
        assert!(is_pump_error(PUMP_ERR_DESTROYED));
        assert!(!is_pump_error(0));
        assert!(!is_pump_error(1));
    }

    #[test]
    fn invalid_window_handle_is_zero() {
        // ‼ Lock : handle 0 = INVALID_WINDOW_HANDLE on both sides.
        assert_eq!(INVALID_WINDOW_HANDLE, 0);
    }

    // ── per-op signature shape locks ─────────────────────────────────────

    #[test]
    fn signature_spawn_shape() {
        let sig = build_window_spawn_signature(CallConv::SystemV, cl_types::I64);
        // (title_ptr, title_len, w, h, flags) -> u64
        assert_eq!(sig.params.len(), 5);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64), "title_ptr=ptr");
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64), "title_len=usize");
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I32), "width");
        assert_eq!(sig.params[3], AbiParam::new(cl_types::I32), "height");
        assert_eq!(sig.params[4], AbiParam::new(cl_types::I32), "flags");
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64), "u64 handle");
    }

    #[test]
    fn signature_pump_shape() {
        let sig = build_window_pump_signature(CallConv::SystemV, cl_types::I64);
        // (handle, events_out, max_events) -> i64
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64), "handle");
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64), "events_out=ptr");
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64), "max_events=usize");
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn signature_request_close_shape() {
        let sig = build_window_request_close_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_destroy_shape() {
        let sig = build_window_destroy_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_raw_handle_shape() {
        let sig = build_window_raw_handle_signature(CallConv::SystemV, cl_types::I64);
        // (handle, out, max_len) -> i32
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64), "out=ptr");
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64), "max_len=usize");
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_get_dims_shape() {
        let sig = build_window_get_dims_signature(CallConv::SystemV, cl_types::I64);
        // (handle, w_out, h_out) -> i32
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64), "w_out=ptr");
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64), "h_out=ptr");
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_with_i32_ptr_ty_for_32bit_targets() {
        // 32-bit hosts use I32 for the host pointer-width.
        let sig = build_window_spawn_signature(CallConv::SystemV, cl_types::I32);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I32));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_call_conv_passes_through() {
        let sysv = build_window_pump_signature(CallConv::SystemV, cl_types::I64);
        let win = build_window_pump_signature(CallConv::WindowsFastcall, cl_types::I64);
        assert_eq!(sysv.call_conv, CallConv::SystemV);
        assert_eq!(win.call_conv, CallConv::WindowsFastcall);
    }

    // ── dispatcher (window_symbol_for + build_window_signature) ─────────

    #[test]
    fn dispatcher_returns_correct_symbol_per_tag() {
        assert_eq!(window_symbol_for(WindowOpTag::Spawn).0, WINDOW_SPAWN_SYMBOL);
        assert_eq!(window_symbol_for(WindowOpTag::Pump).0, WINDOW_PUMP_SYMBOL);
        assert_eq!(
            window_symbol_for(WindowOpTag::RequestClose).0,
            WINDOW_REQUEST_CLOSE_SYMBOL,
        );
        assert_eq!(window_symbol_for(WindowOpTag::Destroy).0, WINDOW_DESTROY_SYMBOL);
        assert_eq!(
            window_symbol_for(WindowOpTag::RawHandle).0,
            WINDOW_RAW_HANDLE_SYMBOL,
        );
        assert_eq!(
            window_symbol_for(WindowOpTag::GetDims).0,
            WINDOW_GET_DIMS_SYMBOL,
        );
    }

    #[test]
    fn dispatcher_returns_correct_arity_per_tag() {
        assert_eq!(window_symbol_for(WindowOpTag::Spawn).1, 5);
        assert_eq!(window_symbol_for(WindowOpTag::Pump).1, 3);
        assert_eq!(window_symbol_for(WindowOpTag::RequestClose).1, 1);
        assert_eq!(window_symbol_for(WindowOpTag::Destroy).1, 1);
        assert_eq!(window_symbol_for(WindowOpTag::RawHandle).1, 3);
        assert_eq!(window_symbol_for(WindowOpTag::GetDims).1, 3);
    }

    #[test]
    fn build_window_signature_dispatches_by_tag() {
        // Each tag must produce the same shape as the per-symbol builder.
        let pump = build_window_signature(WindowOpTag::Pump, CallConv::SystemV, cl_types::I64);
        let pump_direct = build_window_pump_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(pump.params, pump_direct.params);
        assert_eq!(pump.returns, pump_direct.returns);

        let spawn = build_window_signature(WindowOpTag::Spawn, CallConv::SystemV, cl_types::I64);
        let spawn_direct = build_window_spawn_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(spawn.params, spawn_direct.params);
        assert_eq!(spawn.returns, spawn_direct.returns);
    }

    // ── WindowImportSet bitflag invariants ──────────────────────────────

    #[test]
    fn window_import_set_bits_are_distinct() {
        let bits = [
            WindowImportSet::SPAWN,
            WindowImportSet::PUMP,
            WindowImportSet::REQUEST_CLOSE,
            WindowImportSet::DESTROY,
            WindowImportSet::RAW_HANDLE,
            WindowImportSet::GET_DIMS,
        ];
        for (i, &b) in bits.iter().enumerate() {
            assert!(b.is_power_of_two(), "bit at idx {i} = {b:#x} not power-of-two");
            for (j, &b2) in bits.iter().enumerate() {
                if i != j {
                    assert_eq!(b & b2, 0, "bits at {i} + {j} overlap");
                }
            }
        }
    }

    #[test]
    fn window_import_set_with_tag_accumulates() {
        let set = WindowImportSet::empty()
            .with_tag(WindowOpTag::Spawn)
            .with_tag(WindowOpTag::Pump)
            .with_tag(WindowOpTag::Destroy);
        assert!(set.contains(WindowImportSet::SPAWN));
        assert!(set.contains(WindowImportSet::PUMP));
        assert!(set.contains(WindowImportSet::DESTROY));
        assert!(!set.contains(WindowImportSet::REQUEST_CLOSE));
        assert!(!set.contains(WindowImportSet::RAW_HANDLE));
        assert!(!set.contains(WindowImportSet::GET_DIMS));
        assert!(set.any_window_op());
    }

    #[test]
    fn window_import_set_empty_has_no_ops() {
        let s = WindowImportSet::empty();
        assert!(!s.any_window_op());
        assert_eq!(s.0, 0);
    }

    #[test]
    fn needs_window_imports_walks_tag_slice() {
        let tags = [
            WindowOpTag::Spawn,
            WindowOpTag::Pump,
            WindowOpTag::Pump, // duplicate ; bit-or is idempotent.
            WindowOpTag::Destroy,
        ];
        let set = needs_window_imports(&tags);
        assert!(set.contains(WindowImportSet::SPAWN));
        assert!(set.contains(WindowImportSet::PUMP));
        assert!(set.contains(WindowImportSet::DESTROY));
        assert!(!set.contains(WindowImportSet::RAW_HANDLE));
    }

    #[test]
    fn needs_window_imports_empty_slice_is_empty_set() {
        let tags: [WindowOpTag; 0] = [];
        let set = needs_window_imports(&tags);
        assert_eq!(set, WindowImportSet::empty());
        assert!(!set.any_window_op());
    }

    // ── arity validators ─────────────────────────────────────────────────

    #[test]
    fn validate_arity_accepts_canonical_counts() {
        assert!(validate_window_arity(WindowOpTag::Spawn, 5).is_ok());
        assert!(validate_window_arity(WindowOpTag::Pump, 3).is_ok());
        assert!(validate_window_arity(WindowOpTag::RequestClose, 1).is_ok());
        assert!(validate_window_arity(WindowOpTag::Destroy, 1).is_ok());
        assert!(validate_window_arity(WindowOpTag::RawHandle, 3).is_ok());
        assert!(validate_window_arity(WindowOpTag::GetDims, 3).is_ok());
    }

    #[test]
    fn validate_arity_rejects_wrong_counts() {
        let err = validate_window_arity(WindowOpTag::Spawn, 4).unwrap_err();
        assert!(err.contains("5 operands"), "diag = {err}");
        let err2 = validate_window_arity(WindowOpTag::Pump, 2).unwrap_err();
        assert!(err2.contains("3 operands"), "diag = {err2}");
    }

    // ── events_buffer_bytes helper ──────────────────────────────────────

    #[test]
    fn events_buffer_bytes_is_record_size_times_max() {
        assert_eq!(events_buffer_bytes(0), 0);
        assert_eq!(events_buffer_bytes(1), 32);
        assert_eq!(events_buffer_bytes(4), 128);
        assert_eq!(events_buffer_bytes(16), 512);
    }
}

// ── INTEGRATION_NOTE ────────────────────────────────────────────────────
//
// § Wave-D3 dispatch : "Touch ONLY cssl-rt/src/host_window.rs (NEW)
//   + cssl-cgen-cpu-cranelift/src/cgen_window.rs (NEW)".
//
// This module is delivered as a NEW file ; `cssl-cgen-cpu-cranelift/src/
// lib.rs` is intentionally NOT modified per task constraint. The helpers
// compile + are tested in-place via `#[cfg(test)]` references. Their
// canonical role (per-FFI-symbol Signature builder + tag-dispatcher +
// per-fn import-set bitflag scanner) becomes wire-up-able once the
// follow-up below lands.
//
// § Main-thread integration follow-up (small ; ≤ 5 lines across 1 file) :
//   1. Add `pub mod cgen_window;` to `cssl-cgen-cpu-cranelift/src/lib.rs`
//      (in alphabetical order with the other `cgen_*` modules).
//   2. Once cssl-mir grows `CsslOp::Window*` variants (deferred to a
//      future MIR-side wave), replace the `WindowOpTag`-based dispatcher
//      surface with a `&MirOp`-based one mirroring
//      `cgen_net::lower_net_op_to_symbol`. The per-symbol Signature
//      builders + the `WindowImportSet` bitflag scanner stay as-is —
//      only the tag-mapping arms change to match `op.op` arms.
//   3. Migrate the actual cranelift `call`-emit logic from a future
//      `object::emit_window_call` into a co-located helper here once
//      the MIR-side ops land. Until then `object.rs` does not need to
//      know about window symbols.
//
// § PRIME-DIRECTIVE attestation
//   "There was no hurt nor harm in the making of this, to anyone /
//    anything / anybody."
//   This module is pure-Rust pure-function helpers that build cranelift
//   IR signatures + symbol-tables. No I/O, no surveillance, no covert
//   channels, no side-effects. The cgen layer it eventually feeds emits
//   visible-in-source-fn-signature window-effect ops that are
//   structurally observable per `specs/04_EFFECTS.csl`.
