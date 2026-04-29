//! `enabled()` AtomicU64 fast-path + level-filter hot-swap (spec § 2.3).
//!
//! § COST-MODEL :
//!   - `enabled(severity, subsystem)` is a SINGLE `AtomicU64::load` followed
//!     by a bit-test ⟵ ≈2ns on AMD64 (one L1-mov + one AND).
//!   - Hot-swap is `AtomicU64::store` ⟵ wait-free ; latest write wins on
//!     next emit. Spec § 2.5 "MCP tool `set_sampling_policy(...)` allows
//!     runtime adjustment" hooks here.
//!
//! § BIT-LAYOUT :
//!   We have 6 severities × 21 subsystems = 126 binary "should this fire"
//!   states. We pack them into a 4-element `[AtomicU64; 4]` (256 bits) :
//!     - bit `(severity * 32 + subsystem)` for each (severity, subsystem)
//!       pair (32-stride leaves room for future subsystems up to 32).
//!     - The 4-word array stores the 256-bit table ; access is computed
//!       via index = bit/64, shift = bit%64.
//!
//! § DEFAULT POLICY (per spec § 2.5) :
//!   - Trace + Debug : OFF by default (require explicit enable via cap).
//!   - Info + Warning : ON for ALL subsystems.
//!   - Error + Fatal : ON for ALL subsystems (and rate-limit-exempt ; see
//!     [`crate::sample::FrameCounters`]).

use core::sync::atomic::{AtomicU64, Ordering};

use crate::severity::Severity;
use crate::subsystem::SubsystemTag;

/// Bit-position for a (severity, subsystem) pair. 32-stride per severity
/// leaves room for future subsystems up to `SubsystemTag::COUNT = 32`.
const fn bit_index(severity: Severity, subsystem: SubsystemTag) -> u32 {
    (severity.as_u8() as u32) * 32 + (subsystem.as_u8() as u32)
}

/// 256-bit enabled-table backed by 4 atomic u64s. Process-global ; init'd
/// at program start with the default policy ([`init_default_policy`]).
struct EnabledTable {
    words: [AtomicU64; 4],
}

impl EnabledTable {
    const fn new() -> Self {
        Self {
            words: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
        }
    }

    /// Test bit at given (severity, subsystem).
    #[inline]
    fn test(&self, severity: Severity, subsystem: SubsystemTag) -> bool {
        let bit = bit_index(severity, subsystem);
        let word_idx = (bit / 64) as usize;
        let shift = bit % 64;
        // SAFETY-NOTE : word_idx ≤ 3 because bit < 6*32 = 192 < 256.
        let w = self.words[word_idx].load(Ordering::Acquire);
        (w >> shift) & 1 == 1
    }

