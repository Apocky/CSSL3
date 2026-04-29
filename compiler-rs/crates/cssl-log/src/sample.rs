//! Sampling + rate-limit per-frame + per-fingerprint (spec § 2.5 + § 1.6).
//!
//! § PER-FRAME RATE-LIMITS (spec § 2.5) :
//!   Trace ≤ 64/frame ; Debug ≤ 256 ; Info ≤ 1024 ; Warning ≤ 4096 ;
//!   Error / Fatal : NO-CAP (spec exempts).
//!
//! § PER-FINGERPRINT RATE-LIMITS (spec § 1.6 + § 2.5) :
//!   ≤ 4 emissions per fingerprint per frame-bucket ; remainder summarized
//!   as a single record when the bucket closes. Error / Fatal exempt.
//!
//! § FINGERPRINT (spec § 1.6) :
//!   `BLAKE3((subsystem_u8, source_loc.line, source_loc.col, file_path_hash, frame_bucket))`
//!   ⟵ truncated to first 8 bytes for compact in-memory bucketing.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::context::Context;
use crate::severity::Severity;
use crate::subsystem::SubsystemTag;

// ───────────────────────────────────────────────────────────────────────
// § Per-frame counters
// ───────────────────────────────────────────────────────────────────────

/// Per-(severity, subsystem) per-frame counter table. 6 severities × 21
/// subsystems = 126 counters. Encoded as `[AtomicU64; 6][21]` flattened
/// into `[AtomicU64; 126]`.
struct FrameCounters {
    /// Each cell counts emissions seen in the current frame for the
    /// (severity, subsystem) pair.
    cells: [AtomicU64; SubsystemTag::COUNT * 6],
    /// The frame-N these counters belong to. Reset to 0 when frame changes.
    current_frame: AtomicU64,
    /// Per-severity per-frame caps. Default per spec § 2.5 ;
    /// override via [`set_per_frame_cap`].
    caps: [AtomicU64; 6],
}

impl FrameCounters {
    const fn new() -> Self {
        // Const-init array of AtomicU64. AtomicU64 has interior-mutable but
        // const new — array repeat-initializer requires `Copy`. We use the
        // array-init syntax with explicit AtomicU64::new(0).
        const ZERO: AtomicU64 = AtomicU64::new(0);
        Self {
            cells: [ZERO; SubsystemTag::COUNT * 6],
            current_frame: ZERO,
            caps: [
                AtomicU64::new(64),       // Trace
                AtomicU64::new(256),      // Debug
                AtomicU64::new(1024),     // Info
                AtomicU64::new(4096),     // Warning
                AtomicU64::new(u64::MAX), // Error
                AtomicU64::new(u64::MAX), // Fatal
            ],
        }
    }

    fn cell_idx(severity: Severity, subsystem: SubsystemTag) -> usize {
        (severity.as_u8() as usize) * SubsystemTag::COUNT + (subsystem.as_u8() as usize)
    }

    /// Reset all per-(severity, subsystem) cells to 0 because the frame
    /// rolled over.
    fn reset_all_cells(&self) {
        for c in &self.cells {
            c.store(0, Ordering::Release);
        }
    }

    /// Try to record an emission. Returns `true` if the emission fits
    /// within the per-frame cap ; `false` if it exceeds (drop).
    /// Exempt severities (Error/Fatal) always return `true`.
    fn try_record(&self, severity: Severity, subsystem: SubsystemTag, frame_n: u64) -> bool {
        // Roll-over : if frame changed, reset all cells.
        let cur = self.current_frame.load(Ordering::Acquire);
        if cur != frame_n {
            // Best-effort CAS to claim the roll-over. Spurious failures
            // here are fine ; whoever wins resets the cells.
            if self
                .current_frame
                .compare_exchange(cur, frame_n, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.reset_all_cells();
            }
        }

        if severity.is_rate_limit_exempt() {
            // Still increment for telemetry-visibility but skip cap check.
            self.cells[Self::cell_idx(severity, subsystem)].fetch_add(1, Ordering::AcqRel);
            return true;
        }

        let cap = self.caps[severity.as_u8() as usize].load(Ordering::Acquire);
        let prev = self.cells[Self::cell_idx(severity, subsystem)].fetch_add(1, Ordering::AcqRel);
        prev < cap
    }

    fn set_cap(&self, severity: Severity, cap: u64) {
        if !severity.is_rate_limit_exempt() {
            self.caps[severity.as_u8() as usize].store(cap, Ordering::Release);
        }
    }

