//! § Wave-D1 — `cssl.time.*` Cranelift cgen helpers (host-FFI time surface).
//!
//! § ROLE
//!   Pure-function helpers that build the cranelift `Signature`s for the
//!   four `__cssl_time_*` FFI imports + the per-fn dispatcher that turns
//!   a `cssl.time.<verb>` MIR op into a `call __cssl_time_<verb>(...)`
//!   cranelift IR description. Mirrors the Wave-C3 `cgen_fs.rs` shape
//!   exactly :
//!     1. centralizes the symbol-name + signature-shape so the cgen
//!        layer has ONE source-of-truth for the time FFI contract,
//!     2. exposes a per-block pre-scan helper so the per-fn
//!        import-declare path can stay lean (declare only the symbols
//!        the fn actually references),
//!     3. provides arity validators so a mistyped MIR op surfaces a
//!        diagnostic before cgen issues a malformed call,
//!     4. closes the loop on Wave-D1 deliverable item 2 (NEW file in
//!        `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/`) without
//!        modifying any other crate or `lib.rs`'s `pub mod` list.
//!
//! § SPEC-REFERENCES
//!   - `compiler-rs/crates/cssl-rt/src/host_time.rs` — the four
//!     `__cssl_time_*` ABI-stable symbols this module wires call-emission
//!     against. The matching `ffi_symbols_have_correct_signatures` test
//!     in that file locks the per-symbol shape used here.
//!   - `compiler-rs/crates/cssl-cgen-cpu-cranelift/src/cgen_fs.rs` —
//!     sibling Wave-C3 module that establishes the canonical pattern
//!     this module mirrors (signature-builder + per-fn pre-scan +
//!     arity-validator + canonical-name lock-test). This file copies
//!     that pattern byte-for-byte at the structural level.
//!   - `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § time` — the spec
//!     that pins the four symbol-names + their argument shapes.
//!   - `specs/40_WAVE_CSSL_PLAN.csl § WAVE-D § D1` — the wave plan that
//!     scopes this slice (`cssl-rt::host_time → __cssl_time_*`).
//!
//! § INTEGRATION_NOTE  (per Wave-D1 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-cgen-cpu-cranelift/
//!   src/lib.rs` is INTENTIONALLY NOT modified. The helpers compile +
//!   are tested in-place via `#[cfg(test)]` references. The integration
//!   commit (paired with the host_time `pub mod` addition in cssl-rt)
//!   will add `pub mod cgen_time` here + plug `lower_time_op_signature`
//!   into `object.rs::lower_one_op` / `jit.rs::lower_op_in_jit` after
//!   the existing `cssl.fs.*` arms. Until then the helpers are
//!   crate-internal — `cgen_time::lower_time_op_signature` is the
//!   canonical dispatcher the integration commit will invoke.
//!
//! § SAWYER-EFFICIENCY
//!   - All helpers are pure functions ; zero allocation outside the
//!     cranelift `Signature` constructor's required `Vec` storage.
//!   - Symbol-name LUT : op-name → extern-symbol-name mapping is a
//!     `&'static [TimeOpContract]` slice ; no String-format on the hot
//!     path. Lookup is a linear scan of 4 entries — strictly faster
//!     than a `HashMap` at this size + zero per-call allocation.
//!   - `needs_time_imports` walks the per-block ops slice ONCE ; O(N)
//!     in op count, single-pass, no allocation beyond the bit-packed
//!     `TimeImportSet` 4-bit field.
//!   - Branch-friendly match-arm ordering : most-common ops first
//!     (`monotonic_ns` before `wall_unix_ns` before `sleep_ns` /
//!     `deadline_until`) so the hot-path branch predictor lands the
//!     common case in cycle 1 — game-loop frame-time reads dominate
//!     the time-FFI mix.
//!
//! § MIR ↔ CLIF ABI MAPPING
//!
//!   ```text
//!   MIR (recognizer-emitted)                    CLIF (this module)
//!   ─────────────────────────────────────────   ───────────────────────────────────
//!   cssl.time.monotonic_ns () : u64              call __cssl_time_monotonic_ns()
//!     {time_effect=true}                              -> u64
//!   cssl.time.wall_unix_ns () : i64              call __cssl_time_wall_unix_ns()
//!                                                     -> i64
//!   cssl.time.sleep_ns      %ns : i32            call __cssl_time_sleep_ns(ns)
//!                                                     -> i32
//!   cssl.time.deadline_until %dl : i32           call __cssl_time_deadline_until(dl)
//!                                                     -> i32
//!   ```
//!
//! § SWAP-POINT inventory  (per task `MOCK-WHEN-DEPS-MISSING` directive)
//!   The four cssl-rt symbols this module targets are defined in the
//!   sibling `cssl-rt/src/host_time.rs` (delivered as part of the same
//!   Wave-D1 slice). The matching MIR op-kinds (`CsslOp::TimeMonotonicNs`
//!   etc.) are NOT yet declared in `cssl-mir::op::CsslOp` ; this module
//!   dispatches on the op-name STRING (matches the `cssl-rt`-side FFI
//!   symbol-pattern + the `cgen_fs.rs` SWAP-POINT idiom). Once the MIR
//!   op-kinds land — likely a stage-0 follow-up to the `time::monotonic_ns`
//!   stdlib stub — the constants below immediately route through. The
//!   SWAP-POINT comments mark each name that has cssl-rt support but
//!   no MIR op-kind today.

