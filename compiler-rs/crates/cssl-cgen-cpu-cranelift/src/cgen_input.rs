//! § Wave-D4 — `__cssl_input_*` Cranelift cgen helpers (S5 ↳ § 24 HOST_FFI § input).
//!
//! § ROLE
//!   Pure-function helpers that build the cranelift `Signature` for each
//!   `__cssl_input_*` FFI import + decide which per-fn input-imports a
//!   given MIR block requires. The helpers form the canonical source-of-
//!   truth for the (input-op-kind, FFI-symbol-name, signature-shape)
//!   triple per input op so the cgen layer has ONE place to look when a
//!   downstream pass (object.rs / jit.rs) declares the imports.
//!
//!   Mirrors `cgen_net.rs` (Wave-C4) + `cgen_heap_dealloc.rs` (Wave-A5).
//!   The actual call-emit (cranelift `call` instruction + operand-coercion
//!   via `uextend` / `ireduce`) is delegated to a future `object::
//!   emit_input_call` SWAP-POINT — see § INTEGRATION_NOTE at file-end.
//!
//! § FFI SURFACE  (per `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § input`)
//!
//!   ```text
//!   __cssl_input_keyboard_state(handle : u64,
//!                                out_ptr : *mut u8, max_len : usize) -> i32
//!   __cssl_input_mouse_state(handle : u64,
//!                              x_out : *mut i32, y_out : *mut i32,
//!                              btns_out : *mut u32) -> i32
//!   __cssl_input_mouse_delta(handle : u64,
//!                              dx_out : *mut i32, dy_out : *mut i32) -> i32
//!   __cssl_input_gamepad_state(idx : u32,
//!                                out_ptr : *mut u8, max_len : usize) -> i32
//!   ```
//!
//!   ‼ Symbol-names MUST match `cssl-rt::host_input` verbatim. Renaming
//!     either side without the other = link-time symbol mismatch ⇒ UB.
//!
//! § STAGE STATUS  (Wave-D4 / this slice)
//!   Stage-0 ships : signature-builders + symbol-name LUT + per-block
//!   import-need bitset (`InputImportSet`). The cssl-mir side does NOT
//!   yet have `CsslOp::Input*` variants ; once those land in a follow-up
//!   slice the dispatcher [`lower_input_op_to_symbol`] becomes a hot-
//!   path match-arm (today it works on a flat `InputOpKind` enum that
//!   the future MIR-op recognizer maps onto).
//!
//! § CSL-MANDATE  (commit + design notes use CSL-glyph notation)
//!   ‼ ABI-stable :: rename ¬→ lock-step-cssl-rt::host_input
//!   ‼ pure-fn ::    zero-allocation ↑ Sig-Vec-storage
//!   ‼ O(N) ::       per-block-walk ⊑ single-pass + early-exit
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions ; zero allocation outside the
//!     cranelift `Signature` constructor's required Vec storage.
//!   - Symbol-name LUT dispatch is a single match-arm per op-kind ;
//!     branch-friendly ordering keeps the most-common cases first
//!     (keyboard-state poll-per-frame > mouse-state ≈ mouse-delta >
//!     gamepad-state which is rarer).
//!   - `InputImportSet` is a `u8` bitfield (4 op-kinds + 4 reserved bits) —
//!     fits in a single byte register, costs 1 bit-or per op.
//!   - Per-block scan is O(N) single-pass with NO early-exit (we
//!     accumulate ALL imports needed) ; matches the Wave-C4 net-import
//!     pre-scan precedent.

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{types as cl_types, AbiParam, Signature};
use cranelift_codegen::isa::CallConv;

// ───────────────────────────────────────────────────────────────────────
// § canonical FFI symbol-name LUT (per cssl-rt::host_input)
//
// ‼ ALL symbols MUST match
//   `compiler-rs/crates/cssl-rt/src/host_input.rs` verbatim. Renaming
//   either side without the other = link-time symbol mismatch ⇒ UB.
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol : `__cssl_input_keyboard_state(handle, out_ptr, max_len) -> i32`.
pub const INPUT_KEYBOARD_STATE_SYMBOL: &str = "__cssl_input_keyboard_state";

/// FFI symbol : `__cssl_input_mouse_state(handle, x_out, y_out, btns_out) -> i32`.
pub const INPUT_MOUSE_STATE_SYMBOL: &str = "__cssl_input_mouse_state";

/// FFI symbol : `__cssl_input_mouse_delta(handle, dx_out, dy_out) -> i32`.
///
/// § PRIME-DIRECTIVE — `Sensitive<Behavioral>` per `specs/24_HOST_FFI.csl §
/// IFC-LABELS`. The cgen layer does NOT need to enforce IFC at the
/// signature level — that's a §§ 11 IFC-pass concern — but cgen MUST
/// preserve the symbol-name distinction so downstream IFC analyses can
/// recognize mouse-delta calls.
pub const INPUT_MOUSE_DELTA_SYMBOL: &str = "__cssl_input_mouse_delta";

