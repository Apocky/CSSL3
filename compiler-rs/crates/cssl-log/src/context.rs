//! Per-emission [`Context`] + global frame-counter.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 2.3 + § 2.4.
//!
//! § REPLAY-DETERMINISM (§ 2.4 / § 7.2) :
//!   The frame-counter is the canonical "logical clock" for log entries.
//!   When `replay_strict=true`, sinks use `frame_n` instead of wall-clock
//!   timestamps — replay byte-equality holds. Macros bake `frame_n` at
//!   emit-time via [`current_frame`] which reads a process-global atomic.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::path_hash_field::PathHashField;
use crate::severity::{Severity, SourceLocation};
use crate::subsystem::SubsystemTag;

#[cfg(test)]
pub(crate) static FRAME_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Process-global logical frame-counter. Updated by the engine driver via
/// [`set_current_frame`] ; read by every macro-expansion via
/// [`current_frame`].
///
/// § DETERMINISM : Frame-N is the canonical "time" axis for replay
/// (spec § 7.2). Wall-clock timestamps are NEVER baked into log payloads
/// when `replay_strict=true`.
static CURRENT_FRAME: AtomicU64 = AtomicU64::new(0);

/// Set the current logical frame number. Called by the engine driver at
/// the top of each frame. Per-frame rate-limiters key on this value.
pub fn set_current_frame(frame_n: u64) {
    CURRENT_FRAME.store(frame_n, Ordering::Release);
}

/// Read the current logical frame number. If the engine has not yet
/// initialized, returns 0 (per spec § 7.6 "log-before-engine-init uses
/// frame zero").
#[must_use]
pub fn current_frame() -> u64 {
    CURRENT_FRAME.load(Ordering::Acquire)
}

#[cfg(test)]
pub(crate) fn reset_current_frame_for_test() {
    CURRENT_FRAME.store(0, Ordering::SeqCst);
}

/// Per-emission [`Context`] passed from macro-expansion into
/// [`crate::emit::emit_structured`]. Carries the four invariants every
/// log entry needs : severity, subsystem, source-loc, frame-n.
///
/// § SPEC : § 2.3 macro-expansion lowering.
#[derive(Debug, Clone, Copy)]
pub struct Context {
    /// Severity level of the emission.
    pub severity: Severity,
    /// Subsystem tag.
    pub subsystem: SubsystemTag,
    /// Source location of the call-site (file_path_hash, line, col).
    pub source: SourceLocation,
    /// Logical frame-number at emission time.
    pub frame_n: u64,
}

impl Context {
    /// Construct a Context for an emit-site. Macros call this internally ;
    /// app code rarely constructs Context directly (the macros do it).
    #[must_use]
    pub const fn new(
        severity: Severity,
        subsystem: SubsystemTag,
        source: SourceLocation,
        frame_n: u64,
    ) -> Self {
        Self {
            severity,
            subsystem,
            source,
            frame_n,
        }
    }

    /// Convenience : construct with [`current_frame`] pre-baked.
    #[must_use]
    pub fn at_now(severity: Severity, subsystem: SubsystemTag, source: SourceLocation) -> Self {
        Self::new(severity, subsystem, source, current_frame())
    }

    /// Compute the rate-limit fingerprint frame-bucket per spec § 1.6.
    /// `frame_bucket = frame_n / 60` ⟵ ~1-second windows @ 60fps.
    #[must_use]
    pub const fn frame_bucket(&self) -> u64 {
        self.frame_n / 60
    }
}

