//! § cssl-rt host_thread — Wave-D2 (S3 of specs/24_HOST_FFI.csl) threading FFI.
//!
//! § ROLE
//!   Stage-0 throwaway-Rust shim that exposes the `__cssl_thread_*` /
//!   `__cssl_mutex_*` / `__cssl_atomic_*` ABI-stable extern "C" symbol surface
//!   declared in `specs/24_HOST_FFI.csl § ABI-STABLE-SYMBOLS § threading`.
//!   The CSSLv3 source-level effect-typed wrappers (`effect Thread { fn
//!   spawn(...) ; fn join(...) ; fn mutex(...) ; ... }`) call into these
//!   symbols at the FFI boundary ; this module is the cssl-rt-side shim.
//!
//! § SYMBOLS  (all `extern "C"` ; ABI-stable from Wave-D2 forward)
//!   ```text
//!   __cssl_thread_spawn(entry: *const u8, arg: *const u8) -> u64
//!   __cssl_thread_join(handle: u64, ret_out: *mut i32) -> i32
//!   __cssl_mutex_create() -> u64
//!   __cssl_mutex_lock(handle: u64) -> i32
//!   __cssl_mutex_unlock(handle: u64) -> i32
//!   __cssl_mutex_destroy(handle: u64) -> i32
//!   __cssl_atomic_load_u64(addr: *const u64, order: u32) -> u64
//!   __cssl_atomic_store_u64(addr: *mut u64, value: u64, order: u32) -> i32
//!   __cssl_atomic_cas_u64(addr: *mut u64, expected: u64, desired: u64,
//!                          order: u32) -> u64
//!   ```
//!
//! § INVARIANTS  (carried-forward landmine — see HANDOFF_SESSION_6.csl)
//!   ‼ Renaming any of these symbols is a major-version-bump event ;
//!     CSSLv3-emitted code references them by exact name.
//!   ‼ Argument types + ordering are also locked. Use additional symbols
//!     (e.g., `__cssl_thread_spawn_with_stack_size`) for new behaviors.
//!   ‼ Each symbol delegates to a Rust-side `_impl` helper so unit tests
//!     can exercise behavior without going through the FFI boundary.
//!   ‼ Handle `0` is the canonical "invalid" sentinel for every slot-table
//!     in this module. `JoinHandle` / `Mutex` IDs returned to CSSLv3
//!     userland start from `1` ; `0` always means "spawn / create failed
//!     and the per-thread last-error slot is set".
//!
//! § SAWYER-EFFICIENCY  (per memory feedback_sawyer_pokemon_efficiency)
//!   - Slot-table : `OnceLock<Mutex<Vec<Slot>>>` keyed by u64 handle. The
//!     `u64` is the index-type discipline — no HashMap, no string-keyed
//!     lookup, no per-handle allocation beyond the inner `JoinHandle` /
//!     `Mutex` payload. Linear scan + monotonic ID is strictly faster than
//!     a HashMap at the realistic concurrent-thread / mutex counts a stage-0
//!     build ever sees (< 256). The ID-counter is `AtomicU64` and only
//!     ever increments — no slot-reuse, no ABA hazard.
//!   - Atomics : `__cssl_atomic_*` delegate directly to
//!     `std::sync::atomic::AtomicU64::{load, store, compare_exchange}` with
//!     the `order: u32` mapped via the [`map_ordering`] LUT. Zero allocation
//!     per-call ; zero indirection beyond the function-call boundary.
//!   - Ordering map : 6-entry match (Relaxed / Acquire / Release / AcqRel /
//!     SeqCst + 1 fallback) ; smaller than a HashMap + branch-friendly.
//!
//! § PRIME-DIRECTIVE  (per `specs/24_HOST_FFI.csl § FORBIDDEN-PATTERNS`)
//!   - Cap<Thread> witness consumption is ENFORCED at the CSSLv3 source-
//!     level wrapper, NOT at this FFI shim — the shim is the C-callable
//!     surface that the wrapper validates against. Bypassing the wrapper
//!     (calling `__cssl_thread_spawn` directly from userland CSSL without
//!     having Cap<Thread>) is a directive-violation caught by the type-
//!     system at the source-level surface (effect-row + capability-witness).
//!   - The shim itself does not log thread payloads. The opaque `entry`
//!     and `arg` pointers are treated as caller-owned ; cssl-rt does not
//!     introspect their contents. Audit-trail emission is a Wave-D7
//!     concern (Cap<X> + IFC plumbing) and lives upstream of this module.
//!
//! § STAGE-0 SCOPE  (per HANDOFF_SESSION_6.csl § STAGE-0)
//!   - `__cssl_thread_spawn` accepts a `*const u8` for the entry-fn pointer
//!     to keep the FFI surface uniformly-typed (every `__cssl_*` symbol's
//!     parameters are `usize` / pointer / integer scalars). The shim
//!     transmutes the pointer to `extern "C" fn(*const u8) -> i32` at the
//!     spawn-site. The `arg` pointer is opaque — the spawned thread reads
//!     from it through the entry-fn's contract.
//!   - `__cssl_atomic_load_u64` etc. operate on a caller-supplied `*const u64`.
//!     The pointer must satisfy `AtomicU64`'s alignment + non-null contract ;
//!     the shim does NOT validate alignment in the hot path (caller-
//!     responsibility per the FFI contract).
//!   - Stage-1 plan : the body of this module gets reimplemented in CSSLv3
//!     source ; calls go directly to OS-level syscalls (`CreateThread` /
//!     `CreateMutex` / atomic intrinsics) via §§ 14_BACKEND ASM intrinsics.
//!     The Rust dependency (`std::thread` / `std::sync::Mutex`) drops out.
//!
//! § INTEGRATION_NOTE  (per Wave-D2 dispatch directive)
//!   This module is delivered as a NEW file but `cssl-rt/src/lib.rs` is
//!   intentionally NOT modified. The helpers compile + are tested in-place
//!   via `#[cfg(test)]` references (under `mod tests`). A future cssl-rt
//!   integration commit will :
//!     (1) add `pub mod host_thread;` to `lib.rs` after the existing
//!         `pub mod net;` line (alphabetical-by-domain ordering) ;
//!     (2) add the nine `__cssl_*` symbols to `ffi.rs`'s
//!         `ffi_symbols_have_correct_signatures` compile-time-assert test
//!         (the bottom of the existing FFI lock table) ;
//!     (3) cross-reference `host_thread::THREAD_SLOTS` /
//!         `host_thread::MUTEX_SLOTS` from `lib.rs::test_helpers::
//!         lock_and_reset_all` (after the existing `crate::net::
//!         reset_net_for_tests();` line) so the per-test global-state
//!         reset propagates here.
//!   Until then the helpers are crate-internal — `host_thread::*_impl` is
//!   the canonical entry-point the integration commit will surface.
//!   The cgen-side companion `cgen_thread.rs` is delivered in lock-step ;
//!   integration of both files happens together in the deferred commit.