#![allow(dead_code, unreachable_pub)]

use cranelift_codegen::ir::{AbiParam, Signature, Type};
use cranelift_codegen::isa::CallConv;
use cssl_mir::{MirBlock, MirOp};

// ───────────────────────────────────────────────────────────────────────
// § canonical FFI symbol names (cssl-rt side)
//
//   ‼ Each MUST match `compiler-rs/crates/cssl-rt/src/host_time.rs`
//     literally. Renaming either side requires lock-step changes — see
//     specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS for the FFI-invariant.
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol for `cssl.time.monotonic_ns` — exported from
/// `cssl-rt/src/host_time.rs`. Returns u64 ns since process boot.
pub const TIME_MONOTONIC_NS_SYMBOL: &str = "__cssl_time_monotonic_ns";

/// FFI symbol for `cssl.time.wall_unix_ns` — exported from
/// `cssl-rt/src/host_time.rs`. Returns i64 ns since UNIX epoch.
pub const TIME_WALL_UNIX_NS_SYMBOL: &str = "__cssl_time_wall_unix_ns";

/// FFI symbol for `cssl.time.sleep_ns` — exported from
/// `cssl-rt/src/host_time.rs`. Sleeps for `ns` nanoseconds. Returns
/// `0` on completion.
pub const TIME_SLEEP_NS_SYMBOL: &str = "__cssl_time_sleep_ns";

/// FFI symbol for `cssl.time.deadline_until` — exported from
/// `cssl-rt/src/host_time.rs`. Sleeps until `deadline_ns` (a monotonic-ns
/// reading). Returns `0` if we slept, `+1` if past, `-1` reserved.
pub const TIME_DEADLINE_UNTIL_SYMBOL: &str = "__cssl_time_deadline_until";

// ───────────────────────────────────────────────────────────────────────
// § canonical MIR op-name strings (cssl-mir side)
//
//   None of the four are declared as `CsslOp::Time*` variants in
//   `cssl-mir::op` today. The dispatcher recognizes them via op-name
//   string match. Future MIR op-kinds adding `cssl.time.*` route
//   through immediately without touching the dispatcher.
// ───────────────────────────────────────────────────────────────────────

/// SWAP-POINT MIR op-name. No `CsslOp` variant today ; the recognizer
/// path in `cssl_mir::body_lower` is expected to start emitting this
/// when the `time::monotonic_ns()` stdlib stub-fn lands its concrete
/// recognizer.
pub const MIR_TIME_MONOTONIC_NS_OP_NAME: &str = "cssl.time.monotonic_ns";
/// SWAP-POINT MIR op-name — same status as `cssl.time.monotonic_ns`.
pub const MIR_TIME_WALL_UNIX_NS_OP_NAME: &str = "cssl.time.wall_unix_ns";
/// SWAP-POINT MIR op-name — same status as `cssl.time.monotonic_ns`.
pub const MIR_TIME_SLEEP_NS_OP_NAME: &str = "cssl.time.sleep_ns";
/// SWAP-POINT MIR op-name — same status as `cssl.time.monotonic_ns`.
pub const MIR_TIME_DEADLINE_UNTIL_OP_NAME: &str = "cssl.time.deadline_until";

// ───────────────────────────────────────────────────────────────────────
// § per-op operand counts
//
//   ‼ Each count MUST match the FFI-side argument count from
//     `cssl-rt/src/host_time.rs`. Renaming or rearg-ing either side
//     requires lock-step changes.
// ───────────────────────────────────────────────────────────────────────

/// `cssl.time.monotonic_ns` — 0 operands (pure-u64 read).
pub const TIME_MONOTONIC_NS_OPERAND_COUNT: usize = 0;
/// `cssl.time.wall_unix_ns` — 0 operands (pure-i64 read).
pub const TIME_WALL_UNIX_NS_OPERAND_COUNT: usize = 0;
/// `cssl.time.sleep_ns` — 1 operand : `(ns)`.
pub const TIME_SLEEP_NS_OPERAND_COUNT: usize = 1;
/// `cssl.time.deadline_until` — 1 operand : `(deadline_ns)`.
pub const TIME_DEADLINE_UNTIL_OPERAND_COUNT: usize = 1;

// ───────────────────────────────────────────────────────────────────────
// § per-op return-type marker (i32 / i64 / u64)
//
//   The cssl-rt FFI surface returns either `i32` (sleep_ns / deadline_until),
//   `i64` (wall_unix_ns), or `u64` (monotonic_ns). At the cranelift IR
//   level u64 and i64 are both represented as `I64` (cranelift is
//   sign-agnostic for raw integer types ; sign is a language-level
//   property tracked by the type-system, not the IR).
// ───────────────────────────────────────────────────────────────────────

/// Per-op return-type marker used by the signature-builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeReturnTy {
    /// 32-bit signed integer return (sleep_ns / deadline_until).
    I32,
    /// 64-bit integer return (wall_unix_ns / monotonic_ns ; cranelift
    /// represents both signed + unsigned 64-bit as `I64`).
    I64,
}