/// Zero-value Context for static-init / scaffolding contexts. Source-loc
/// defaults to zero-hash + (0, 0) ; frame_n is `0`. Severity defaults to
/// [`Severity::Info`] ; subsystem defaults to [`SubsystemTag::Test`].
impl Default for Context {
    fn default() -> Self {
        Self {
            severity: Severity::Info,
            subsystem: SubsystemTag::Test,
            source: SourceLocation::new(PathHashField::zero(), 0, 0),
            frame_n: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        current_frame, reset_current_frame_for_test, set_current_frame, Context, SourceLocation,
    };
    use crate::path_hash_field::PathHashField;
    use crate::severity::Severity;
    use crate::subsystem::SubsystemTag;
    use cssl_telemetry::PathHasher;

    // § Frame-counter atomic semantics.

    fn fresh_loc() -> SourceLocation {
        let hasher = PathHasher::from_seed([0u8; 32]);
        let h = hasher.hash_str("/test/file.rs");
        SourceLocation::new(PathHashField::from_path_hash(h), 42, 7)
    }

    fn lock_frame() -> std::sync::MutexGuard<'static, ()> {
        super::FRAME_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn frame_counter_updates_visible() {
        // Frame-counter is process-global ; tests serialize on FRAME_TEST_LOCK.
        let _g = lock_frame();
        reset_current_frame_for_test();
        set_current_frame(100);
        assert_eq!(current_frame(), 100);
        set_current_frame(200);
        assert_eq!(current_frame(), 200);
    }

    #[test]
    fn frame_counter_at_zero_after_reset() {
        let _g = lock_frame();
        reset_current_frame_for_test();
        assert_eq!(current_frame(), 0);
    }

    // § Context constructor.

    #[test]
    fn context_new_pins_all_fields() {
        let loc = fresh_loc();
        let ctx = Context::new(Severity::Info, SubsystemTag::Render, loc, 99);
        assert_eq!(ctx.severity, Severity::Info);
        assert_eq!(ctx.subsystem, SubsystemTag::Render);
        assert_eq!(ctx.source, loc);
        assert_eq!(ctx.frame_n, 99);
    }

    #[test]
    fn context_at_now_uses_current_frame() {
        let _g = lock_frame();
        reset_current_frame_for_test();
        set_current_frame(777);
        let loc = fresh_loc();
        let ctx = Context::at_now(Severity::Warning, SubsystemTag::Audio, loc);
        assert_eq!(ctx.frame_n, 777);
    }

    #[test]
    fn context_default_is_well_formed() {
        let ctx = Context::default();
        assert_eq!(ctx.severity, Severity::Info);
        assert_eq!(ctx.subsystem, SubsystemTag::Test);
        assert_eq!(ctx.frame_n, 0);
    }

    // § Frame-bucket spec § 1.6.

    #[test]
    fn frame_bucket_zero_for_first_60_frames() {
        let loc = fresh_loc();
        for f in 0..60u64 {
            let ctx = Context::new(Severity::Trace, SubsystemTag::Test, loc, f);
            assert_eq!(ctx.frame_bucket(), 0);
        }
    }

    #[test]
    fn frame_bucket_one_at_frame_60() {
        let loc = fresh_loc();
        let ctx = Context::new(Severity::Trace, SubsystemTag::Test, loc, 60);
        assert_eq!(ctx.frame_bucket(), 1);
    }

    #[test]
    fn frame_bucket_aliases_within_window() {
        let loc = fresh_loc();
        let ctx_a = Context::new(Severity::Trace, SubsystemTag::Test, loc, 60);
        let ctx_b = Context::new(Severity::Trace, SubsystemTag::Test, loc, 119);
        assert_eq!(ctx_a.frame_bucket(), ctx_b.frame_bucket());
    }

    #[test]
    fn frame_bucket_increments_each_60() {
        let loc = fresh_loc();
        let ctx = Context::new(Severity::Trace, SubsystemTag::Test, loc, 12_000);
        assert_eq!(ctx.frame_bucket(), 200);
    }

    // § Copy + Debug semantics.

    #[test]
    fn context_is_copy() {
        let loc = fresh_loc();
        let ctx_a = Context::new(Severity::Info, SubsystemTag::Test, loc, 1);
        let ctx_b = ctx_a;
        assert_eq!(ctx_a.frame_n, ctx_b.frame_n);
    }
}