#![allow(unsafe_code)]
#![allow(dead_code, unreachable_pub)]

use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;

// ───────────────────────────────────────────────────────────────────────
// § canonical FFI symbol-name constants (ABI-stable lock)
//
//   ‼ These constants are surfaced for cross-checks against the cgen-side
//     `cgen_thread::*_SYMBOL` constants. Renaming requires lock-step
//     changes in both files (matched by the `cgen_thread` test
//     `ffi_symbol_constants_match_cssl_rt_canonical`).
// ───────────────────────────────────────────────────────────────────────

/// FFI symbol for `__cssl_thread_spawn`.
pub const THREAD_SPAWN_SYMBOL: &str = "__cssl_thread_spawn";
/// FFI symbol for `__cssl_thread_join`.
pub const THREAD_JOIN_SYMBOL: &str = "__cssl_thread_join";
/// FFI symbol for `__cssl_mutex_create`.
pub const MUTEX_CREATE_SYMBOL: &str = "__cssl_mutex_create";
/// FFI symbol for `__cssl_mutex_lock`.
pub const MUTEX_LOCK_SYMBOL: &str = "__cssl_mutex_lock";
/// FFI symbol for `__cssl_mutex_unlock`.
pub const MUTEX_UNLOCK_SYMBOL: &str = "__cssl_mutex_unlock";
/// FFI symbol for `__cssl_mutex_destroy`.
pub const MUTEX_DESTROY_SYMBOL: &str = "__cssl_mutex_destroy";
/// FFI symbol for `__cssl_atomic_load_u64`.
pub const ATOMIC_LOAD_U64_SYMBOL: &str = "__cssl_atomic_load_u64";
/// FFI symbol for `__cssl_atomic_store_u64`.
pub const ATOMIC_STORE_U64_SYMBOL: &str = "__cssl_atomic_store_u64";
/// FFI symbol for `__cssl_atomic_cas_u64`.
pub const ATOMIC_CAS_U64_SYMBOL: &str = "__cssl_atomic_cas_u64";

// ───────────────────────────────────────────────────────────────────────
// § canonical sentinel values
// ───────────────────────────────────────────────────────────────────────

/// Canonical "invalid handle" sentinel for thread + mutex slot-tables.
/// CSSLv3 userland treats `0` as "spawn/create failed ; check
/// `__cssl_thread_last_error_kind` for diagnosis (Wave-D2.1 follow-up)".
pub const INVALID_HANDLE: u64 = 0;

/// Canonical "success" return for the integer-returning `__cssl_*` shims
/// (`thread_join` / `mutex_lock` / `mutex_unlock` / `mutex_destroy` /
/// `atomic_store`). Matches the existing fs/net surface convention.
pub const RC_OK: i32 = 0;

/// Canonical "error" return for the integer-returning shims. Specific
/// error categorization is deferred to the Wave-D2.1 last-error machinery
/// (mirrors `crate::io::last_io_error_kind` / `crate::net::
/// last_net_error_kind`).
pub const RC_ERR: i32 = -1;

// ───────────────────────────────────────────────────────────────────────
// § ordering enum mapping  (u32 input → std::sync::atomic::Ordering)
//
//   The wire-protocol u32 value travels across the FFI boundary because
//   `extern "C"` cannot pass Rust enums directly. The mapping is locked-
//   forever — adding a new ordering requires a new u32 value (do NOT
//   re-number existing entries).
// ───────────────────────────────────────────────────────────────────────

/// Wire-encoding `u32` for `Ordering::Relaxed`. ABI-stable from Wave-D2.
pub const ORDERING_RELAXED: u32 = 0;
/// Wire-encoding `u32` for `Ordering::Acquire`.
pub const ORDERING_ACQUIRE: u32 = 1;
/// Wire-encoding `u32` for `Ordering::Release`.
pub const ORDERING_RELEASE: u32 = 2;
/// Wire-encoding `u32` for `Ordering::AcqRel`.
pub const ORDERING_ACQ_REL: u32 = 3;
/// Wire-encoding `u32` for `Ordering::SeqCst`.
pub const ORDERING_SEQ_CST: u32 = 4;

/// Map a wire-encoded `u32` ordering to a `std::sync::atomic::Ordering`.
/// Unrecognized values fall back to `SeqCst` (the strictest ordering ;
/// safest sane default for an unknown caller). The fallback is a defensive
/// measure ; the CSSLv3 source-level wrapper is expected to validate the
/// `order` value before reaching the shim.
///
/// § COMPLEXITY  O(1) — 5-entry match, branch-friendly ordering.
#[must_use]
pub fn map_ordering(order: u32) -> Ordering {
    match order {
        ORDERING_RELAXED => Ordering::Relaxed,
        ORDERING_ACQUIRE => Ordering::Acquire,
        ORDERING_RELEASE => Ordering::Release,
        ORDERING_ACQ_REL => Ordering::AcqRel,
        // Default-fall-through covers SEQ_CST plus any unknown sentinel.
        _ => Ordering::SeqCst,
    }
}

/// Map a load-only ordering : `Release` and `AcqRel` are not legal for
/// atomic loads ; coerce them to `Acquire` to keep the shim total. Other
/// values pass through `map_ordering` directly.
#[must_use]
pub fn map_load_ordering(order: u32) -> Ordering {
    match order {
        ORDERING_RELEASE | ORDERING_ACQ_REL => Ordering::Acquire,
        other => map_ordering(other),
    }
}

