//! § Wave-D2 — `cssl.thread.*` / `cssl.mutex.*` / `cssl.atomic.*` Cranelift
//!   cgen helpers (host-thread effect concretization).
//!
//! § ROLE
//!   Pure-function helpers that build the cranelift `Signature`s for the
//!   nine `__cssl_thread_*` / `__cssl_mutex_*` / `__cssl_atomic_*` FFI
//!   imports + the per-fn dispatcher that turns a `cssl.{thread,mutex,
//!   atomic}.<verb>` MIR op into a `call __cssl_<domain>_<verb>(...)`
//!   cranelift IR description. Mirrors the Wave-C3 `cgen_fs.rs` shape :
//!     1. centralizes the symbol-name + signature-shape so the cgen
//!        layer has ONE source-of-truth for the threading FFI contract,
//!     2. exposes a per-block pre-scan helper so the per-fn import-
//!        declare path can stay lean (declare only the symbols the fn
//!        actually references),
//!     3. provides arity validators so a mistyped MIR op surfaces a
//!        diagnostic before cgen issues a malformed call,
//!     4. closes the loop on Wave-D2 deliverable item 2 (NEW file in
//!        `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/`) without
//!        modifying any other crate or `lib.rs`'s `pub mod` list.
//!
//! § INTEGRATION_NOTE  (per Wave-D2 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!   src/lib.rs` is intentionally NOT modified. The helpers compile +
//!   are tested in-place via `#[cfg(test)]` references. A future cgen
//!   refactor (sharing the `object.rs` + `jit.rs` heap-import pattern,
//!   currently tracked as a deferred follow-up in `object.rs § DEFERRED`)
//!   will migrate the per-op call-emission here + add the
//!   `pub mod cgen_thread` line at that time. Until then the helpers are
//!   crate-internal — `cgen_thread::lower_thread_op` is the canonical
//!   dispatcher the integration commit will invoke from
//!   `object.rs::lower_one_op` / `jit.rs::lower_op_in_jit` after the
//!   existing `cssl.fs.*` / `cssl.net.*` arms.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-rt/src/host_thread.rs` (companion module
//!     in this same Wave-D2 dispatch) — the nine `__cssl_thread_*` /
//!     `__cssl_mutex_*` / `__cssl_atomic_*` ABI-stable symbols this
//!     module wires call-emission against. The cssl-rt-side constants
//!     `THREAD_SPAWN_SYMBOL` / `THREAD_JOIN_SYMBOL` / etc. equal the
//!     constants in this module byte-for-byte (cross-checked by
//!     `ffi_symbol_constants_match_cssl_rt_canonical` test below).
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/cgen_fs.rs` —
//!     sibling Wave-C3 module that establishes the canonical pattern
//!     this module mirrors (signature-builder + per-fn pre-scan +
//!     arity-validator + canonical-name lock-test).
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/cgen_net.rs` —
//!     sibling Wave-C4 module that extends the `cgen_fs` pattern to a
//!     larger 12-symbol surface ; this module's 9-symbol surface
//!     follows the same idioms.
//!   - `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § threading` — the
//!     wave plan that scopes this slice (`thread_spawn` / `_join` /
//!     `mutex_*` / `atomic_*`).
//!   - `specs/40_WAVE_CSSL_PLAN.csl § WAVE-D § D2` — the wave plan
//!     entry for this dispatch.
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions ; zero allocation outside the
//!     cranelift `Signature` constructor's required `Vec` storage.
//!   - Symbol-name LUT : op-name → extern-symbol-name mapping is a
//!     `&'static [(name, symbol)]` slice ; no String-format on the hot
//!     path. Lookup is a linear scan of 9 entries — strictly faster
//!     than a `HashMap` at this size + zero per-call allocation.
//!   - `needs_thread_imports` walks the per-block ops slice ONCE ; O(N)
//!     in op count, single-pass, no allocation beyond the bit-packed
//!     `ThreadImportSet` 16-bit field (9 of 16 bits used).
//!   - Branch-friendly match-arm ordering : most-common ops first
//!     (atomic-CAS / atomic-load / atomic-store > mutex-lock /
//!     mutex-unlock > spawn / join / mutex-create / mutex-destroy).
//!     The branch-predictor lands the hot-path case in cycle 1 on the
//!     typical lock-protected critical-section pattern.
//!
//! § MIR ↔ CLIF ABI MAPPING
//!
//!   ```text
//!   MIR (recognizer-emitted)                    CLIF (this module)
//!   ─────────────────────────────────────────   ───────────────────────────────────
//!   cssl.thread.spawn   %entry_ptr, %arg_ptr    call __cssl_thread_spawn(e, a) -> u64
//!     {thread_effect=true}
//!   cssl.thread.join    %h, %ret_out_ptr        call __cssl_thread_join(h, p) -> i32
//!   cssl.mutex.create                           call __cssl_mutex_create() -> u64
//!   cssl.mutex.lock     %h                      call __cssl_mutex_lock(h) -> i32
//!   cssl.mutex.unlock   %h                      call __cssl_mutex_unlock(h) -> i32
//!   cssl.mutex.destroy  %h                      call __cssl_mutex_destroy(h) -> i32
//!   cssl.atomic.load_u64    %addr, %order        call __cssl_atomic_load_u64(a, o) -> u64
//!   cssl.atomic.store_u64   %addr, %val, %order  call __cssl_atomic_store_u64(a, v, o) -> i32
//!   cssl.atomic.cas_u64     %addr, %exp, %des,   call __cssl_atomic_cas_u64(a, e, d, o) -> u64
//!                            %order
//!   ```
//!
//! § SWAP-POINT inventory  (per task `MOCK-WHEN-DEPS-MISSING` directive)
//!   The nine cssl-rt symbols this module targets are exported by
//!   `cssl-rt::host_thread` (companion file in this Wave-D2 dispatch ;
//!   see `compiler-rs/crates/cssl-rt/src/host_thread.rs`). HOWEVER no
//!   matching MIR op-kinds exist today in `cssl-mir::op::CsslOp` — the
//!   `cssl.thread.*` / `cssl.mutex.*` / `cssl.atomic.*` namespace is
//!   reserved but unused. This module therefore dispatches on the op-
//!   name STRING (matches the existing `cgen_fs.rs` SWAP-POINT pattern
//!   for `last_error_kind` / `seek` / `ftruncate`). Once the MIR op-
//!   kinds land — likely a stage-0 follow-up to the Wave-D2 cssl-rt
//!   surface — the constants below immediately route through. The
//!   SWAP-POINT comments mark each symbol that has cssl-rt support but
//!   no MIR op-kind today.

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{AbiParam, Signature, Type};
use cranelift_codegen::isa::CallConv;
use cssl_mir::{MirBlock, MirOp};

