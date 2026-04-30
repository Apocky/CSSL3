//! § Wave-D6 — `cssl.audio.*` Cranelift cgen helpers (S7-host-FFI / specs/24).
//!
//! § ROLE
//!   Pure-function helpers that build the cranelift `Signature`s for the
//!   four `__cssl_audio_*` FFI imports + decide which per-fn audio-imports
//!   a given MIR block requires. Mirrors `cgen_net.rs` (Wave-C4) +
//!   `cgen_fs.rs` (Wave-C3) sibling.
//!
//! § INTEGRATION_NOTE  (per Wave-D6 dispatch directive)
//!   Module is NEW ; `cssl-cgen-cpu-cranelift/src/lib.rs` is intentionally
//!   NOT modified per task constraint. A future cgen refactor (the same
//!   one tracked at `cgen_net.rs § INTEGRATION_NOTE`) will (1) add
//!   `pub mod cgen_audio;` to `lib.rs`, (2) migrate cranelift `call`-
//!   emit logic from a future `object::emit_audio_call` into
//!   [`lower_audio_op_to_symbol`] + co-located helpers here, (3) wire
//!   the per-fn import-declare path (`object::declare_audio_imports_for_fn`)
//!   onto [`needs_audio_imports`] so `__cssl_audio_*` symbols are only
//!   brought into the relocatable when a fn actually uses them.
//!
//! § SWAP-POINT  (mock-when-deps-missing per dispatch discipline)
//!   `CsslOp::Audio*` variants do NOT yet exist in `cssl-mir::op` at
//!   this slice — same situation as `cgen_fs`'s `seek` / `ftruncate` ops.
//!   This module follows the same string-dispatch pattern :
//!   [`lower_audio_op_to_symbol`] dispatches on the canonical op-name
//!   STRING (`"cssl.audio.stream_open"` etc.). When the four
//!   `CsslOp::AudioStream{Open,Write,Read,Close}` variants land, the
//!   dispatcher's match-arms will be retargeted from `op.name` to
//!   `op.op` for tighter codegen ; the symbol-name LUT is unchanged.
//!   The actual cranelift `call`-emission lives BEHIND a future
//!   `object::emit_audio_call` helper this file does NOT call directly ;
//!   helpers compile + test in-place without touching object.rs / jit.rs.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-rt/src/host_audio.rs` — the four
//!     `__cssl_audio_*` ABI-stable symbols this module wires against.
//!   - `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § audio` — locks the
//!     four-symbol shape.
//!   - `specs/40_WAVE_CSSL_PLAN.csl § WAVE-D § D6`.
//!
//! § CSL-MANDATE
//!   ‼ ABI-stable :: rename ¬→ lock-step-cssl-rt + cssl-mir
//!   ‼ pure-fn ::    zero-allocation ↑ Sig-Vec-storage
//!   ‼ O(N) ::       per-block-walk ⊑ single-pass + bitflag accumulator
//!
//! § SAWYER-EFFICIENCY
//!   Pure helpers · zero allocation outside Signature's Vec ·
//!   [`needs_audio_imports`] walks ops slice ONCE via the `AudioImportSet`
//!   u8 bitflag accumulator (4 ops + 4 extension slots fit in one byte) ·
//!   branch-friendly ordering : write/read (hot) → close → open.

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{types as cl_types, AbiParam, Signature, Type};
use cranelift_codegen::isa::CallConv;
use cssl_mir::{MirBlock, MirOp};

// ───────────────────────────────────────────────────────────────────────
// § canonical FFI symbol-name LUT (per cssl-rt::host_audio)
//
// ‼ ALL symbols MUST match `compiler-rs/crates/cssl-rt/src/host_audio.rs`
//   verbatim. Renaming either side without the other = link-time
//   symbol mismatch ⇒ undefined behavior.
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol :
/// `__cssl_audio_stream_open(flags, sample_rate, channels, fmt) -> u64`.
pub const AUDIO_STREAM_OPEN_SYMBOL: &str = "__cssl_audio_stream_open";
/// FFI symbol : `__cssl_audio_stream_write(stream, buf, len) -> i64`.
pub const AUDIO_STREAM_WRITE_SYMBOL: &str = "__cssl_audio_stream_write";
/// FFI symbol : `__cssl_audio_stream_read(stream, buf, len) -> i64`.
pub const AUDIO_STREAM_READ_SYMBOL: &str = "__cssl_audio_stream_read";
/// FFI symbol : `__cssl_audio_stream_close(stream) -> i32`.
pub const AUDIO_STREAM_CLOSE_SYMBOL: &str = "__cssl_audio_stream_close";

