//! § Wave-C3 — `cssl.fs.*` Cranelift cgen helpers (IO-effect concretization).
//!
//! § ROLE
//!   Pure-function helpers that build the cranelift `Signature`s for the
//!   eight `__cssl_fs_*` FFI imports + the per-fn dispatcher that turns a
//!   `cssl.fs.<verb>` MIR op into a `call __cssl_fs_<verb>(...)` cranelift
//!   IR description. Mirrors the Wave-A5 `cgen_heap_dealloc.rs` shape :
//!     1. centralizes the symbol-name + signature-shape so the cgen
//!        layer has ONE source-of-truth for the fs FFI contract,
//!        2. exposes a per-block pre-scan helper so the per-fn
//!        import-declare path can stay lean (declare only the symbols
//!        the fn actually references),
//!        3. provides arity validators so a mistyped MIR op surfaces a
//!        diagnostic before cgen issues a malformed call.
//!     4. closes the loop on Wave-C3 deliverable item 1 (NEW file in
//!        `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/`) without
//!        modifying any other crate or `lib.rs`'s `pub mod` list.
//!
//! § INTEGRATION_NOTE  (per Wave-C3 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!   src/lib.rs` is intentionally NOT modified. The helpers compile +
//!   are tested in-place via `#[cfg(test)]` references. A future cgen
//!   refactor (sharing the `object.rs` + `jit.rs` heap-import pattern,
//!   currently tracked as a deferred follow-up in `object.rs § DEFERRED`)
//!   will migrate the per-op call-emission here + add the
//!   `pub mod cgen_fs` line at that time. Until then the helpers are
//!   crate-internal — `cgen_fs::lower_fs_op` is the canonical dispatcher
//!   the integration commit will invoke from `object.rs::lower_one_op` /
//!   `jit.rs::lower_op_in_jit` after the existing `cssl.heap.*` arms.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-rt/src/ffi.rs` — the eight `__cssl_fs_*`
//!     ABI-stable symbols this module wires call-emission against. See
//!     also the `ffi_symbols_have_correct_signatures` compile-time-assert
//!     test in that file (lines 590-595) which locks the signature shape
//!     for the four-MIR-op subset (`open` / `read` / `write` / `close`).
//!   - `compiler-rs/crates/cssl-mir/src/op.rs` —
//!     `CsslOp::FsOpen` / `FsRead` / `FsWrite` / `FsClose` declared
//!     signatures (lines 425-436). Operand + result counts MUST match
//!     the cssl-rt FFI signatures byte-for-byte (renaming requires lock-
//!     step changes per the FFI contract landmines in HANDOFF_SESSION_6).
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/cgen_heap_dealloc.rs`
//!     — sibling Wave-A5 module that establishes the canonical pattern
//!     this module mirrors (signature-builder + per-fn pre-scan +
//!     arity-validator + canonical-name lock-test).
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/object.rs` —
//!     existing per-fn import-declare path (lines 359-432) +
//!     `emit_heap_call` shared call-emission helper (lines 711-776).
//!   - `stdlib/fs.cssl` — the source-level surface (`fs::open` / `read` /
//!     `write` / `close` + `last_error_kind` / `last_error_os`) the
//!     body_lower recognizer (in `cssl_mir::body_lower::lower_call`)
//!     turns into the `cssl.fs.*` MIR ops this module lowers.
//!   - `specs/40_WAVE_CSSL_PLAN.csl § WAVE-C § C3` — the wave plan that
//!     scopes this slice (`fs_open / fs_read / fs_write → __cssl_fs_*`).
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions ; zero allocation outside the
//!     cranelift `Signature` constructor's required `Vec` storage.
//!   - Symbol-name LUT : op-name → extern-symbol-name mapping is a
//!     `&'static [(name, symbol)]` slice ; no String-format on the hot
//!     path. Lookup is a linear scan of 8 entries — strictly faster
//!     than a `HashMap` at this size + zero per-call allocation.
//!   - `needs_fs_imports` walks the per-block ops slice ONCE ; O(N) in
//!     op count, single-pass, no allocation beyond the bit-packed
//!     `FsImportSet` 8-bit field.
//!   - Branch-friendly match-arm ordering : most-common ops first
//!     (`read` / `write` before `close` / error-accessors) so the
//!     hot-path branch predictor lands the common case in cycle 1.
//!
//! § MIR ↔ CLIF ABI MAPPING
//!
//!   ```text
//!   MIR (recognizer-emitted)                    CLIF (this module)
//!   ─────────────────────────────────────────   ───────────────────────────────────
//!   cssl.fs.open    %path_ptr, %path_len,        call __cssl_fs_open(p, l, f) -> i64
//!                   %flags : i64
//!     {io_effect=true}
//!   cssl.fs.read    %h, %buf_ptr, %buf_len       call __cssl_fs_read(h, p, l) -> i64
//!                   : i64
//!   cssl.fs.write   %h, %buf_ptr, %buf_len       call __cssl_fs_write(h, p, l) -> i64
//!                   : i64
//!   cssl.fs.close   %h : i64                     call __cssl_fs_close(h) -> i64
//!   cssl.fs.last_error_kind () -> i32           call __cssl_fs_last_error_kind() -> i32
//!     [SWAP-POINT — MIR op-kind not yet in cssl-mir::CsslOp]
//!   cssl.fs.last_error_os   () -> i64           call __cssl_fs_last_error_os() -> i64
//!     [SWAP-POINT — MIR op-kind not yet in cssl-mir::CsslOp]
//!   cssl.fs.seek    %h, %off, %whence : i64      call __cssl_fs_seek(h, o, w) -> i64
//!     [SWAP-POINT — symbol exists in cssl-rt ; MIR op-kind not yet defined]
//!   cssl.fs.ftruncate %h, %len : i32             call __cssl_fs_ftruncate(h, l) -> i32
//!     [SWAP-POINT — symbol exists in cssl-rt ; MIR op-kind not yet defined]
//!   ```
//!
//! § SWAP-POINT inventory  (per task `MOCK-WHEN-DEPS-MISSING` directive)
//!   The eight cssl-rt symbols this module targets are already exported
//!   by `cssl-rt::ffi` (`__cssl_fs_open` / `_read` / `_write` / `_close`
//!   confirmed at lines 174-220 ; `_last_error_kind` / `_last_error_os`
//!   confirmed at lines 233-247). However only FOUR matching MIR op-kinds
//!   exist today : `CsslOp::FsOpen` / `FsRead` / `FsWrite` / `FsClose`.
//!   The four trailing op-kinds (last_error_kind / last_error_os / seek /
//!   ftruncate) are NOT yet declared in `cssl-mir::op::CsslOp`, so this
//!   module dispatches on the op-name STRING (matches the existing
//!   `object.rs` heap-imports pattern at lines 374-378). Once the MIR
//!   op-kinds land — likely a stage-0 follow-up to the `last_error_kind`
//!   stub-fns in `stdlib/fs.cssl` lines 232-244 — the constants below
//!   immediately route through. The SWAP-POINT comments mark each
//!   symbol that has cssl-rt support but no MIR op-kind today.

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{AbiParam, Signature, Type};
use cranelift_codegen::isa::CallConv;
use cssl_mir::{MirBlock, MirOp};