    fn cap(&self, severity: Severity) -> u64 {
        self.caps[severity.as_u8() as usize].load(Ordering::Acquire)
    }

    fn count(&self, severity: Severity, subsystem: SubsystemTag, frame_n: u64) -> u64 {
        let cur = self.current_frame.load(Ordering::Acquire);
        if cur != frame_n {
            return 0;
        }
        self.cells[Self::cell_idx(severity, subsystem)].load(Ordering::Acquire)
    }
}

static FRAME_COUNTERS: FrameCounters = FrameCounters::new();

/// Try to record an emission against per-frame caps. Returns `true` if
/// the emission is permitted ; `false` if it exceeds the cap (drop).
///
/// Exempt severities (Error / Fatal) always return `true`.
#[must_use]
pub fn try_record_per_frame(severity: Severity, subsystem: SubsystemTag, frame_n: u64) -> bool {
    FRAME_COUNTERS.try_record(severity, subsystem, frame_n)
}

/// Override the per-frame cap for `severity`. Has no effect on Error/Fatal
/// (those are spec-exempt).
pub fn set_per_frame_cap(severity: Severity, cap: u64) {
    FRAME_COUNTERS.set_cap(severity, cap);
}

/// Read the current per-frame cap for `severity`.
#[must_use]
pub fn per_frame_cap(severity: Severity) -> u64 {
    FRAME_COUNTERS.cap(severity)
}

/// Read the current count for (severity, subsystem) within the current
/// frame. Used by tests + by the summary-record emitter at frame-close.
#[must_use]
pub fn current_count(severity: Severity, subsystem: SubsystemTag, frame_n: u64) -> u64 {
    FRAME_COUNTERS.count(severity, subsystem, frame_n)
}

#[cfg(test)]
pub(crate) fn reset_frame_counters_for_test() {
    FRAME_COUNTERS.reset_all_cells();
    FRAME_COUNTERS.current_frame.store(0, Ordering::Release);
    // Restore default caps too.
    FRAME_COUNTERS.set_cap(Severity::Trace, 64);
    FRAME_COUNTERS.set_cap(Severity::Debug, 256);
    FRAME_COUNTERS.set_cap(Severity::Info, 1024);
    FRAME_COUNTERS.set_cap(Severity::Warning, 4096);
}

// ───────────────────────────────────────────────────────────────────────
// § Per-fingerprint rate-limit (spec § 1.6 + § 2.5)
// ───────────────────────────────────────────────────────────────────────

/// Compute a fingerprint for a (subsystem, source-loc, frame-bucket) per
/// spec § 1.6. Truncated to first 8 bytes for compact in-memory bucketing.
#[must_use]
pub fn compute_fingerprint(ctx: &Context) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[ctx.subsystem.as_u8()]);
    hasher.update(&ctx.source.line.to_be_bytes());
    hasher.update(&ctx.source.column.to_be_bytes());
    hasher.update(ctx.source.file_path_hash.as_bytes());
    hasher.update(&ctx.frame_bucket().to_be_bytes());
    let out = hasher.finalize();
    let bytes = out.as_bytes();
    u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

/// Per-fingerprint per-bucket rate-limit. Up to 4 emissions per FP per
/// bucket ; remainder dropped (counted into `dropped_count` for the
/// summary-record at bucket-close).
///
/// § DESIGN : bucket is `frame_n / 60`. Storage is a small open-addressed
/// fingerprint hash-table holding the most recently seen fingerprints +
/// counts. Fixed size = 256 entries ⟵ if collision, we evict the oldest
/// (FIFO sweep). This is a soft-LRU ; perfect accuracy is NOT required —
/// the property we need is "≤ 4 emit per fingerprint per bucket modulo
/// table-eviction", with eviction mode being "occasional under-counting"
/// (spec accepts).
struct FingerprintTable {
    entries: parking_lot_lite::Mutex<FingerprintEntries>,
}

struct FingerprintEntries {
    table: Vec<FingerprintEntry>,
    next_evict: usize,
    current_bucket: u64,
}

#[derive(Debug, Clone, Copy)]
struct FingerprintEntry {
    fingerprint: u64,
    count: u32,
}

const FP_TABLE_SIZE: usize = 256;
const PER_FP_PER_BUCKET_CAP: u32 = 4;

impl FingerprintTable {
    fn new() -> Self {
        Self {
            entries: parking_lot_lite::Mutex::new(FingerprintEntries {
                table: vec![
                    FingerprintEntry {
                        fingerprint: 0,
                        count: 0,
                    };
                    FP_TABLE_SIZE
                ],
                next_evict: 0,
                current_bucket: 0,
            }),
        }
    }