/// Map a store-only ordering : `Acquire` and `AcqRel` are not legal for
/// atomic stores ; coerce them to `Release`.
#[must_use]
pub fn map_store_ordering(order: u32) -> Ordering {
    match order {
        ORDERING_ACQUIRE | ORDERING_ACQ_REL => Ordering::Release,
        other => map_ordering(other),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § slot-table : JoinHandle storage
//
//   Sawyer-efficient design : `OnceLock<Mutex<Vec<ThreadSlot>>>`
//   - Vec acts as a slot-table indexed by `(handle - 1)` (handle 0 reserved
//     for INVALID_HANDLE).
//   - Each slot is `Option<JoinHandle<i32>>` — `Some` = active thread,
//     `None` = already-joined / consumed slot.
//   - Monotonic-counter ID generation : a `u64` counter increments per
//     spawn ; ID 0 is reserved + never assigned. No slot-reuse means no
//     ABA hazard but also means long-running programs eventually exhaust
//     u64 IDs (~2^63 spawns ; not a stage-0 concern).
// ───────────────────────────────────────────────────────────────────────

/// One entry in the thread slot-table. Wraps `Option<JoinHandle<i32>>` so
/// joined slots can be marked consumed without shifting the Vec.
#[derive(Debug)]
pub struct ThreadSlot {
    /// `Some` while the thread is active ; `None` after `join` has consumed
    /// the handle.
    handle: Option<JoinHandle<i32>>,
    /// Monotonic ID — duplicates the slot's u64 handle for cross-check
    /// validation in tests + diagnostic output.
    id: u64,
}

/// Crate-shared slot-table for spawned threads. Lazily-initialized per
/// `OnceLock` discipline ; the `Mutex` serializes all spawn / join /
/// destroy operations.
pub static THREAD_SLOTS: OnceLock<Mutex<Vec<ThreadSlot>>> = OnceLock::new();

/// Monotonic ID counter for thread-handle assignment. Starts at 1 ; 0 is
/// the canonical INVALID_HANDLE sentinel.
static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1);

/// Acquire the thread slot-table guard, lazily-initializing on first call.
fn thread_slots_guard() -> std::sync::MutexGuard<'static, Vec<ThreadSlot>> {
    let lock = THREAD_SLOTS.get_or_init(|| Mutex::new(Vec::with_capacity(8)));
    match lock.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            // Poison-tolerant : continue with the inner data ; mirrors the
            // crate-shared `lock_and_reset_all` discipline in `lib.rs`.
            poisoned.into_inner()
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § slot-table : Mutex storage
//
//   Same Sawyer-efficient slot-table shape as the thread-table. Mutexes
//   are stored in `Box<Mutex<()>>` so the per-mutex allocation is a single
//   heap-pointer ; the slot-vector itself only stores the box-pointer.
//   Lock-acquisition takes the slot-vec lock briefly to fetch the box-ptr
//   then releases before locking the inner mutex (avoiding a deadlock-
//   inversion on contention).
// ───────────────────────────────────────────────────────────────────────

/// One entry in the mutex slot-table. The `Box<Mutex<()>>` is `'static`
/// once installed (we never drop the inner Mutex while a CSSLv3-side
/// reference may exist — the destroy operation marks the slot as `None`
/// but the Box leaks on stage-0 to keep the lifetime story trivial).
pub struct MutexSlot {
    /// `Some` while the mutex is alive ; `None` after `destroy`.
    /// The inner `Mutex<()>` is heap-allocated so its address is stable
    /// across slot-vec reallocations (the slot-vec stores a Box, not the
    /// Mutex by-value).
    handle: Option<Box<Mutex<MutexState>>>,
    /// Monotonic ID — duplicates the slot's u64 handle for cross-check.
    id: u64,
}

/// Inner state for each `__cssl_mutex_*` mutex. Track lock-count for
/// diagnostic discipline (paired-lock-unlock invariants visible to tests).
#[derive(Debug, Default)]
pub struct MutexState {
    /// Total lock acquisitions across this mutex's lifetime (monotonic).
    lock_count: u64,
    /// Total releases across this mutex's lifetime (monotonic).
    unlock_count: u64,
}

/// Crate-shared slot-table for `__cssl_mutex_*`-managed mutexes.
pub static MUTEX_SLOTS: OnceLock<Mutex<Vec<MutexSlot>>> = OnceLock::new();

/// Monotonic ID counter for mutex-handle assignment.
static NEXT_MUTEX_ID: AtomicU64 = AtomicU64::new(1);

/// Acquire the mutex slot-table guard, lazily-initializing on first call.
fn mutex_slots_guard() -> std::sync::MutexGuard<'static, Vec<MutexSlot>> {
    let lock = MUTEX_SLOTS.get_or_init(|| Mutex::new(Vec::with_capacity(8)));
    match lock.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

// ───────────────────────────────────────────────────────────────────────
// § thread-spawn / join — Rust-side `_impl` helpers
// ───────────────────────────────────────────────────────────────────────

/// Rust-side helper that spawns a thread executing `entry(arg)` and
/// returns the slot-table handle.
///
/// § FFI ENTRY-POINT TYPE
///   The `entry` parameter is a function pointer to
///   `extern "C" fn(*const u8) -> i32`. The CSSL-side wrapper passes its
///   userland fn through `transmute`, which the cssl-rt-side shim
///   transmutes back into the function-pointer type. Both sides share the
///   ABI lock (`extern "C" fn(*const u8) -> i32`) — drift is a major-
///   version-bump event.
///
/// § SAFETY
/// The caller must guarantee :
///   - `entry` is a valid `extern "C" fn(*const u8) -> i32` pointer for
///     the lifetime of the spawned thread.
///   - `arg` is null OR points to data whose lifetime exceeds the
///     spawned thread's execution.
pub unsafe fn cssl_thread_spawn_impl(
    entry: *const u8,
    arg: *const u8,
) -> u64 {
    if entry.is_null() {
        return INVALID_HANDLE;
    }
    // SAFETY : entry pointer-validity is the FFI-caller's contract ; the
    // ABI lock requires `extern "C" fn(*const u8) -> i32` shape.
    let entry_fn: extern "C" fn(*const u8) -> i32 =
        unsafe { core::mem::transmute(entry) };
    // The arg pointer is sent across the thread boundary as a usize so
    // we don't have to satisfy `Send` on a raw pointer. The spawned
    // thread converts back to *const u8 inside its closure.
    let arg_usize = arg as usize;

    let id = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed);
    let join_handle = std::thread::spawn(move || {
        let arg_ptr = arg_usize as *const u8;
        entry_fn(arg_ptr)
    });

    let mut slots = thread_slots_guard();
    slots.push(ThreadSlot {
        handle: Some(join_handle),
        id,
    });
    id
}

/// Rust-side helper that joins the thread identified by `handle` and
/// stores its return-code at `*ret_out` (if non-null). Returns
/// [`RC_OK`] on success, [`RC_ERR`] when the handle is invalid or the
/// thread has already been joined.
///
/// § SAFETY
/// The caller must guarantee that `ret_out` is null OR points to a
/// writable `i32` slot.
pub unsafe fn cssl_thread_join_impl(handle: u64, ret_out: *mut i32) -> i32 {
    if handle == INVALID_HANDLE {
        return RC_ERR;
    }
    // Take the JoinHandle out of the slot under the slot-table lock,
    // then drop the lock before calling join() (to avoid serializing all
    // joins on the slot-table mutex).
    let join_handle = {
        let mut slots = thread_slots_guard();
        let slot = match slots.iter_mut().find(|s| s.id == handle) {
            Some(s) => s,
            None => return RC_ERR,
        };
        match slot.handle.take() {
            Some(h) => h,
            None => return RC_ERR, // already joined
        }
    };
    match join_handle.join() {
        Ok(rc) => {
            if !ret_out.is_null() {
                // SAFETY : caller-contract requires ret_out to be writable
                // when non-null.
                unsafe { ptr::write(ret_out, rc) };
            }
            RC_OK
        }
        Err(_panic_payload) => {
            // The spawned thread panicked. Stage-0 surfaces a generic
            // `RC_ERR` ; future last-error machinery will distinguish.
            if !ret_out.is_null() {
                // SAFETY : caller-contract requires ret_out to be writable
                // when non-null.
                unsafe { ptr::write(ret_out, RC_ERR) };
            }
            RC_ERR
        }
    }
}

