//! § cssl-rt cap-verify — runtime capability-witness defense-in-depth helper.
//! ════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `specs/12_CAPABILITIES.csl § ISO-OWNERSHIP` +
//!                      `specs/11_IFC.csl § PRIME-DIRECTIVE ENCODING` +
//!                      `specs/01_BOOTSTRAP.csl § RUNTIME-LIB`.
//!
//! § ROLE (T11-D286 / W-E5-3)
//!   The HIR `cap_check` pass (Pony-6 + IFC) is COMPILE-TIME authoritative.
//!   This module supplies the RUNTIME defense-in-depth path : a single FFI
//!   helper `__cssl_cap_verify(cap_handle, op_kind) -> bool` that the
//!   `cssl-mir::cap_runtime_check` pass emits at every cap-boundary. If
//!   the static analysis is correct, the runtime check is redundant + free
//!   (LUT lookup) ; if the static analysis is wrong, the runtime check
//!   catches the violation before the cap-protected operation fires.
//!
//! § FFI SURFACE
//!   ```text
//!   __cssl_cap_verify(cap_handle: u64, op_kind: u32) -> u8
//!     0  = denied  (caller responsibility : abort / panic / unwind)
//!     1  = allowed (caller proceeds with cap-protected op)
//!   ```
//!
//! § cap_handle ENCODING
//!   The low 8 bits encode the `CapKind` index (0=Iso, 1=Trn, 2=Ref,
//!   3=Val, 4=Box, 5=Tag) ; the upper 56 bits are reserved for future
//!   extensions (e.g. linear-track epoch + IFC-label fingerprint). Stage-0
//!   cgen emits the low-byte form ; the verify-fn ignores the upper bits.
//!
//! § op_kind ENCODING
//!   Stage-0 records the call-site shape :
//!     0 = `OP_CALL_PASS_PARAM` — caller passes value to callee param
//!     1 = `OP_FN_ENTRY`        — callee fn-entry with cap-required-param
//!     2 = `OP_FIELD_ACCESS`    — struct-field access (deferred slice)
//!     3 = `OP_RETURN`          — return-value cap-check
//!
//! § DECISION-TABLE (Pony-6 ALLOW-RULES @ stage-0)
//!   Each (cap, op_kind) pair is a yes/no in a small bit-LUT. Sawyer-
//!   efficiency : the table is `[u8; 6 * 4]` = 24 bytes total, lives in
//!   .rodata, and the verify-fn is a single bit-test against it (zero
//!   branches, zero allocations).
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone /
//!    anything / anybody."
//!   Cap-verify is defense-in-depth, not surveillance. The verify-fn does
//!   NOT log or telemeter ; on denial it returns 0 + the caller decides
//!   the panic/abort policy. Per-process counters track verify-call rate
//!   for testability — they are local-only, never exfiltrated.

#![allow(unsafe_code)]

use core::sync::atomic::{AtomicU64, Ordering};

// ───────────────────────────────────────────────────────────────────────
// § op-kind constants — wire-protocol with cssl-mir + cssl-cgen
// ───────────────────────────────────────────────────────────────────────

/// Op-kind : caller-passes-value-to-callee-param.
pub const OP_CALL_PASS_PARAM: u32 = 0;
/// Op-kind : callee fn-entry (cap-required-param).
pub const OP_FN_ENTRY: u32 = 1;
/// Op-kind : struct-field access (T11-phase-2 deferred ; reserved).
pub const OP_FIELD_ACCESS: u32 = 2;
/// Op-kind : fn-return value cap-check.
pub const OP_RETURN: u32 = 3;
/// Maximum encoded op-kind ; values >= this are denied.
pub const OP_KIND_MAX: u32 = 4;

// ───────────────────────────────────────────────────────────────────────
// § cap-encoding — low byte of cap_handle
// ───────────────────────────────────────────────────────────────────────

/// `CapKind` index encoding — must mirror `cssl_caps::CapKind::index()`.
pub const CAP_INDEX_ISO: u8 = 0;
pub const CAP_INDEX_TRN: u8 = 1;
pub const CAP_INDEX_REF: u8 = 2;
pub const CAP_INDEX_VAL: u8 = 3;
pub const CAP_INDEX_BOX: u8 = 4;
pub const CAP_INDEX_TAG: u8 = 5;
/// Number of distinct caps tracked.
pub const CAP_KIND_COUNT: usize = 6;