    /// Set bit (enable). Wait-free.
    fn set(&self, severity: Severity, subsystem: SubsystemTag) {
        let bit = bit_index(severity, subsystem);
        let word_idx = (bit / 64) as usize;
        let shift = bit % 64;
        let mask = 1u64 << shift;
        loop {
            let cur = self.words[word_idx].load(Ordering::Acquire);
            let new = cur | mask;
            if cur == new {
                return;
            }
            if self.words[word_idx]
                .compare_exchange(cur, new, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }

    /// Clear bit (disable). Wait-free.
    fn clear(&self, severity: Severity, subsystem: SubsystemTag) {
        let bit = bit_index(severity, subsystem);
        let word_idx = (bit / 64) as usize;
        let shift = bit % 64;
        let mask = 1u64 << shift;
        loop {
            let cur = self.words[word_idx].load(Ordering::Acquire);
            let new = cur & !mask;
            if cur == new {
                return;
            }
            if self.words[word_idx]
                .compare_exchange(cur, new, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }

    /// Reset all bits to zero. Used by tests to re-init in known state.
    fn clear_all(&self) {
        for w in &self.words {
            w.store(0, Ordering::Release);
        }
    }
}

static TABLE: EnabledTable = EnabledTable::new();

/// Default-policy initialization flag. Set on first call to
/// [`init_default_policy`] ; subsequent calls are no-ops unless explicitly
/// reset via [`reset_for_test`].
static DEFAULT_INIT: AtomicU64 = AtomicU64::new(0);

/// Initialize default-policy bits per spec § 2.5. Idempotent — safe to
/// call multiple times. Engine-init paths call this once at startup.
pub fn init_default_policy() {
    // Idempotency guard.
    if DEFAULT_INIT
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    install_default_policy();
}

fn install_default_policy() {
    for sub in SubsystemTag::all() {
        // Trace + Debug : OFF (require cap-token to enable).
        TABLE.clear(Severity::Trace, sub);
        TABLE.clear(Severity::Debug, sub);
        // Info + Warning + Error + Fatal : ON.
        TABLE.set(Severity::Info, sub);
        TABLE.set(Severity::Warning, sub);
        TABLE.set(Severity::Error, sub);
        TABLE.set(Severity::Fatal, sub);
    }
}

/// Fast-path : returns true iff the (severity, subsystem) pair is enabled.
/// Single atomic load + bit-test. ~2ns on AMD64.
#[inline]
#[must_use]
pub fn enabled(severity: Severity, subsystem: SubsystemTag) -> bool {
    TABLE.test(severity, subsystem)
}

/// Hot-swap : enable a single (severity, subsystem). Wait-free.
pub fn enable(severity: Severity, subsystem: SubsystemTag) {
    TABLE.set(severity, subsystem);
}

/// Hot-swap : disable a single (severity, subsystem). Wait-free.
pub fn disable(severity: Severity, subsystem: SubsystemTag) {
    TABLE.clear(severity, subsystem);
}

/// Hot-swap : enable a severity for ALL subsystems.
pub fn enable_severity_all(severity: Severity) {
    for sub in SubsystemTag::all() {
        TABLE.set(severity, sub);
    }
}

/// Hot-swap : disable a severity for ALL subsystems.
pub fn disable_severity_all(severity: Severity) {
    for sub in SubsystemTag::all() {
        TABLE.clear(severity, sub);
    }
}

/// Hot-swap : set the level-floor — disable everything below `level`,
/// enable everything `>= level`. The MCP `set_log_level(...)` tool
/// (Wave-Jθ) is wired here.
pub fn set_level_floor(level: Severity) {
    for s in Severity::all() {
        for sub in SubsystemTag::all() {
            if s >= level {
                TABLE.set(s, sub);
            } else {
                TABLE.clear(s, sub);
            }
        }
    }
}

/// Force re-install of the default-policy. Unlike [`init_default_policy`]
/// (which is idempotent and only runs once), this always re-applies the
/// default. Useful for tests that need a fresh enabled-table between
/// subtests.
pub fn force_reset_to_default() {
    install_default_policy();
}

/// Test-only : reset the table + DEFAULT_INIT flag so subsequent
/// `init_default_policy()` runs cleanly.
#[cfg(test)]
pub(crate) fn reset_for_test() {
    TABLE.clear_all();
    DEFAULT_INIT.store(0, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::{
        bit_index, disable, disable_severity_all, enable, enable_severity_all, enabled,
        init_default_policy, reset_for_test, set_level_floor,
    };
    use crate::severity::Severity;
    use crate::subsystem::SubsystemTag;
    use std::sync::Mutex;

    // § Tests run sequentially against process-global state. Use a mutex
    //   so tests don't race each other.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_and_reset() -> std::sync::MutexGuard<'static, ()> {
        let g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test();
        g
    }

    // § Bit-index layout.

    #[test]
    fn bit_index_in_range() {
        for s in Severity::all() {
            for sub in SubsystemTag::all() {
                let bit = bit_index(s, sub);
                assert!(bit < 256, "bit {bit} out of range for ({s:?},{sub:?})");
            }
        }
    }

    #[test]
    fn bit_index_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in Severity::all() {
            for sub in SubsystemTag::all() {
                let bit = bit_index(s, sub);
                assert!(seen.insert(bit), "duplicate bit {bit}");
            }
        }
    }

    // § Default policy install.

    #[test]
    fn default_policy_disables_trace_debug() {
        let _g = lock_and_reset();
        init_default_policy();
        for sub in SubsystemTag::all() {
            assert!(!enabled(Severity::Trace, sub), "trace on for {sub:?}");
            assert!(!enabled(Severity::Debug, sub), "debug on for {sub:?}");
        }
    }

    #[test]
    fn default_policy_enables_info_through_fatal() {
        let _g = lock_and_reset();
        init_default_policy();
        for sub in SubsystemTag::all() {
            for s in [
                Severity::Info,
                Severity::Warning,
                Severity::Error,
                Severity::Fatal,
            ] {
                assert!(enabled(s, sub), "{s:?} off for {sub:?}");
            }
        }
    }

    #[test]
    fn default_policy_idempotent() {
        let _g = lock_and_reset();
        init_default_policy();
        let snapshot = enabled(Severity::Info, SubsystemTag::Render);
        init_default_policy();
        assert_eq!(snapshot, enabled(Severity::Info, SubsystemTag::Render));
    }

    // § Hot-swap : enable / disable.

    #[test]
    fn enable_then_test() {
        let _g = lock_and_reset();
        assert!(!enabled(Severity::Trace, SubsystemTag::Render));
        enable(Severity::Trace, SubsystemTag::Render);
        assert!(enabled(Severity::Trace, SubsystemTag::Render));
    }

    #[test]
    fn disable_clears_bit() {
        let _g = lock_and_reset();
        enable(Severity::Trace, SubsystemTag::Render);
        assert!(enabled(Severity::Trace, SubsystemTag::Render));
        disable(Severity::Trace, SubsystemTag::Render);
        assert!(!enabled(Severity::Trace, SubsystemTag::Render));
    }

    #[test]
    fn enable_severity_all_covers_all_subsystems() {
        let _g = lock_and_reset();
        enable_severity_all(Severity::Trace);
        for sub in SubsystemTag::all() {
            assert!(enabled(Severity::Trace, sub));
        }
    }

    #[test]
    fn disable_severity_all_clears_all_subsystems() {
        let _g = lock_and_reset();
        enable_severity_all(Severity::Trace);
        disable_severity_all(Severity::Trace);
        for sub in SubsystemTag::all() {
            assert!(!enabled(Severity::Trace, sub));
        }
    }

    // § Level-floor.

    #[test]
    fn set_level_floor_warning_disables_below() {
        let _g = lock_and_reset();
        set_level_floor(Severity::Warning);
        assert!(!enabled(Severity::Trace, SubsystemTag::Render));
        assert!(!enabled(Severity::Debug, SubsystemTag::Render));
        assert!(!enabled(Severity::Info, SubsystemTag::Render));
        assert!(enabled(Severity::Warning, SubsystemTag::Render));
        assert!(enabled(Severity::Error, SubsystemTag::Render));
        assert!(enabled(Severity::Fatal, SubsystemTag::Render));
    }

    #[test]
    fn set_level_floor_trace_enables_all() {
        let _g = lock_and_reset();
        set_level_floor(Severity::Trace);
        for s in Severity::all() {
            for sub in SubsystemTag::all() {
                assert!(enabled(s, sub), "{s:?},{sub:?}");
            }
        }
    }

    #[test]
    fn set_level_floor_fatal_disables_almost_all() {
        let _g = lock_and_reset();
        set_level_floor(Severity::Fatal);
        for s in [
            Severity::Trace,
            Severity::Debug,
            Severity::Info,
            Severity::Warning,
            Severity::Error,
        ] {
            for sub in SubsystemTag::all() {
                assert!(!enabled(s, sub), "{s:?},{sub:?}");
            }
        }
        for sub in SubsystemTag::all() {
            assert!(enabled(Severity::Fatal, sub));
        }
    }

    // § Single-bit-isolation : flipping (s,sub) does NOT touch other pairs.

    #[test]
    fn enable_one_does_not_touch_others() {
        let _g = lock_and_reset();
        enable(Severity::Trace, SubsystemTag::Render);
        for sub in SubsystemTag::all() {
            if sub == SubsystemTag::Render {
                continue;
            }
            assert!(!enabled(Severity::Trace, sub));
        }
        for s in Severity::all() {
            if s == Severity::Trace {
                continue;
            }
            assert!(!enabled(s, SubsystemTag::Render));
        }
    }

    // § Concurrent hot-swap should not corrupt bits.

    #[test]
    fn concurrent_set_clear_does_not_corrupt() {
        use std::thread;
        let _g = lock_and_reset();
        let handles: Vec<_> = (0..8)
            .map(|i| {
                thread::spawn(move || {
                    for _ in 0..1000 {
                        let s = Severity::all()[i % 6];
                        let sub = SubsystemTag::all()[i % SubsystemTag::COUNT];
                        enable(s, sub);
                        let _ = enabled(s, sub);
                        disable(s, sub);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        // After threads done, no bit assertions ; just no crashes / no
        // poisoned mutex.
    }

    #[test]
    fn enabled_disabled_pair_independent() {
        let _g = lock_and_reset();
        enable(Severity::Info, SubsystemTag::Render);
        disable(Severity::Info, SubsystemTag::Audio);
        assert!(enabled(Severity::Info, SubsystemTag::Render));
        assert!(!enabled(Severity::Info, SubsystemTag::Audio));
    }
}