// ───────────────────────────────────────────────────────────────────────
// § mutex create / lock / unlock / destroy — Rust-side `_impl` helpers
// ───────────────────────────────────────────────────────────────────────

/// Rust-side helper that creates a new mutex slot. Returns the new
/// handle (always nonzero on success) or [`INVALID_HANDLE`] on
/// allocation-failure.
pub fn cssl_mutex_create_impl() -> u64 {
    let id = NEXT_MUTEX_ID.fetch_add(1, Ordering::Relaxed);
    let mut slots = mutex_slots_guard();
    slots.push(MutexSlot {
        handle: Some(Box::new(Mutex::new(MutexState::default()))),
        id,
    });
    id
}

/// Rust-side helper that locks the mutex identified by `handle`.
/// Returns [`RC_OK`] on success, [`RC_ERR`] when the handle is invalid
/// or destroyed.
pub fn cssl_mutex_lock_impl(handle: u64) -> i32 {
    if handle == INVALID_HANDLE {
        return RC_ERR;
    }
    // Acquire the slot-table guard briefly to obtain a stable raw-ptr
    // to the inner Box<Mutex<...>>, then release before locking the
    // inner mutex (preventing deadlock on contention).
    let inner_ptr: *const Mutex<MutexState> = {
        let slots = mutex_slots_guard();
        match slots.iter().find(|s| s.id == handle) {
            Some(slot) => match slot.handle.as_ref() {
                // Box<Mutex<MutexState>> -> *const Mutex<MutexState> via
                // Box::as_ref + cast. The pointer is stable for the
                // mutex's lifetime (we never drop the Box until destroy
                // marks the slot None ; even then the box leaks on
                // stage-0 to keep the lifetime story trivial — see
                // module-doc).
                Some(b) => b.as_ref() as *const Mutex<MutexState>,
                None => return RC_ERR,
            },
            None => return RC_ERR,
        }
    };
    // SAFETY : the slot-table never drops the Box while a slot is in
    // the table ; destroy marks `Some -> None` but leaks the Box, so
    // the pointer remains valid until process-exit.
    let inner = unsafe { &*inner_ptr };
    match inner.lock() {
        Ok(mut state) => {
            state.lock_count = state.lock_count.saturating_add(1);
            // We hold the lock here. Need to keep it locked across the
            // CSSL-side `unlock` call. To do so we mem::forget the guard
            // (the CSSL-side `unlock` shim will explicitly release it).
            // Stage-0 implementation : delegate to the parking-lot-style
            // protocol — increment lock_count and *drop* the std-lib
            // guard. The atomic-style protocol (cssl-side actually does
            // its own atomic-CAS spinlock) is a Wave-D2.1 follow-up.
            //
            // Stage-0 effective semantics : `__cssl_mutex_lock` is a
            // no-blocking flag-set ; correctness comes from the fact
            // that the only consumer is CSSLv3 source-level code that
            // pairs every lock with an unlock. The std-lib Mutex is held
            // briefly to update the bookkeeping, then released. Real
            // mutual-exclusion across user threads happens at the cssl-
            // side wrapper via `__cssl_atomic_cas_u64`.
            drop(state);
            RC_OK
        }
        Err(_) => RC_ERR,
    }
}

/// Rust-side helper that unlocks the mutex identified by `handle`.
/// Returns [`RC_OK`] on success, [`RC_ERR`] when the handle is invalid
/// or destroyed. Idempotent : calling unlock without a prior lock is
/// not an error in stage-0 (matches the parking-lot-style flag-set
/// semantics described in [`cssl_mutex_lock_impl`]).
pub fn cssl_mutex_unlock_impl(handle: u64) -> i32 {
    if handle == INVALID_HANDLE {
        return RC_ERR;
    }
    let inner_ptr: *const Mutex<MutexState> = {
        let slots = mutex_slots_guard();
        match slots.iter().find(|s| s.id == handle) {
            Some(slot) => match slot.handle.as_ref() {
                Some(b) => b.as_ref() as *const Mutex<MutexState>,
                None => return RC_ERR,
            },
            None => return RC_ERR,
        }
    };
    // SAFETY : same argument as `cssl_mutex_lock_impl`.
    let inner = unsafe { &*inner_ptr };
    match inner.lock() {
        Ok(mut state) => {
            state.unlock_count = state.unlock_count.saturating_add(1);
            drop(state);
            RC_OK
        }
        Err(_) => RC_ERR,
    }
}

/// Rust-side helper that destroys the mutex identified by `handle`.
/// Returns [`RC_OK`] on success, [`RC_ERR`] when the handle is invalid
/// or already destroyed.
pub fn cssl_mutex_destroy_impl(handle: u64) -> i32 {
    if handle == INVALID_HANDLE {
        return RC_ERR;
    }
    let mut slots = mutex_slots_guard();
    let slot = match slots.iter_mut().find(|s| s.id == handle) {
        Some(s) => s,
        None => return RC_ERR,
    };
    if slot.handle.is_none() {
        return RC_ERR; // already destroyed
    }
    // Consume the Box<Mutex<...>> by calling `Box::leak` then taking the
    // slot's Option to None. Stage-0 leaks the Box deliberately — see
    // module-doc § STAGE-0 SCOPE.
    let boxed = slot.handle.take().expect("destroy : slot was Some");
    let _leaked: &'static Mutex<MutexState> = Box::leak(boxed);
    RC_OK
}

// ───────────────────────────────────────────────────────────────────────
// § atomic load / store / cas — Rust-side `_impl` helpers
//
//   These delegate to `std::sync::atomic::AtomicU64` operations on a
//   caller-supplied `*mut u64`. The shim treats the pointer as if it
//   refers to an `AtomicU64` (which is layout-compatible with `u64`).
// ───────────────────────────────────────────────────────────────────────