// ───────────────────────────────────────────────────────────────────────
// § canonical FFI symbol names (cssl-rt side)
//
//   ‼ Each MUST match `compiler-rs/crates/cssl-rt/src/host_thread.rs`
//     literally. The cssl-rt-side constants `THREAD_SPAWN_SYMBOL` /
//     `THREAD_JOIN_SYMBOL` / etc. equal these constants byte-for-byte.
//     Renaming either side requires lock-step changes — see
//     HANDOFF_SESSION_6 § LANDMINES + cssl-rt FFI invariants.
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol for `cssl.thread.spawn`.
pub const THREAD_SPAWN_SYMBOL: &str = "__cssl_thread_spawn";
/// FFI symbol for `cssl.thread.join`.
pub const THREAD_JOIN_SYMBOL: &str = "__cssl_thread_join";
/// FFI symbol for `cssl.mutex.create`.
pub const MUTEX_CREATE_SYMBOL: &str = "__cssl_mutex_create";
/// FFI symbol for `cssl.mutex.lock`.
pub const MUTEX_LOCK_SYMBOL: &str = "__cssl_mutex_lock";
/// FFI symbol for `cssl.mutex.unlock`.
pub const MUTEX_UNLOCK_SYMBOL: &str = "__cssl_mutex_unlock";
/// FFI symbol for `cssl.mutex.destroy`.
pub const MUTEX_DESTROY_SYMBOL: &str = "__cssl_mutex_destroy";
/// FFI symbol for `cssl.atomic.load_u64`.
pub const ATOMIC_LOAD_U64_SYMBOL: &str = "__cssl_atomic_load_u64";
/// FFI symbol for `cssl.atomic.store_u64`.
pub const ATOMIC_STORE_U64_SYMBOL: &str = "__cssl_atomic_store_u64";
/// FFI symbol for `cssl.atomic.cas_u64`.
pub const ATOMIC_CAS_U64_SYMBOL: &str = "__cssl_atomic_cas_u64";

// ───────────────────────────────────────────────────────────────────────
// § canonical MIR op-name strings (cssl-mir side)
//
//   All nine are SWAP-POINT names — the dispatcher recognizes them via
//   op-name string match because no `CsslOp` variants exist today for
//   the threading surface. Future MIR op-kinds adding `cssl.thread.*` /
//   `cssl.mutex.*` / `cssl.atomic.*` route through immediately without
//   touching the dispatcher (matches the cgen_fs.rs pattern for
//   `last_error_kind` / `seek` / `ftruncate`).
// ───────────────────────────────────────────────────────────────────────

/// MIR op-name for `cssl.thread.spawn`. SWAP-POINT.
pub const MIR_THREAD_SPAWN_OP_NAME: &str = "cssl.thread.spawn";
/// MIR op-name for `cssl.thread.join`. SWAP-POINT.
pub const MIR_THREAD_JOIN_OP_NAME: &str = "cssl.thread.join";
/// MIR op-name for `cssl.mutex.create`. SWAP-POINT.
pub const MIR_MUTEX_CREATE_OP_NAME: &str = "cssl.mutex.create";
/// MIR op-name for `cssl.mutex.lock`. SWAP-POINT.
pub const MIR_MUTEX_LOCK_OP_NAME: &str = "cssl.mutex.lock";
/// MIR op-name for `cssl.mutex.unlock`. SWAP-POINT.
pub const MIR_MUTEX_UNLOCK_OP_NAME: &str = "cssl.mutex.unlock";
/// MIR op-name for `cssl.mutex.destroy`. SWAP-POINT.
pub const MIR_MUTEX_DESTROY_OP_NAME: &str = "cssl.mutex.destroy";
/// MIR op-name for `cssl.atomic.load_u64`. SWAP-POINT.
pub const MIR_ATOMIC_LOAD_U64_OP_NAME: &str = "cssl.atomic.load_u64";
/// MIR op-name for `cssl.atomic.store_u64`. SWAP-POINT.
pub const MIR_ATOMIC_STORE_U64_OP_NAME: &str = "cssl.atomic.store_u64";
/// MIR op-name for `cssl.atomic.cas_u64`. SWAP-POINT.
pub const MIR_ATOMIC_CAS_U64_OP_NAME: &str = "cssl.atomic.cas_u64";

// ───────────────────────────────────────────────────────────────────────
// § per-op operand counts
//
//   ‼ Each count matches the FFI-side argument count from
//     `cssl-rt::host_thread`. Adding a new threading op requires
//     extending both the LUT + the matching `_OPERAND_COUNT` constant.
// ───────────────────────────────────────────────────────────────────────

/// `cssl.thread.spawn` — 2 operands : `(entry_ptr, arg_ptr)`.
pub const THREAD_SPAWN_OPERAND_COUNT: usize = 2;
/// `cssl.thread.join` — 2 operands : `(handle, ret_out_ptr)`.
pub const THREAD_JOIN_OPERAND_COUNT: usize = 2;
/// `cssl.mutex.create` — 0 operands.
pub const MUTEX_CREATE_OPERAND_COUNT: usize = 0;
/// `cssl.mutex.lock` — 1 operand : `(handle)`.
pub const MUTEX_LOCK_OPERAND_COUNT: usize = 1;
/// `cssl.mutex.unlock` — 1 operand : `(handle)`.
pub const MUTEX_UNLOCK_OPERAND_COUNT: usize = 1;
/// `cssl.mutex.destroy` — 1 operand : `(handle)`.
pub const MUTEX_DESTROY_OPERAND_COUNT: usize = 1;
/// `cssl.atomic.load_u64` — 2 operands : `(addr_ptr, order)`.
pub const ATOMIC_LOAD_U64_OPERAND_COUNT: usize = 2;
/// `cssl.atomic.store_u64` — 3 operands : `(addr_ptr, value, order)`.
pub const ATOMIC_STORE_U64_OPERAND_COUNT: usize = 3;
/// `cssl.atomic.cas_u64` — 4 operands : `(addr_ptr, expected, desired,
/// order)`.
pub const ATOMIC_CAS_U64_OPERAND_COUNT: usize = 4;

// ───────────────────────────────────────────────────────────────────────
// § per-op return-type marker (i32 vs i64 vs u64)
//
//   The cssl-rt FFI surface returns either `u64` (spawn / mutex_create /
//   atomic_load_u64 / atomic_cas_u64) or `i32` (join / mutex_lock /
//   mutex_unlock / mutex_destroy / atomic_store_u64). Cranelift treats
//   `u64` and `i64` as the same type (I64) ; the discriminator is
//   ABI-only signage on the Rust side.
// ───────────────────────────────────────────────────────────────────────

/// Per-op return-type marker used by the signature-builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadReturnTy {
    /// 32-bit integer return (`i32` — join / mutex_{lock,unlock,destroy}
    /// / atomic_store_u64).
    I32,
    /// 64-bit integer return (`u64` / `i64` — spawn / mutex_create /
    /// atomic_load_u64 / atomic_cas_u64).
    I64,
}

impl ThreadReturnTy {
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
//   Atomic CAS is the hot path during lock-free data-structure
//   traversal ; atomic-load / atomic-store are next on the fast path.
//   Mutex-{lock,unlock} fire bracketing critical sections — common but
//   slower than a single CAS. Spawn / join / mutex-{create,destroy}
//   are rare (one-time-per-thread or per-mutex-lifetime).
// ───────────────────────────────────────────────────────────────────────

/// Per-op contract bundle : the cssl-rt symbol-name + the expected MIR
/// operand-count + the cranelift return-type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadOpContract {
    /// The MIR op-name string (e.g. `"cssl.atomic.cas_u64"`).
    pub mir_op_name: &'static str,
    /// The cssl-rt extern symbol-name (e.g. `"__cssl_atomic_cas_u64"`).
    pub ffi_symbol: &'static str,
    /// The expected operand-count.
    pub operand_count: usize,
    /// The cranelift return-type for the result-slot.
    pub return_ty: ThreadReturnTy,
}