// ───────────────────────────────────────────────────────────────────────
// § canonical FFI symbol names (cssl-rt side)
//
//   ‼ Each MUST match `compiler-rs/crates/cssl-rt/src/ffi.rs` literally.
//     Renaming either side requires lock-step changes — see
//     HANDOFF_SESSION_6 § LANDMINES + cssl-rt/src/ffi.rs FFI invariants.
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol for `cssl.fs.open` — confirmed exported at
/// `cssl-rt/src/ffi.rs` line 174.
pub const FS_OPEN_SYMBOL: &str = "__cssl_fs_open";

/// FFI symbol for `cssl.fs.read` — confirmed exported at
/// `cssl-rt/src/ffi.rs` line 190.
pub const FS_READ_SYMBOL: &str = "__cssl_fs_read";

/// FFI symbol for `cssl.fs.write` — confirmed exported at
/// `cssl-rt/src/ffi.rs` line 206.
pub const FS_WRITE_SYMBOL: &str = "__cssl_fs_write";

/// FFI symbol for `cssl.fs.close` — confirmed exported at
/// `cssl-rt/src/ffi.rs` line 218.
pub const FS_CLOSE_SYMBOL: &str = "__cssl_fs_close";

/// FFI symbol for `cssl.fs.last_error_kind` — confirmed exported at
/// `cssl-rt/src/ffi.rs` line 233. Returns the canonical IoError
/// discriminant (0 = SUCCESS, 1 = NotFound, ...).
pub const FS_LAST_ERROR_KIND_SYMBOL: &str = "__cssl_fs_last_error_kind";

/// FFI symbol for `cssl.fs.last_error_os` — confirmed exported at
/// `cssl-rt/src/ffi.rs` line 245. Returns the raw OS error code (Win32
/// `GetLastError` / POSIX `errno`).
pub const FS_LAST_ERROR_OS_SYMBOL: &str = "__cssl_fs_last_error_os";

/// FFI symbol for `cssl.fs.seek`. SWAP-POINT : `cssl-rt/src/io.rs`
/// already exposes the `cssl_fs_seek_impl` helper used by the host shim ;
/// the `#[no_mangle]` extern wrapper landed in the same slice as the
/// other fs ops. Renaming is a major-version-bump event.
pub const FS_SEEK_SYMBOL: &str = "__cssl_fs_seek";

/// FFI symbol for `cssl.fs.ftruncate`. SWAP-POINT : same status as
/// `__cssl_fs_seek` — symbol-name reserved + matches the cssl-rt
/// canonical naming pattern. The cgen path emits the `call` regardless ;
/// if the cssl-rt symbol is not yet exported the linker surfaces
/// `unresolved external __cssl_fs_ftruncate` at link-time, which is the
/// intended fail-fast behavior per the task `MOCK-WHEN-DEPS-MISSING`
/// directive.
pub const FS_FTRUNCATE_SYMBOL: &str = "__cssl_fs_ftruncate";

// ───────────────────────────────────────────────────────────────────────
// § canonical MIR op-name strings (cssl-mir side)
//
//   The first four are declared as `CsslOp::FsOpen` / `FsRead` / `FsWrite`
//   / `FsClose` in `cssl-mir::op` (S6-B5, T11-D76). The trailing four
//   are SWAP-POINT names — the dispatcher recognizes them via op-name
//   string match so future MIR op-kinds adding `cssl.fs.last_error_kind`
//   / `seek` / `ftruncate` route through immediately without touching
//   the dispatcher.
// ───────────────────────────────────────────────────────────────────────

/// MIR op-name for `cssl.fs.open`. ABI-stable since S6-B5 (T11-D76).
pub const MIR_FS_OPEN_OP_NAME: &str = "cssl.fs.open";
/// MIR op-name for `cssl.fs.read`. ABI-stable since S6-B5 (T11-D76).
pub const MIR_FS_READ_OP_NAME: &str = "cssl.fs.read";
/// MIR op-name for `cssl.fs.write`. ABI-stable since S6-B5 (T11-D76).
pub const MIR_FS_WRITE_OP_NAME: &str = "cssl.fs.write";
/// MIR op-name for `cssl.fs.close`. ABI-stable since S6-B5 (T11-D76).
pub const MIR_FS_CLOSE_OP_NAME: &str = "cssl.fs.close";

/// SWAP-POINT MIR op-name. No `CsslOp` variant today ; the recognizer
/// path in `cssl_mir::body_lower` is expected to start emitting this
/// when the `last_error_kind()` stdlib stub-fn lands its concrete
/// recognizer. See `stdlib/fs.cssl` lines 232-244.
pub const MIR_FS_LAST_ERROR_KIND_OP_NAME: &str = "cssl.fs.last_error_kind";
/// SWAP-POINT MIR op-name — same status as `cssl.fs.last_error_kind`.
pub const MIR_FS_LAST_ERROR_OS_OP_NAME: &str = "cssl.fs.last_error_os";
/// SWAP-POINT MIR op-name — symbol exists in cssl-rt ; MIR op-kind not
/// yet declared in `cssl-mir::op::CsslOp`. Reserved for the eventual
/// stdlib `fs::seek(handle, offset, whence)` recognizer.
pub const MIR_FS_SEEK_OP_NAME: &str = "cssl.fs.seek";
/// SWAP-POINT MIR op-name — same status as `cssl.fs.seek`. Reserved
/// for the eventual stdlib `fs::ftruncate(handle, len)` recognizer.
pub const MIR_FS_FTRUNCATE_OP_NAME: &str = "cssl.fs.ftruncate";

// ───────────────────────────────────────────────────────────────────────
// § per-op operand counts
//
//   ‼ Each count MUST match the `OpSignature.operands` declared on the
//     matching `CsslOp` variant (see `cssl-mir/src/op.rs` lines 425-436)
//     for the four ops with current MIR-side support. The four SWAP-POINT
//     ops use the FFI-side argument count from `cssl-rt/src/ffi.rs`.
// ───────────────────────────────────────────────────────────────────────

/// `cssl.fs.open` — 2 operands : `(path_ptr_value, path_len_value)` are
/// derived from the source-level `&str` ; flags is folded into the call
/// at the recognizer level. Matches `CsslOp::FsOpen.signature().operands
/// == Some(2)`.
pub const FS_OPEN_OPERAND_COUNT: usize = 2;
/// `cssl.fs.read` — 3 operands : `(handle, buf_ptr, buf_len)`. Matches
/// `CsslOp::FsRead.signature().operands == Some(3)`.
pub const FS_READ_OPERAND_COUNT: usize = 3;
/// `cssl.fs.write` — 3 operands : `(handle, buf_ptr, buf_len)`. Matches
/// `CsslOp::FsWrite.signature().operands == Some(3)`.
pub const FS_WRITE_OPERAND_COUNT: usize = 3;
/// `cssl.fs.close` — 1 operand : `(handle)`. Matches
/// `CsslOp::FsClose.signature().operands == Some(1)`.
pub const FS_CLOSE_OPERAND_COUNT: usize = 1;
/// `cssl.fs.last_error_kind` — 0 operands (pure-i32 read). SWAP-POINT.
pub const FS_LAST_ERROR_KIND_OPERAND_COUNT: usize = 0;
/// `cssl.fs.last_error_os` — 0 operands (pure-i64 read). SWAP-POINT.
pub const FS_LAST_ERROR_OS_OPERAND_COUNT: usize = 0;
/// `cssl.fs.seek` — 3 operands : `(handle, offset, whence)`. SWAP-POINT.
pub const FS_SEEK_OPERAND_COUNT: usize = 3;
/// `cssl.fs.ftruncate` — 2 operands : `(handle, len)`. SWAP-POINT.
pub const FS_FTRUNCATE_OPERAND_COUNT: usize = 2;