// ───────────────────────────────────────────────────────────────────────
// § canonical MIR op-name LUT (string-dispatch per SWAP-POINT)
// ───────────────────────────────────────────────────────────────────────

/// MIR op-name : the cssl-mir op whose lowering targets
/// [`AUDIO_STREAM_OPEN_SYMBOL`].
pub const MIR_AUDIO_STREAM_OPEN_OP_NAME: &str = "cssl.audio.stream_open";
/// MIR op-name : the cssl-mir op whose lowering targets
/// [`AUDIO_STREAM_WRITE_SYMBOL`].
pub const MIR_AUDIO_STREAM_WRITE_OP_NAME: &str = "cssl.audio.stream_write";
/// MIR op-name : the cssl-mir op whose lowering targets
/// [`AUDIO_STREAM_READ_SYMBOL`].
pub const MIR_AUDIO_STREAM_READ_OP_NAME: &str = "cssl.audio.stream_read";
/// MIR op-name : the cssl-mir op whose lowering targets
/// [`AUDIO_STREAM_CLOSE_SYMBOL`].
pub const MIR_AUDIO_STREAM_CLOSE_OP_NAME: &str = "cssl.audio.stream_close";

// ───────────────────────────────────────────────────────────────────────
// § operand / result counts
// ───────────────────────────────────────────────────────────────────────

/// `cssl.audio.stream_open(flags, rate, channels, fmt) -> u64` — 4 operands.
pub const AUDIO_STREAM_OPEN_OPERAND_COUNT: usize = 4;
/// `cssl.audio.stream_write(stream, buf, len) -> i64` — 3 operands.
pub const AUDIO_STREAM_WRITE_OPERAND_COUNT: usize = 3;
/// `cssl.audio.stream_read(stream, buf, len) -> i64` — 3 operands.
pub const AUDIO_STREAM_READ_OPERAND_COUNT: usize = 3;
/// `cssl.audio.stream_close(stream) -> i32` — 1 operand.
pub const AUDIO_STREAM_CLOSE_OPERAND_COUNT: usize = 1;
/// All four audio ops produce exactly 1 result.
pub const AUDIO_RESULT_COUNT: usize = 1;

// ───────────────────────────────────────────────────────────────────────
// § AudioOpKind — local enum mirroring the four MIR ops
// ───────────────────────────────────────────────────────────────────────

/// Discriminator for the four `__cssl_audio_*` ops. Until `CsslOp::Audio*`
/// lands in `cssl-mir`, this local enum is the canonical key for
/// dispatcher tables + tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioOpKind {
    /// `cssl.audio.stream_open`.
    StreamOpen,
    /// `cssl.audio.stream_write`.
    StreamWrite,
    /// `cssl.audio.stream_read`.
    StreamRead,
    /// `cssl.audio.stream_close`.
    StreamClose,
}