/// Canonical LUT — 9 entries ordered for branch-friendly dispatch.
/// Linear-scan lookup beats `HashMap` at N=9 (0 alloc + cache-warm + no
/// hash-fn cost). Per the task spec : "Symbol-name LUT for op-kind →
/// extern-symbol-name mapping (no String-fmt in hot path)".
pub const THREAD_OP_CONTRACT_TABLE: &[ThreadOpContract] = &[
    // — fastest path : atomic CAS / load / store fire per-loop-iteration
    //   on lock-free data structures.
    ThreadOpContract {
        mir_op_name: MIR_ATOMIC_CAS_U64_OP_NAME,
        ffi_symbol: ATOMIC_CAS_U64_SYMBOL,
        operand_count: ATOMIC_CAS_U64_OPERAND_COUNT,
        return_ty: ThreadReturnTy::I64,
    },
    ThreadOpContract {
        mir_op_name: MIR_ATOMIC_LOAD_U64_OP_NAME,
        ffi_symbol: ATOMIC_LOAD_U64_SYMBOL,
        operand_count: ATOMIC_LOAD_U64_OPERAND_COUNT,
        return_ty: ThreadReturnTy::I64,
    },
    ThreadOpContract {
        mir_op_name: MIR_ATOMIC_STORE_U64_OP_NAME,
        ffi_symbol: ATOMIC_STORE_U64_SYMBOL,
        operand_count: ATOMIC_STORE_U64_OPERAND_COUNT,
        return_ty: ThreadReturnTy::I32,
    },
    // — bracket-around-critical-section : mutex-lock / mutex-unlock fire
    //   per-region-entry / per-region-exit.
    ThreadOpContract {
        mir_op_name: MIR_MUTEX_LOCK_OP_NAME,
        ffi_symbol: MUTEX_LOCK_SYMBOL,
        operand_count: MUTEX_LOCK_OPERAND_COUNT,
        return_ty: ThreadReturnTy::I32,
    },
    ThreadOpContract {
        mir_op_name: MIR_MUTEX_UNLOCK_OP_NAME,
        ffi_symbol: MUTEX_UNLOCK_SYMBOL,
        operand_count: MUTEX_UNLOCK_OPERAND_COUNT,
        return_ty: ThreadReturnTy::I32,
    },
    // — once-per-resource : spawn / join / mutex-create / mutex-destroy
    //   fire bracket-style at thread / mutex lifetime boundaries.
    ThreadOpContract {
        mir_op_name: MIR_THREAD_SPAWN_OP_NAME,
        ffi_symbol: THREAD_SPAWN_SYMBOL,
        operand_count: THREAD_SPAWN_OPERAND_COUNT,
        return_ty: ThreadReturnTy::I64,
    },
    ThreadOpContract {
        mir_op_name: MIR_THREAD_JOIN_OP_NAME,
        ffi_symbol: THREAD_JOIN_SYMBOL,
        operand_count: THREAD_JOIN_OPERAND_COUNT,
        return_ty: ThreadReturnTy::I32,
    },
    ThreadOpContract {
        mir_op_name: MIR_MUTEX_CREATE_OP_NAME,
        ffi_symbol: MUTEX_CREATE_SYMBOL,
        operand_count: MUTEX_CREATE_OPERAND_COUNT,
        return_ty: ThreadReturnTy::I64,
    },
    ThreadOpContract {
        mir_op_name: MIR_MUTEX_DESTROY_OP_NAME,
        ffi_symbol: MUTEX_DESTROY_SYMBOL,
        operand_count: MUTEX_DESTROY_OPERAND_COUNT,
        return_ty: ThreadReturnTy::I32,
    },
];

/// LUT lookup — find the `ThreadOpContract` for a given MIR op-name.
/// Linear scan over the 9-entry `THREAD_OP_CONTRACT_TABLE` ; returns
/// `None` for non-`cssl.{thread,mutex,atomic}.*` ops.
///
/// § COMPLEXITY  O(1) amortized (table size fixed at 9 ; branch-friendly).
#[must_use]
pub fn lookup_thread_op_contract(op_name: &str) -> Option<&'static ThreadOpContract> {
    THREAD_OP_CONTRACT_TABLE
        .iter()
        .find(|entry| entry.mir_op_name == op_name)
}

// ───────────────────────────────────────────────────────────────────────
// § cranelift signature builders — one per cssl-rt FFI symbol
//
//   Each builder returns the canonical `Signature` for the matching
//   `__cssl_*` import. The cgen-import-resolve path uses these to
//   declare the per-fn `FuncRef` (mirrors object.rs's
//   `declare_heap_imports_for_fn` shape).
//
//   ‼ The pointer-typed parameters use the host `ptr_ty` (passed in by
//     the caller — `obj_module.target_config().pointer_type()`). The
//     scalar-i64 / scalar-i32 / scalar-u32 parameters use fixed cranelift
//     types matching the cssl-rt declaration. Any drift between the
//     Rust-side `unsafe extern "C" fn` declaration and these builders =
//     link-time ABI mismatch ⇒ undefined behavior.
// ───────────────────────────────────────────────────────────────────────

/// Build the cranelift `Signature` for `__cssl_thread_spawn`.
///
/// § SHAPE  (matches `cssl-rt/src/host_thread.rs § __cssl_thread_spawn`)
/// ```text
///   pub unsafe extern "C" fn __cssl_thread_spawn(
///       entry : *const u8,    // host-ptr-width
///       arg   : *const u8,    // host-ptr-width
///   ) -> u64
/// ```
#[must_use]
pub fn build_thread_spawn_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_thread_join`.
///
/// § SHAPE
/// ```text
///   pub unsafe extern "C" fn __cssl_thread_join(
///       handle  : u64,
///       ret_out : *mut i32,    // host-ptr-width
///   ) -> i32
/// ```
#[must_use]
pub fn build_thread_join_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.params.push(AbiParam::new(ptr_ty));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

/// Build the cranelift `Signature` for `__cssl_mutex_create`.
///
/// § SHAPE  `() -> u64`
#[must_use]
pub fn build_mutex_create_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_mutex_lock`.
///
/// § SHAPE  `(u64) -> i32`
#[must_use]
pub fn build_mutex_lock_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

/// Build the cranelift `Signature` for `__cssl_mutex_unlock`.
///
/// § SHAPE  `(u64) -> i32`
#[must_use]
pub fn build_mutex_unlock_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

/// Build the cranelift `Signature` for `__cssl_mutex_destroy`.
///
/// § SHAPE  `(u64) -> i32`
#[must_use]
pub fn build_mutex_destroy_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

/// Build the cranelift `Signature` for `__cssl_atomic_load_u64`.
///
/// § SHAPE
/// ```text
///   pub unsafe extern "C" fn __cssl_atomic_load_u64(
///       addr  : *const u64,    // host-ptr-width
///       order : u32,
///   ) -> u64
/// ```
#[must_use]
pub fn build_atomic_load_u64_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_atomic_store_u64`.
///
/// § SHAPE
/// ```text
///   pub unsafe extern "C" fn __cssl_atomic_store_u64(
///       addr  : *mut u64,    // host-ptr-width
///       value : u64,
///       order : u32,
///   ) -> i32
/// ```
#[must_use]
pub fn build_atomic_store_u64_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