// ───────────────────────────────────────────────────────────────────────
// § decision-LUT — Pony-6 allow-rules @ stage-0
// ───────────────────────────────────────────────────────────────────────
//
// Indexing : `LUT[cap_index * OP_KIND_MAX + op_kind] == 1` ⇒ allowed.
// At stage-0 we ALLOW every (cap, op_kind) pair the static analysis
// would have permitted ; deny only the obviously-invalid combinations
// (op_kind >= OP_KIND_MAX, cap_index >= CAP_KIND_COUNT, or the FIELD_ACCESS
// op-kind on Tag-cap which carries no payload). This keeps the runtime
// check from second-guessing static analysis while still catching
// cap-handle decoding bugs and out-of-range op-kinds.
//
// A future slice may tighten Box/Tag rules once the field-access pass
// lands at HIR ; until then the LUT is permissive within the
// well-formed-encoding envelope.

const ALLOW_LUT: [u8; CAP_KIND_COUNT * (OP_KIND_MAX as usize)] = [
    // CAP_ISO : iso may be passed (consumed) + may be at fn-entry + returned ; field-access on iso also OK.
    1, 1, 1, 1,
    // CAP_TRN : trn may pass / enter / return ; field-access permitted (writable-unique).
    1, 1, 1, 1,
    // CAP_REF : ref may pass / enter / return / field-access (Vale gen-ref check is at deref-time, not here).
    1, 1, 1, 1,
    // CAP_VAL : val (deep-immutable) may pass / enter / return / field-access (read-only).
    1, 1, 1, 1,
    // CAP_BOX : box (read-only-view) may pass / enter / return / field-access.
    1, 1, 1, 1,
    // CAP_TAG : tag (opaque-handle) may pass / enter / return ; FIELD_ACCESS DENIED (no data behind tag).
    1, 1, 0, 1,
];

// ───────────────────────────────────────────────────────────────────────
// § audit counters — local-only, no exfiltration
// ───────────────────────────────────────────────────────────────────────

static VERIFY_CALL_COUNT: AtomicU64 = AtomicU64::new(0);
static VERIFY_DENY_COUNT: AtomicU64 = AtomicU64::new(0);

/// Number of `__cssl_cap_verify` invocations since last reset.
#[must_use]
pub fn verify_call_count() -> u64 {
    VERIFY_CALL_COUNT.load(Ordering::Relaxed)
}

/// Number of denied `__cssl_cap_verify` calls since last reset.
#[must_use]
pub fn verify_deny_count() -> u64 {
    VERIFY_DENY_COUNT.load(Ordering::Relaxed)
}

/// Reset the audit counters. Test-only.
pub fn reset_cap_verify_for_tests() {
    VERIFY_CALL_COUNT.store(0, Ordering::Relaxed);
    VERIFY_DENY_COUNT.store(0, Ordering::Relaxed);
}

// ───────────────────────────────────────────────────────────────────────
// § Rust-side impl — also callable from unit tests
// ───────────────────────────────────────────────────────────────────────

/// Decide whether `(cap_handle, op_kind)` is allowed. Returns `true` for
/// allow, `false` for deny. Pure function modulo audit counters.
///
/// § DECISION-LOGIC
///   1. Decode cap_index = `cap_handle & 0xff`.
///   2. If cap_index >= [`CAP_KIND_COUNT`] → deny.
///   3. If op_kind >= [`OP_KIND_MAX`] → deny.
///   4. Else look up `ALLOW_LUT[cap_index * OP_KIND_MAX + op_kind]`.
///
/// § SAWYER-EFFICIENCY
///   - 1 mask + 2 bounds-checks + 1 LUT load + 2 atomic-bumps. Total
///     amortized O(1) ; no allocation, no branches beyond the bounds-checks.
#[must_use]
pub fn cap_verify_impl(cap_handle: u64, op_kind: u32) -> bool {
    VERIFY_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    let cap_index = (cap_handle & 0xff) as usize;
    if cap_index >= CAP_KIND_COUNT || op_kind >= OP_KIND_MAX {
        VERIFY_DENY_COUNT.fetch_add(1, Ordering::Relaxed);
        return false;
    }
    let lut_idx = cap_index * (OP_KIND_MAX as usize) + (op_kind as usize);
    let allowed = ALLOW_LUT[lut_idx] != 0;
    if !allowed {
        VERIFY_DENY_COUNT.fetch_add(1, Ordering::Relaxed);
    }
    allowed
}

// ───────────────────────────────────────────────────────────────────────
// § FFI shim — `__cssl_cap_verify`
// ───────────────────────────────────────────────────────────────────────

/// FFI : verify a (cap, op-kind) pair is allowed.
///
/// Returns `1` for allow, `0` for deny. The caller (cgen-emitted preamble)
/// decides the deny-policy ; stage-0 callers branch to `__cssl_panic` on
/// `0`. The `extern "C" fn() -> u8` shape keeps the ABI thin (single byte
/// return) for cheap dispatch from JIT'd cap-check preambles.
///
/// # Safety
/// Always safe to call ; `unsafe` only because of `extern "C"` ABI rules.
/// All inputs are integers — no pointer dereferencing.
#[no_mangle]
pub unsafe extern "C" fn __cssl_cap_verify(cap_handle: u64, op_kind: u32) -> u8 {
    u8::from(cap_verify_impl(cap_handle, op_kind))
}