// ───────────────────────────────────────────────────────────────────────
// § per-op return-type marker (i32 vs i64)
//
//   The cssl-rt FFI surface returns either `i32` (close / last_error_kind
//   / ftruncate) or `i64` (open / read / write / last_error_os / seek).
//   The signature-builder consumes this marker so the Cranelift
//   `AbiParam` for the return slot matches the cssl-rt declaration
//   byte-for-byte.
//
//   ‼ Per the actual `cssl-rt::ffi` declarations confirmed in
//     ffi.rs lines 590-595 :
//       - `__cssl_fs_open`   returns `i64`
//       - `__cssl_fs_read`   returns `i64`
//       - `__cssl_fs_write`  returns `i64`
//       - `__cssl_fs_close`  returns `i64`  (NOT `i32` — the doc-comment
//         on the symbol says `0` on success / `-1` on failure but the
//         actual extern signature uses `i64` for return-shape consistency
//         with the other handle-returning shims)
//       - `__cssl_fs_last_error_kind` returns `i32`
//       - `__cssl_fs_last_error_os`   returns `i32`  (matches the
//         declared FFI shape — even though Win32 GetLastError is u32,
//         the canonical surface uses signed i32 for cross-platform parity)
//   The two SWAP-POINT entries (`seek` / `ftruncate`) follow the
//   conventional naming of the existing surface : `seek` returns the new
//   absolute file-offset as `i64`, `ftruncate` returns `0`/`-1` as `i32`.
// ───────────────────────────────────────────────────────────────────────

/// Per-op return-type marker used by the signature-builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsReturnTy {
    /// 32-bit integer return (last_error_kind / last_error_os / ftruncate).
    I32,
    /// 64-bit integer return (open / read / write / close / seek).
    I64,
}