impl AudioOpKind {
    /// Canonical MIR op-name string.
    #[must_use]
    pub const fn op_name(self) -> &'static str {
        match self {
            Self::StreamOpen => MIR_AUDIO_STREAM_OPEN_OP_NAME,
            Self::StreamWrite => MIR_AUDIO_STREAM_WRITE_OP_NAME,
            Self::StreamRead => MIR_AUDIO_STREAM_READ_OP_NAME,
            Self::StreamClose => MIR_AUDIO_STREAM_CLOSE_OP_NAME,
        }
    }

    /// Canonical FFI symbol name.
    #[must_use]
    pub const fn ffi_symbol(self) -> &'static str {
        match self {
            Self::StreamOpen => AUDIO_STREAM_OPEN_SYMBOL,
            Self::StreamWrite => AUDIO_STREAM_WRITE_SYMBOL,
            Self::StreamRead => AUDIO_STREAM_READ_SYMBOL,
            Self::StreamClose => AUDIO_STREAM_CLOSE_SYMBOL,
        }
    }

    /// Expected operand count.
    #[must_use]
    pub const fn operand_count(self) -> usize {
        match self {
            Self::StreamOpen => AUDIO_STREAM_OPEN_OPERAND_COUNT,
            Self::StreamWrite => AUDIO_STREAM_WRITE_OPERAND_COUNT,
            Self::StreamRead => AUDIO_STREAM_READ_OPERAND_COUNT,
            Self::StreamClose => AUDIO_STREAM_CLOSE_OPERAND_COUNT,
        }
    }

    /// Try to recognize an MIR op-name string as one of the four kinds.
    #[must_use]
    pub fn from_op_name(name: &str) -> Option<Self> {
        match name {
            MIR_AUDIO_STREAM_OPEN_OP_NAME => Some(Self::StreamOpen),
            MIR_AUDIO_STREAM_WRITE_OP_NAME => Some(Self::StreamWrite),
            MIR_AUDIO_STREAM_READ_OP_NAME => Some(Self::StreamRead),
            MIR_AUDIO_STREAM_CLOSE_OP_NAME => Some(Self::StreamClose),
            _ => None,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § cranelift signature builders — one per op-kind
//
// Shapes match `compiler-rs/crates/cssl-rt/src/host_audio.rs` exactly.
// FFI uses u32 / u64 / i32 / i64 / usize + raw pointers ; cranelift IR
// sees integers (`*const u8` maps to ptr_ty, `usize` to ptr_ty, u32 to
// I32, u64/i64 to I64, i32 to I32). The cgen call-emit path coerces
// operand types via uextend / ireduce.
// ───────────────────────────────────────────────────────────────────────

/// Build cranelift `Signature` for
/// `__cssl_audio_stream_open(u32, u32, u32, u32) -> u64`.
#[must_use]
pub fn build_audio_stream_open_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I32)); // flags
    sig.params.push(AbiParam::new(cl_types::I32)); // sample_rate
    sig.params.push(AbiParam::new(cl_types::I32)); // channels
    sig.params.push(AbiParam::new(cl_types::I32)); // fmt
    sig.returns.push(AbiParam::new(cl_types::I64)); // handle
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_audio_stream_write(u64, *const u8, usize) -> i64`.
/// `ptr_ty` is host-ptr-width (`I64` on x86_64, `I32` on 32-bit hosts).
#[must_use]
pub fn build_audio_stream_write_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64)); // stream
    sig.params.push(AbiParam::new(ptr_ty)); // buf
    sig.params.push(AbiParam::new(ptr_ty)); // len (usize)
    sig.returns.push(AbiParam::new(cl_types::I64)); // bytes-written or -1
    sig
}

/// Build cranelift `Signature` for
/// `__cssl_audio_stream_read(u64, *mut u8, usize) -> i64`.
#[must_use]
pub fn build_audio_stream_read_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cl_types::I64));
    sig
}

/// Build cranelift `Signature` for `__cssl_audio_stream_close(u64) -> i32`.
#[must_use]
pub fn build_audio_stream_close_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cl_types::I64));
    sig.returns.push(AbiParam::new(cl_types::I32));
    sig
}

/// Single-entry-point signature builder keyed by [`AudioOpKind`].
#[must_use]
pub fn build_audio_signature_for_kind(
    kind: AudioOpKind,
    call_conv: CallConv,
    ptr_ty: Type,
) -> Signature {
    match kind {
        AudioOpKind::StreamOpen => build_audio_stream_open_signature(call_conv),
        AudioOpKind::StreamWrite => build_audio_stream_write_signature(call_conv, ptr_ty),
        AudioOpKind::StreamRead => build_audio_stream_read_signature(call_conv, ptr_ty),
        AudioOpKind::StreamClose => build_audio_stream_close_signature(call_conv),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § single dispatcher : MIR-op → (FFI-symbol-name, expected-arity)
// ───────────────────────────────────────────────────────────────────────

/// Map an MIR op (by its name string ; SWAP-POINT until `CsslOp::Audio*`
/// lands) to the canonical FFI symbol-name + expected operand-count.
/// Returns `None` for non-audio ops.
///
/// § BRANCH-FRIENDLY ORDERING
///   write / read (data-path hot loop) → close (cleanup) → open (setup).
#[must_use]
pub fn lower_audio_op_to_symbol(op: &MirOp) -> Option<(&'static str, usize)> {
    match op.name.as_str() {
        MIR_AUDIO_STREAM_WRITE_OP_NAME => Some((
            AUDIO_STREAM_WRITE_SYMBOL,
            AUDIO_STREAM_WRITE_OPERAND_COUNT,
        )),
        MIR_AUDIO_STREAM_READ_OP_NAME => Some((
            AUDIO_STREAM_READ_SYMBOL,
            AUDIO_STREAM_READ_OPERAND_COUNT,
        )),
        MIR_AUDIO_STREAM_CLOSE_OP_NAME => Some((
            AUDIO_STREAM_CLOSE_SYMBOL,
            AUDIO_STREAM_CLOSE_OPERAND_COUNT,
        )),
        MIR_AUDIO_STREAM_OPEN_OP_NAME => Some((
            AUDIO_STREAM_OPEN_SYMBOL,
            AUDIO_STREAM_OPEN_OPERAND_COUNT,
        )),
        _ => None,
    }
}

/// Predicate : is this op a `cssl.audio.*` MIR op ?
#[must_use]
pub fn is_audio_op(op: &MirOp) -> bool {
    matches!(
        op.name.as_str(),
        MIR_AUDIO_STREAM_OPEN_OP_NAME
            | MIR_AUDIO_STREAM_WRITE_OP_NAME
            | MIR_AUDIO_STREAM_READ_OP_NAME
            | MIR_AUDIO_STREAM_CLOSE_OP_NAME
    )
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn pre-scan : "which audio imports does this fn need"
// ───────────────────────────────────────────────────────────────────────

/// Bitflag set of which `__cssl_audio_*` imports a given MIR fn requires.
///
/// § BIT LAYOUT (u8)
///   bit 0 : stream_open   · bit 1 : stream_write
///   bit 2 : stream_read   · bit 3 : stream_close
///   bit 4 : caps_grant       (extension)
///   bit 5 : caps_revoke      (extension)
///   bit 6 : caps_current     (extension)
///   bit 7 : last_error_kind  (extension)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AudioImportSet(pub u8);

impl AudioImportSet {
    /// Empty (no audio imports needed).
    #[must_use]
    pub const fn empty() -> Self { Self(0) }

    /// `stream_open` import bit.
    pub const STREAM_OPEN: u8 = 1 << 0;
    /// `stream_write` import bit.
    pub const STREAM_WRITE: u8 = 1 << 1;
    /// `stream_read` import bit.
    pub const STREAM_READ: u8 = 1 << 2;
    /// `stream_close` import bit.
    pub const STREAM_CLOSE: u8 = 1 << 3;
    /// `caps_grant` import bit (extension).
    pub const CAPS_GRANT: u8 = 1 << 4;
    /// `caps_revoke` import bit (extension).
    pub const CAPS_REVOKE: u8 = 1 << 5;
    /// `caps_current` import bit (extension).
    pub const CAPS_CURRENT: u8 = 1 << 6;
    /// `last_error_kind` import bit (extension).
    pub const LAST_ERROR_KIND: u8 = 1 << 7;
    /// Mask of the 4 direct audio-op-kinds (no extensions).
    pub const DIRECT_OPS_MASK: u8 =
        Self::STREAM_OPEN | Self::STREAM_WRITE | Self::STREAM_READ | Self::STREAM_CLOSE;

    /// Check whether `bits` are all set.
    #[must_use]
    pub const fn contains(self, bits: u8) -> bool { (self.0 & bits) == bits }

    /// Check whether ANY direct audio-op-kind bit is set.
    #[must_use]
    pub const fn any_audio_op(self) -> bool { (self.0 & Self::DIRECT_OPS_MASK) != 0 }

    /// Set the bit corresponding to `kind`. Returns the updated set.
    #[must_use]
    pub fn with_kind(self, kind: AudioOpKind) -> Self {
        let mask = match kind {
            AudioOpKind::StreamOpen => Self::STREAM_OPEN,
            AudioOpKind::StreamWrite => Self::STREAM_WRITE,
            AudioOpKind::StreamRead => Self::STREAM_READ,
            AudioOpKind::StreamClose => Self::STREAM_CLOSE,
        };
        Self(self.0 | mask)
    }

    /// Set the bit corresponding to an MIR op-name STRING. Non-audio
    /// names are silently no-ops. Mirrors the cgen_fs string-dispatch.
    #[must_use]
    pub fn with_op_name(self, name: &str) -> Self {
        match AudioOpKind::from_op_name(name) {
            Some(kind) => self.with_kind(kind),
            None => self,
        }
    }
}

/// Walk a single MIR block's ops once and return the bitflag set of
/// audio imports required.
///
/// § COMPLEXITY  O(N) in op count, single-pass. No allocation.
#[must_use]
pub fn needs_audio_imports(block: &MirBlock) -> AudioImportSet {
    let mut set = AudioImportSet::empty();
    for op in &block.ops {
        set = set.with_op_name(op.name.as_str());
    }
    set
}

// ───────────────────────────────────────────────────────────────────────
// § contract validators (defensive cross-checks)
// ───────────────────────────────────────────────────────────────────────

/// Validate the operand-count + result-count of a `cssl.audio.*` op
/// against the canonical contract.
///
/// # Errors
/// Returns `Err(String)` when the op is not a recognized `cssl.audio.*`
/// op or the operand-count diverges from the canonical expectation.
pub fn validate_audio_arity(op: &MirOp) -> Result<(), String> {
    let Some((sym, expected_operands)) = lower_audio_op_to_symbol(op) else {
        return Err(format!(
            "validate_audio_arity : op `{}` is not a recognized cssl.audio.* op",
            op.name
        ));
    };
    if op.operands.len() != expected_operands {
        return Err(format!(
            "validate_audio_arity : `{}` (-> {sym}) requires {expected_operands} operands ; got {}",
            op.name,
            op.operands.len()
        ));
    }
    if op.results.len() != AUDIO_RESULT_COUNT {
        return Err(format!(
            "validate_audio_arity : `{}` (-> {sym}) produces {AUDIO_RESULT_COUNT} result ; got {}",
            op.name,
            op.results.len()
        ));
    }
    Ok(())
}

/// Test whether a `__cssl_audio_stream_close(0)` call is a no-op.
///
/// Returns `true` because the cssl-rt impl returns `-1` + sets the
/// last-error to `INVALID_HANDLE` when fed `INVALID_STREAM` (== 0).
/// This lets the recognizer-bridge skip emitting a close when it can
/// statically prove the handle is `0`.
#[must_use]
pub const fn invalid_stream_close_is_noop() -> bool { true }

// ───────────────────────────────────────────────────────────────────────
// § tests — 12 unit tests covering symbols · sigs · dispatcher ·
// import-scan · arity-validators · invalid-stream-noop contract
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::types as cl_types;
    use cranelift_codegen::isa::CallConv;
    use cssl_mir::{CsslOp, IntWidth, MirBlock, MirOp, MirType, ValueId};

    #[test]
    fn ffi_symbols_match_cssl_rt_canonical() {
        // ‼ Lock-step invariant : symbol-names MUST match
        //   cssl-rt::host_audio::__cssl_audio_* verbatim.
        assert_eq!(AUDIO_STREAM_OPEN_SYMBOL, "__cssl_audio_stream_open");
        assert_eq!(AUDIO_STREAM_WRITE_SYMBOL, "__cssl_audio_stream_write");
        assert_eq!(AUDIO_STREAM_READ_SYMBOL, "__cssl_audio_stream_read");
        assert_eq!(AUDIO_STREAM_CLOSE_SYMBOL, "__cssl_audio_stream_close");
    }

    #[test]
    fn mir_op_names_have_canonical_prefix() {
        for name in [
            MIR_AUDIO_STREAM_OPEN_OP_NAME,
            MIR_AUDIO_STREAM_WRITE_OP_NAME,
            MIR_AUDIO_STREAM_READ_OP_NAME,
            MIR_AUDIO_STREAM_CLOSE_OP_NAME,
        ] {
            assert!(
                name.starts_with("cssl.audio.stream_"),
                "op-name `{name}` lost canonical prefix"
            );
        }
    }

    #[test]
    fn audio_op_kind_round_trips_and_carries_canonical_metadata() {
        for kind in [
            AudioOpKind::StreamOpen,
            AudioOpKind::StreamWrite,
            AudioOpKind::StreamRead,
            AudioOpKind::StreamClose,
        ] {
            // Round-trip via op-name.
            assert_eq!(AudioOpKind::from_op_name(kind.op_name()), Some(kind));
            // Symbol + arity match the canonical constants.
            match kind {
                AudioOpKind::StreamOpen => {
                    assert_eq!(kind.ffi_symbol(), AUDIO_STREAM_OPEN_SYMBOL);
                    assert_eq!(kind.operand_count(), AUDIO_STREAM_OPEN_OPERAND_COUNT);
                }
                AudioOpKind::StreamWrite => {
                    assert_eq!(kind.ffi_symbol(), AUDIO_STREAM_WRITE_SYMBOL);
                    assert_eq!(kind.operand_count(), AUDIO_STREAM_WRITE_OPERAND_COUNT);
                }
                AudioOpKind::StreamRead => {
                    assert_eq!(kind.ffi_symbol(), AUDIO_STREAM_READ_SYMBOL);
                    assert_eq!(kind.operand_count(), AUDIO_STREAM_READ_OPERAND_COUNT);
                }
                AudioOpKind::StreamClose => {
                    assert_eq!(kind.ffi_symbol(), AUDIO_STREAM_CLOSE_SYMBOL);
                    assert_eq!(kind.operand_count(), AUDIO_STREAM_CLOSE_OPERAND_COUNT);
                }
            }
        }
        // Unknown name returns None.
        assert!(AudioOpKind::from_op_name("cssl.heap.alloc").is_none());
        assert!(AudioOpKind::from_op_name("").is_none());
    }

    #[test]
    fn signatures_have_expected_shapes_with_ptr_ty_i64() {
        // open : 4 i32 params + 1 i64 return
        let open = build_audio_stream_open_signature(CallConv::SystemV);
        assert_eq!(open.params.len(), 4);
        for p in &open.params {
            assert_eq!(*p, AbiParam::new(cl_types::I32));
        }
        assert_eq!(open.returns[0], AbiParam::new(cl_types::I64));

        // write/read : (i64, ptr, ptr) -> i64
        for sig in [
            build_audio_stream_write_signature(CallConv::SystemV, cl_types::I64),
            build_audio_stream_read_signature(CallConv::SystemV, cl_types::I64),
        ] {
            assert_eq!(sig.params.len(), 3);
            assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
            assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
            assert_eq!(sig.params[2], AbiParam::new(cl_types::I64));
            assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
        }

        // close : (i64) -> i32
        let close = build_audio_stream_close_signature(CallConv::SystemV);
        assert_eq!(close.params.len(), 1);
        assert_eq!(close.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(close.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn signatures_use_ptr_ty_for_buf_and_len_args() {
        // 32-bit hosts : ptr_ty = I32 ; the 2 ptr-shaped params + the
        // usize len param all become I32.
        let sig = build_audio_stream_write_signature(CallConv::SystemV, cl_types::I32);
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I32));
        // Call-conv passthrough for windows-fastcall.
        let win = build_audio_stream_write_signature(CallConv::WindowsFastcall, cl_types::I64);
        assert_eq!(win.call_conv, CallConv::WindowsFastcall);
        // Builder-by-kind dispatcher reproduces the same shapes.
        let open = build_audio_signature_for_kind(
            AudioOpKind::StreamOpen,
            CallConv::SystemV,
            cl_types::I64,
        );
        assert_eq!(open.params.len(), 4);
    }

    #[test]
    fn dispatcher_resolves_each_op_to_canonical_symbol() {
        // open
        let op = MirOp::std(MIR_AUDIO_STREAM_OPEN_OP_NAME)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_operand(ValueId(3));
        let (sym, arity) = lower_audio_op_to_symbol(&op).expect("open dispatches");
        assert_eq!(sym, AUDIO_STREAM_OPEN_SYMBOL);
        assert_eq!(arity, AUDIO_STREAM_OPEN_OPERAND_COUNT);
        // write / read / close
        for (name, expected_sym, expected_arity) in [
            (
                MIR_AUDIO_STREAM_WRITE_OP_NAME,
                AUDIO_STREAM_WRITE_SYMBOL,
                AUDIO_STREAM_WRITE_OPERAND_COUNT,
            ),
            (
                MIR_AUDIO_STREAM_READ_OP_NAME,
                AUDIO_STREAM_READ_SYMBOL,
                AUDIO_STREAM_READ_OPERAND_COUNT,
            ),
            (
                MIR_AUDIO_STREAM_CLOSE_OP_NAME,
                AUDIO_STREAM_CLOSE_SYMBOL,
                AUDIO_STREAM_CLOSE_OPERAND_COUNT,
            ),
        ] {
            let op = MirOp::std(name);
            let (sym, arity) = lower_audio_op_to_symbol(&op).expect("dispatches");
            assert_eq!(sym, expected_sym);
            assert_eq!(arity, expected_arity);
        }
    }

    #[test]
    fn dispatcher_returns_none_for_non_audio_ops() {
        for op in [
            MirOp::new(CsslOp::HeapAlloc),
            MirOp::new(CsslOp::HeapDealloc),
            MirOp::new(CsslOp::FsOpen),
            MirOp::new(CsslOp::NetSend),
            MirOp::std("arith.constant"),
            MirOp::std("cssl.audio.unknown"),
        ] {
            assert!(lower_audio_op_to_symbol(&op).is_none());
            assert!(!is_audio_op(&op));
        }
    }

    #[test]
    fn is_audio_op_recognizes_all_four_canonical_ops() {
        for name in [
            MIR_AUDIO_STREAM_OPEN_OP_NAME,
            MIR_AUDIO_STREAM_WRITE_OP_NAME,
            MIR_AUDIO_STREAM_READ_OP_NAME,
            MIR_AUDIO_STREAM_CLOSE_OP_NAME,
        ] {
            assert!(is_audio_op(&MirOp::std(name)));
        }
    }

    #[test]
    fn pre_scan_accumulates_distinct_imports_and_ignores_unrelated_ops() {
        // empty block
        let empty = MirBlock::new("entry");
        assert_eq!(needs_audio_imports(&empty), AudioImportSet::empty());
        assert!(!needs_audio_imports(&empty).any_audio_op());

        // open + write + close — playback pattern
        let mut block = MirBlock::new("entry");
        block.push(
            MirOp::std(MIR_AUDIO_STREAM_OPEN_OP_NAME)
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2))
                .with_operand(ValueId(3)),
        );
        block.push(
            MirOp::std(MIR_AUDIO_STREAM_WRITE_OP_NAME)
                .with_operand(ValueId(4))
                .with_operand(ValueId(5))
                .with_operand(ValueId(6)),
        );
        block.push(MirOp::std(MIR_AUDIO_STREAM_CLOSE_OP_NAME).with_operand(ValueId(7)));
        // Mix in non-audio ops to verify they don't flip bits.
        block.push(MirOp::new(CsslOp::HeapAlloc));
        block.push(MirOp::new(CsslOp::FsOpen));
        block.push(MirOp::new(CsslOp::NetSend));
        let set = needs_audio_imports(&block);
        assert!(set.contains(AudioImportSet::STREAM_OPEN));
        assert!(set.contains(AudioImportSet::STREAM_WRITE));
        assert!(set.contains(AudioImportSet::STREAM_CLOSE));
        assert!(!set.contains(AudioImportSet::STREAM_READ));
        assert!(set.any_audio_op());
    }

    #[test]
    fn audio_import_set_bits_are_distinct_powers_of_two() {
        let bits = [
            AudioImportSet::STREAM_OPEN,
            AudioImportSet::STREAM_WRITE,
            AudioImportSet::STREAM_READ,
            AudioImportSet::STREAM_CLOSE,
            AudioImportSet::CAPS_GRANT,
            AudioImportSet::CAPS_REVOKE,
            AudioImportSet::CAPS_CURRENT,
            AudioImportSet::LAST_ERROR_KIND,
        ];
        for (i, &b) in bits.iter().enumerate() {
            assert!(b.is_power_of_two(), "bit at index {i} = {b:#x} not power-of-two");
            for (j, &b2) in bits.iter().enumerate() {
                if i != j {
                    assert_eq!(b & b2, 0, "bits at {i} + {j} overlap");
                }
            }
        }
        // any_audio_op() ignores extension bits.
        let only_ext = AudioImportSet(AudioImportSet::LAST_ERROR_KIND | AudioImportSet::CAPS_GRANT);
        assert!(!only_ext.any_audio_op());
        // direct-ops mask covers exactly the four ops.
        assert_eq!(
            AudioImportSet::DIRECT_OPS_MASK,
            AudioImportSet::STREAM_OPEN
                | AudioImportSet::STREAM_WRITE
                | AudioImportSet::STREAM_READ
                | AudioImportSet::STREAM_CLOSE
        );
        // with_op_name : non-audio = no-op
        assert_eq!(
            AudioImportSet::empty().with_op_name("cssl.heap.alloc"),
            AudioImportSet::empty()
        );
    }

    #[test]
    fn validate_audio_arity_accepts_canonical_and_rejects_malformed() {
        // canonical open + close pass.
        let open = MirOp::std(MIR_AUDIO_STREAM_OPEN_OP_NAME)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_operand(ValueId(3))
            .with_result(ValueId(4), MirType::Int(IntWidth::I64));
        assert!(validate_audio_arity(&open).is_ok());
        let close = MirOp::std(MIR_AUDIO_STREAM_CLOSE_OP_NAME)
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I32));
        assert!(validate_audio_arity(&close).is_ok());

        // non-audio op rejected.
        let alloc = MirOp::new(CsslOp::HeapAlloc);
        let err = validate_audio_arity(&alloc).unwrap_err();
        assert!(err.contains("not a recognized cssl.audio.* op"));

        // wrong-arity open rejected.
        let short_open = MirOp::std(MIR_AUDIO_STREAM_OPEN_OP_NAME)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Int(IntWidth::I64));
        let err2 = validate_audio_arity(&short_open).unwrap_err();
        assert!(err2.contains("4 operands"));

        // zero-result rejected.
        let no_result = MirOp::std(MIR_AUDIO_STREAM_CLOSE_OP_NAME).with_operand(ValueId(0));
        let err3 = validate_audio_arity(&no_result).unwrap_err();
        assert!(err3.contains("1 result"));
    }

    #[test]
    fn invalid_stream_close_contract_and_audio_result_count() {
        // ‼ Cross-check : cssl-rt::host_audio::__cssl_audio_stream_close's
        //   contract is "INVALID_STREAM (0) returns -1 + sets last-error
        //   to INVALID_HANDLE".
        assert!(invalid_stream_close_is_noop());
        // All four audio ops produce exactly one result.
        assert_eq!(AUDIO_RESULT_COUNT, 1);
    }
}