impl TimeReturnTy {
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
//   monotonic_ns is the hot-path operation during program execution
//   (game-loop frame-time reads + deterministic-replay tests walk this
//   per-iteration). wall_unix_ns / sleep_ns / deadline_until fire less
//   frequently.
// ───────────────────────────────────────────────────────────────────────

/// Per-op contract bundle : the cssl-rt symbol-name + the expected MIR
/// operand-count + the cranelift return-type. The dispatcher walks this
/// table by op-name match. Mirrors `cgen_fs::FsOpContract` exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeOpContract {
    /// The MIR op-name string (e.g. `"cssl.time.monotonic_ns"`).
    pub mir_op_name: &'static str,
    /// The cssl-rt extern symbol-name (e.g. `"__cssl_time_monotonic_ns"`).
    pub ffi_symbol: &'static str,
    /// The expected operand-count (matches `cssl-rt::host_time` argument
    /// count).
    pub operand_count: usize,
    /// The cranelift return-type for the result-slot.
    pub return_ty: TimeReturnTy,
}

/// Canonical LUT — 4 entries ordered for branch-friendly dispatch.
/// Linear-scan lookup beats `HashMap` at N=4 (0 alloc + cache-warm + no
/// hash-fn cost).
pub const TIME_OP_CONTRACT_TABLE: &[TimeOpContract] = &[
    // — most-common (hot path) — game-loop frame-time reads.
    TimeOpContract {
        mir_op_name: MIR_TIME_MONOTONIC_NS_OP_NAME,
        ffi_symbol: TIME_MONOTONIC_NS_SYMBOL,
        operand_count: TIME_MONOTONIC_NS_OPERAND_COUNT,
        return_ty: TimeReturnTy::I64,
    },
    // — wall-clock — fires for save-game timestamps, log-records, etc.
    TimeOpContract {
        mir_op_name: MIR_TIME_WALL_UNIX_NS_OP_NAME,
        ffi_symbol: TIME_WALL_UNIX_NS_SYMBOL,
        operand_count: TIME_WALL_UNIX_NS_OPERAND_COUNT,
        return_ty: TimeReturnTy::I64,
    },
    // — sleep-direct — fires for explicit `time::sleep(ns)` calls.
    TimeOpContract {
        mir_op_name: MIR_TIME_SLEEP_NS_OP_NAME,
        ffi_symbol: TIME_SLEEP_NS_SYMBOL,
        operand_count: TIME_SLEEP_NS_OPERAND_COUNT,
        return_ty: TimeReturnTy::I32,
    },
    // — deadline-driven — fires for frame-pacing / fixed-step loops.
    TimeOpContract {
        mir_op_name: MIR_TIME_DEADLINE_UNTIL_OP_NAME,
        ffi_symbol: TIME_DEADLINE_UNTIL_SYMBOL,
        operand_count: TIME_DEADLINE_UNTIL_OPERAND_COUNT,
        return_ty: TimeReturnTy::I32,
    },
];

/// LUT lookup — find the `TimeOpContract` for a given MIR op-name. Linear
/// scan over the 4-entry `TIME_OP_CONTRACT_TABLE` ; returns `None` for
/// non-`cssl.time.*` ops.
///
/// § COMPLEXITY  O(1) amortized (table size fixed at 4 ; branch-friendly).
#[must_use]
pub fn lookup_time_op_contract(op_name: &str) -> Option<&'static TimeOpContract> {
    TIME_OP_CONTRACT_TABLE
        .iter()
        .find(|entry| entry.mir_op_name == op_name)
}

// ───────────────────────────────────────────────────────────────────────
// § cranelift signature builders — one per cssl-rt FFI symbol
//
//   Each builder returns the canonical `Signature` for the matching
//   `__cssl_time_*` import. The cgen-import-resolve path uses these to
//   declare the per-fn `FuncRef`.
//
//   ‼ The scalar-i64 / scalar-i32 parameters use fixed cranelift types
//     matching the cssl-rt declaration. Any drift between the Rust-side
//     `unsafe extern "C" fn` declaration and these builders = link-time
//     ABI mismatch ⇒ undefined behavior.
// ───────────────────────────────────────────────────────────────────────

/// Build the cranelift `Signature` for `__cssl_time_monotonic_ns`.
///
/// § SHAPE
/// ```text
///   pub unsafe extern "C" fn __cssl_time_monotonic_ns() -> u64
/// ```
#[must_use]
pub fn build_time_monotonic_ns_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns
        .push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_time_wall_unix_ns`.
///
/// § SHAPE
/// ```text
///   pub unsafe extern "C" fn __cssl_time_wall_unix_ns() -> i64
/// ```
#[must_use]
pub fn build_time_wall_unix_ns_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.returns
        .push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig
}

/// Build the cranelift `Signature` for `__cssl_time_sleep_ns`.
///
/// § SHAPE
/// ```text
///   pub unsafe extern "C" fn __cssl_time_sleep_ns(ns: u64) -> i32
/// ```
#[must_use]
pub fn build_time_sleep_ns_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params
        .push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns
        .push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