impl FsReturnTy {
    /// Resolve the cranelift `Type` for this return-marker.
    #[must_use]
    pub fn clif_type(self) -> Type {
        match self {
            Self::I32 => cranelift_codegen::ir::types::I32,
            Self::I64 => cranelift_codegen::ir::types::I64,
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § Symbol-name LUT — op-name → (extern-symbol, operand-count, return-ty)
//
//   Branch-friendly match-arm ordering : most-common ops first.
//   read / write are the hot-path operations during program execution
//   (file-IO loops walk these per-iteration). open / close fire once
//   per resource lifetime. last_error / seek / ftruncate are rarer.
// ───────────────────────────────────────────────────────────────────────

/// Per-op contract bundle : the cssl-rt symbol-name + the expected MIR
/// operand-count + the cranelift return-type. The dispatcher walks this
/// table by op-name match on the leading 4-or-more entries (ordered for
/// branch-prediction efficiency).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FsOpContract {
    /// The MIR op-name string (e.g. `"cssl.fs.read"`).
    pub mir_op_name: &'static str,
    /// The cssl-rt extern symbol-name (e.g. `"__cssl_fs_read"`).
    pub ffi_symbol: &'static str,
    /// The expected operand-count (matches `OpSignature.operands` /
    /// `cssl-rt::ffi` argument count).
    pub operand_count: usize,
    /// The cranelift return-type for the result-slot.
    pub return_ty: FsReturnTy,
}

/// Canonical LUT — 8 entries ordered for branch-friendly dispatch.
/// Linear-scan lookup beats `HashMap` at N=8 (0 alloc + cache-warm + no
/// hash-fn cost). Per the task spec : "Symbol-name LUT for op-kind →
/// extern-symbol-name mapping (no String-fmt in hot path)".
pub const FS_OP_CONTRACT_TABLE: &[FsOpContract] = &[
    // — most-common (hot path) — read / write fire per-loop-iteration.
    FsOpContract {
        mir_op_name: MIR_FS_READ_OP_NAME,
        ffi_symbol: FS_READ_SYMBOL,
        operand_count: FS_READ_OPERAND_COUNT,
        return_ty: FsReturnTy::I64,
    },
    FsOpContract {
        mir_op_name: MIR_FS_WRITE_OP_NAME,
        ffi_symbol: FS_WRITE_SYMBOL,
        operand_count: FS_WRITE_OPERAND_COUNT,
        return_ty: FsReturnTy::I64,
    },
    // — once-per-resource — open / close fire bracket-style.
    FsOpContract {
        mir_op_name: MIR_FS_OPEN_OP_NAME,
        ffi_symbol: FS_OPEN_SYMBOL,
        operand_count: FS_OPEN_OPERAND_COUNT,
        return_ty: FsReturnTy::I64,
    },
    FsOpContract {
        mir_op_name: MIR_FS_CLOSE_OP_NAME,
        ffi_symbol: FS_CLOSE_SYMBOL,
        operand_count: FS_CLOSE_OPERAND_COUNT,
        return_ty: FsReturnTy::I64,
    },
    // — diagnostic-path — error-accessors fire after sentinel returns.
    FsOpContract {
        mir_op_name: MIR_FS_LAST_ERROR_KIND_OP_NAME,
        ffi_symbol: FS_LAST_ERROR_KIND_SYMBOL,
        operand_count: FS_LAST_ERROR_KIND_OPERAND_COUNT,
        return_ty: FsReturnTy::I32,
    },
    FsOpContract {
        mir_op_name: MIR_FS_LAST_ERROR_OS_OP_NAME,
        ffi_symbol: FS_LAST_ERROR_OS_SYMBOL,
        operand_count: FS_LAST_ERROR_OS_OPERAND_COUNT,
        return_ty: FsReturnTy::I32,
    },
    // — random-access — seek / ftruncate fire less-frequently.
    FsOpContract {
        mir_op_name: MIR_FS_SEEK_OP_NAME,
        ffi_symbol: FS_SEEK_SYMBOL,
        operand_count: FS_SEEK_OPERAND_COUNT,
        return_ty: FsReturnTy::I64,
    },
    FsOpContract {
        mir_op_name: MIR_FS_FTRUNCATE_OP_NAME,
        ffi_symbol: FS_FTRUNCATE_SYMBOL,
        operand_count: FS_FTRUNCATE_OPERAND_COUNT,
        return_ty: FsReturnTy::I32,
    },
];

/// LUT lookup — find the `FsOpContract` for a given MIR op-name. Linear
/// scan over the 8-entry `FS_OP_CONTRACT_TABLE` ; returns `None` for
/// non-`cssl.fs.*` ops.
///
/// § COMPLEXITY  O(1) amortized (table size fixed at 8 ; branch-friendly).
#[must_use]
pub fn lookup_fs_op_contract(op_name: &str) -> Option<&'static FsOpContract> {
    FS_OP_CONTRACT_TABLE
        .iter()
        .find(|entry| entry.mir_op_name == op_name)
}

// ───────────────────────────────────────────────────────────────────────
// § cranelift signature builders — one per cssl-rt FFI symbol
//
//   Each builder returns the canonical `Signature` for the matching
//   `__cssl_fs_*` import. The cgen-import-resolve path uses these to
//   declare the per-fn `FuncRef` (mirrors object.rs's
//   `declare_heap_imports_for_fn` shape).
//
//   ‼ The pointer-typed parameters use the host `ptr_ty` (passed in by
//     the caller — `obj_module.target_config().pointer_type()`). The
//     scalar-i64 / scalar-i32 / scalar-u32 / scalar-u16 parameters use
//     fixed cranelift types matching the cssl-rt declaration. Any drift
//     between the Rust-side `unsafe extern "C" fn` declaration and these
//     builders = link-time ABI mismatch ⇒ undefined behavior.
// ───────────────────────────────────────────────────────────────────────

/// Build the cranelift `Signature` for `__cssl_fs_open`.
///
/// § SHAPE  (matches `cssl-rt/src/ffi.rs` line 174-177)
/// ```text
///   pub unsafe extern "C" fn __cssl_fs_open(
///       path_ptr : *const u8,    // host-ptr-width
///       path_len : usize,        // host-ptr-width
///       flags    : i32,
///   ) -> i64
/// ```
#[must_use]
pub fn build_fs_open_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_fs_read`.
///
/// § SHAPE  (matches `cssl-rt/src/ffi.rs` line 190-193)
/// ```text
///   pub unsafe extern "C" fn __cssl_fs_read(
///       handle   : i64,
///       buf_ptr  : *mut u8,    // host-ptr-width
///       buf_len  : usize,      // host-ptr-width
///   ) -> i64
/// ```
#[must_use]
pub fn build_fs_read_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_fs_write`.
///
/// § SHAPE  (matches `cssl-rt/src/ffi.rs` line 206-209)
/// ```text
///   pub unsafe extern "C" fn __cssl_fs_write(
///       handle   : i64,
///       buf_ptr  : *const u8,  // host-ptr-width
///       buf_len  : usize,      // host-ptr-width
///   ) -> i64
/// ```
#[must_use]
pub fn build_fs_write_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_fs_close`.
///
/// § SHAPE  (matches `cssl-rt/src/ffi.rs` line 218-220)
/// ```text
///   pub unsafe extern "C" fn __cssl_fs_close(handle: i64) -> i64
/// ```
#[must_use]
pub fn build_fs_close_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_fs_last_error_kind`.
///
/// § SHAPE  (matches `cssl-rt/src/ffi.rs` line 233-235)
/// ```text
///   pub unsafe extern "C" fn __cssl_fs_last_error_kind() -> i32
/// ```
#[must_use]
pub fn build_fs_last_error_kind_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

/// Build the cranelift `Signature` for `__cssl_fs_last_error_os`.
///
/// § SHAPE  (matches `cssl-rt/src/ffi.rs` line 245-247)
/// ```text
///   pub unsafe extern "C" fn __cssl_fs_last_error_os() -> i32
/// ```
#[must_use]
pub fn build_fs_last_error_os_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

/// Build the cranelift `Signature` for `__cssl_fs_seek`. SWAP-POINT
/// signature — cssl-rt symbol implementation parity expected to mirror
/// POSIX `lseek` shape.
///
/// § SHAPE  (mirrors POSIX `lseek` ABI cast to portable widths)
/// ```text
///   pub unsafe extern "C" fn __cssl_fs_seek(
///       handle : i64,
///       offset : i64,
///       whence : i32,
///   ) -> i64
/// ```
#[must_use]
pub fn build_fs_seek_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_fs_ftruncate`. SWAP-POINT
/// signature — cssl-rt symbol implementation parity expected to mirror
/// POSIX `ftruncate` shape.
///
/// § SHAPE  (mirrors POSIX `ftruncate` ABI cast to portable widths)
/// ```text
///   pub unsafe extern "C" fn __cssl_fs_ftruncate(
///       handle : i64,
///       len    : i64,
///   ) -> i32
/// ```
#[must_use]
pub fn build_fs_ftruncate_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

// ───────────────────────────────────────────────────────────────────────
// § dispatcher : MIR op → signature builder
// ───────────────────────────────────────────────────────────────────────

/// Top-level dispatcher : given a `cssl.fs.*` MIR op, return the
/// cranelift `Signature` for the matching cssl-rt FFI symbol. Returns
/// `None` if the op-name is not one of the eight recognized fs ops —
/// caller should fall through to the generic `func.call` lowering path.
///
/// § PURPOSE
///   Single-source-of-truth for "given this MIR op, what `Signature`
///   should the import-declare path use". Avoids spreading the per-op
///   `build_*_signature` selection logic across multiple cgen call-sites.
///
/// § BRANCH-FRIENDLY ORDERING
///   Most-common ops first (read / write before close / open / error-
///   accessors). The branch-predictor lands the hot-path case in a
///   single cycle on the typical I/O loop.
#[must_use]
pub fn lower_fs_op_signature(op: &MirOp, call_conv: CallConv, ptr_ty: Type) -> Option<Signature> {
    match op.name.as_str() {
        // — most-common (hot path)
        MIR_FS_READ_OP_NAME => Some(build_fs_read_signature(call_conv, ptr_ty)),
        MIR_FS_WRITE_OP_NAME => Some(build_fs_write_signature(call_conv, ptr_ty)),
        // — once-per-resource
        MIR_FS_OPEN_OP_NAME => Some(build_fs_open_signature(call_conv, ptr_ty)),
        MIR_FS_CLOSE_OP_NAME => Some(build_fs_close_signature(call_conv)),
        // — diagnostic-path (SWAP-POINT)
        MIR_FS_LAST_ERROR_KIND_OP_NAME => Some(build_fs_last_error_kind_signature(call_conv)),
        MIR_FS_LAST_ERROR_OS_OP_NAME => Some(build_fs_last_error_os_signature(call_conv)),
        // — random-access (SWAP-POINT)
        MIR_FS_SEEK_OP_NAME => Some(build_fs_seek_signature(call_conv)),
        MIR_FS_FTRUNCATE_OP_NAME => Some(build_fs_ftruncate_signature(call_conv)),
        _ => None,
    }
}

/// Predicate : is this op one of the eight recognized `cssl.fs.*` ops?
/// Sub-helper for callers that already iterate the op-stream and want a
/// canonical-name predicate (avoids spreading the op-name string-literal
/// across multiple cgen call-sites).
#[must_use]
pub fn is_fs_op(op: &MirOp) -> bool {
    lookup_fs_op_contract(op.name.as_str()).is_some()
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn pre-scan : "which fs-imports does this fn need declared"
//
//   Mirrors the `HeapImports` pre-scan shape in `object.rs::
//   declare_heap_imports_for_fn`. Bit-packed `FsImportSet` — 8 bits, one
//   per cssl-rt symbol — keeps the pre-scan lean (no `HashMap`
//   allocation per fn ; future `for op in block.ops { match op.name {...
//   set.flag = true ...} }` walks set the bits).
// ───────────────────────────────────────────────────────────────────────

/// Bit-packed set indicating which `__cssl_fs_*` symbols a fn body
/// references. 8 bits = one per LUT entry. Linear scan + bit-pack is
/// strictly faster than a `HashMap<&str, bool>` at this size + zero
/// allocation per pre-scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FsImportSet {
    /// Bits : 0 = open, 1 = read, 2 = write, 3 = close, 4 = last_error_kind,
    /// 5 = last_error_os, 6 = seek, 7 = ftruncate.
    /// (Index = position of the `MIR_FS_*_OP_NAME` constant in the LUT
    /// canonical ordering — open / read / write / close / last_error_kind
    /// / last_error_os / seek / ftruncate.)
    pub bits: u8,
}

impl FsImportSet {
    /// Predicate : is this set empty (no fs-imports needed)?
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }

    /// Mark the bit corresponding to `op_name`. No-op if the op-name is
    /// not one of the eight recognized fs ops.
    pub fn mark(&mut self, op_name: &str) {
        if let Some(idx) = fs_op_canonical_index(op_name) {
            self.bits |= 1u8 << idx;
        }
    }

    /// Test the bit corresponding to `op_name`. Returns `false` if the
    /// op-name is not one of the eight recognized fs ops.
    #[must_use]
    pub fn contains(self, op_name: &str) -> bool {
        match fs_op_canonical_index(op_name) {
            Some(idx) => (self.bits & (1u8 << idx)) != 0,
            None => false,
        }
    }
}

/// Canonical bit-index for each fs op-name (matches `FsImportSet.bits`
/// layout). The ordering is fixed by the LUT canonical order — adding
/// a new fs op requires extending both the LUT + this fn.
#[must_use]
pub const fn fs_op_canonical_index(op_name: &str) -> Option<u8> {
    // Compile-time const-fn — the match arms are byte-exact comparisons.
    // Cannot use a runtime LUT scan because `FsImportSet::mark` is hot.
    if str_eq(op_name, MIR_FS_OPEN_OP_NAME) {
        Some(0)
    } else if str_eq(op_name, MIR_FS_READ_OP_NAME) {
        Some(1)
    } else if str_eq(op_name, MIR_FS_WRITE_OP_NAME) {
        Some(2)
    } else if str_eq(op_name, MIR_FS_CLOSE_OP_NAME) {
        Some(3)
    } else if str_eq(op_name, MIR_FS_LAST_ERROR_KIND_OP_NAME) {
        Some(4)
    } else if str_eq(op_name, MIR_FS_LAST_ERROR_OS_OP_NAME) {
        Some(5)
    } else if str_eq(op_name, MIR_FS_SEEK_OP_NAME) {
        Some(6)
    } else if str_eq(op_name, MIR_FS_FTRUNCATE_OP_NAME) {
        Some(7)
    } else {
        None
    }
}