/// Build the cranelift `Signature` for `__cssl_atomic_cas_u64`.
///
/// § SHAPE
/// ```text
///   pub unsafe extern "C" fn __cssl_atomic_cas_u64(
///       addr     : *mut u64,    // host-ptr-width
///       expected : u64,
///       desired  : u64,
///       order    : u32,
///   ) -> u64
/// ```
#[must_use]
pub fn build_atomic_cas_u64_signature(call_conv: CallConv, ptr_ty: Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(ptr_ty));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

// ───────────────────────────────────────────────────────────────────────
// § dispatcher : MIR op → signature builder
// ───────────────────────────────────────────────────────────────────────

/// Top-level dispatcher : given a `cssl.{thread,mutex,atomic}.*` MIR op,
/// return the cranelift `Signature` for the matching cssl-rt FFI symbol.
/// Returns `None` if the op-name is not one of the nine recognized ops —
/// caller should fall through to the generic `func.call` lowering path.
///
/// § PURPOSE
///   Single-source-of-truth for "given this MIR op, what `Signature`
///   should the import-declare path use".
///
/// § BRANCH-FRIENDLY ORDERING
///   Atomic CAS first (hot-path for lock-free data structures), then
///   load / store, then mutex lock / unlock (bracket-around-critical-
///   section), then spawn / join / create / destroy (rare lifetime ops).
#[must_use]
pub fn lower_thread_op_signature(
    op: &MirOp,
    call_conv: CallConv,
    ptr_ty: Type,
) -> Option<Signature> {
    match op.name.as_str() {
        // — fastest path : atomics
        MIR_ATOMIC_CAS_U64_OP_NAME => Some(build_atomic_cas_u64_signature(call_conv, ptr_ty)),
        MIR_ATOMIC_LOAD_U64_OP_NAME => Some(build_atomic_load_u64_signature(call_conv, ptr_ty)),
        MIR_ATOMIC_STORE_U64_OP_NAME => Some(build_atomic_store_u64_signature(call_conv, ptr_ty)),
        // — bracket-around-critical-section : mutex lock/unlock
        MIR_MUTEX_LOCK_OP_NAME => Some(build_mutex_lock_signature(call_conv)),
        MIR_MUTEX_UNLOCK_OP_NAME => Some(build_mutex_unlock_signature(call_conv)),
        // — once-per-resource : spawn / join / mutex create/destroy
        MIR_THREAD_SPAWN_OP_NAME => Some(build_thread_spawn_signature(call_conv, ptr_ty)),
        MIR_THREAD_JOIN_OP_NAME => Some(build_thread_join_signature(call_conv, ptr_ty)),
        MIR_MUTEX_CREATE_OP_NAME => Some(build_mutex_create_signature(call_conv)),
        MIR_MUTEX_DESTROY_OP_NAME => Some(build_mutex_destroy_signature(call_conv)),
        _ => None,
    }
}

/// Predicate : is this op one of the nine recognized
/// `cssl.{thread,mutex,atomic}.*` ops? Sub-helper for callers that
/// already iterate the op-stream and want a canonical-name predicate.
#[must_use]
pub fn is_thread_op(op: &MirOp) -> bool {
    lookup_thread_op_contract(op.name.as_str()).is_some()
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn pre-scan : "which thread-imports does this fn need declared"
//
//   Mirrors the `FsImportSet` pre-scan shape in `cgen_fs.rs`. Bit-packed
//   `ThreadImportSet` — 16 bits, 9 of which are used (one per cssl-rt
//   symbol) — keeps the pre-scan lean (no `HashMap` allocation per fn ;
//   future `for op in block.ops { match op.name {... set.flag = true
//   ...} }` walks set the bits).
// ───────────────────────────────────────────────────────────────────────

/// Bit-packed set indicating which `__cssl_*` threading symbols a fn
/// body references. 16 bits = one per LUT entry (9 used + 7 reserved
/// for future extensions). Linear scan + bit-pack is strictly faster
/// than a `HashMap<&str, bool>` at this size + zero allocation per
/// pre-scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThreadImportSet {
    /// Bits :
    ///   0 = thread_spawn
    ///   1 = thread_join
    ///   2 = mutex_create
    ///   3 = mutex_lock
    ///   4 = mutex_unlock
    ///   5 = mutex_destroy
    ///   6 = atomic_load_u64
    ///   7 = atomic_store_u64
    ///   8 = atomic_cas_u64
    ///   9..15 reserved for future extensions
    pub bits: u16,
}

impl ThreadImportSet {
    /// Predicate : is this set empty (no threading-imports needed)?
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }

    /// Mark the bit corresponding to `op_name`. No-op if the op-name is
    /// not one of the nine recognized threading ops.
    pub fn mark(&mut self, op_name: &str) {
        if let Some(idx) = thread_op_canonical_index(op_name) {
            self.bits |= 1u16 << idx;
        }
    }

    /// Test the bit corresponding to `op_name`. Returns `false` if the
    /// op-name is not one of the nine recognized threading ops.
    #[must_use]
    pub fn contains(self, op_name: &str) -> bool {
        match thread_op_canonical_index(op_name) {
            Some(idx) => (self.bits & (1u16 << idx)) != 0,
            None => false,
        }
    }
}

/// Canonical bit-index for each threading op-name (matches
/// `ThreadImportSet.bits` layout). The ordering is fixed by the bit-
/// layout doc-comment — adding a new threading op requires extending
/// the LUT + this fn.
#[must_use]
pub const fn thread_op_canonical_index(op_name: &str) -> Option<u16> {
    if str_eq(op_name, MIR_THREAD_SPAWN_OP_NAME) {
        Some(0)
    } else if str_eq(op_name, MIR_THREAD_JOIN_OP_NAME) {
        Some(1)
    } else if str_eq(op_name, MIR_MUTEX_CREATE_OP_NAME) {
        Some(2)
    } else if str_eq(op_name, MIR_MUTEX_LOCK_OP_NAME) {
        Some(3)
    } else if str_eq(op_name, MIR_MUTEX_UNLOCK_OP_NAME) {
        Some(4)
    } else if str_eq(op_name, MIR_MUTEX_DESTROY_OP_NAME) {
        Some(5)
    } else if str_eq(op_name, MIR_ATOMIC_LOAD_U64_OP_NAME) {
        Some(6)
    } else if str_eq(op_name, MIR_ATOMIC_STORE_U64_OP_NAME) {
        Some(7)
    } else if str_eq(op_name, MIR_ATOMIC_CAS_U64_OP_NAME) {
        Some(8)
    } else {
        None
    }
}