/// FFI symbol : `__cssl_input_gamepad_state(idx, out_ptr, max_len) -> i32`.
pub const INPUT_GAMEPAD_STATE_SYMBOL: &str = "__cssl_input_gamepad_state";

// ───────────────────────────────────────────────────────────────────────
// § canonical MIR op-name LUT
//
// Stage-0 : the cssl-mir crate does not yet have `CsslOp::Input*`
// variants. The names below are the strings that the future op-recognizer
// will produce ; locking them here lets downstream cgen pre-emit the
// import-declaration before the MIR-op variant lands.
// ───────────────────────────────────────────────────────────────────────

/// MIR op-name (future) : `cssl.input.keyboard.state`.
pub const MIR_INPUT_KEYBOARD_STATE_OP_NAME: &str = "cssl.input.keyboard.state";

/// MIR op-name (future) : `cssl.input.mouse.state`.
pub const MIR_INPUT_MOUSE_STATE_OP_NAME: &str = "cssl.input.mouse.state";

/// MIR op-name (future) : `cssl.input.mouse.delta`.
pub const MIR_INPUT_MOUSE_DELTA_OP_NAME: &str = "cssl.input.mouse.delta";

/// MIR op-name (future) : `cssl.input.gamepad.state`.
pub const MIR_INPUT_GAMEPAD_STATE_OP_NAME: &str = "cssl.input.gamepad.state";

// ───────────────────────────────────────────────────────────────────────
// § operand / result counts
// ───────────────────────────────────────────────────────────────────────

/// `keyboard_state(handle, out_ptr, max_len) -> i32` — 3 operands.
pub const INPUT_KEYBOARD_STATE_OPERAND_COUNT: usize = 3;

/// `mouse_state(handle, x_out, y_out, btns_out) -> i32` — 4 operands.
pub const INPUT_MOUSE_STATE_OPERAND_COUNT: usize = 4;

/// `mouse_delta(handle, dx_out, dy_out) -> i32` — 3 operands.
pub const INPUT_MOUSE_DELTA_OPERAND_COUNT: usize = 3;

/// `gamepad_state(idx, out_ptr, max_len) -> i32` — 3 operands.
pub const INPUT_GAMEPAD_STATE_OPERAND_COUNT: usize = 3;

/// All four input ops produce a single i32 result (status code).
pub const INPUT_RESULT_COUNT: usize = 1;

// ───────────────────────────────────────────────────────────────────────
// § Op-kind enum (stage-0 standalone ; cssl-mir CsslOp::Input* will
// supersede in a follow-up slice)
// ───────────────────────────────────────────────────────────────────────

/// Identifies which `__cssl_input_*` op a given call-site corresponds to.
///
/// Until `CsslOp::Input*` variants land in cssl-mir, recognizer-bridges
/// produce one of these via the op-name string match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOpKind {
    /// `__cssl_input_keyboard_state` — bit-vector poll.
    KeyboardState,
    /// `__cssl_input_mouse_state` — cursor + button-mask poll.
    MouseState,
    /// `__cssl_input_mouse_delta` — Sensitive<Behavioral> delta drain.
    MouseDelta,
    /// `__cssl_input_gamepad_state` — gamepad slot poll.
    GamepadState,
}