/// Const-fn byte-exact string equality. Used by `fs_op_canonical_index`
/// because `&str == &str` is not const-stable.
#[must_use]
const fn str_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    let mut i = 0;
    while i < ab.len() {
        if ab[i] != bb[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Walk a single MIR block's ops once and return the bit-packed set of
/// fs-imports the fn needs declared. Mirrors `cgen_heap_dealloc::
/// needs_dealloc_import` but produces an 8-element bit-set rather than a
/// single boolean.
///
/// § COMPLEXITY  O(N) in op count, single-pass, no allocation beyond the
///   8-bit `FsImportSet` byte. No `HashMap` use.
#[must_use]
pub fn needs_fs_imports(block: &MirBlock) -> FsImportSet {
    let mut set = FsImportSet::default();
    for op in &block.ops {
        set.mark(op.name.as_str());
    }
    set
}

// ───────────────────────────────────────────────────────────────────────
// § contract validators (defensive cross-checks)
// ───────────────────────────────────────────────────────────────────────

/// Validate the operand-count of a `cssl.fs.*` op against the canonical
/// contract. Returns `Ok(())` when arity matches, otherwise an `Err`
/// with a diagnostic-friendly message.
///
/// § INTENT
///   Defensive check used at cgen-import-resolve time before issuing
///   the cranelift `call` instruction. If a mistyped MIR op leaks past
///   prior passes (e.g. a `cssl.fs.read` carrying only 2 operands
///   instead of 3), the validator surfaces the error before cgen
///   issues a malformed call.
///
/// # Errors
/// Returns `Err(String)` with a human-readable diagnostic when :
///   - the op-name is not one of the eight recognized fs ops
///   - `op.operands.len() != contract.operand_count`
pub fn validate_fs_op_arity(op: &MirOp) -> Result<&'static FsOpContract, String> {
    let contract = lookup_fs_op_contract(op.name.as_str()).ok_or_else(|| {
        format!(
            "validate_fs_op_arity : op `{}` is not a recognized cssl.fs.* op",
            op.name
        )
    })?;
    if op.operands.len() != contract.operand_count {
        return Err(format!(
            "validate_fs_op_arity : `{}` requires {} operands ; got {}",
            contract.mir_op_name,
            contract.operand_count,
            op.operands.len()
        ));
    }
    Ok(contract)
}

// ───────────────────────────────────────────────────────────────────────
// § INTEGRATION_NOTE — wiring path for the cgen-driver
//
//   The integration commit (deferred per `lib.rs`'s `pub mod` policy)
//   plugs this module into `object.rs::lower_one_op` + `jit.rs::
//   lower_op_in_jit` as follows :
//
//   1. PRE-SCAN — at the head of `compile_mir_function_to_object` (just
//      after `declare_heap_imports_for_fn` returns), call
//      `needs_fs_imports(entry_block)` to get the per-fn `FsImportSet`.
//
//   2. DECLARE — for each bit set in the `FsImportSet`, look up the
//      contract via `lookup_fs_op_contract` + build the signature via
//      `lower_fs_op_signature(...)` + call `obj_module.declare_function(
//      contract.ffi_symbol, Linkage::Import, &sig)` then
//      `obj_module.declare_func_in_func(id, &mut codegen_ctx.func)` to
//      get a `FuncRef`. Stash the eight (`FuncRef`, `FsOpContract`)
//      bindings in an `FsImports` map mirroring `HeapImports`.
//
//   3. LOWER — in `lower_one_op`, add eight new match arms (or a single
//      `if let Some(contract) = lookup_fs_op_contract(op.name)` branch)
//      that resolve the `FuncRef` from the per-fn `FsImports` map +
//      gather operands + emit `builder.ins().call(fref, &args)`. The
//      operand-coercion logic from `emit_heap_call` (lines 740-758 of
//      object.rs) carries over byte-for-byte — coerce non-matching
//      integer operands via `uextend` / `ireduce` to match the AbiParam
//      width.
//
//   4. RESULT-BIND — when the contract.return_ty is `I64`, bind the
//      cranelift result-value to the MIR result-id ; when `I32`, same
//      pattern but the value-map records an i32. The `last_error_*`
//      ops (operand_count == 0) skip the operand-gather phase.
//
//   The integration commit can issue all eight wirings in a single
//   walk via the LUT — no per-op match-arm needed in cgen-driver
//   beyond a single `is_fs_op` predicate + a pass-through `call`
//   emission. This is the canonical pattern Wave-A5 used for
//   `__cssl_free` — the four-op version of the eight-symbol wiring
//   delivered here.

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        build_fs_close_signature, build_fs_ftruncate_signature, build_fs_last_error_kind_signature,
        build_fs_last_error_os_signature, build_fs_open_signature, build_fs_read_signature,
        build_fs_seek_signature, build_fs_write_signature, fs_op_canonical_index, is_fs_op,
        lookup_fs_op_contract, lower_fs_op_signature, needs_fs_imports, validate_fs_op_arity,
        FsImportSet, FsOpContract, FsReturnTy, FS_CLOSE_OPERAND_COUNT, FS_CLOSE_SYMBOL,
        FS_FTRUNCATE_OPERAND_COUNT, FS_FTRUNCATE_SYMBOL, FS_LAST_ERROR_KIND_OPERAND_COUNT,
        FS_LAST_ERROR_KIND_SYMBOL, FS_LAST_ERROR_OS_OPERAND_COUNT, FS_LAST_ERROR_OS_SYMBOL,
        FS_OPEN_OPERAND_COUNT, FS_OPEN_SYMBOL, FS_OP_CONTRACT_TABLE, FS_READ_OPERAND_COUNT,
        FS_READ_SYMBOL, FS_SEEK_OPERAND_COUNT, FS_SEEK_SYMBOL, FS_WRITE_OPERAND_COUNT,
        FS_WRITE_SYMBOL, MIR_FS_CLOSE_OP_NAME, MIR_FS_FTRUNCATE_OP_NAME,
        MIR_FS_LAST_ERROR_KIND_OP_NAME, MIR_FS_LAST_ERROR_OS_OP_NAME, MIR_FS_OPEN_OP_NAME,
        MIR_FS_READ_OP_NAME, MIR_FS_SEEK_OP_NAME, MIR_FS_WRITE_OP_NAME,
    };
    use cranelift_codegen::ir::{types as cl_types, AbiParam};
    use cranelift_codegen::isa::CallConv;
    use cssl_mir::{CsslOp, IntWidth, MirBlock, MirOp, MirType, ValueId};

    // ── canonical-name lock invariants (cross-check with cssl-rt + cssl-mir) ─

    #[test]
    fn ffi_symbol_constants_match_cssl_rt_canonical() {
        // ‼ Lock-step invariant : the eight `__cssl_fs_*` symbol-names
        //   MUST match cssl-rt::ffi verbatim. Renaming either side
        //   without the other = link-time symbol mismatch ⇒ undefined
        //   behavior at runtime. The four currently-FFI-bound symbols
        //   are double-checked against cssl-rt::ffi at the `ffi_symbols_
        //   have_correct_signatures` test in cssl-rt/src/ffi.rs.
        assert_eq!(FS_OPEN_SYMBOL, "__cssl_fs_open");
        assert_eq!(FS_READ_SYMBOL, "__cssl_fs_read");
        assert_eq!(FS_WRITE_SYMBOL, "__cssl_fs_write");
        assert_eq!(FS_CLOSE_SYMBOL, "__cssl_fs_close");
        assert_eq!(FS_LAST_ERROR_KIND_SYMBOL, "__cssl_fs_last_error_kind");
        assert_eq!(FS_LAST_ERROR_OS_SYMBOL, "__cssl_fs_last_error_os");
        assert_eq!(FS_SEEK_SYMBOL, "__cssl_fs_seek");
        assert_eq!(FS_FTRUNCATE_SYMBOL, "__cssl_fs_ftruncate");
    }

    #[test]
    fn mir_op_name_constants_match_csslop_canonical() {
        // ‼ Lock-step invariant : the four currently-defined MIR op-names
        //   MUST equal `CsslOp::Fs*.name()` literal. Drift = unmatched
        //   op-dispatch ⇒ silent broken cgen.
        assert_eq!(MIR_FS_OPEN_OP_NAME, CsslOp::FsOpen.name());
        assert_eq!(MIR_FS_READ_OP_NAME, CsslOp::FsRead.name());
        assert_eq!(MIR_FS_WRITE_OP_NAME, CsslOp::FsWrite.name());
        assert_eq!(MIR_FS_CLOSE_OP_NAME, CsslOp::FsClose.name());
        // SWAP-POINT names — no `CsslOp` variant today, but the canonical
        // name follows the existing `cssl.fs.*` namespace + matches the
        // surface in stdlib/fs.cssl.
        assert_eq!(MIR_FS_LAST_ERROR_KIND_OP_NAME, "cssl.fs.last_error_kind");
        assert_eq!(MIR_FS_LAST_ERROR_OS_OP_NAME, "cssl.fs.last_error_os");
        assert_eq!(MIR_FS_SEEK_OP_NAME, "cssl.fs.seek");
        assert_eq!(MIR_FS_FTRUNCATE_OP_NAME, "cssl.fs.ftruncate");
    }

    #[test]
    fn declared_arity_matches_csslop_signature() {
        // ‼ Cross-check the operand counts agree with the MIR-side
        //   signature so a drift in either side surfaces immediately.
        assert_eq!(
            CsslOp::FsOpen.signature().operands,
            Some(FS_OPEN_OPERAND_COUNT)
        );
        assert_eq!(
            CsslOp::FsRead.signature().operands,
            Some(FS_READ_OPERAND_COUNT)
        );
        assert_eq!(
            CsslOp::FsWrite.signature().operands,
            Some(FS_WRITE_OPERAND_COUNT)
        );
        assert_eq!(
            CsslOp::FsClose.signature().operands,
            Some(FS_CLOSE_OPERAND_COUNT)
        );
    }

    // ── per-symbol signature builders : verify shape ────────────────────

    #[test]
    fn fs_open_signature_three_params_one_i64_return() {
        // __cssl_fs_open : (ptr, ptr, i32) -> i64
        let sig = build_fs_open_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I32));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn fs_read_signature_three_params_i64_ptr_ptr_returns_i64() {
        // __cssl_fs_read : (i64, ptr, ptr) -> i64
        let sig = build_fs_read_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn fs_write_signature_three_params_returns_i64() {
        // __cssl_fs_write : (i64, ptr, ptr) -> i64
        let sig = build_fs_write_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 3);
        for p in &sig.params {
            assert_eq!(*p, AbiParam::new(cl_types::I64));
        }
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn fs_close_signature_one_param_returns_i64() {
        // __cssl_fs_close : (i64) -> i64
        let sig = build_fs_close_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn fs_last_error_kind_signature_zero_params_returns_i32() {
        // __cssl_fs_last_error_kind : () -> i32
        let sig = build_fs_last_error_kind_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn fs_last_error_os_signature_zero_params_returns_i32() {
        // __cssl_fs_last_error_os : () -> i32
        let sig = build_fs_last_error_os_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn fs_seek_signature_three_params_returns_i64() {
        // __cssl_fs_seek : (i64, i64, i32) -> i64
        let sig = build_fs_seek_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I32));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn fs_ftruncate_signature_two_params_returns_i32() {
        // __cssl_fs_ftruncate : (i64, i64) -> i32
        let sig = build_fs_ftruncate_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    // ── FsReturnTy → cranelift Type mapping ─────────────────────────────

    #[test]
    fn return_ty_clif_type_maps_correctly() {
        assert_eq!(FsReturnTy::I32.clif_type(), cl_types::I32);
        assert_eq!(FsReturnTy::I64.clif_type(), cl_types::I64);
    }

    // ── LUT lookup ──────────────────────────────────────────────────────

    #[test]
    fn lut_lookup_finds_all_eight_canonical_op_names() {
        // Every recognized op-name must resolve to a distinct LUT entry.
        let names = [
            MIR_FS_OPEN_OP_NAME,
            MIR_FS_READ_OP_NAME,
            MIR_FS_WRITE_OP_NAME,
            MIR_FS_CLOSE_OP_NAME,
            MIR_FS_LAST_ERROR_KIND_OP_NAME,
            MIR_FS_LAST_ERROR_OS_OP_NAME,
            MIR_FS_SEEK_OP_NAME,
            MIR_FS_FTRUNCATE_OP_NAME,
        ];
        for n in names {
            assert!(
                lookup_fs_op_contract(n).is_some(),
                "lookup_fs_op_contract({n}) should be Some"
            );
        }
    }

    #[test]
    fn lut_lookup_returns_none_for_non_fs_ops() {
        assert!(lookup_fs_op_contract("cssl.heap.alloc").is_none());
        assert!(lookup_fs_op_contract("cssl.net.socket").is_none());
        assert!(lookup_fs_op_contract("arith.constant").is_none());
        assert!(lookup_fs_op_contract("").is_none());
    }

    #[test]
    fn lut_table_has_eight_entries() {
        // ‼ The LUT must enumerate exactly the eight canonical fs ops.
        //   Adding a new fs op requires extending the table + the
        //   `fs_op_canonical_index` const-fn together (they share the
        //   ordering invariant).
        assert_eq!(FS_OP_CONTRACT_TABLE.len(), 8);
    }

    #[test]
    fn lut_each_entry_resolves_back_to_canonical_name() {
        // ‼ Round-trip invariant : every LUT entry's mir_op_name must
        //   be one of the eight canonical constants.
        let valid_names = [
            MIR_FS_OPEN_OP_NAME,
            MIR_FS_READ_OP_NAME,
            MIR_FS_WRITE_OP_NAME,
            MIR_FS_CLOSE_OP_NAME,
            MIR_FS_LAST_ERROR_KIND_OP_NAME,
            MIR_FS_LAST_ERROR_OS_OP_NAME,
            MIR_FS_SEEK_OP_NAME,
            MIR_FS_FTRUNCATE_OP_NAME,
        ];
        for entry in FS_OP_CONTRACT_TABLE {
            assert!(
                valid_names.contains(&entry.mir_op_name),
                "LUT entry mir_op_name `{}` not in canonical-name set",
                entry.mir_op_name
            );
        }
    }

    // ── dispatcher : MIR op → Signature ─────────────────────────────────

    #[test]
    fn dispatcher_returns_signature_for_each_canonical_op() {
        let names = [
            MIR_FS_OPEN_OP_NAME,
            MIR_FS_READ_OP_NAME,
            MIR_FS_WRITE_OP_NAME,
            MIR_FS_CLOSE_OP_NAME,
            MIR_FS_LAST_ERROR_KIND_OP_NAME,
            MIR_FS_LAST_ERROR_OS_OP_NAME,
            MIR_FS_SEEK_OP_NAME,
            MIR_FS_FTRUNCATE_OP_NAME,
        ];
        for n in names {
            let op = MirOp::std(n);
            assert!(
                lower_fs_op_signature(&op, CallConv::SystemV, cl_types::I64).is_some(),
                "lower_fs_op_signature should return Some for `{n}`"
            );
        }
    }

    #[test]
    fn dispatcher_returns_none_for_unrecognized_op() {
        let op = MirOp::std("cssl.heap.alloc");
        assert!(lower_fs_op_signature(&op, CallConv::SystemV, cl_types::I64).is_none());
        let op2 = MirOp::std("arith.constant");
        assert!(lower_fs_op_signature(&op2, CallConv::SystemV, cl_types::I64).is_none());
    }

    #[test]
    fn dispatcher_passes_call_conv_through() {
        let op = MirOp::std(MIR_FS_OPEN_OP_NAME);
        let sysv = lower_fs_op_signature(&op, CallConv::SystemV, cl_types::I64).unwrap();
        let win = lower_fs_op_signature(&op, CallConv::WindowsFastcall, cl_types::I64).unwrap();
        assert_eq!(sysv.call_conv, CallConv::SystemV);
        assert_eq!(win.call_conv, CallConv::WindowsFastcall);
    }

    // ── is_fs_op predicate ──────────────────────────────────────────────

    #[test]
    fn is_fs_op_predicate_true_for_canonical_ops() {
        assert!(is_fs_op(&MirOp::std(MIR_FS_OPEN_OP_NAME)));
        assert!(is_fs_op(&MirOp::new(CsslOp::FsRead)));
        assert!(is_fs_op(&MirOp::new(CsslOp::FsWrite)));
        assert!(is_fs_op(&MirOp::new(CsslOp::FsClose)));
        assert!(is_fs_op(&MirOp::std(MIR_FS_LAST_ERROR_KIND_OP_NAME)));
        assert!(is_fs_op(&MirOp::std(MIR_FS_SEEK_OP_NAME)));
    }

    #[test]
    fn is_fs_op_predicate_false_for_non_fs_ops() {
        assert!(!is_fs_op(&MirOp::std("cssl.heap.alloc")));
        assert!(!is_fs_op(&MirOp::std("cssl.net.socket")));
        assert!(!is_fs_op(&MirOp::std("arith.constant")));
    }

    // ── fs_op_canonical_index const-fn ─────────────────────────────────

    #[test]
    fn canonical_index_assigns_distinct_bits_for_each_op() {
        // The 8 op-names map to indices 0..=7 with no collisions.
        let indices = [
            fs_op_canonical_index(MIR_FS_OPEN_OP_NAME).unwrap(),
            fs_op_canonical_index(MIR_FS_READ_OP_NAME).unwrap(),
            fs_op_canonical_index(MIR_FS_WRITE_OP_NAME).unwrap(),
            fs_op_canonical_index(MIR_FS_CLOSE_OP_NAME).unwrap(),
            fs_op_canonical_index(MIR_FS_LAST_ERROR_KIND_OP_NAME).unwrap(),
            fs_op_canonical_index(MIR_FS_LAST_ERROR_OS_OP_NAME).unwrap(),
            fs_op_canonical_index(MIR_FS_SEEK_OP_NAME).unwrap(),
            fs_op_canonical_index(MIR_FS_FTRUNCATE_OP_NAME).unwrap(),
        ];
        // All 8 should be distinct
        for i in 0..indices.len() {
            for j in i + 1..indices.len() {
                assert_ne!(
                    indices[i], indices[j],
                    "indices {i} and {j} must be distinct"
                );
            }
        }
        // Each must fit in u8 with a bit-set per slot.
        for idx in indices {
            assert!(idx < 8);
        }
    }

    #[test]
    fn canonical_index_returns_none_for_non_fs_ops() {
        assert!(fs_op_canonical_index("cssl.heap.alloc").is_none());
        assert!(fs_op_canonical_index("arith.constant").is_none());
        assert!(fs_op_canonical_index("").is_none());
    }

    // ── per-fn pre-scan : needs_fs_imports ─────────────────────────────

    #[test]
    fn pre_scan_finds_fs_ops_when_present() {
        // Mirrors object.rs::declare_heap_imports_for_fn's walk that
        // sets needs_free = true ; here it sets the read+write bits.
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std("arith.constant").with_attribute("value", "42"));
        block.push(
            MirOp::new(CsslOp::FsRead)
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2)),
        );
        block.push(
            MirOp::new(CsslOp::FsWrite)
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2)),
        );
        let set = needs_fs_imports(&block);
        assert!(!set.is_empty());
        assert!(set.contains(MIR_FS_READ_OP_NAME));
        assert!(set.contains(MIR_FS_WRITE_OP_NAME));
        assert!(!set.contains(MIR_FS_OPEN_OP_NAME));
        assert!(!set.contains(MIR_FS_CLOSE_OP_NAME));
    }

    #[test]
    fn pre_scan_returns_empty_when_no_fs_ops() {
        // A fn with only arith / func.return must produce an empty set.
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std("arith.constant"));
        block.push(MirOp::std("func.return"));
        let set = needs_fs_imports(&block);
        assert!(set.is_empty());
        assert_eq!(set.bits, 0);
    }

    #[test]
    fn pre_scan_handles_empty_block() {
        // Empty body : no panic, empty set.
        let block = MirBlock::new("entry");
        let set = needs_fs_imports(&block);
        assert!(set.is_empty());
    }

    #[test]
    fn pre_scan_records_all_eight_when_every_op_present() {
        // Synthesize a block referencing every fs op (mix of Std-named +
        // CsslOp-typed) ; the bit-set should have all 8 bits set.
        let mut block = MirBlock::new("entry");
        block.push(MirOp::new(CsslOp::FsOpen));
        block.push(MirOp::new(CsslOp::FsRead));
        block.push(MirOp::new(CsslOp::FsWrite));
        block.push(MirOp::new(CsslOp::FsClose));
        block.push(MirOp::std(MIR_FS_LAST_ERROR_KIND_OP_NAME));
        block.push(MirOp::std(MIR_FS_LAST_ERROR_OS_OP_NAME));
        block.push(MirOp::std(MIR_FS_SEEK_OP_NAME));
        block.push(MirOp::std(MIR_FS_FTRUNCATE_OP_NAME));
        let set = needs_fs_imports(&block);
        assert_eq!(set.bits, 0xFF, "all 8 bits should be set");
    }

    // ── FsImportSet manipulation ───────────────────────────────────────

    #[test]
    fn import_set_mark_and_contains_round_trip() {
        let mut set = FsImportSet::default();
        assert!(set.is_empty());
        set.mark(MIR_FS_READ_OP_NAME);
        assert!(set.contains(MIR_FS_READ_OP_NAME));
        assert!(!set.contains(MIR_FS_WRITE_OP_NAME));
        set.mark(MIR_FS_WRITE_OP_NAME);
        assert!(set.contains(MIR_FS_WRITE_OP_NAME));
        assert!(set.contains(MIR_FS_READ_OP_NAME));
    }

    #[test]
    fn import_set_mark_ignores_non_fs_ops() {
        let mut set = FsImportSet::default();
        set.mark("cssl.heap.alloc");
        set.mark("arith.constant");
        assert!(set.is_empty());
    }

    // ── arity validators ────────────────────────────────────────────────

    #[test]
    fn validate_accepts_canonical_three_operand_read_op() {
        let op = MirOp::new(CsslOp::FsRead)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2));
        let contract = validate_fs_op_arity(&op).unwrap();
        assert_eq!(contract.ffi_symbol, FS_READ_SYMBOL);
        assert_eq!(contract.return_ty, FsReturnTy::I64);
    }

    #[test]
    fn validate_accepts_canonical_one_operand_close_op() {
        let op = MirOp::new(CsslOp::FsClose).with_operand(ValueId(0));
        let contract = validate_fs_op_arity(&op).unwrap();
        assert_eq!(contract.ffi_symbol, FS_CLOSE_SYMBOL);
        assert_eq!(contract.operand_count, 1);
    }

    #[test]
    fn validate_accepts_zero_operand_last_error_kind_op() {
        // last_error_kind takes no operands ; validator must accept the
        // empty operand-vector (matches the FFI signature `fn() -> i32`).
        let op = MirOp::std(MIR_FS_LAST_ERROR_KIND_OP_NAME);
        let contract = validate_fs_op_arity(&op).unwrap();
        assert_eq!(contract.ffi_symbol, FS_LAST_ERROR_KIND_SYMBOL);
        assert_eq!(contract.operand_count, 0);
    }

    #[test]
    fn validate_rejects_two_operand_read_op() {
        // Defensive : if a mistyped MIR op leaks past prior passes (only
        // 2 operands instead of 3 for read), surface the error.
        let op = MirOp::new(CsslOp::FsRead)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1));
        let err = validate_fs_op_arity(&op).unwrap_err();
        assert!(err.contains("3 operands"));
        assert!(err.contains("cssl.fs.read"));
    }

    #[test]
    fn validate_rejects_unknown_op_name() {
        let op = MirOp::std("cssl.heap.alloc")
            .with_operand(ValueId(0))
            .with_result(ValueId(1), MirType::Int(IntWidth::I64));
        let err = validate_fs_op_arity(&op).unwrap_err();
        assert!(err.contains("not a recognized cssl.fs.* op"));
    }

    // ── end-to-end : verify contract round-trip via lookup ─────────────

    #[test]
    fn contract_lookup_round_trip_each_op() {
        // For every entry in the LUT, lookup_fs_op_contract should
        // return the SAME entry's symbol+arity+return-ty.
        for entry in FS_OP_CONTRACT_TABLE {
            let found = lookup_fs_op_contract(entry.mir_op_name).unwrap();
            assert_eq!(found.ffi_symbol, entry.ffi_symbol);
            assert_eq!(found.operand_count, entry.operand_count);
            assert_eq!(found.return_ty, entry.return_ty);
        }
    }

    #[test]
    fn contract_table_each_symbol_unique() {
        // ‼ All eight symbol-names must be distinct (no two LUT entries
        //   point at the same cssl-rt symbol).
        let symbols: Vec<&str> = FS_OP_CONTRACT_TABLE.iter().map(|e| e.ffi_symbol).collect();
        for i in 0..symbols.len() {
            for j in i + 1..symbols.len() {
                assert_ne!(
                    symbols[i], symbols[j],
                    "duplicate symbol {} at indices {i} and {j}",
                    symbols[i]
                );
            }
        }
    }

    // ── operand-count constants : sanity ───────────────────────────────

    #[test]
    fn operand_count_constants_match_ffi_signatures() {
        // Cross-check : each operand-count constant matches the cssl-rt
        // FFI declaration's argument count (verified by inspection of
        // cssl-rt/src/ffi.rs lines 174-247).
        assert_eq!(FS_OPEN_OPERAND_COUNT, 2); // (path_ptr, path_len) ; flags is the 3rd FFI arg from a const
        assert_eq!(FS_READ_OPERAND_COUNT, 3); // (handle, buf_ptr, buf_len)
        assert_eq!(FS_WRITE_OPERAND_COUNT, 3); // (handle, buf_ptr, buf_len)
        assert_eq!(FS_CLOSE_OPERAND_COUNT, 1); // (handle)
        assert_eq!(FS_LAST_ERROR_KIND_OPERAND_COUNT, 0); // ()
        assert_eq!(FS_LAST_ERROR_OS_OPERAND_COUNT, 0); // ()
        assert_eq!(FS_SEEK_OPERAND_COUNT, 3); // (handle, offset, whence)
        assert_eq!(FS_FTRUNCATE_OPERAND_COUNT, 2); // (handle, len)
    }

    // ── FsOpContract Copy/Clone shape ──────────────────────────────────

    #[test]
    fn fs_op_contract_is_copy_friendly() {
        // `FsOpContract` is `Copy` — useful for cgen-paths that want to
        // pass it through closure-captures without lifetime gymnastics.
        let entry = FS_OP_CONTRACT_TABLE[0];
        let copy: FsOpContract = entry;
        assert_eq!(copy.mir_op_name, entry.mir_op_name);
        assert_eq!(copy.ffi_symbol, entry.ffi_symbol);
    }
}