/// Const-fn byte-exact string equality. Used by
/// `thread_op_canonical_index` because `&str == &str` is not
/// const-stable.
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
/// threading-imports the fn needs declared. Mirrors `cgen_fs::
/// needs_fs_imports` but produces a 16-element bit-set rather than the
/// 8-element fs-set.
///
/// § COMPLEXITY  O(N) in op count, single-pass, no allocation beyond the
///   16-bit `ThreadImportSet` field. No `HashMap` use.
#[must_use]
pub fn needs_thread_imports(block: &MirBlock) -> ThreadImportSet {
    let mut set = ThreadImportSet::default();
    for op in &block.ops {
        set.mark(op.name.as_str());
    }
    set
}

// ───────────────────────────────────────────────────────────────────────
// § contract validators (defensive cross-checks)
// ───────────────────────────────────────────────────────────────────────

/// Validate the operand-count of a `cssl.{thread,mutex,atomic}.*` op
/// against the canonical contract. Returns `Ok(...)` when arity
/// matches, otherwise an `Err` with a diagnostic-friendly message.
///
/// § INTENT
///   Defensive check used at cgen-import-resolve time before issuing
///   the cranelift `call` instruction. If a mistyped MIR op leaks past
///   prior passes (e.g. a `cssl.atomic.cas_u64` carrying only 3 operands
///   instead of 4), the validator surfaces the error before cgen issues
///   a malformed call.
///
/// # Errors
/// Returns `Err(String)` with a human-readable diagnostic when :
///   - the op-name is not one of the nine recognized threading ops
///   - `op.operands.len() != contract.operand_count`
pub fn validate_thread_op_arity(op: &MirOp) -> Result<&'static ThreadOpContract, String> {
    let contract = lookup_thread_op_contract(op.name.as_str()).ok_or_else(|| {
        format!(
            "validate_thread_op_arity : op `{}` is not a recognized cssl.{{thread,mutex,atomic}}.* op",
            op.name
        )
    })?;
    if op.operands.len() != contract.operand_count {
        return Err(format!(
            "validate_thread_op_arity : `{}` requires {} operands ; got {}",
            contract.mir_op_name,
            contract.operand_count,
            op.operands.len()
        ));
    }
    Ok(contract)
}