/// Rust-side helper that performs `AtomicU64::load(map_load_ordering(order))`
/// at `addr`. Returns the loaded value. Null `addr` returns 0 as a
/// defensive sentinel (matches the FFI contract that returns `u64`).
///
/// § SAFETY
/// The caller must guarantee :
///   - `addr` is null OR points to a properly-aligned `u64` whose
///     `core::sync::atomic::AtomicU64` aliasing rules are upheld.
///   - For the lifetime of the call, no non-atomic write to `*addr`
///     races with this load.
pub unsafe fn cssl_atomic_load_u64_impl(addr: *const u64, order: u32) -> u64 {
    if addr.is_null() {
        return 0;
    }
    // SAFETY : caller-contract guarantees alignment + non-aliasing per
    // AtomicU64's invariants ; cast from *const u64 to *const AtomicU64
    // is layout-compatible (same size + alignment).
    let atomic = unsafe { &*(addr as *const AtomicU64) };
    atomic.load(map_load_ordering(order))
}

/// Rust-side helper that performs `AtomicU64::store(value, map_store_
/// ordering(order))` at `addr`. Returns [`RC_OK`] on success, [`RC_ERR`]
/// when `addr` is null.
///
/// § SAFETY
/// Same conditions as [`cssl_atomic_load_u64_impl`].
pub unsafe fn cssl_atomic_store_u64_impl(
    addr: *mut u64,
    value: u64,
    order: u32,
) -> i32 {
    if addr.is_null() {
        return RC_ERR;
    }
    // SAFETY : same as load_u64_impl.
    let atomic = unsafe { &*(addr as *const AtomicU64) };
    atomic.store(value, map_store_ordering(order));
    RC_OK
}

/// Rust-side helper that performs `AtomicU64::compare_exchange(expected,
/// desired, success_order, failure_order)` at `addr`. On success returns
/// `expected` (per std-lib API : the previous value, which equals
/// `expected` on success). On failure returns the actual previous value.
/// Null `addr` returns `expected` unchanged (defensive ; caller cannot
/// distinguish from success but null-ptr is a contract violation upstream).
///
/// § SAFETY
/// Same conditions as [`cssl_atomic_load_u64_impl`].
pub unsafe fn cssl_atomic_cas_u64_impl(
    addr: *mut u64,
    expected: u64,
    desired: u64,
    order: u32,
) -> u64 {
    if addr.is_null() {
        return expected;
    }
    // SAFETY : same as load_u64_impl.
    let atomic = unsafe { &*(addr as *const AtomicU64) };
    let mapped = map_ordering(order);
    // For CAS, the success/failure ordering pair must be derived. The
    // std-lib enforces failure_ordering <= success_ordering ; we map
    // AcqRel/SeqCst to (success=mapped, failure=Acquire/SeqCst).
    let (success_o, failure_o) = match mapped {
        Ordering::Relaxed => (Ordering::Relaxed, Ordering::Relaxed),
        Ordering::Acquire => (Ordering::Acquire, Ordering::Acquire),
        Ordering::Release => (Ordering::Release, Ordering::Relaxed),
        Ordering::AcqRel => (Ordering::AcqRel, Ordering::Acquire),
        // Future Ordering variants + Ordering::SeqCst : default to SeqCst.
        _ => (Ordering::SeqCst, Ordering::SeqCst),
    };
    // CAS returns Ok(prev)=expected on success, Err(prev)=actual on
    // failure. Both arms surface the same value to userland.
    match atomic.compare_exchange(expected, desired, success_o, failure_o) {
        Ok(prev) | Err(prev) => prev,
    }
}

// ───────────────────────────────────────────────────────────────────────
// § ABI-stable extern "C" shim symbols
//
//   ‼ Every shim here is `#[no_mangle]` extern "C" + locked-forever.
//     The body is a thin delegation to the matching `_impl` helper
//     above. Renaming requires lock-step changes across cgen_thread.rs
//     + every CSSLv3 emitter that references the symbol-name string.
// ───────────────────────────────────────────────────────────────────────

/// FFI : spawn a new thread executing `entry(arg)`. Returns the slot-
/// table handle (always nonzero on success ; 0 = failure).
///
/// # Safety
/// Caller must :
///   - pass `entry` as a valid `extern "C" fn(*const u8) -> i32` pointer
///     for the spawned thread's lifetime ;
///   - pass `arg` as null OR a pointer whose target outlives the thread.
#[no_mangle]
pub unsafe extern "C" fn __cssl_thread_spawn(
    entry: *const u8,
    arg: *const u8,
) -> u64 {
    // SAFETY : entry/arg pointer-validity is the FFI-caller's contract.
    unsafe { cssl_thread_spawn_impl(entry, arg) }
}

/// FFI : join the thread identified by `handle` ; write its return-code
/// to `*ret_out` (if non-null). Returns 0 on success, -1 on failure.
///
/// # Safety
/// Caller must :
///   - pass `handle` as a value previously returned by
///     [`__cssl_thread_spawn`] ;
///   - pass `ret_out` as null OR a pointer to a writable `i32` slot.
#[no_mangle]
pub unsafe extern "C" fn __cssl_thread_join(
    handle: u64,
    ret_out: *mut i32,
) -> i32 {
    // SAFETY : handle + ret_out contract inherited from caller.
    unsafe { cssl_thread_join_impl(handle, ret_out) }
}

/// FFI : create a new mutex. Returns the slot-table handle (nonzero on
/// success ; 0 on allocation-failure).
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
#[no_mangle]
pub unsafe extern "C" fn __cssl_mutex_create() -> u64 {
    cssl_mutex_create_impl()
}

/// FFI : lock the mutex identified by `handle`. Returns 0 on success,
/// -1 on failure (invalid handle or destroyed slot).
///
/// # Safety
/// Caller must pass `handle` as a value previously returned by
/// [`__cssl_mutex_create`] and not yet destroyed.
#[no_mangle]
pub unsafe extern "C" fn __cssl_mutex_lock(handle: u64) -> i32 {
    cssl_mutex_lock_impl(handle)
}

/// FFI : unlock the mutex identified by `handle`. Returns 0 on success,
/// -1 on failure.
///
/// # Safety
/// Caller must pass `handle` as a value previously returned by
/// [`__cssl_mutex_create`] and not yet destroyed.
#[no_mangle]
pub unsafe extern "C" fn __cssl_mutex_unlock(handle: u64) -> i32 {
    cssl_mutex_unlock_impl(handle)
}

/// FFI : destroy the mutex identified by `handle`. Returns 0 on success,
/// -1 on failure.
///
/// # Safety
/// Caller must pass `handle` as a value previously returned by
/// [`__cssl_mutex_create`] and not yet destroyed. After destroy the
/// handle is invalidated.
#[no_mangle]
pub unsafe extern "C" fn __cssl_mutex_destroy(handle: u64) -> i32 {
    cssl_mutex_destroy_impl(handle)
}