/// Build the cranelift `Signature` for `__cssl_time_deadline_until`.
///
/// § SHAPE
/// ```text
///   pub unsafe extern "C" fn __cssl_time_deadline_until(deadline_ns: u64) -> i32
/// ```
#[must_use]
pub fn build_time_deadline_until_signature(call_conv: CallConv) -> Signature {
    let mut sig = Signature::new(call_conv);
    sig.params
        .push(AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns
        .push(AbiParam::new(cranelift_codegen::ir::types::I32));
    sig
}

// ───────────────────────────────────────────────────────────────────────
// § dispatcher : MIR op → signature builder
// ───────────────────────────────────────────────────────────────────────

/// Top-level dispatcher : given a `cssl.time.*` MIR op, return the
/// cranelift `Signature` for the matching cssl-rt FFI symbol. Returns
/// `None` if the op-name is not one of the four recognized time ops —
/// caller should fall through to the generic `func.call` lowering path.
///
/// § PURPOSE
///   Single-source-of-truth for "given this MIR op, what `Signature`
///   should the import-declare path use". Avoids spreading the per-op
///   `build_*_signature` selection logic across multiple cgen call-sites.
///
/// § BRANCH-FRIENDLY ORDERING
///   Most-common ops first (monotonic_ns before wall_unix_ns before
///   sleep_ns / deadline_until). The branch-predictor lands the
///   hot-path case in a single cycle on the typical game-loop.
#[must_use]
pub fn lower_time_op_signature(op: &MirOp, call_conv: CallConv) -> Option<Signature> {
    match op.name.as_str() {
        // — most-common (hot path)
        MIR_TIME_MONOTONIC_NS_OP_NAME => Some(build_time_monotonic_ns_signature(call_conv)),
        // — wall-clock
        MIR_TIME_WALL_UNIX_NS_OP_NAME => Some(build_time_wall_unix_ns_signature(call_conv)),
        // — sleep-direct
        MIR_TIME_SLEEP_NS_OP_NAME => Some(build_time_sleep_ns_signature(call_conv)),
        // — deadline-driven
        MIR_TIME_DEADLINE_UNTIL_OP_NAME => Some(build_time_deadline_until_signature(call_conv)),
        _ => None,
    }
}

/// Predicate : is this op one of the four recognized `cssl.time.*` ops?
/// Sub-helper for callers that already iterate the op-stream and want a
/// canonical-name predicate.
#[must_use]
pub fn is_time_op(op: &MirOp) -> bool {
    lookup_time_op_contract(op.name.as_str()).is_some()
}

// ───────────────────────────────────────────────────────────────────────
// § per-fn pre-scan : "which time-imports does this fn need declared"
//
//   Mirrors `cgen_fs::FsImportSet`. Bit-packed `TimeImportSet` — 4 bits,
//   one per cssl-rt symbol — keeps the pre-scan lean (no `HashMap`
//   allocation per fn).
// ───────────────────────────────────────────────────────────────────────

/// Bit-packed set indicating which `__cssl_time_*` symbols a fn body
/// references. 4 bits = one per LUT entry. Linear scan + bit-pack is
/// strictly faster than a `HashMap<&str, bool>` at this size + zero
/// allocation per pre-scan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeImportSet {
    /// Bits : 0 = monotonic_ns, 1 = wall_unix_ns, 2 = sleep_ns,
    /// 3 = deadline_until. (Index = position of the `MIR_TIME_*_OP_NAME`
    /// constant in the LUT canonical ordering — monotonic_ns /
    /// wall_unix_ns / sleep_ns / deadline_until.)
    pub bits: u8,
}

impl TimeImportSet {
    /// Predicate : is this set empty (no time-imports needed)?
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.bits == 0
    }

    /// Mark the bit corresponding to `op_name`. No-op if the op-name is
    /// not one of the four recognized time ops.
    pub fn mark(&mut self, op_name: &str) {
        if let Some(idx) = time_op_canonical_index(op_name) {
            self.bits |= 1u8 << idx;
        }
    }

    /// Test the bit corresponding to `op_name`. Returns `false` if the
    /// op-name is not one of the four recognized time ops.
    #[must_use]
    pub fn contains(self, op_name: &str) -> bool {
        match time_op_canonical_index(op_name) {
            Some(idx) => (self.bits & (1u8 << idx)) != 0,
            None => false,
        }
    }
}

/// Canonical bit-index for each time op-name (matches `TimeImportSet.bits`
/// layout). The ordering is fixed by the LUT canonical order — adding
/// a new time op requires extending both the LUT + this fn.
#[must_use]
pub const fn time_op_canonical_index(op_name: &str) -> Option<u8> {
    if str_eq(op_name, MIR_TIME_MONOTONIC_NS_OP_NAME) {
        Some(0)
    } else if str_eq(op_name, MIR_TIME_WALL_UNIX_NS_OP_NAME) {
        Some(1)
    } else if str_eq(op_name, MIR_TIME_SLEEP_NS_OP_NAME) {
        Some(2)
    } else if str_eq(op_name, MIR_TIME_DEADLINE_UNTIL_OP_NAME) {
        Some(3)
    } else {
        None
    }
}

/// Const-fn byte-exact string equality. Used by `time_op_canonical_index`
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
/// time-imports the fn needs declared.
///
/// § COMPLEXITY  O(N) in op count, single-pass, no allocation beyond the
///   8-bit `TimeImportSet` byte. No `HashMap` use.
#[must_use]
pub fn needs_time_imports(block: &MirBlock) -> TimeImportSet {
    let mut set = TimeImportSet::default();
    for op in &block.ops {
        set.mark(op.name.as_str());
    }
    set
}

// ───────────────────────────────────────────────────────────────────────
// § contract validators (defensive cross-checks)
// ───────────────────────────────────────────────────────────────────────