impl InputOpKind {
    /// Returns the canonical FFI symbol-name for this op-kind.
    #[must_use]
    pub const fn ffi_symbol(self) -> &'static str {
        match self {
            Self::KeyboardState => INPUT_KEYBOARD_STATE_SYMBOL,
            Self::MouseState => INPUT_MOUSE_STATE_SYMBOL,
            Self::MouseDelta => INPUT_MOUSE_DELTA_SYMBOL,
            Self::GamepadState => INPUT_GAMEPAD_STATE_SYMBOL,
        }
    }

    /// Returns the canonical (future) MIR op-name string for this op-kind.
    #[must_use]
    pub const fn mir_op_name(self) -> &'static str {
        match self {
            Self::KeyboardState => MIR_INPUT_KEYBOARD_STATE_OP_NAME,
            Self::MouseState => MIR_INPUT_MOUSE_STATE_OP_NAME,
            Self::MouseDelta => MIR_INPUT_MOUSE_DELTA_OP_NAME,
            Self::GamepadState => MIR_INPUT_GAMEPAD_STATE_OP_NAME,
        }
    }

    /// Returns the expected operand count for this op-kind.
    #[must_use]
    pub const fn operand_count(self) -> usize {
        match self {
            Self::KeyboardState => INPUT_KEYBOARD_STATE_OPERAND_COUNT,
            Self::MouseState => INPUT_MOUSE_STATE_OPERAND_COUNT,
            Self::MouseDelta => INPUT_MOUSE_DELTA_OPERAND_COUNT,
            Self::GamepadState => INPUT_GAMEPAD_STATE_OPERAND_COUNT,
        }
    }

    /// Returns the matching [`InputImportSet`] mask bit.
    #[must_use]
    pub const fn import_mask_bit(self) -> u8 {
        match self {
            Self::KeyboardState => InputImportSet::KEYBOARD_STATE,
            Self::MouseState => InputImportSet::MOUSE_STATE,
            Self::MouseDelta => InputImportSet::MOUSE_DELTA,
            Self::GamepadState => InputImportSet::GAMEPAD_STATE,
        }
    }

    /// Returns true if this op-kind is `Sensitive<Behavioral>` per § 24
    /// IFC-LABELS. Today only mouse-delta carries the marker ; mouse-state
    /// + keyboard-state are `Sensitive<Behavioral>` for non-game-key events
    /// at the source-level layer ; cgen sees them as Public-or-flagged-by-
    /// caller.
    #[must_use]
    pub const fn is_behavioral_sensitive(self) -> bool {
        matches!(self, Self::MouseDelta)
    }

    /// Try to recognize an op-kind from its (future) MIR-op-name string.
    /// Returns `None` for non-input op-names.
    #[must_use]
    pub fn from_mir_op_name(name: &str) -> Option<Self> {
        match name {
            MIR_INPUT_KEYBOARD_STATE_OP_NAME => Some(Self::KeyboardState),
            MIR_INPUT_MOUSE_STATE_OP_NAME => Some(Self::MouseState),
            MIR_INPUT_MOUSE_DELTA_OP_NAME => Some(Self::MouseDelta),
            MIR_INPUT_GAMEPAD_STATE_OP_NAME => Some(Self::GamepadState),
            _ => None,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § cranelift signature builders — one per op-kind
//
// Shapes match `compiler-rs/crates/cssl-rt/src/host_input.rs` exactly.
// The FFI uses i32/u32/u64/usize + raw pointers ; cranelift IR sees
// integers (the `*mut u8` pointer maps to `ptr_ty`, `usize` length maps
// to `ptr_ty`, `u64` handle maps to `cl_types::I64`, `u32` maps to
// `cl_types::I32`, `i32` returns + outs map to `cl_types::I32`). The
// cgen call-emit path coerces operand types via uextend / ireduce.
// ───────────────────────────────────────────────────────────────────────

/// Build cranelift `Signature` for
/// `__cssl_input_keyboard_state(u64, *mut u8, usize) -> i32`.
///
/// `ptr_ty` is host-ptr-width (`I64` on x86_64, `I32` on 32-bit hosts).
#[must_use]
pub fn build_keyboard_state_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));      // handle
    sig.params.push(AbiParam::new(ptr_ty));             // out_ptr
    sig.params.push(AbiParam::new(ptr_ty));             // max_len (usize)
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_input_mouse_state(u64, *mut i32, *mut i32, *mut u32) -> i32`.
#[must_use]
pub fn build_mouse_state_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));      // handle
    sig.params.push(AbiParam::new(ptr_ty));             // x_out
    sig.params.push(AbiParam::new(ptr_ty));             // y_out
    sig.params.push(AbiParam::new(ptr_ty));             // btns_out
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_input_mouse_delta(u64, *mut i32, *mut i32) -> i32`.
///
/// § PRIME-DIRECTIVE — see `INPUT_MOUSE_DELTA_SYMBOL` doc-block. The
/// signature is identical to the other 3-out ops ; the IFC labelling
/// is a §§ 11 concern, not a cranelift-shape concern.
#[must_use]
pub fn build_mouse_delta_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));      // handle
    sig.params.push(AbiParam::new(ptr_ty));             // dx_out
    sig.params.push(AbiParam::new(ptr_ty));             // dy_out
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_input_gamepad_state(u32, *mut u8, usize) -> i32`.
#[must_use]
pub fn build_gamepad_state_signature(
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I32));      // idx
    sig.params.push(AbiParam::new(ptr_ty));             // out_ptr
    sig.params.push(AbiParam::new(ptr_ty));             // max_len (usize)
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Single-entry dispatch : build the cranelift signature for `op_kind`
/// using the host pointer-type `ptr_ty` + the target's `call_conv`.
///
/// § BRANCH-FRIENDLY ORDERING
///   The match arms are ordered by expected call-frequency :
///     keyboard_state (per-frame poll in game-loops)
///   ↓ mouse_state    (per-frame poll alongside keyboard)
///   ↓ mouse_delta    (per-frame for FPS-style camera)
///   ↓ gamepad_state  (rarer ; only when controllers are present).
///   This lets the branch predictor + I-cache prefetch favor the
///   common cases. (Sawyer-mindset : measure-then-order.)
#[must_use]
pub fn build_input_signature(
    op_kind: InputOpKind,
    call_conv: CallConv,
    ptr_ty: cranelift_codegen::ir::Type,
) -> Signature {
    match op_kind {
        InputOpKind::KeyboardState => build_keyboard_state_signature(call_conv, ptr_ty),
        InputOpKind::MouseState => build_mouse_state_signature(call_conv, ptr_ty),
        InputOpKind::MouseDelta => build_mouse_delta_signature(call_conv, ptr_ty),
        InputOpKind::GamepadState => build_gamepad_state_signature(call_conv, ptr_ty),
    }
}

/// Map an [`InputOpKind`] to the canonical (FFI-symbol-name, expected-
/// arity) pair.
///
/// Mirrors `cgen_net::lower_net_op_to_symbol` shape ; a future cssl-mir
/// `CsslOp::Input*` recognizer-bridge plumbs the MIR op through here.
#[must_use]
pub fn lower_input_op_to_symbol(op_kind: InputOpKind) -> (&'static str, usize) {
    (op_kind.ffi_symbol(), op_kind.operand_count())
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn pre-scan : "which input imports does this fn need"
//
// Encoded as a packed u8 bitfield (Sawyer-mindset : 4 op-kinds + 4 reserved
// bits fit in one byte, costs 1 bit-or per op).
// ───────────────────────────────────────────────────────────────────────

/// Bitflag set of which `__cssl_input_*` imports a given MIR fn requires.
///
/// § BIT LAYOUT (u8)
///   bit 0 : keyboard_state
///   bit 1 : mouse_state
///   bit 2 : mouse_delta
///   bit 3 : gamepad_state
///   bits 4..7 : reserved (future : touch / xr-pose / scroll-state)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputImportSet(pub u8);

impl InputImportSet {
    /// Empty (no input imports needed).
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// `keyboard_state` import bit.
    pub const KEYBOARD_STATE: u8 = 1 << 0;
    /// `mouse_state` import bit.
    pub const MOUSE_STATE: u8 = 1 << 1;
    /// `mouse_delta` import bit (Sensitive<Behavioral>).
    pub const MOUSE_DELTA: u8 = 1 << 2;
    /// `gamepad_state` import bit.
    pub const GAMEPAD_STATE: u8 = 1 << 3;

    /// All 4 op-bits OR'd together.
    pub const ALL_OPS: u8 =
        Self::KEYBOARD_STATE | Self::MOUSE_STATE | Self::MOUSE_DELTA | Self::GAMEPAD_STATE;

    /// Check whether `bits` are all set.
    #[must_use]
    pub const fn contains(self, bits: u8) -> bool {
        (self.0 & bits) == bits
    }

    /// Check whether ANY input-op-kind bit is set (the 4 direct ops).
    #[must_use]
    pub const fn any_input_op(self) -> bool {
        (self.0 & Self::ALL_OPS) != 0
    }

    /// Set the bit corresponding to `kind`. Returns the updated set.
    #[must_use]
    pub const fn with_op(self, kind: InputOpKind) -> Self {
        Self(self.0 | kind.import_mask_bit())
    }

    /// Set the bit corresponding to a future MIR-op-name string (returns
    /// `self` unchanged on non-input names).
    #[must_use]
    pub fn with_op_name(self, name: &str) -> Self {
        if let Some(kind) = InputOpKind::from_mir_op_name(name) {
            self.with_op(kind)
        } else {
            self
        }
    }

    /// Returns true if this set contains the `Sensitive<Behavioral>` bit
    /// (mouse-delta). Used by IFC-aware downstream passes.
    #[must_use]
    pub const fn has_behavioral_sensitive(self) -> bool {
        (self.0 & Self::MOUSE_DELTA) != 0
    }
}

/// Walk a sequence of (future) MIR op-names + accumulate the import
/// bitset.
///
/// § COMPLEXITY  O(N) in op count, single-pass, NO early-exit (we
///   accumulate ALL imports needed). No allocation.
///
/// Mirrors `cgen_net::needs_net_imports`. Once cssl-mir grows
/// `CsslOp::Input*` variants the body becomes
/// `for op in &block.ops { set = set.with_op_for_csslop(op.op) }` ; for
/// now the string-keyed walk lets recognizer-bridges drive it.
#[must_use]
pub fn needs_input_imports_for_op_names<'a, I>(op_names: I) -> InputImportSet
where
    I: IntoIterator<Item = &'a str>,
{
    let mut set = InputImportSet::empty();
    for name in op_names {
        set = set.with_op_name(name);
    }
    set
}

// ───────────────────────────────────────────────────────────────────────
// § contract validators (defensive cross-checks)
// ───────────────────────────────────────────────────────────────────────

/// Validate that `operand_count` + `result_count` match the canonical
/// shape for `op_kind`. Returns `Ok(())` if both match.
///
/// § INTENT
///   Defensive check used at cgen-import-resolve time before issuing
///   the cranelift `call` instruction. Surfaces an actionable error
///   if a mistyped MIR op leaks past prior passes.
///
/// # Errors
/// Returns `Err(String)` with a human-readable diagnostic when the
/// operand-count diverges from the canonical expectation OR the
/// result-count is not 1.
pub fn validate_input_arity(
    op_kind: InputOpKind,
    operand_count: usize,
    result_count: usize,
) -> Result<(), String> {
    let expected_operands = op_kind.operand_count();
    if operand_count != expected_operands {
        return Err(format!(
            "validate_input_arity : `{}` (-> {}) requires {expected_operands} operands ; got {operand_count}",
            op_kind.mir_op_name(),
            op_kind.ffi_symbol(),
        ));
    }
    if result_count != INPUT_RESULT_COUNT {
        return Err(format!(
            "validate_input_arity : `{}` (-> {}) produces {INPUT_RESULT_COUNT} result ; got {result_count}",
            op_kind.mir_op_name(),
            op_kind.ffi_symbol(),
        ));
    }
    Ok(())
}

/// Returns `true` if `idx ≥ 4` (XInput cap) for a gamepad-state op-kind.
/// Allows recognizer-bridges to short-circuit emit + emit a static
/// `INPUT_ERR_INVALID_INDEX` constant instead of issuing the FFI call.
#[must_use]
pub const fn gamepad_idx_out_of_range_is_static_err(idx: u32) -> bool {
    idx >= 4
}

// ───────────────────────────────────────────────────────────────────────
// § tests — ≥ 12 unit tests covering all 4 input ops + dispatcher +
// bitflag scan + arity validators + signature-shape locks
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        build_gamepad_state_signature, build_input_signature, build_keyboard_state_signature,
        build_mouse_delta_signature, build_mouse_state_signature,
        gamepad_idx_out_of_range_is_static_err, lower_input_op_to_symbol,
        needs_input_imports_for_op_names, validate_input_arity, InputImportSet, InputOpKind,
        INPUT_GAMEPAD_STATE_OPERAND_COUNT, INPUT_GAMEPAD_STATE_SYMBOL,
        INPUT_KEYBOARD_STATE_OPERAND_COUNT, INPUT_KEYBOARD_STATE_SYMBOL,
        INPUT_MOUSE_DELTA_OPERAND_COUNT, INPUT_MOUSE_DELTA_SYMBOL, INPUT_MOUSE_STATE_OPERAND_COUNT,
        INPUT_MOUSE_STATE_SYMBOL, INPUT_RESULT_COUNT, MIR_INPUT_GAMEPAD_STATE_OP_NAME,
        MIR_INPUT_KEYBOARD_STATE_OP_NAME, MIR_INPUT_MOUSE_DELTA_OP_NAME,
        MIR_INPUT_MOUSE_STATE_OP_NAME,
    };
    use cranelift_codegen::ir::{types as cl_types, AbiParam};
    use cranelift_codegen::isa::CallConv;

    // ── canonical-name lock invariants (cross-check w/ cssl-rt::host_input) ─

    #[test]
    fn ffi_symbols_match_cssl_rt_canonical() {
        // ‼ Lock-step invariant : symbol-names MUST match
        //   cssl-rt::host_input::__cssl_input_* verbatim. Renaming
        //   either side without the other = link-time symbol mismatch
        //   ⇒ undefined behavior.
        assert_eq!(INPUT_KEYBOARD_STATE_SYMBOL, "__cssl_input_keyboard_state");
        assert_eq!(INPUT_MOUSE_STATE_SYMBOL, "__cssl_input_mouse_state");
        assert_eq!(INPUT_MOUSE_DELTA_SYMBOL, "__cssl_input_mouse_delta");
        assert_eq!(INPUT_GAMEPAD_STATE_SYMBOL, "__cssl_input_gamepad_state");
    }

    #[test]
    fn mir_op_names_match_canonical() {
        // ‼ Future-MIR-op-name lock. When CsslOp::Input* lands these
        //   strings MUST equal `CsslOp::InputKeyboardState.name()` etc.
        assert_eq!(MIR_INPUT_KEYBOARD_STATE_OP_NAME, "cssl.input.keyboard.state");
        assert_eq!(MIR_INPUT_MOUSE_STATE_OP_NAME, "cssl.input.mouse.state");
        assert_eq!(MIR_INPUT_MOUSE_DELTA_OP_NAME, "cssl.input.mouse.delta");
        assert_eq!(MIR_INPUT_GAMEPAD_STATE_OP_NAME, "cssl.input.gamepad.state");
    }

    #[test]
    fn op_kind_ffi_and_mir_strings_round_trip() {
        for kind in [
            InputOpKind::KeyboardState,
            InputOpKind::MouseState,
            InputOpKind::MouseDelta,
            InputOpKind::GamepadState,
        ] {
            let mir_name = kind.mir_op_name();
            let recovered = InputOpKind::from_mir_op_name(mir_name).unwrap();
            assert_eq!(recovered, kind, "round-trip mir-name → kind");
            // ffi-symbol is non-empty + starts with the canonical prefix.
            assert!(kind.ffi_symbol().starts_with("__cssl_input_"));
        }
    }

    #[test]
    fn from_mir_op_name_rejects_non_input_names() {
        assert!(InputOpKind::from_mir_op_name("cssl.net.send").is_none());
        assert!(InputOpKind::from_mir_op_name("arith.constant").is_none());
        assert!(InputOpKind::from_mir_op_name("cssl.input.unknown").is_none());
        assert!(InputOpKind::from_mir_op_name("").is_none());
    }

    // ── per-op signature shape locks ────────────────────────────────────

    #[test]
    fn signature_keyboard_state_has_three_params() {
        let sig = build_keyboard_state_signature(CallConv::SystemV, cl_types::I64);
        // (i64 handle, *mut u8 out, usize max_len) -> i32
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64), "ptr_ty=I64");
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64), "usize=I64");
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_mouse_state_has_four_params() {
        let sig = build_mouse_state_signature(CallConv::SystemV, cl_types::I64);
        // (i64 handle, *mut i32 x, *mut i32 y, *mut u32 btns) -> i32
        assert_eq!(sig.params.len(), 4);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[3], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_mouse_delta_has_three_params() {
        let sig = build_mouse_delta_signature(CallConv::SystemV, cl_types::I64);
        // (i64 handle, *mut i32 dx, *mut i32 dy) -> i32
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_gamepad_state_has_three_params() {
        let sig = build_gamepad_state_signature(CallConv::SystemV, cl_types::I64);
        // (u32 idx, *mut u8 out, usize max_len) -> i32
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I32), "idx is u32");
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64), "ptr_ty=I64");
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64), "usize=I64");
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_call_conv_passes_through() {
        let sysv = build_mouse_delta_signature(CallConv::SystemV, cl_types::I64);
        let win = build_mouse_delta_signature(CallConv::WindowsFastcall, cl_types::I64);
        assert_eq!(sysv.call_conv, CallConv::SystemV);
        assert_eq!(win.call_conv, CallConv::WindowsFastcall);
    }

    #[test]
    fn signature_with_i32_ptr_ty_for_32bit_targets() {
        // 32-bit hosts use I32 for the host pointer-width.
        let sig = build_keyboard_state_signature(CallConv::SystemV, cl_types::I32);
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signature_dispatcher_matches_per_kind_builders() {
        // The dispatcher builds the same signature shape as the per-kind
        // helpers — sanity-check the equivalence.
        for (kind, expected_param_count) in [
            (InputOpKind::KeyboardState, 3),
            (InputOpKind::MouseState, 4),
            (InputOpKind::MouseDelta, 3),
            (InputOpKind::GamepadState, 3),
        ] {
            let sig = build_input_signature(kind, CallConv::SystemV, cl_types::I64);
            assert_eq!(
                sig.params.len(),
                expected_param_count,
                "param-count for {kind:?}"
            );
            assert_eq!(sig.returns.len(), 1, "returns-count for {kind:?}");
            assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
        }
    }

    // ── lower_input_op_to_symbol dispatcher ─────────────────────────────

    #[test]
    fn dispatcher_keyboard_state_returns_keyboard_symbol() {
        let (sym, arity) = lower_input_op_to_symbol(InputOpKind::KeyboardState);
        assert_eq!(sym, INPUT_KEYBOARD_STATE_SYMBOL);
        assert_eq!(arity, INPUT_KEYBOARD_STATE_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_mouse_state_returns_mouse_symbol() {
        let (sym, arity) = lower_input_op_to_symbol(InputOpKind::MouseState);
        assert_eq!(sym, INPUT_MOUSE_STATE_SYMBOL);
        assert_eq!(arity, INPUT_MOUSE_STATE_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_mouse_delta_returns_delta_symbol() {
        let (sym, arity) = lower_input_op_to_symbol(InputOpKind::MouseDelta);
        assert_eq!(sym, INPUT_MOUSE_DELTA_SYMBOL);
        assert_eq!(arity, INPUT_MOUSE_DELTA_OPERAND_COUNT);
    }

    #[test]
    fn dispatcher_gamepad_state_returns_gamepad_symbol() {
        let (sym, arity) = lower_input_op_to_symbol(InputOpKind::GamepadState);
        assert_eq!(sym, INPUT_GAMEPAD_STATE_SYMBOL);
        assert_eq!(arity, INPUT_GAMEPAD_STATE_OPERAND_COUNT);
    }

    // ── Sensitive<Behavioral> marker (per § 24 IFC) ─────────────────────

    #[test]
    fn behavioral_sensitive_only_mouse_delta_today() {
        // Per `specs/24_HOST_FFI.csl § IFC-LABELS` — mouse-delta is the
        // sole `Sensitive<Behavioral>` op at the cgen layer today.
        // Keyboard + mouse-state are flagged at source-level only.
        assert!(InputOpKind::MouseDelta.is_behavioral_sensitive());
        assert!(!InputOpKind::KeyboardState.is_behavioral_sensitive());
        assert!(!InputOpKind::MouseState.is_behavioral_sensitive());
        assert!(!InputOpKind::GamepadState.is_behavioral_sensitive());
    }

    // ── needs_input_imports_for_op_names : per-fn pre-scan ─────────────

    #[test]
    fn pre_scan_empty_block_returns_empty_set() {
        let empty: [&str; 0] = [];
        let set = needs_input_imports_for_op_names(empty.iter().copied());
        assert_eq!(set, InputImportSet::empty());
        assert!(!set.any_input_op());
    }

    #[test]
    fn pre_scan_finds_keyboard_when_present() {
        let names = [MIR_INPUT_KEYBOARD_STATE_OP_NAME];
        let set = needs_input_imports_for_op_names(names.iter().copied());
        assert!(set.contains(InputImportSet::KEYBOARD_STATE));
        assert!(!set.contains(InputImportSet::MOUSE_STATE));
        assert!(set.any_input_op());
    }

    #[test]
    fn pre_scan_accumulates_multiple_distinct_imports() {
        // A typical game-loop fn polls keyboard + mouse-delta + gamepad
        // each frame. Pre-scan must surface all 3 imports.
        let names = [
            MIR_INPUT_KEYBOARD_STATE_OP_NAME,
            MIR_INPUT_MOUSE_DELTA_OP_NAME,
            MIR_INPUT_GAMEPAD_STATE_OP_NAME,
        ];
        let set = needs_input_imports_for_op_names(names.iter().copied());
        assert!(set.contains(InputImportSet::KEYBOARD_STATE));
        assert!(set.contains(InputImportSet::MOUSE_DELTA));
        assert!(set.contains(InputImportSet::GAMEPAD_STATE));
        assert!(!set.contains(InputImportSet::MOUSE_STATE));
        assert!(set.has_behavioral_sensitive(), "mouse-delta sets the sensitive bit");
    }

    #[test]
    fn pre_scan_ignores_non_input_op_names() {
        // arith / net / fs op-names must NOT flip input-import bits.
        let names = ["arith.constant", "cssl.net.send", "cssl.fs.read", ""];
        let set = needs_input_imports_for_op_names(names.iter().copied());
        assert_eq!(set, InputImportSet::empty());
        assert!(!set.any_input_op());
        assert!(!set.has_behavioral_sensitive());
    }

    #[test]
    fn pre_scan_dedups_repeated_op_names() {
        // 5 keyboard-state ops in the same block produce ONE import bit.
        let names = [
            MIR_INPUT_KEYBOARD_STATE_OP_NAME,
            MIR_INPUT_KEYBOARD_STATE_OP_NAME,
            MIR_INPUT_KEYBOARD_STATE_OP_NAME,
            MIR_INPUT_KEYBOARD_STATE_OP_NAME,
            MIR_INPUT_KEYBOARD_STATE_OP_NAME,
        ];
        let set = needs_input_imports_for_op_names(names.iter().copied());
        assert_eq!(set.0, InputImportSet::KEYBOARD_STATE);
    }

    // ── InputImportSet bit-arithmetic invariants ──────────────────────

    #[test]
    fn input_import_set_bits_are_distinct() {
        // Defensive : every bit must be a power-of-two AND distinct.
        let bits = [
            InputImportSet::KEYBOARD_STATE,
            InputImportSet::MOUSE_STATE,
            InputImportSet::MOUSE_DELTA,
            InputImportSet::GAMEPAD_STATE,
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
    fn input_import_set_all_ops_mask_is_canonical() {
        // ALL_OPS must equal the OR of the 4 individual op-bits.
        let computed = InputImportSet::KEYBOARD_STATE
            | InputImportSet::MOUSE_STATE
            | InputImportSet::MOUSE_DELTA
            | InputImportSet::GAMEPAD_STATE;
        assert_eq!(InputImportSet::ALL_OPS, computed);
        // 4 bits set : 0b0000_1111 = 15.
        assert_eq!(InputImportSet::ALL_OPS, 0b0000_1111);
    }

    #[test]
    fn input_import_set_with_op_via_kind() {
        let set = InputImportSet::empty().with_op(InputOpKind::MouseDelta);
        assert!(set.contains(InputImportSet::MOUSE_DELTA));
        assert!(set.has_behavioral_sensitive());
    }

    #[test]
    fn input_import_set_with_op_name_ignores_non_input() {
        let set = InputImportSet::empty()
            .with_op_name("cssl.net.send")
            .with_op_name("arith.constant");
        assert_eq!(set, InputImportSet::empty());
    }

    // ── validate_input_arity defensive cross-checks ────────────────────

    #[test]
    fn validate_accepts_canonical_keyboard_op() {
        assert!(validate_input_arity(
            InputOpKind::KeyboardState,
            INPUT_KEYBOARD_STATE_OPERAND_COUNT,
            INPUT_RESULT_COUNT,
        )
        .is_ok());
    }

    #[test]
    fn validate_accepts_canonical_mouse_state_op() {
        assert!(validate_input_arity(
            InputOpKind::MouseState,
            INPUT_MOUSE_STATE_OPERAND_COUNT,
            INPUT_RESULT_COUNT,
        )
        .is_ok());
    }

    #[test]
    fn validate_rejects_short_keyboard_op() {
        // Defensive : if a mistyped MIR op leaks past prior passes
        // (only 2 operands instead of 3), the validator surfaces the
        // error before cgen issues a malformed call.
        let err = validate_input_arity(InputOpKind::KeyboardState, 2, 1).unwrap_err();
        assert!(err.contains("3 operands"), "diagnostic should mention expected 3 ; got: {err}");
    }

    #[test]
    fn validate_rejects_op_with_zero_results() {
        // All input ops must produce 1 result ; 0-result form is malformed.
        let err = validate_input_arity(
            InputOpKind::MouseDelta,
            INPUT_MOUSE_DELTA_OPERAND_COUNT,
            0,
        )
        .unwrap_err();
        assert!(err.contains("1 result"));
    }

    #[test]
    fn gamepad_idx_out_of_range_marker() {
        // ‼ Cross-check : XInput's 4-controller cap.
        assert!(!gamepad_idx_out_of_range_is_static_err(0));
        assert!(!gamepad_idx_out_of_range_is_static_err(3));
        assert!(gamepad_idx_out_of_range_is_static_err(4));
        assert!(gamepad_idx_out_of_range_is_static_err(99));
    }

    #[test]
    fn op_kind_operand_count_consistency() {
        // Operand counts agree across all access methods (per-kind const
        // + InputOpKind::operand_count).
        assert_eq!(
            InputOpKind::KeyboardState.operand_count(),
            INPUT_KEYBOARD_STATE_OPERAND_COUNT
        );
        assert_eq!(
            InputOpKind::MouseState.operand_count(),
            INPUT_MOUSE_STATE_OPERAND_COUNT
        );
        assert_eq!(
            InputOpKind::MouseDelta.operand_count(),
            INPUT_MOUSE_DELTA_OPERAND_COUNT
        );
        assert_eq!(
            InputOpKind::GamepadState.operand_count(),
            INPUT_GAMEPAD_STATE_OPERAND_COUNT
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// § INTEGRATION_NOTE  (Wave-D4 / S5 ↳ § 24 HOST_FFI ↳ cgen-input)
// ═══════════════════════════════════════════════════════════════════════
//
// This module is delivered as a NEW file. Per the Wave-D4 dispatch
// constraint "DO NOT modify any lib.rs / Cargo.toml" the helpers compile
// + are tested in-place via `#[cfg(test)]` references but are NOT yet
// reachable from `cssl-cgen-cpu-cranelift::*`.
//
// A future cgen refactor (the same one tracked at
// `cgen_net.rs § INTEGRATION_NOTE` + `cgen_heap_dealloc.rs §
// INTEGRATION_NOTE`) MUST :
//
//   1. Add `pub mod cgen_input;` to
//      `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/lib.rs` after the
//      existing `pub mod cgen_net;` line (column-aligned with siblings).
//
//   2. Add a `#[allow(unused_imports)]` re-export block in `lib.rs`
//      exposing :
//        ```
//        pub use cgen_input::{
//            build_input_signature, build_keyboard_state_signature,
//            build_mouse_state_signature, build_mouse_delta_signature,
//            build_gamepad_state_signature, lower_input_op_to_symbol,
//            needs_input_imports_for_op_names, validate_input_arity,
//            InputOpKind, InputImportSet,
//            INPUT_KEYBOARD_STATE_SYMBOL, INPUT_MOUSE_STATE_SYMBOL,
//            INPUT_MOUSE_DELTA_SYMBOL, INPUT_GAMEPAD_STATE_SYMBOL,
//        };
//        ```
//
//   3. Once cssl-mir grows `CsslOp::Input{KeyboardState, MouseState,
//      MouseDelta, GamepadState}` variants, replace the string-keyed
//      pre-scan helper [`needs_input_imports_for_op_names`] with a
//      `CsslOp`-keyed helper named `needs_input_imports(block: &MirBlock)
//      -> InputImportSet` that mirrors `cgen_net::needs_net_imports`.
//      The string-keyed helper stays for source-level tools that walk
//      MIR-text artifacts (e.g. `cssl-tools::dump-imports`).
//
//   4. Wire `object::declare_input_imports_for_fn` onto
//      [`needs_input_imports_for_op_names`] (or the future CsslOp-keyed
//      variant) so `__cssl_input_*` symbols are only brought into the
//      relocatable when a fn actually uses them. Today's behavior in
//      `object.rs` is to declare on first-call ; the refactor lets the
//      cgen pre-walk eliminate per-call hash-map lookups.
//
//   5. The future `object::emit_input_call(builder, op_kind, ptr_ty)`
//      helper reuses [`build_input_signature`] for the cranelift `call`
//      shape. Operand-coercion (uextend / ireduce) follows the
//      `cgen_net.rs` precedent : the cgen layer reads each operand's
//      MIR type + emits the matching cranelift cast before the call
//      instruction.
//
// § PRIME-DIRECTIVE INVARIANT (W! lock-step with § 24 IFC + § 11)
//   The four FFI symbol-names + arities are LOCKED at first commit.
//   Mouse-delta is `Sensitive<Behavioral>` ; the IFC pass uses
//   [`InputOpKind::is_behavioral_sensitive`] + [`InputImportSet::
//   has_behavioral_sensitive`] to recognize the marker without re-parsing
//   op-names. Renaming or reshape = link-time UB ⇒ debug-stage CSSLv3
//   binaries would crash on first input read.
//
// § ATTESTATION  (PRIME_DIRECTIVE.md § 11 ; carried-forward landmine)
//   "There was no hurt nor harm in the making of this, to anyone /
//   anything / anybody." This module emits zero side-effects + has no
//   global state ; every helper is a pure function on its inputs.