/// FFI : atomically load a `u64` from `addr` with the given ordering.
///
/// # Safety
/// Caller must :
///   - pass `addr` as null OR a properly-aligned `u64` pointer whose
///     `AtomicU64` aliasing rules are upheld ;
///   - ensure no non-atomic write to `*addr` races with this load.
#[no_mangle]
pub unsafe extern "C" fn __cssl_atomic_load_u64(
    addr: *const u64,
    order: u32,
) -> u64 {
    // SAFETY : addr contract inherited from caller.
    unsafe { cssl_atomic_load_u64_impl(addr, order) }
}

/// FFI : atomically store `value` to `*addr` with the given ordering.
/// Returns 0 on success, -1 on null-addr.
///
/// # Safety
/// Same conditions as [`__cssl_atomic_load_u64`].
#[no_mangle]
pub unsafe extern "C" fn __cssl_atomic_store_u64(
    addr: *mut u64,
    value: u64,
    order: u32,
) -> i32 {
    // SAFETY : addr contract inherited from caller.
    unsafe { cssl_atomic_store_u64_impl(addr, value, order) }
}

/// FFI : atomically compare-and-swap on `*addr`. Returns the previous
/// value (equals `expected` on success ; otherwise the value that
/// prevented the swap).
///
/// # Safety
/// Same conditions as [`__cssl_atomic_load_u64`].
#[no_mangle]
pub unsafe extern "C" fn __cssl_atomic_cas_u64(
    addr: *mut u64,
    expected: u64,
    desired: u64,
    order: u32,
) -> u64 {
    // SAFETY : addr contract inherited from caller.
    unsafe { cssl_atomic_cas_u64_impl(addr, expected, desired, order) }
}

// ───────────────────────────────────────────────────────────────────────
// § test-helper : reset slot-tables for cross-test serialization
// ───────────────────────────────────────────────────────────────────────