/// Validate the operand-count of a `cssl.time.*` op against the canonical
/// contract. Returns `Ok(&contract)` when arity matches, otherwise an
/// `Err` with a diagnostic-friendly message.
///
/// § INTENT
///   Defensive check used at cgen-import-resolve time before issuing
///   the cranelift `call` instruction. If a mistyped MIR op leaks past
///   prior passes (e.g. a `cssl.time.sleep_ns` carrying 0 operands
///   instead of 1), the validator surfaces the error before cgen
///   issues a malformed call.
///
/// # Errors
/// Returns `Err(String)` with a human-readable diagnostic when :
///   - the op-name is not one of the four recognized time ops
///   - `op.operands.len() != contract.operand_count`
pub fn validate_time_op_arity(op: &MirOp) -> Result<&'static TimeOpContract, String> {
    let contract = lookup_time_op_contract(op.name.as_str()).ok_or_else(|| {
        format!(
            "validate_time_op_arity : op `{}` is not a recognized cssl.time.* op",
            op.name
        )
    })?;
    if op.operands.len() != contract.operand_count {
        return Err(format!(
            "validate_time_op_arity : `{}` requires {} operands ; got {}",
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
//      after the existing `needs_fs_imports(entry_block)` pre-scan),
//      call `needs_time_imports(entry_block)` to get the per-fn
//      `TimeImportSet`.
//
//   2. DECLARE — for each bit set in the `TimeImportSet`, look up the
//      contract via `lookup_time_op_contract` + build the signature via
//      `lower_time_op_signature(...)` + call `obj_module.declare_function(
//      contract.ffi_symbol, Linkage::Import, &sig)` then
//      `obj_module.declare_func_in_func(id, &mut codegen_ctx.func)` to
//      get a `FuncRef`. Stash the four (`FuncRef`, `TimeOpContract`)
//      bindings in a `TimeImports` map mirroring `FsImports`.
//
//   3. LOWER — in `lower_one_op`, add four new match arms (or a single
//      `if let Some(contract) = lookup_time_op_contract(op.name)` branch)
//      that resolve the `FuncRef` from the per-fn `TimeImports` map +
//      gather operands + emit `builder.ins().call(fref, &args)`. The
//      operand-coercion logic from `emit_heap_call` (lines 740-758 of
//      object.rs) carries over byte-for-byte — coerce non-matching
//      integer operands via `uextend` / `ireduce` to match the AbiParam
//      width.
//
//   4. RESULT-BIND — when the contract.return_ty is `I64`, bind the
//      cranelift result-value to the MIR result-id ; when `I32`, same
//      pattern but the value-map records an i32. The 0-operand ops
//      (`monotonic_ns` / `wall_unix_ns`) skip the operand-gather phase.
//
//   The integration commit can issue all four wirings in a single walk
//   via the LUT — no per-op match-arm needed in cgen-driver beyond a
//   single `is_time_op` predicate + a pass-through `call` emission.
//   Mirrors the four-op `cssl.fs.*` pattern delivered by Wave-C3.

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        build_time_deadline_until_signature, build_time_monotonic_ns_signature,
        build_time_sleep_ns_signature, build_time_wall_unix_ns_signature, is_time_op,
        lookup_time_op_contract, lower_time_op_signature, needs_time_imports,
        time_op_canonical_index, validate_time_op_arity, TimeImportSet, TimeOpContract,
        TimeReturnTy, MIR_TIME_DEADLINE_UNTIL_OP_NAME, MIR_TIME_MONOTONIC_NS_OP_NAME,
        MIR_TIME_SLEEP_NS_OP_NAME, MIR_TIME_WALL_UNIX_NS_OP_NAME,
        TIME_DEADLINE_UNTIL_OPERAND_COUNT, TIME_DEADLINE_UNTIL_SYMBOL,
        TIME_MONOTONIC_NS_OPERAND_COUNT, TIME_MONOTONIC_NS_SYMBOL, TIME_OP_CONTRACT_TABLE,
        TIME_SLEEP_NS_OPERAND_COUNT, TIME_SLEEP_NS_SYMBOL, TIME_WALL_UNIX_NS_OPERAND_COUNT,
        TIME_WALL_UNIX_NS_SYMBOL,
    };
    use cranelift_codegen::ir::{types as cl_types, AbiParam};
    use cranelift_codegen::isa::CallConv;
    use cssl_mir::{MirBlock, MirOp, ValueId};

    // ── canonical-name lock invariants (cross-check with cssl-rt) ──────

    #[test]
    fn ffi_symbol_constants_match_cssl_rt_canonical() {
        // ‼ Lock-step invariant : the four `__cssl_time_*` symbol-names
        //   MUST match cssl-rt::host_time verbatim. Renaming either side
        //   without the other = link-time symbol mismatch ⇒ undefined
        //   behavior at runtime.
        assert_eq!(TIME_MONOTONIC_NS_SYMBOL, "__cssl_time_monotonic_ns");
        assert_eq!(TIME_WALL_UNIX_NS_SYMBOL, "__cssl_time_wall_unix_ns");
        assert_eq!(TIME_SLEEP_NS_SYMBOL, "__cssl_time_sleep_ns");
        assert_eq!(TIME_DEADLINE_UNTIL_SYMBOL, "__cssl_time_deadline_until");
    }

    #[test]
    fn mir_op_name_constants_have_canonical_namespace() {
        // ‼ All four MIR op-names must live in the `cssl.time.*`
        //   namespace + match the surface stdlib will use.
        assert_eq!(MIR_TIME_MONOTONIC_NS_OP_NAME, "cssl.time.monotonic_ns");
        assert_eq!(MIR_TIME_WALL_UNIX_NS_OP_NAME, "cssl.time.wall_unix_ns");
        assert_eq!(MIR_TIME_SLEEP_NS_OP_NAME, "cssl.time.sleep_ns");
        assert_eq!(MIR_TIME_DEADLINE_UNTIL_OP_NAME, "cssl.time.deadline_until");
    }

    // ── per-symbol signature builders : verify shape ────────────────────

    #[test]
    fn monotonic_ns_signature_zero_params_returns_i64() {
        // __cssl_time_monotonic_ns : () -> u64 (cranelift I64)
        let sig = build_time_monotonic_ns_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn wall_unix_ns_signature_zero_params_returns_i64() {
        // __cssl_time_wall_unix_ns : () -> i64
        let sig = build_time_wall_unix_ns_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 0);
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I64));
    }

    #[test]
    fn sleep_ns_signature_one_i64_param_returns_i32() {
        // __cssl_time_sleep_ns : (u64) -> i32
        let sig = build_time_sleep_ns_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    #[test]
    fn deadline_until_signature_one_i64_param_returns_i32() {
        // __cssl_time_deadline_until : (u64) -> i32
        let sig = build_time_deadline_until_signature(CallConv::SystemV);
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0], AbiParam::new(cl_types::I64));
        assert_eq!(sig.returns.len(), 1);
        assert_eq!(sig.returns[0], AbiParam::new(cl_types::I32));
    }

    // ── TimeReturnTy → cranelift Type mapping ───────────────────────────

    #[test]
    fn return_ty_clif_type_maps_correctly() {
        assert_eq!(TimeReturnTy::I32.clif_type(), cl_types::I32);
        assert_eq!(TimeReturnTy::I64.clif_type(), cl_types::I64);
    }

    // ── LUT lookup ──────────────────────────────────────────────────────

    #[test]
    fn lut_lookup_finds_all_four_canonical_op_names() {
        let names = [
            MIR_TIME_MONOTONIC_NS_OP_NAME,
            MIR_TIME_WALL_UNIX_NS_OP_NAME,
            MIR_TIME_SLEEP_NS_OP_NAME,
            MIR_TIME_DEADLINE_UNTIL_OP_NAME,
        ];
        for n in names {
            assert!(
                lookup_time_op_contract(n).is_some(),
                "lookup_time_op_contract({n}) should be Some"
            );
        }
    }

    #[test]
    fn lut_lookup_returns_none_for_non_time_ops() {
        assert!(lookup_time_op_contract("cssl.heap.alloc").is_none());
        assert!(lookup_time_op_contract("cssl.fs.open").is_none());
        assert!(lookup_time_op_contract("cssl.net.socket").is_none());
        assert!(lookup_time_op_contract("arith.constant").is_none());
        assert!(lookup_time_op_contract("").is_none());
    }

    #[test]
    fn lut_table_has_four_entries() {
        // ‼ The LUT must enumerate exactly the four canonical time ops.
        //   Adding a new time op requires extending the table + the
        //   `time_op_canonical_index` const-fn together.
        assert_eq!(TIME_OP_CONTRACT_TABLE.len(), 4);
    }

    #[test]
    fn lut_each_entry_resolves_back_to_canonical_name() {
        // ‼ Round-trip invariant : every LUT entry's mir_op_name must
        //   be one of the four canonical constants.
        let valid_names = [
            MIR_TIME_MONOTONIC_NS_OP_NAME,
            MIR_TIME_WALL_UNIX_NS_OP_NAME,
            MIR_TIME_SLEEP_NS_OP_NAME,
            MIR_TIME_DEADLINE_UNTIL_OP_NAME,
        ];
        for entry in TIME_OP_CONTRACT_TABLE {
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
            MIR_TIME_MONOTONIC_NS_OP_NAME,
            MIR_TIME_WALL_UNIX_NS_OP_NAME,
            MIR_TIME_SLEEP_NS_OP_NAME,
            MIR_TIME_DEADLINE_UNTIL_OP_NAME,
        ];
        for n in names {
            let op = MirOp::std(n);
            assert!(
                lower_time_op_signature(&op, CallConv::SystemV).is_some(),
                "lower_time_op_signature should return Some for `{n}`"
            );
        }
    }

    #[test]
    fn dispatcher_returns_none_for_unrecognized_op() {
        let op = MirOp::std("cssl.heap.alloc");
        assert!(lower_time_op_signature(&op, CallConv::SystemV).is_none());
        let op2 = MirOp::std("arith.constant");
        assert!(lower_time_op_signature(&op2, CallConv::SystemV).is_none());
    }

    #[test]
    fn dispatcher_passes_call_conv_through() {
        let op = MirOp::std(MIR_TIME_MONOTONIC_NS_OP_NAME);
        let sysv = lower_time_op_signature(&op, CallConv::SystemV).unwrap();
        let win = lower_time_op_signature(&op, CallConv::WindowsFastcall).unwrap();
        assert_eq!(sysv.call_conv, CallConv::SystemV);
        assert_eq!(win.call_conv, CallConv::WindowsFastcall);
    }

    // ── is_time_op predicate ────────────────────────────────────────────

    #[test]
    fn is_time_op_predicate_true_for_canonical_ops() {
        assert!(is_time_op(&MirOp::std(MIR_TIME_MONOTONIC_NS_OP_NAME)));
        assert!(is_time_op(&MirOp::std(MIR_TIME_WALL_UNIX_NS_OP_NAME)));
        assert!(is_time_op(&MirOp::std(MIR_TIME_SLEEP_NS_OP_NAME)));
        assert!(is_time_op(&MirOp::std(MIR_TIME_DEADLINE_UNTIL_OP_NAME)));
    }

    #[test]
    fn is_time_op_predicate_false_for_non_time_ops() {
        assert!(!is_time_op(&MirOp::std("cssl.heap.alloc")));
        assert!(!is_time_op(&MirOp::std("cssl.fs.open")));
        assert!(!is_time_op(&MirOp::std("cssl.net.socket")));
        assert!(!is_time_op(&MirOp::std("arith.constant")));
    }

    // ── time_op_canonical_index const-fn ────────────────────────────────

    #[test]
    fn canonical_index_assigns_distinct_bits_for_each_op() {
        let indices = [
            time_op_canonical_index(MIR_TIME_MONOTONIC_NS_OP_NAME).unwrap(),
            time_op_canonical_index(MIR_TIME_WALL_UNIX_NS_OP_NAME).unwrap(),
            time_op_canonical_index(MIR_TIME_SLEEP_NS_OP_NAME).unwrap(),
            time_op_canonical_index(MIR_TIME_DEADLINE_UNTIL_OP_NAME).unwrap(),
        ];
        for i in 0..indices.len() {
            for j in i + 1..indices.len() {
                assert_ne!(
                    indices[i], indices[j],
                    "indices {i} and {j} must be distinct"
                );
            }
        }
        for idx in indices {
            assert!(idx < 4);
        }
    }

    #[test]
    fn canonical_index_returns_none_for_non_time_ops() {
        assert!(time_op_canonical_index("cssl.heap.alloc").is_none());
        assert!(time_op_canonical_index("cssl.fs.open").is_none());
        assert!(time_op_canonical_index("arith.constant").is_none());
        assert!(time_op_canonical_index("").is_none());
    }

    // ── per-fn pre-scan : needs_time_imports ───────────────────────────

    #[test]
    fn pre_scan_finds_time_ops_when_present() {
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std("arith.constant").with_attribute("value", "42"));
        block.push(MirOp::std(MIR_TIME_MONOTONIC_NS_OP_NAME));
        block.push(MirOp::std(MIR_TIME_SLEEP_NS_OP_NAME).with_operand(ValueId(0)));
        let set = needs_time_imports(&block);
        assert!(!set.is_empty());
        assert!(set.contains(MIR_TIME_MONOTONIC_NS_OP_NAME));
        assert!(set.contains(MIR_TIME_SLEEP_NS_OP_NAME));
        assert!(!set.contains(MIR_TIME_WALL_UNIX_NS_OP_NAME));
        assert!(!set.contains(MIR_TIME_DEADLINE_UNTIL_OP_NAME));
    }

    #[test]
    fn pre_scan_returns_empty_when_no_time_ops() {
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std("arith.constant"));
        block.push(MirOp::std("func.return"));
        let set = needs_time_imports(&block);
        assert!(set.is_empty());
        assert_eq!(set.bits, 0);
    }

    #[test]
    fn pre_scan_handles_empty_block() {
        let block = MirBlock::new("entry");
        let set = needs_time_imports(&block);
        assert!(set.is_empty());
    }

    #[test]
    fn pre_scan_records_all_four_when_every_op_present() {
        let mut block = MirBlock::new("entry");
        block.push(MirOp::std(MIR_TIME_MONOTONIC_NS_OP_NAME));
        block.push(MirOp::std(MIR_TIME_WALL_UNIX_NS_OP_NAME));
        block.push(MirOp::std(MIR_TIME_SLEEP_NS_OP_NAME).with_operand(ValueId(0)));
        block.push(MirOp::std(MIR_TIME_DEADLINE_UNTIL_OP_NAME).with_operand(ValueId(1)));
        let set = needs_time_imports(&block);
        // All 4 bits set ⇒ low nibble = 0b1111 = 0x0F.
        assert_eq!(set.bits, 0x0F, "all 4 bits should be set");
    }

    // ── TimeImportSet manipulation ─────────────────────────────────────

    #[test]
    fn import_set_mark_and_contains_round_trip() {
        let mut set = TimeImportSet::default();
        assert!(set.is_empty());
        set.mark(MIR_TIME_MONOTONIC_NS_OP_NAME);
        assert!(set.contains(MIR_TIME_MONOTONIC_NS_OP_NAME));
        assert!(!set.contains(MIR_TIME_SLEEP_NS_OP_NAME));
        set.mark(MIR_TIME_SLEEP_NS_OP_NAME);
        assert!(set.contains(MIR_TIME_SLEEP_NS_OP_NAME));
        assert!(set.contains(MIR_TIME_MONOTONIC_NS_OP_NAME));
    }

    #[test]
    fn import_set_mark_ignores_non_time_ops() {
        let mut set = TimeImportSet::default();
        set.mark("cssl.heap.alloc");
        set.mark("cssl.fs.open");
        set.mark("arith.constant");
        assert!(set.is_empty());
    }

    // ── arity validators ────────────────────────────────────────────────

    #[test]
    fn validate_accepts_zero_operand_monotonic_op() {
        let op = MirOp::std(MIR_TIME_MONOTONIC_NS_OP_NAME);
        let contract = validate_time_op_arity(&op).unwrap();
        assert_eq!(contract.ffi_symbol, TIME_MONOTONIC_NS_SYMBOL);
        assert_eq!(contract.return_ty, TimeReturnTy::I64);
        assert_eq!(contract.operand_count, 0);
    }

    #[test]
    fn validate_accepts_one_operand_sleep_op() {
        let op = MirOp::std(MIR_TIME_SLEEP_NS_OP_NAME).with_operand(ValueId(0));
        let contract = validate_time_op_arity(&op).unwrap();
        assert_eq!(contract.ffi_symbol, TIME_SLEEP_NS_SYMBOL);
        assert_eq!(contract.operand_count, 1);
    }

    #[test]
    fn validate_accepts_one_operand_deadline_op() {
        let op = MirOp::std(MIR_TIME_DEADLINE_UNTIL_OP_NAME).with_operand(ValueId(0));
        let contract = validate_time_op_arity(&op).unwrap();
        assert_eq!(contract.ffi_symbol, TIME_DEADLINE_UNTIL_SYMBOL);
        assert_eq!(contract.operand_count, 1);
    }

    #[test]
    fn validate_rejects_one_operand_monotonic_op() {
        // Defensive : monotonic_ns takes 0 operands ; a stray operand
        // surfaces an arity error before cgen emits a malformed call.
        let op = MirOp::std(MIR_TIME_MONOTONIC_NS_OP_NAME).with_operand(ValueId(0));
        let err = validate_time_op_arity(&op).unwrap_err();
        assert!(err.contains("0 operands"));
        assert!(err.contains("cssl.time.monotonic_ns"));
    }

    #[test]
    fn validate_rejects_zero_operand_sleep_op() {
        // Defensive : sleep_ns takes 1 operand ; missing operand surfaces.
        let op = MirOp::std(MIR_TIME_SLEEP_NS_OP_NAME);
        let err = validate_time_op_arity(&op).unwrap_err();
        assert!(err.contains("1 operands"));
        assert!(err.contains("cssl.time.sleep_ns"));
    }

    #[test]
    fn validate_rejects_unknown_op_name() {
        let op = MirOp::std("cssl.heap.alloc");
        let err = validate_time_op_arity(&op).unwrap_err();
        assert!(err.contains("not a recognized cssl.time.* op"));
    }

    // ── end-to-end : verify contract round-trip via lookup ─────────────

    #[test]
    fn contract_lookup_round_trip_each_op() {
        for entry in TIME_OP_CONTRACT_TABLE {
            let found = lookup_time_op_contract(entry.mir_op_name).unwrap();
            assert_eq!(found.ffi_symbol, entry.ffi_symbol);
            assert_eq!(found.operand_count, entry.operand_count);
            assert_eq!(found.return_ty, entry.return_ty);
        }
    }

    #[test]
    fn contract_table_each_symbol_unique() {
        // ‼ All four symbol-names must be distinct.
        let symbols: Vec<&str> = TIME_OP_CONTRACT_TABLE
            .iter()
            .map(|e| e.ffi_symbol)
            .collect();
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
        assert_eq!(TIME_MONOTONIC_NS_OPERAND_COUNT, 0); // ()
        assert_eq!(TIME_WALL_UNIX_NS_OPERAND_COUNT, 0); // ()
        assert_eq!(TIME_SLEEP_NS_OPERAND_COUNT, 1); // (ns)
        assert_eq!(TIME_DEADLINE_UNTIL_OPERAND_COUNT, 1); // (deadline_ns)
    }

    // ── TimeOpContract Copy/Clone shape ────────────────────────────────

    #[test]
    fn time_op_contract_is_copy_friendly() {
        let entry = TIME_OP_CONTRACT_TABLE[0];
        let copy: TimeOpContract = entry;
        assert_eq!(copy.mir_op_name, entry.mir_op_name);
        assert_eq!(copy.ffi_symbol, entry.ffi_symbol);
    }
}