    fn try_record(&self, fp: u64, bucket: u64) -> FingerprintDecision {
        let mut e = self.entries.lock();

        // Bucket roll-over : flush all entries.
        if e.current_bucket != bucket {
            e.current_bucket = bucket;
            for slot in &mut e.table {
                *slot = FingerprintEntry {
                    fingerprint: 0,
                    count: 0,
                };
            }
            e.next_evict = 0;
        }

        // Find existing.
        for slot in &mut e.table {
            if slot.fingerprint == fp && slot.count > 0 {
                if slot.count < PER_FP_PER_BUCKET_CAP {
                    slot.count = slot.count.saturating_add(1);
                    return FingerprintDecision::Permit { count: slot.count };
                }
                slot.count = slot.count.saturating_add(1);
                return FingerprintDecision::Drop {
                    dropped_count: slot.count - PER_FP_PER_BUCKET_CAP,
                };
            }
        }

        // Insert new at next_evict slot.
        let idx = e.next_evict % FP_TABLE_SIZE;
        e.table[idx] = FingerprintEntry {
            fingerprint: fp,
            count: 1,
        };
        e.next_evict = e.next_evict.wrapping_add(1);
        FingerprintDecision::Permit { count: 1 }
    }

    fn reset_for_test(&self) {
        let mut e = self.entries.lock();
        for slot in &mut e.table {
            *slot = FingerprintEntry {
                fingerprint: 0,
                count: 0,
            };
        }
        e.next_evict = 0;
        e.current_bucket = 0;
    }
}

/// Per-fingerprint rate-limit decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FingerprintDecision {
    /// Emission permitted ; `count` is how many times this fingerprint
    /// has been seen in the current bucket (1..=cap).
    Permit { count: u32 },
    /// Emission dropped ; `dropped_count` is how many extra emissions
    /// have been suppressed so far (0 = first drop).
    Drop { dropped_count: u32 },
}

impl FingerprintDecision {
    /// True iff the emission is permitted.
    #[must_use]
    pub const fn is_permit(self) -> bool {
        matches!(self, Self::Permit { .. })
    }
}

// We use a tiny custom mutex-newtype so we don't pull in parking_lot.
// `std::sync::Mutex` works fine for our needs. Renamed to avoid shadowing.
mod parking_lot_lite {
    use std::sync::Mutex as StdMutex;
    use std::sync::MutexGuard;

    pub(super) struct Mutex<T>(StdMutex<T>);

    impl<T> Mutex<T> {
        pub(super) const fn new(v: T) -> Self {
            Self(StdMutex::new(v))
        }
        pub(super) fn lock(&self) -> MutexGuard<'_, T> {
            // Recover from poisoning : a panic during a hold should not
            // disable the rate-limiter for the rest of the process.
            self.0.lock().unwrap_or_else(|e| e.into_inner())
        }
    }
}

// Lazily-initialized fingerprint table (since `Vec::new` isn't const).
fn fingerprint_table() -> &'static FingerprintTable {
    use std::sync::OnceLock;
    static TABLE: OnceLock<FingerprintTable> = OnceLock::new();
    TABLE.get_or_init(FingerprintTable::new)
}

/// Try to record a per-fingerprint emission. Exempt severities
/// (Error / Fatal) bypass and always return `Permit`.
#[must_use]
pub fn try_record_per_fingerprint(severity: Severity, ctx: &Context) -> FingerprintDecision {
    if severity.is_rate_limit_exempt() {
        return FingerprintDecision::Permit { count: 1 };
    }
    let fp = compute_fingerprint(ctx);
    fingerprint_table().try_record(fp, ctx.frame_bucket())
}

#[cfg(test)]
pub(crate) fn reset_fingerprint_table_for_test() {
    fingerprint_table().reset_for_test();
}