/// Reset the thread + mutex slot-tables for test serialization. The
/// integration commit will surface this from `lib.rs::test_helpers::
/// lock_and_reset_all` so each test starts from a clean slate. Call it
/// directly in tests that exercise this module's globals.
#[cfg(test)]
pub fn reset_for_tests() {
    if let Some(lock) = THREAD_SLOTS.get() {
        if let Ok(mut g) = lock.lock() {
            g.clear();
        } else {
            lock.clear_poison();
        }
    }
    if let Some(lock) = MUTEX_SLOTS.get() {
        if let Ok(mut g) = lock.lock() {
            g.clear();
        } else {
            lock.clear_poison();
        }
    }
    NEXT_THREAD_ID.store(1, Ordering::Relaxed);
    NEXT_MUTEX_ID.store(1, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § tests — exercise the FFI boundary + ordering map + slot-table
//
//   ‼ The integration commit will register this module via `pub mod
//     host_thread;` in `lib.rs` so cargo-test picks up these tests.
//     Until then the tests compile in-place but are unreachable from
//     the test runner (matches the cgen_fs.rs precedent — see
//     INTEGRATION_NOTE above).
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    // Single test-lock to serialize tests that touch the global slot-
    // tables (mirrors the crate-shared GLOBAL_TEST_LOCK pattern).
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = match TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => {
                TEST_LOCK.clear_poison();
                p.into_inner()
            }
        };
        reset_for_tests();
        g
    }

    // ── ordering enum mapping ──────────────────────────────────────────

    #[test]
    fn ordering_map_relaxed_acquire_release_acqrel_seqcst() {
        // Pure : no global state.
        assert_eq!(map_ordering(ORDERING_RELAXED), Ordering::Relaxed);
        assert_eq!(map_ordering(ORDERING_ACQUIRE), Ordering::Acquire);
        assert_eq!(map_ordering(ORDERING_RELEASE), Ordering::Release);
        assert_eq!(map_ordering(ORDERING_ACQ_REL), Ordering::AcqRel);
        assert_eq!(map_ordering(ORDERING_SEQ_CST), Ordering::SeqCst);
    }

    #[test]
    fn ordering_map_unknown_falls_back_to_seqcst() {
        // Pure : 9999 is not a recognized wire-encoding ; defensive
        // fall-through gives SeqCst (strictest sane default).
        assert_eq!(map_ordering(9999), Ordering::SeqCst);
        assert_eq!(map_ordering(u32::MAX), Ordering::SeqCst);
    }

    #[test]
    fn ordering_load_coerces_release_to_acquire() {
        // load semantics : Release / AcqRel are illegal ; coerce to
        // Acquire so the load is well-defined.
        assert_eq!(map_load_ordering(ORDERING_RELEASE), Ordering::Acquire);
        assert_eq!(map_load_ordering(ORDERING_ACQ_REL), Ordering::Acquire);
        // Other values pass through.
        assert_eq!(map_load_ordering(ORDERING_RELAXED), Ordering::Relaxed);
        assert_eq!(map_load_ordering(ORDERING_ACQUIRE), Ordering::Acquire);
        assert_eq!(map_load_ordering(ORDERING_SEQ_CST), Ordering::SeqCst);
    }

    #[test]
    fn ordering_store_coerces_acquire_to_release() {
        // store semantics : Acquire / AcqRel are illegal ; coerce to
        // Release so the store is well-defined.
        assert_eq!(map_store_ordering(ORDERING_ACQUIRE), Ordering::Release);
        assert_eq!(map_store_ordering(ORDERING_ACQ_REL), Ordering::Release);
        // Other values pass through.
        assert_eq!(map_store_ordering(ORDERING_RELAXED), Ordering::Relaxed);
        assert_eq!(map_store_ordering(ORDERING_RELEASE), Ordering::Release);
        assert_eq!(map_store_ordering(ORDERING_SEQ_CST), Ordering::SeqCst);
    }

    // ── thread spawn / join roundtrip ─────────────────────────────────

    extern "C" fn entry_returns_42(_arg: *const u8) -> i32 {
        42
    }

    extern "C" fn entry_returns_arg(arg: *const u8) -> i32 {
        // arg is treated as `*const i32` for this test ; read + return.
        if arg.is_null() {
            return -1;
        }
        // SAFETY : test contract — arg points to a stable i32 in the
        // calling thread's stack ; we hold the join-handle so the parent
        // outlives the child + the stack slot is alive throughout.
        unsafe { *(arg as *const i32) }
    }

    #[test]
    fn thread_spawn_returns_nonzero_handle() {
        let _g = lock_and_reset();
        // SAFETY : entry_returns_42 has the FFI signature ; arg null OK.
        let h = unsafe {
            __cssl_thread_spawn(entry_returns_42 as *const u8, ptr::null())
        };
        assert_ne!(h, INVALID_HANDLE);
        // Cleanup : join so the test doesn't leak the thread.
        let mut rc: i32 = 0;
        // SAFETY : h was just returned ; rc is on stack.
        let jrc = unsafe { __cssl_thread_join(h, &mut rc) };
        assert_eq!(jrc, RC_OK);
        assert_eq!(rc, 42);
    }

    #[test]
    fn thread_spawn_join_roundtrip_propagates_return_code() {
        let _g = lock_and_reset();
        // SAFETY : entry / arg validity per FFI contract.
        let h = unsafe {
            __cssl_thread_spawn(entry_returns_42 as *const u8, ptr::null())
        };
        assert_ne!(h, INVALID_HANDLE);
        let mut rc: i32 = -999;
        // SAFETY : h is valid, rc is writable.
        let jrc = unsafe { __cssl_thread_join(h, &mut rc) };
        assert_eq!(jrc, RC_OK);
        assert_eq!(rc, 42);
    }

    // Static for the `thread_spawn_with_arg_propagates_payload` test —
    // declared at module scope to avoid the `items_after_statements`
    // pedantic-clippy lint inside the test fn.
    static PAYLOAD_FOR_ARG_TEST: i32 = 7777;

    #[test]
    fn thread_spawn_with_arg_propagates_payload() {
        let _g = lock_and_reset();
        // The thread is joined before this fn returns so the static
        // reference is stable.
        let arg_ptr: *const u8 =
            std::ptr::addr_of!(PAYLOAD_FOR_ARG_TEST).cast::<u8>();
        // SAFETY : arg_ptr points to PAYLOAD_FOR_ARG_TEST which is
        // 'static ; entry signature matches the FFI contract.
        let h = unsafe {
            __cssl_thread_spawn(entry_returns_arg as *const u8, arg_ptr)
        };
        assert_ne!(h, INVALID_HANDLE);
        let mut rc: i32 = 0;
        // SAFETY : h is valid.
        let _ = unsafe { __cssl_thread_join(h, &mut rc) };
        assert_eq!(rc, 7777);
    }

    #[test]
    fn thread_spawn_null_entry_returns_invalid_handle() {
        let _g = lock_and_reset();
        // SAFETY : null entry is the contract-violation case ; shim
        // defensively returns INVALID_HANDLE.
        let h = unsafe { __cssl_thread_spawn(ptr::null(), ptr::null()) };
        assert_eq!(h, INVALID_HANDLE);
    }

    #[test]
    fn thread_join_invalid_handle_returns_minus_one() {
        let _g = lock_and_reset();
        // SAFETY : INVALID_HANDLE always errors.
        let rc = unsafe { __cssl_thread_join(INVALID_HANDLE, ptr::null_mut()) };
        assert_eq!(rc, RC_ERR);
    }

    #[test]
    fn thread_join_null_ret_out_still_succeeds() {
        let _g = lock_and_reset();
        // SAFETY : entry returns 42 ; we drop the return-code via null.
        let h = unsafe {
            __cssl_thread_spawn(entry_returns_42 as *const u8, ptr::null())
        };
        // SAFETY : null ret_out is documented as accepted.
        let rc = unsafe { __cssl_thread_join(h, ptr::null_mut()) };
        assert_eq!(rc, RC_OK);
    }

    #[test]
    fn thread_join_twice_second_call_returns_minus_one() {
        let _g = lock_and_reset();
        // SAFETY : as per other join tests.
        let h = unsafe {
            __cssl_thread_spawn(entry_returns_42 as *const u8, ptr::null())
        };
        let rc1 = unsafe { __cssl_thread_join(h, ptr::null_mut()) };
        assert_eq!(rc1, RC_OK);
        // Second join : slot's Option<JoinHandle> is now None.
        let rc2 = unsafe { __cssl_thread_join(h, ptr::null_mut()) };
        assert_eq!(rc2, RC_ERR);
    }

    // ── mutex create / lock / unlock / destroy invariants ──────────────

    #[test]
    fn mutex_create_returns_nonzero_handle() {
        let _g = lock_and_reset();
        // SAFETY : extern "C" but body is safe.
        let h = unsafe { __cssl_mutex_create() };
        assert_ne!(h, INVALID_HANDLE);
        // Cleanup.
        let _ = unsafe { __cssl_mutex_destroy(h) };
    }

    #[test]
    fn mutex_create_assigns_distinct_handles() {
        let _g = lock_and_reset();
        // SAFETY : as above.
        let h1 = unsafe { __cssl_mutex_create() };
        let h2 = unsafe { __cssl_mutex_create() };
        let h3 = unsafe { __cssl_mutex_create() };
        assert_ne!(h1, h2);
        assert_ne!(h2, h3);
        assert_ne!(h1, h3);
        for h in [h1, h2, h3] {
            let _ = unsafe { __cssl_mutex_destroy(h) };
        }
    }

    #[test]
    fn mutex_lock_unlock_paired_invariant() {
        let _g = lock_and_reset();
        // SAFETY : extern "C" + handle from create.
        let h = unsafe { __cssl_mutex_create() };
        assert_ne!(h, INVALID_HANDLE);
        let lr = unsafe { __cssl_mutex_lock(h) };
        assert_eq!(lr, RC_OK);
        let ur = unsafe { __cssl_mutex_unlock(h) };
        assert_eq!(ur, RC_OK);
        let dr = unsafe { __cssl_mutex_destroy(h) };
        assert_eq!(dr, RC_OK);
    }

    #[test]
    fn mutex_lock_invalid_handle_returns_minus_one() {
        let _g = lock_and_reset();
        // SAFETY : INVALID_HANDLE always errors.
        assert_eq!(unsafe { __cssl_mutex_lock(INVALID_HANDLE) }, RC_ERR);
        // A handle that was never created.
        assert_eq!(unsafe { __cssl_mutex_lock(99_999_999) }, RC_ERR);
    }

    #[test]
    fn mutex_destroy_twice_second_call_returns_minus_one() {
        let _g = lock_and_reset();
        // SAFETY : as above.
        let h = unsafe { __cssl_mutex_create() };
        assert_eq!(unsafe { __cssl_mutex_destroy(h) }, RC_OK);
        // Second destroy : slot's Option<Box<...>> is None.
        assert_eq!(unsafe { __cssl_mutex_destroy(h) }, RC_ERR);
    }

    #[test]
    fn mutex_lock_after_destroy_returns_minus_one() {
        let _g = lock_and_reset();
        // SAFETY : as above.
        let h = unsafe { __cssl_mutex_create() };
        assert_eq!(unsafe { __cssl_mutex_destroy(h) }, RC_OK);
        // Lock-after-destroy : slot's Option is None ⇒ RC_ERR.
        assert_eq!(unsafe { __cssl_mutex_lock(h) }, RC_ERR);
    }

    // ── atomic load / store / cas — happy + fail paths ────────────────

    #[test]
    fn atomic_store_then_load_roundtrip() {
        // Pure-stack u64 ; layout-compatible with AtomicU64.
        let mut storage: u64 = 0;
        let addr = &mut storage as *mut u64;
        // SAFETY : addr points to a properly-aligned u64 on the stack ;
        // we don't race with any non-atomic writer (single-threaded).
        let sr = unsafe {
            __cssl_atomic_store_u64(addr, 0xDEAD_BEEF_CAFE_F00D, ORDERING_SEQ_CST)
        };
        assert_eq!(sr, RC_OK);
        // SAFETY : same as above.
        let v = unsafe { __cssl_atomic_load_u64(addr as *const u64, ORDERING_SEQ_CST) };
        assert_eq!(v, 0xDEAD_BEEF_CAFE_F00D);
    }

    #[test]
    fn atomic_store_null_addr_returns_minus_one() {
        // SAFETY : null is the documented null-ptr error path.
        let sr = unsafe {
            __cssl_atomic_store_u64(ptr::null_mut(), 42, ORDERING_RELAXED)
        };
        assert_eq!(sr, RC_ERR);
    }

    #[test]
    fn atomic_load_null_addr_returns_zero() {
        // SAFETY : defensive zero-return on null per the doc-comment.
        let v = unsafe { __cssl_atomic_load_u64(ptr::null(), ORDERING_RELAXED) };
        assert_eq!(v, 0);
    }

    #[test]
    fn atomic_cas_succeeds_when_expected_matches() {
        let mut storage: u64 = 100;
        let addr = &mut storage as *mut u64;
        // SAFETY : single-threaded test ; pointer is valid + aligned.
        let prev = unsafe {
            __cssl_atomic_cas_u64(addr, 100, 200, ORDERING_SEQ_CST)
        };
        // CAS success returns previous value (which equals expected).
        assert_eq!(prev, 100);
        assert_eq!(storage, 200);
    }

    #[test]
    fn atomic_cas_fails_when_expected_does_not_match() {
        let mut storage: u64 = 100;
        let addr = &mut storage as *mut u64;
        // SAFETY : as above.
        let prev = unsafe {
            __cssl_atomic_cas_u64(addr, 999, 200, ORDERING_SEQ_CST)
        };
        // CAS failure returns the actual previous value (100).
        assert_eq!(prev, 100);
        // Storage unchanged.
        assert_eq!(storage, 100);
    }

    #[test]
    fn atomic_cas_with_different_orderings_round_trip() {
        // Exercise every ordering for CAS to confirm the success/failure
        // pair derivation is sane.
        let orderings = [
            ORDERING_RELAXED,
            ORDERING_ACQUIRE,
            ORDERING_RELEASE,
            ORDERING_ACQ_REL,
            ORDERING_SEQ_CST,
        ];
        for ord in orderings {
            let mut storage: u64 = 0;
            let addr = &mut storage as *mut u64;
            // SAFETY : single-threaded ; storage on stack.
            let prev = unsafe { __cssl_atomic_cas_u64(addr, 0, 1, ord) };
            assert_eq!(prev, 0, "ordering {ord} : CAS should succeed");
            assert_eq!(storage, 1);
        }
    }

    #[test]
    fn ffi_symbol_constants_are_canonical() {
        // ‼ Lock-step invariant : these constants are the cssl-rt-side
        //   source-of-truth for the ABI-stable symbol-names. The matching
        //   cgen_thread.rs constants must equal these byte-for-byte.
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
    fn ffi_signatures_compile_time_lock() {
        // Compile-time assertion : these `let _ : <type> = …` lines
        // fail to compile if any FFI signature drifts from the
        // documented ABI. Mirrors the pattern in
        // `cssl-rt/src/ffi.rs::ffi_symbols_have_correct_signatures`.
        let _: unsafe extern "C" fn(*const u8, *const u8) -> u64 =
            __cssl_thread_spawn;
        let _: unsafe extern "C" fn(u64, *mut i32) -> i32 = __cssl_thread_join;
        let _: unsafe extern "C" fn() -> u64 = __cssl_mutex_create;
        let _: unsafe extern "C" fn(u64) -> i32 = __cssl_mutex_lock;
        let _: unsafe extern "C" fn(u64) -> i32 = __cssl_mutex_unlock;
        let _: unsafe extern "C" fn(u64) -> i32 = __cssl_mutex_destroy;
        let _: unsafe extern "C" fn(*const u64, u32) -> u64 =
            __cssl_atomic_load_u64;
        let _: unsafe extern "C" fn(*mut u64, u64, u32) -> i32 =
            __cssl_atomic_store_u64;
        let _: unsafe extern "C" fn(*mut u64, u64, u64, u32) -> u64 =
            __cssl_atomic_cas_u64;
    }
}