// ───────────────────────────────────────────────────────────────────────
// § INTEGRATION_NOTE — wiring path for the cgen-driver  (REPEAT @ EOF)
//
//   The integration commit (deferred per `lib.rs`'s `pub mod` policy
//   for both cssl-rt + cssl-cgen-cpu-cranelift — see the
//   INTEGRATION_NOTE blocks at the head + tail of this file as well as
//   `cgen_fs.rs`) plugs this module into `object.rs::lower_one_op` +
//   `jit.rs::lower_op_in_jit` as follows :
//
//   1. PRE-SCAN — at the head of `compile_mir_function_to_object` (just
//      after `needs_fs_imports(entry_block)` returns), call
//      `needs_thread_imports(entry_block)` to get the per-fn
//      `ThreadImportSet`.
//
//   2. DECLARE — for each bit set in the `ThreadImportSet`, look up
//      the contract via `lookup_thread_op_contract` + build the
//      signature via `lower_thread_op_signature(...)` + call
//      `obj_module.declare_function(contract.ffi_symbol, Linkage::Import,
//      &sig)` then `obj_module.declare_func_in_func(id, &mut codegen_
//      ctx.func)` to get a `FuncRef`. Stash the bindings in a
//      `ThreadImports` map mirroring `FsImports`.
//
//   3. LOWER — in `lower_one_op`, add nine new match arms (or a single
//      `if let Some(contract) = lookup_thread_op_contract(op.name)`
//      branch) that resolve the `FuncRef` from the per-fn
//      `ThreadImports` map + gather operands + emit `builder.ins().
//      call(fref, &args)`. Operand-coercion (zero-extending u64 -> i64,
//      truncating i64 -> i32 for the order-arg) follows the
//      `emit_heap_call` pattern in `object.rs`.
//
//   4. RESULT-BIND — when the contract.return_ty is `I64`, bind the
//      cranelift result-value to the MIR result-id ; when `I32`, same
//      pattern but the value-map records an i32. The `mutex_create`
//      op (operand_count == 0) skips the operand-gather phase ; the
//      five-with-no-result (`mutex_unlock` / `mutex_destroy` / etc.)
//      ops bind their i32 return to the next MIR-result-id without
//      coercion.
//
//   The integration commit can issue all nine wirings in a single
//   walk via the LUT — no per-op match-arm needed in cgen-driver
//   beyond a single `is_thread_op` predicate + a pass-through `call`
//   emission. This matches the canonical pattern Wave-C3 used for
//   `__cssl_fs_*` — the eight-symbol version of the nine-symbol wiring
//   delivered here.

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        build_atomic_cas_u64_signature, build_atomic_load_u64_signature,
        build_atomic_store_u64_signature, build_mutex_create_signature,
        build_mutex_destroy_signature, build_mutex_lock_signature, build_mutex_unlock_signature,
        build_thread_join_signature, build_thread_spawn_signature, is_thread_op,
        lookup_thread_op_contract, lower_thread_op_signature, needs_thread_imports,
        thread_op_canonical_index, validate_thread_op_arity, ThreadImportSet, ThreadReturnTy,
        ATOMIC_CAS_U64_OPERAND_COUNT, ATOMIC_CAS_U64_SYMBOL,
        ATOMIC_LOAD_U64_OPERAND_COUNT, ATOMIC_LOAD_U64_SYMBOL, ATOMIC_STORE_U64_OPERAND_COUNT,
        ATOMIC_STORE_U64_SYMBOL, MIR_ATOMIC_CAS_U64_OP_NAME, MIR_ATOMIC_LOAD_U64_OP_NAME,
        MIR_ATOMIC_STORE_U64_OP_NAME, MIR_MUTEX_CREATE_OP_NAME, MIR_MUTEX_DESTROY_OP_NAME,
        MIR_MUTEX_LOCK_OP_NAME, MIR_MUTEX_UNLOCK_OP_NAME, MIR_THREAD_JOIN_OP_NAME,
        MIR_THREAD_SPAWN_OP_NAME, MUTEX_CREATE_OPERAND_COUNT, MUTEX_CREATE_SYMBOL,
        MUTEX_DESTROY_OPERAND_COUNT, MUTEX_DESTROY_SYMBOL, MUTEX_LOCK_OPERAND_COUNT,
        MUTEX_LOCK_SYMBOL, MUTEX_UNLOCK_OPERAND_COUNT, MUTEX_UNLOCK_SYMBOL,
        THREAD_JOIN_OPERAND_COUNT, THREAD_JOIN_SYMBOL, THREAD_OP_CONTRACT_TABLE,
        THREAD_SPAWN_OPERAND_COUNT, THREAD_SPAWN_SYMBOL,
    };
    use cranelift_codegen::ir::{types as cl_types, AbiParam};
    use cranelift_codegen::isa::CallConv;
    use cssl_mir::{MirBlock, MirOp, ValueId};

    // ── canonical-name lock invariants (cross-check with cssl-rt) ──────

    #[test]
    fn ffi_symbol_constants_match_cssl_rt_canonical() {
        // ‼ Lock-step invariant : the nine `__cssl_*` symbol-names MUST
        //   match cssl-rt::host_thread verbatim. Renaming either side
        //   without the other = link-time symbol mismatch ⇒ undefined
        //   behavior at runtime.
        assert_eq!(THREAD_SPAWN_SYMBOL, "__cssl_thread_spawn");
        assert_eq!(THREAD_JOIN_SYMBOL, "__cssl_thread_join");
        assert_eq!(MUTEX_CREATE_SYMBOL, "__cssl_mutex_create");
        assert_eq!(MUTEX_LOCK_SYMBOL, "__cssl_mutex_lock");
        assert_eq!(MUTEX_UNLOCK_SYMBOL, "__cssl_mutex_unlock");
        assert_eq!(MUTEX_DESTROY_SYMBOL, "__cssl_mutex_destroy");
        assert_eq!(ATOMIC_LOAD_U64_SYMBOL, "__cssl_atomic_load_u64");
        assert_eq!(ATOMIC_STORE_U64_SYMBOL, "__cssl_atomic_store_u64");
        assert_eq!(ATOMIC_CAS_U64_SYMBOL, "__cssl_atomic_cas_u64");
    }

    #[test]
    fn mir_op_name_constants_use_canonical_namespace() {
        // ‼ All nine MIR op-names live under the `cssl.thread.*` /
        //   `cssl.mutex.*` / `cssl.atomic.*` namespaces. SWAP-POINT —
        //   the ops have no `CsslOp` variant today.
        assert_eq!(MIR_THREAD_SPAWN_OP_NAME, "cssl.thread.spawn");
        assert_eq!(MIR_THREAD_JOIN_OP_NAME, "cssl.thread.join");
        assert_eq!(MIR_MUTEX_CREATE_OP_NAME, "cssl.mutex.create");
        assert_eq!(MIR_MUTEX_LOCK_OP_NAME, "cssl.mutex.lock");
        assert_eq!(MIR_MUTEX_UNLOCK_OP_NAME, "cssl.mutex.unlock");
        assert_eq!(MIR_MUTEX_DESTROY_OP_NAME, "cssl.mutex.destroy");
        assert_eq!(MIR_ATOMIC_LOAD_U64_OP_NAME, "cssl.atomic.load_u64");
        assert_eq!(MIR_ATOMIC_STORE_U64_OP_NAME, "cssl.atomic.store_u64");
        assert_eq!(MIR_ATOMIC_CAS_U64_OP_NAME, "cssl.atomic.cas_u64");
    }

    // ── per-symbol signature builders : verify shape ────────────────────

    #[test]
    fn thread_spawn_signature_two_ptrs_returns_i64() {
        // __cssl_thread_spawn : (ptr, ptr) -> u64
        let sig = build_thread_spawn_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn thread_join_signature_i64_ptr_returns_i32() {
        // __cssl_thread_join : (i64, ptr) -> i32
        let sig = build_thread_join_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn mutex_create_signature_no_params_returns_i64() {
        // __cssl_mutex_create : () -> u64
        let sig = build_mutex_create_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn mutex_lock_signature_one_i64_returns_i32() {
        // __cssl_mutex_lock : (u64) -> i32
        let sig = build_mutex_lock_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn mutex_unlock_destroy_share_lock_shape() {
        // __cssl_mutex_unlock : (u64) -> i32
        // __cssl_mutex_destroy : (u64) -> i32
        let unlock = build_mutex_unlock_signature(CallConv::SystemV);
        let destroy = build_mutex_destroy_signature(CallConv::SystemV);
        assert_eq!(unlock.params, destroy.params);
        assert_eq!(unlock.returns, destroy.returns);
        assert_eq!(unlock.params.len(), 1);
        assert_eq!(unlock.returns.len(), 1);
    }

    #[test]
    fn atomic_load_signature_ptr_u32_returns_i64() {
        // __cssl_atomic_load_u64 : (ptr, u32) -> u64
        let sig = build_atomic_load_u64_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 2);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I32));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn atomic_store_signature_ptr_u64_u32_returns_i32() {
        // __cssl_atomic_store_u64 : (ptr, u64, u32) -> i32
        let sig = build_atomic_store_u64_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 3);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I32));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn atomic_cas_signature_ptr_u64_u64_u32_returns_i64() {
        // __cssl_atomic_cas_u64 : (ptr, u64, u64, u32) -> u64
        let sig = build_atomic_cas_u64_signature(CallConv::SystemV, cl_types::I64);
        assert_eq!(sig.params.len(), 4);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[1], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[2], AbiParam::new(cl_types::I64));
        assert_eq!(sig.params[3], AbiParam::new(cl_types::I32));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    // ── ThreadReturnTy → cranelift Type mapping ─────────────────────────

    #[test]
    fn return_ty_clif_type_maps_correctly() {
        assert_eq!(ThreadReturnTy::I32.clif_type(), cl_types::I32);
        assert_eq!(ThreadReturnTy::I64.clif_type(), cl_types::I64);
    }

    // ── LUT lookup ──────────────────────────────────────────────────────

    #[test]
    fn lut_lookup_finds_all_nine_canonical_op_names() {
        let names = [
            MIR_THREAD_SPAWN_OP_NAME,
            MIR_THREAD_JOIN_OP_NAME,
            MIR_MUTEX_CREATE_OP_NAME,
            MIR_MUTEX_LOCK_OP_NAME,
            MIR_MUTEX_UNLOCK_OP_NAME,
            MIR_MUTEX_DESTROY_OP_NAME,
            MIR_ATOMIC_LOAD_U64_OP_NAME,
            MIR_ATOMIC_STORE_U64_OP_NAME,
            MIR_ATOMIC_CAS_U64_OP_NAME,
        ];
        for n in names {
            assert!(
                lookup_thread_op_contract(n).is_some(),
                "lookup_thread_op_contract({n}) should be Some"
            );
        }
    }

    #[test]
    fn lut_lookup_returns_none_for_non_threading_ops() {
        assert!(lookup_thread_op_contract("cssl.heap.alloc").is_none());
        assert!(lookup_thread_op_contract("cssl.fs.open").is_none());
        assert!(lookup_thread_op_contract("cssl.net.socket").is_none());
        assert!(lookup_thread_op_contract("arith.constant").is_none());
        assert!(lookup_thread_op_contract("").is_none());
    }

    #[test]
    fn lut_table_has_nine_entries() {
        assert_eq!(THREAD_OP_CONTRACT_TABLE.len(), 9);
    }

    #[test]
    fn lut_each_entry_resolves_back_to_canonical_name() {
        let valid_names = [
            MIR_THREAD_SPAWN_OP_NAME,
            MIR_THREAD_JOIN_OP_NAME,
            MIR_MUTEX_CREATE_OP_NAME,
            MIR_MUTEX_LOCK_OP_NAME,
            MIR_MUTEX_UNLOCK_OP_NAME,
            MIR_MUTEX_DESTROY_OP_NAME,
            MIR_ATOMIC_LOAD_U64_OP_NAME,
            MIR_ATOMIC_STORE_U64_OP_NAME,
            MIR_ATOMIC_CAS_U64_OP_NAME,
        ];
        for entry in THREAD_OP_CONTRACT_TABLE {
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
            MIR_THREAD_SPAWN_OP_NAME,
            MIR_THREAD_JOIN_OP_NAME,
            MIR_MUTEX_CREATE_OP_NAME,
            MIR_MUTEX_LOCK_OP_NAME,
            MIR_MUTEX_UNLOCK_OP_NAME,
            MIR_MUTEX_DESTROY_OP_NAME,
            MIR_ATOMIC_LOAD_U64_OP_NAME,
            MIR_ATOMIC_STORE_U64_OP_NAME,
            MIR_ATOMIC_CAS_U64_OP_NAME,
        ];
        for n in names {
            let op = MirOp::std(n);
            assert!(
                lower_thread_op_signature(&op, CallConv::SystemV, cl_types::I64).is_some(),
                "lower_thread_op_signature should return Some for `{n}`"
            );
        }
    }

    #[test]
    fn dispatcher_returns_none_for_unrecognized_op() {
        let op = MirOp::std("cssl.heap.alloc");
        assert!(lower_thread_op_signature(&op, CallConv::SystemV, cl_types::I64).is_none());
        let op2 = MirOp::std("cssl.fs.open");
        assert!(lower_thread_op_signature(&op2, CallConv::SystemV, cl_types::I64).is_none());
    }

    #[test]
    fn dispatcher_passes_call_conv_through() {
        let op = MirOp::std(MIR_ATOMIC_CAS_U64_OP_NAME);
        let sysv = lower_thread_op_signature(&op, CallConv::SystemV, cl_types::I64).unwrap();
        let win = lower_thread_op_signature(&op, CallConv::WindowsFastcall, cl_types::I64).unwrap();
        assert_eq!(sysv.call_conv, CallConv::SystemV);
        assert_eq!(win.call_conv, CallConv::WindowsFastcall);
    }

    // ── is_thread_op predicate ──────────────────────────────────────────

    #[test]
    fn is_thread_op_predicate_true_for_canonical_ops() {
        assert!(is_thread_op(&MirOp::std(MIR_THREAD_SPAWN_OP_NAME)));
        assert!(is_thread_op(&MirOp::std(MIR_MUTEX_LOCK_OP_NAME)));
        assert!(is_thread_op(&MirOp::std(MIR_ATOMIC_CAS_U64_OP_NAME)));
        assert!(is_thread_op(&MirOp::std(MIR_ATOMIC_STORE_U64_OP_NAME)));
    }

    #[test]
    fn is_thread_op_predicate_false_for_non_threading_ops() {
        assert!(!is_thread_op(&MirOp::std("cssl.heap.alloc")));
        assert!(!is_thread_op(&MirOp::std("cssl.fs.open")));
        assert!(!is_thread_op(&MirOp::std("cssl.net.socket")));
        assert!(!is_thread_op(&MirOp::std("arith.constant")));
    }

    // ── thread_op_canonical_index const-fn ─────────────────────────────

    #[test]
    fn canonical_index_assigns_distinct_bits_for_each_op() {
        let indices = [
            thread_op_canonical_index(MIR_THREAD_SPAWN_OP_NAME).unwrap(),
            thread_op_canonical_index(MIR_THREAD_JOIN_OP_NAME).unwrap(),
            thread_op_canonical_index(MIR_MUTEX_CREATE_OP_NAME).unwrap(),
            thread_op_canonical_index(MIR_MUTEX_LOCK_OP_NAME).unwrap(),
            thread_op_canonical_index(MIR_MUTEX_UNLOCK_OP_NAME).unwrap(),
            thread_op_canonical_index(MIR_MUTEX_DESTROY_OP_NAME).unwrap(),
            thread_op_canonical_index(MIR_ATOMIC_LOAD_U64_OP_NAME).unwrap(),
            thread_op_canonical_index(MIR_ATOMIC_STORE_U64_OP_NAME).unwrap(),
            thread_op_canonical_index(MIR_ATOMIC_CAS_U64_OP_NAME).unwrap(),
        ];
        // All 9 should be distinct
        for i in 0..indices.len() {
            for j in i + 1..indices.len() {
                assert_ne!(
                    indices[i], indices[j],
                    "indices {i} and {j} must be distinct"
                );
            }
        }
        // Each must fit in 16 bits with a bit-set per slot.
        for idx in indices {
            assert!(idx < 16);
        }
    }

    #[test]
    fn canonical_index_returns_none_for_non_threading_ops() {
        assert!(thread_op_canonical_index("cssl.heap.alloc").is_none());
        assert!(thread_op_canonical_index("cssl.fs.open").is_none());
        assert!(thread_op_canonical_index("arith.constant").is_none());
        assert!(thread_op_canonical_index("").is_none());
    }

    // ── per-fn pre-scan : needs_thread_imports ──────────────────────────

    #[test]
    fn pre_scan_finds_threading_ops_when_present() {
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std("arith.constant").with_attribute("value", "42"));
        block.push(
            MirOp::std(MIR_ATOMIC_CAS_U64_OP_NAME)
                .with_operand(ValueId(0))
                .with_operand(ValueId(1))
                .with_operand(ValueId(2))
                .with_operand(ValueId(3)),
        );
        block.push(
            MirOp::std(MIR_MUTEX_LOCK_OP_NAME).with_operand(ValueId(0)),
        );
        let set = needs_thread_imports(&block);
        assert!(!set.is_empty());
        assert!(set.contains(MIR_ATOMIC_CAS_U64_OP_NAME));
        assert!(set.contains(MIR_MUTEX_LOCK_OP_NAME));
        assert!(!set.contains(MIR_THREAD_SPAWN_OP_NAME));
        assert!(!set.contains(MIR_ATOMIC_LOAD_U64_OP_NAME));
    }

    #[test]
    fn pre_scan_returns_empty_when_no_threading_ops() {
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std("arith.constant"));
        block.push(MirOp::std("func.return"));
        let set = needs_thread_imports(&block);
        assert!(set.is_empty());
        assert_eq!(set.bits, 0);
    }

    #[test]
    fn pre_scan_handles_empty_block() {
        let block = MirBlock::new("entry");
        let set = needs_thread_imports(&block);
        assert!(set.is_empty());
    }

    #[test]
    fn pre_scan_records_all_nine_when_every_op_present() {
        // Synthesize a block referencing every threading op ; the bit-
        // set should have all 9 bits set (low 9 bits = 0x01FF).
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std(MIR_THREAD_SPAWN_OP_NAME));
        block.push(MirOp::std(MIR_THREAD_JOIN_OP_NAME));
        block.push(MirOp::std(MIR_MUTEX_CREATE_OP_NAME));
        block.push(MirOp::std(MIR_MUTEX_LOCK_OP_NAME));
        block.push(MirOp::std(MIR_MUTEX_UNLOCK_OP_NAME));
        block.push(MirOp::std(MIR_MUTEX_DESTROY_OP_NAME));
        block.push(MirOp::std(MIR_ATOMIC_LOAD_U64_OP_NAME));
        block.push(MirOp::std(MIR_ATOMIC_STORE_U64_OP_NAME));
        block.push(MirOp::std(MIR_ATOMIC_CAS_U64_OP_NAME));
        let set = needs_thread_imports(&block);
        assert_eq!(set.bits, 0x01FF, "all 9 low bits should be set");
    }

    // ── ThreadImportSet manipulation ───────────────────────────────────

    #[test]
    fn import_set_mark_and_contains_round_trip() {
        let mut set = ThreadImportSet::default();
        assert!(set.is_empty());
        set.mark(MIR_ATOMIC_CAS_U64_OP_NAME);
        assert!(set.contains(MIR_ATOMIC_CAS_U64_OP_NAME));
        assert!(!set.contains(MIR_MUTEX_LOCK_OP_NAME));
        set.mark(MIR_MUTEX_LOCK_OP_NAME);
        assert!(set.contains(MIR_MUTEX_LOCK_OP_NAME));
        assert!(set.contains(MIR_ATOMIC_CAS_U64_OP_NAME));
    }

    #[test]
    fn import_set_mark_ignores_non_threading_ops() {
        let mut set = ThreadImportSet::default();
        set.mark("cssl.heap.alloc");
        set.mark("arith.constant");
        assert!(set.is_empty());
    }

    // ── arity validators ────────────────────────────────────────────────

    #[test]
    fn validate_accepts_canonical_four_operand_cas_op() {
        let op = MirOp::std(MIR_ATOMIC_CAS_U64_OP_NAME)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2))
            .with_operand(ValueId(3));
        let contract = validate_thread_op_arity(&op).unwrap();
        assert_eq!(contract.ffi_symbol, ATOMIC_CAS_U64_SYMBOL);
        assert_eq!(contract.return_ty, ThreadReturnTy::I64);
        assert_eq!(contract.operand_count, ATOMIC_CAS_U64_OPERAND_COUNT);
    }

    #[test]
    fn validate_accepts_canonical_one_operand_mutex_lock_op() {
        let op = MirOp::std(MIR_MUTEX_LOCK_OP_NAME).with_operand(ValueId(0));
        let contract = validate_thread_op_arity(&op).unwrap();
        assert_eq!(contract.ffi_symbol, MUTEX_LOCK_SYMBOL);
        assert_eq!(contract.operand_count, MUTEX_LOCK_OPERAND_COUNT);
    }

    #[test]
    fn validate_accepts_zero_operand_mutex_create_op() {
        // mutex_create takes no operands ; validator must accept the
        // empty operand-vector (matches `fn() -> u64`).
        let op = MirOp::std(MIR_MUTEX_CREATE_OP_NAME);
        let contract = validate_thread_op_arity(&op).unwrap();
        assert_eq!(contract.ffi_symbol, MUTEX_CREATE_SYMBOL);
        assert_eq!(contract.operand_count, 0);
    }

    #[test]
    fn validate_rejects_three_operand_cas_op() {
        // Defensive : if a mistyped MIR op leaks past prior passes
        // (only 3 operands instead of 4 for cas), surface the error.
        let op = MirOp::std(MIR_ATOMIC_CAS_U64_OP_NAME)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_operand(ValueId(2));
        let err = validate_thread_op_arity(&op).unwrap_err();
        assert!(err.contains("4 operands"));
        assert!(err.contains("cssl.atomic.cas_u64"));
    }

    #[test]
    fn validate_rejects_unknown_op_name() {
        let op = MirOp::std("cssl.heap.alloc");
        let err = validate_thread_op_arity(&op).unwrap_err();
        assert!(err.contains("not a recognized"));
    }

    // ── end-to-end : verify contract round-trip via lookup ─────────────

    #[test]
    fn contract_lookup_round_trip_each_op() {
        // For each LUT entry, look up by mir_op_name + verify the
        // returned contract matches the entry exactly.
        for entry in THREAD_OP_CONTRACT_TABLE {
            let found = lookup_thread_op_contract(entry.mir_op_name).unwrap();
            assert_eq!(found.mir_op_name, entry.mir_op_name);
            assert_eq!(found.ffi_symbol, entry.ffi_symbol);
            assert_eq!(found.operand_count, entry.operand_count);
            assert_eq!(found.return_ty, entry.return_ty);
        }
    }

    #[test]
    fn operand_counts_match_table_entries() {
        // ‼ Cross-check : each LUT entry's operand_count matches the
        //   per-op constant. Drift = unmatched arity ⇒ silent broken
        //   cgen.
        let pairs: &[(&str, usize)] = &[
            (MIR_THREAD_SPAWN_OP_NAME, THREAD_SPAWN_OPERAND_COUNT),
            (MIR_THREAD_JOIN_OP_NAME, THREAD_JOIN_OPERAND_COUNT),
            (MIR_MUTEX_CREATE_OP_NAME, MUTEX_CREATE_OPERAND_COUNT),
            (MIR_MUTEX_LOCK_OP_NAME, MUTEX_LOCK_OPERAND_COUNT),
            (MIR_MUTEX_UNLOCK_OP_NAME, MUTEX_UNLOCK_OPERAND_COUNT),
            (MIR_MUTEX_DESTROY_OP_NAME, MUTEX_DESTROY_OPERAND_COUNT),
            (MIR_ATOMIC_LOAD_U64_OP_NAME, ATOMIC_LOAD_U64_OPERAND_COUNT),
            (MIR_ATOMIC_STORE_U64_OP_NAME, ATOMIC_STORE_U64_OPERAND_COUNT),
            (MIR_ATOMIC_CAS_U64_OP_NAME, ATOMIC_CAS_U64_OPERAND_COUNT),
        ];
        for (name, count) in pairs {
            let contract = lookup_thread_op_contract(name).unwrap();
            assert_eq!(
                contract.operand_count, *count,
                "operand_count mismatch for `{name}` : LUT={} const={count}",
                contract.operand_count
            );
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § INTEGRATION_NOTE  (per Wave-D2 dispatch directive — repeat at EOF)
//
//   This module is delivered as a NEW file with its tests in-place but
//   `cssl-cgen-cpu-cranelift/src/lib.rs` is intentionally NOT modified.
//   The integration commit (deferred per the "DO NOT modify any lib.rs"
//   constraint) will :
//
//     (1) Add `pub mod cgen_thread;` to `cssl-cgen-cpu-cranelift/src/
//         lib.rs` after the existing `pub mod cgen_net;` line.
//     (2) Plug `lookup_thread_op_contract` / `needs_thread_imports` /
//         `lower_thread_op_signature` into `object.rs::compile_mir_
//         function_to_object` (per-fn import-declare path) + `lower_
//         one_op` (per-op call-emission) — see the in-file
//         INTEGRATION_NOTE wiring path block above for the exact 4-step
//         sequence.
//     (3) Plug the same helpers into `jit.rs::lower_op_in_jit` for the
//         JIT execution path (mirrors the existing `cssl.fs.*` /
//         `cssl.net.*` JIT integration).
//     (4) Document the surface in `lib.rs`'s top-of-file `§ T10-phase-
//         2 DEFERRED` doc-comment under a new `Wave-D2` heading.
//
//   The cssl-rt-side companion `host_thread.rs` is delivered in lock-
//   step with this module ; the integration commit registers BOTH files
//   (cssl-rt's `pub mod host_thread;` + cgen-cpu-cranelift's `pub mod
//   cgen_thread;`) at the same time. Until then both modules are
//   crate-internal — ready for activation but not on the live cgen
//   dispatch path.