#[cfg(test)]
mod tests {
    use super::{
        compute_fingerprint, current_count, per_frame_cap, reset_fingerprint_table_for_test,
        reset_frame_counters_for_test, set_per_frame_cap, try_record_per_fingerprint,
        try_record_per_frame, FingerprintDecision,
    };
    use crate::context::Context;
    use crate::path_hash_field::PathHashField;
    use crate::severity::{Severity, SourceLocation};
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::PathHasher;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_frame_counters_for_test();
        reset_fingerprint_table_for_test();
        g
    }

    fn fresh_ctx(line: u32, frame_n: u64) -> Context {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test.rs");
        Context::new(
            Severity::Info,
            SubsystemTag::Render,
            SourceLocation::new(PathHashField::from_path_hash(h), line, 0),
            frame_n,
        )
    }

    // § Per-frame caps.

    #[test]
    fn default_per_frame_caps_match_spec() {
        let _g = lock_and_reset();
        assert_eq!(per_frame_cap(Severity::Trace), 64);
        assert_eq!(per_frame_cap(Severity::Debug), 256);
        assert_eq!(per_frame_cap(Severity::Info), 1024);
        assert_eq!(per_frame_cap(Severity::Warning), 4096);
        assert_eq!(per_frame_cap(Severity::Error), u64::MAX);
        assert_eq!(per_frame_cap(Severity::Fatal), u64::MAX);
    }

    #[test]
    fn try_record_under_cap_permits() {
        let _g = lock_and_reset();
        for _ in 0..10 {
            assert!(try_record_per_frame(
                Severity::Info,
                SubsystemTag::Render,
                1
            ));
        }
    }

    #[test]
    fn try_record_over_cap_drops() {
        let _g = lock_and_reset();
        set_per_frame_cap(Severity::Info, 5);
        for _ in 0..5 {
            assert!(try_record_per_frame(
                Severity::Info,
                SubsystemTag::Render,
                1
            ));
        }
        // Sixth attempt → drop.
        assert!(!try_record_per_frame(
            Severity::Info,
            SubsystemTag::Render,
            1
        ));
    }

    #[test]
    fn frame_rollover_resets_counter() {
        let _g = lock_and_reset();
        set_per_frame_cap(Severity::Info, 2);
        assert!(try_record_per_frame(
            Severity::Info,
            SubsystemTag::Render,
            1
        ));
        assert!(try_record_per_frame(
            Severity::Info,
            SubsystemTag::Render,
            1
        ));
        assert!(!try_record_per_frame(
            Severity::Info,
            SubsystemTag::Render,
            1
        ));
        // Frame rollover.
        assert!(try_record_per_frame(
            Severity::Info,
            SubsystemTag::Render,
            2
        ));
    }

    #[test]
    fn error_exempt_from_cap() {
        let _g = lock_and_reset();
        set_per_frame_cap(Severity::Info, 1);
        // Set Error cap-via-API would silently no-op (exempt). Verify.
        for _ in 0..10_000 {
            assert!(try_record_per_frame(
                Severity::Error,
                SubsystemTag::Render,
                1
            ));
        }
    }

    #[test]
    fn fatal_exempt_from_cap() {
        let _g = lock_and_reset();
        for _ in 0..10_000 {
            assert!(try_record_per_frame(
                Severity::Fatal,
                SubsystemTag::Render,
                1
            ));
        }
    }

    #[test]
    fn set_per_frame_cap_no_op_on_exempt() {
        let _g = lock_and_reset();
        set_per_frame_cap(Severity::Error, 5); // No-op for exempt.
        assert_eq!(per_frame_cap(Severity::Error), u64::MAX);
        set_per_frame_cap(Severity::Fatal, 5);
        assert_eq!(per_frame_cap(Severity::Fatal), u64::MAX);
    }

    #[test]
    fn separate_subsystems_have_separate_counters() {
        let _g = lock_and_reset();
        set_per_frame_cap(Severity::Info, 1);
        assert!(try_record_per_frame(
            Severity::Info,
            SubsystemTag::Render,
            1
        ));
        assert!(try_record_per_frame(Severity::Info, SubsystemTag::Audio, 1));
    }

    #[test]
    fn separate_severities_have_separate_counters() {
        let _g = lock_and_reset();
        set_per_frame_cap(Severity::Info, 1);
        set_per_frame_cap(Severity::Warning, 1);
        assert!(try_record_per_frame(
            Severity::Info,
            SubsystemTag::Render,
            1
        ));
        assert!(try_record_per_frame(
            Severity::Warning,
            SubsystemTag::Render,
            1
        ));
    }

    #[test]
    fn current_count_visible_after_record() {
        let _g = lock_and_reset();
        let _ = try_record_per_frame(Severity::Info, SubsystemTag::Render, 5);
        let _ = try_record_per_frame(Severity::Info, SubsystemTag::Render, 5);
        assert_eq!(current_count(Severity::Info, SubsystemTag::Render, 5), 2);
    }

    #[test]
    fn current_count_zero_for_other_frame() {
        let _g = lock_and_reset();
        let _ = try_record_per_frame(Severity::Info, SubsystemTag::Render, 5);
        assert_eq!(current_count(Severity::Info, SubsystemTag::Render, 999), 0);
    }

    // § Fingerprint compute.

    #[test]
    fn fingerprint_deterministic() {
        let _g = lock_and_reset();
        let ctx = fresh_ctx(42, 100);
        let fp1 = compute_fingerprint(&ctx);
        let fp2 = compute_fingerprint(&ctx);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn fingerprint_differs_per_line() {
        let _g = lock_and_reset();
        let a = fresh_ctx(1, 100);
        let b = fresh_ctx(2, 100);
        assert_ne!(compute_fingerprint(&a), compute_fingerprint(&b));
    }

    #[test]
    fn fingerprint_aliases_within_bucket() {
        let _g = lock_and_reset();
        // Same bucket = frame/60. 60 and 119 share bucket 1.
        let a = fresh_ctx(1, 60);
        let b = fresh_ctx(1, 119);
        assert_eq!(compute_fingerprint(&a), compute_fingerprint(&b));
    }

    #[test]
    fn fingerprint_differs_across_buckets() {
        let _g = lock_and_reset();
        let a = fresh_ctx(1, 0);
        let b = fresh_ctx(1, 60);
        assert_ne!(compute_fingerprint(&a), compute_fingerprint(&b));
    }

    // § Per-fingerprint rate-limit.

    #[test]
    fn per_fingerprint_first_4_permit() {
        let _g = lock_and_reset();
        let ctx = fresh_ctx(99, 0);
        for _ in 0..4 {
            let d = try_record_per_fingerprint(Severity::Info, &ctx);
            assert!(d.is_permit());
        }
    }

    #[test]
    fn per_fingerprint_5th_drops() {
        let _g = lock_and_reset();
        let ctx = fresh_ctx(99, 0);
        for _ in 0..4 {
            let _ = try_record_per_fingerprint(Severity::Info, &ctx);
        }
        let d = try_record_per_fingerprint(Severity::Info, &ctx);
        assert!(!d.is_permit());
        if let FingerprintDecision::Drop { dropped_count } = d {
            assert_eq!(dropped_count, 1);
        }
    }

    #[test]
    fn per_fingerprint_drop_counts_increment() {
        let _g = lock_and_reset();
        let ctx = fresh_ctx(99, 0);
        for _ in 0..4 {
            let _ = try_record_per_fingerprint(Severity::Info, &ctx);
        }
        let d1 = try_record_per_fingerprint(Severity::Info, &ctx);
        let d2 = try_record_per_fingerprint(Severity::Info, &ctx);
        if let (
            FingerprintDecision::Drop { dropped_count: a },
            FingerprintDecision::Drop { dropped_count: b },
        ) = (d1, d2)
        {
            assert!(b > a);
        } else {
            panic!("expected Drop");
        }
    }

    #[test]
    fn per_fingerprint_bucket_rollover_resets() {
        let _g = lock_and_reset();
        let ctx_a = fresh_ctx(99, 0);
        let ctx_b = fresh_ctx(99, 60);
        // Saturate bucket 0.
        for _ in 0..4 {
            assert!(try_record_per_fingerprint(Severity::Info, &ctx_a).is_permit());
        }
        // Bucket 1 — fresh state.
        let d = try_record_per_fingerprint(Severity::Info, &ctx_b);
        assert!(d.is_permit());
    }

    #[test]
    fn per_fingerprint_distinct_lines_independent() {
        let _g = lock_and_reset();
        let ctx_a = fresh_ctx(1, 0);
        let ctx_b = fresh_ctx(2, 0);
        // Saturate FP-A.
        for _ in 0..4 {
            let _ = try_record_per_fingerprint(Severity::Info, &ctx_a);
        }
        // FP-B still has full budget.
        let d = try_record_per_fingerprint(Severity::Info, &ctx_b);
        assert!(d.is_permit());
    }

    #[test]
    fn per_fingerprint_error_bypasses() {
        let _g = lock_and_reset();
        let ctx = fresh_ctx(99, 0);
        for _ in 0..1000 {
            let d = try_record_per_fingerprint(Severity::Error, &ctx);
            assert!(d.is_permit());
        }
    }

    #[test]
    fn per_fingerprint_fatal_bypasses() {
        let _g = lock_and_reset();
        let ctx = fresh_ctx(99, 0);
        for _ in 0..1000 {
            let d = try_record_per_fingerprint(Severity::Fatal, &ctx);
            assert!(d.is_permit());
        }
    }

    #[test]
    fn fingerprint_decision_is_permit_method() {
        assert!(FingerprintDecision::Permit { count: 1 }.is_permit());
        assert!(!FingerprintDecision::Drop { dropped_count: 0 }.is_permit());
    }
}