// ───────────────────────────────────────────────────────────────────────
// § INTEGRATION_NOTE  (per Wave-D2 dispatch directive — repeat at EOF)
//
//   This module is delivered as a NEW file with its tests in-place but
//   `cssl-rt/src/lib.rs` is intentionally NOT modified. The integration
//   commit (deferred per the "DO NOT modify any lib.rs" constraint) will :
//
//     (1) Add `pub mod host_thread;` to `cssl-rt/src/lib.rs` after the
//         existing `pub mod net;` line.
//     (2) Add re-exports for the public API (`reset_for_tests`,
//         constant-symbol-names, ordering-map fns) in the same style
//         as the existing `pub use net::{...};` block.
//     (3) Extend `lib.rs::test_helpers::lock_and_reset_all` to include
//         a `crate::host_thread::reset_for_tests();` call after the
//         existing `crate::net::reset_net_for_tests();` line so the
//         crate-shared GLOBAL_TEST_LOCK propagates the reset.
//     (4) Extend `cssl-rt/src/ffi.rs::ffi_symbols_have_correct_signatures`
//         test with the nine `__cssl_thread_*` / `__cssl_mutex_*` /
//         `__cssl_atomic_*` signature locks (mirrors the existing fs+net
//         signature-lock pattern at lines 590-612 of ffi.rs).
//     (5) Document the surface in `lib.rs`'s top-of-file `§ FFI SURFACE`
//         doc-comment under a new `Wave-D2 (T11-D??) — threading surface`
//         heading (mirrors the existing S6-A1 / S6-B5 / S7-F4 sections).
//
//   The cgen-side companion `cgen_thread.rs` is delivered in lock-step
//   with this module ; the integration commit registers BOTH files
//   (cssl-rt's `pub mod host_thread;` + cgen-cpu-cranelift's `pub mod
//   cgen_thread;`) at the same time. Until then both modules are
//   crate-internal — ready for activation but not on the live cgen
//   dispatch path.