// ───────────────────────────────────────────────────────────────────────
// § tests
// ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{
        cap_verify_impl, reset_cap_verify_for_tests, verify_call_count, verify_deny_count,
        CAP_INDEX_ISO, CAP_INDEX_TAG, CAP_INDEX_VAL, OP_CALL_PASS_PARAM, OP_FIELD_ACCESS,
        OP_FN_ENTRY, OP_KIND_MAX, OP_RETURN,
    };
    use std::sync::Mutex;

    /// Module-local lock so tests serialize on the audit-counters without
    /// fighting the crate-shared GLOBAL_TEST_LOCK (which is held by the
    /// alloc/panic/exit families).
    static CAP_VERIFY_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = match CAP_VERIFY_TEST_LOCK.lock() {
            Ok(g) => g,
            Err(p) => {
                CAP_VERIFY_TEST_LOCK.clear_poison();
                p.into_inner()
            }
        };
        reset_cap_verify_for_tests();
        g
    }

    #[test]
    fn iso_passes_call_pass_param() {
        let _g = lock_and_reset();
        // Stage-0 : iso may be consumed at a callsite (linear transfer).
        assert!(cap_verify_impl(u64::from(CAP_INDEX_ISO), OP_CALL_PASS_PARAM));
        assert_eq!(verify_call_count(), 1);
        assert_eq!(verify_deny_count(), 0);
    }

    #[test]
    fn val_passes_fn_entry() {
        let _g = lock_and_reset();
        assert!(cap_verify_impl(u64::from(CAP_INDEX_VAL), OP_FN_ENTRY));
        assert_eq!(verify_deny_count(), 0);
    }

    #[test]
    fn tag_denies_field_access() {
        let _g = lock_and_reset();
        // Tag carries no payload ⇒ field-access denied per Pony-6 alias-matrix.
        assert!(!cap_verify_impl(
            u64::from(CAP_INDEX_TAG),
            OP_FIELD_ACCESS
        ));
        assert_eq!(verify_deny_count(), 1);
    }

    #[test]
    fn out_of_range_op_kind_denies() {
        let _g = lock_and_reset();
        // op_kind beyond OP_KIND_MAX is a malformed verify-call ; deny.
        assert!(!cap_verify_impl(u64::from(CAP_INDEX_ISO), OP_KIND_MAX));
        assert!(!cap_verify_impl(u64::from(CAP_INDEX_ISO), 999));
        assert_eq!(verify_deny_count(), 2);
    }

    #[test]
    fn out_of_range_cap_index_denies() {
        let _g = lock_and_reset();
        // cap_index beyond CAP_KIND_COUNT is also malformed.
        assert!(!cap_verify_impl(99, OP_CALL_PASS_PARAM));
        assert_eq!(verify_deny_count(), 1);
    }

    #[test]
    fn upper_bits_of_cap_handle_ignored() {
        let _g = lock_and_reset();
        // The decoder masks low byte ; high bits hold reserved metadata.
        let handle = (0xDEAD_BEEF_u64 << 8) | u64::from(CAP_INDEX_ISO);
        assert!(cap_verify_impl(handle, OP_RETURN));
    }

    #[test]
    fn audit_counters_track_total_and_denials() {
        let _g = lock_and_reset();
        let _ = cap_verify_impl(u64::from(CAP_INDEX_ISO), OP_FN_ENTRY); // allow
        let _ = cap_verify_impl(u64::from(CAP_INDEX_TAG), OP_FIELD_ACCESS); // deny
        let _ = cap_verify_impl(u64::from(CAP_INDEX_VAL), OP_RETURN); // allow
        let _ = cap_verify_impl(99, OP_CALL_PASS_PARAM); // deny (bad cap)
        assert_eq!(verify_call_count(), 4);
        assert_eq!(verify_deny_count(), 2);
    }

    #[test]
    fn ffi_returns_byte_one_for_allow_zero_for_deny() {
        let _g = lock_and_reset();
        // SAFETY : __cssl_cap_verify is `unsafe extern "C"` for ABI reasons
        // but contains no unsafe operations on its inputs. Calling it from
        // a test is sound.
        let allow = unsafe {
            super::__cssl_cap_verify(u64::from(CAP_INDEX_ISO), OP_FN_ENTRY)
        };
        let deny = unsafe {
            super::__cssl_cap_verify(u64::from(CAP_INDEX_TAG), OP_FIELD_ACCESS)
        };
        assert_eq!(allow, 1);
        assert_eq!(deny, 0);
    }
}