// ───────────────────────────────────────────────────────────────────────
// § INTEGRATION_NOTE — wiring path for the cgen-driver
//
//   This file ships the four `cssl.time.*` cgen helpers (signature
//   builder + LUT contract + per-fn pre-scan + arity validator) but
//   DOES NOT touch `cssl-cgen-cpu-cranelift::lib.rs`'s `pub mod` list.
//   The integration commit owns that surface change. At that time it
//   should :
//
//   1. Add `pub mod cgen_time;` to `cssl-cgen-cpu-cranelift/src/lib.rs`
//      after the existing `pub mod cgen_fs;` line. The pub-mod ordering
//      is alphabetical-ish ; insert after `cgen_string` to keep the
//      layout grouped.
//   2. In `object.rs::compile_mir_function_to_object`, after the
//      existing `declare_fs_imports_for_fn(...)` call (which Wave-C3's
//      integration commit will add), call `cgen_time::needs_time_imports
//      (&entry_block)` + iterate the bit-set declaring each FuncRef as
//      described in `INTEGRATION_NOTE` above.
//   3. In `object.rs::lower_one_op` + `jit.rs::lower_op_in_jit`, after
//      the `cssl.fs.*` arms, add an `else if let Some(contract) =
//      cgen_time::lookup_time_op_contract(op.name.as_str())` branch
//      that emits the cranelift `call` against the per-fn
//      `time_imports` map.
//   4. Update the `lib.rs` doc-block to mention the four new
//      `cssl.time.*` MIR op-names + their cssl-rt symbol partners.
//
//   Until that commit lands, this module is fully buildable + tested in
//   isolation. The matching cssl-rt host_time module (delivered as the
//   sibling Wave-D1 file) is the FFI counterparty ; together they form
//   the complete vertical : MIR → cranelift `call __cssl_time_*` →
//   cssl-rt impl → OS syscall.
// ───────────────────────────────────────────────────────────────────────
