// revert.rs — 30-second revert-window machinery
// ══════════════════════════════════════════════════════════════════
// § structural-invariant : window_ms is set ONCE at arm() time
// § try_revert succeeds iff now_ms - applied_at <= window_ms (saturating)
// § auto-revert hook (e.g. crash-detector) and manual-revert share this gate
// ══════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// Active revert-window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevertWindow {
    /// Wall-clock millis when the edit was Applied.
    pub applied_at_ms: u64,
    /// Window duration in millis (typically 30_000).
    pub window_ms: u64,
    /// Auto-revert enabled flag (per-edit).
    pub auto_revert_enabled: bool,
}

impl RevertWindow {
    /// Arm a fresh window.
    pub const fn arm(applied_at_ms: u64, window_ms: u64) -> Self {
        Self {
            applied_at_ms,
            window_ms,
            auto_revert_enabled: true,
        }
    }

    /// Compute the revert deadline (millis-since-epoch).
    pub const fn deadline_ms(&self) -> u64 {
        self.applied_at_ms.saturating_add(self.window_ms)
    }

    /// `true` iff the window is still open at `now_ms`.
    pub const fn is_open_at(&self, now_ms: u64) -> bool {
        now_ms <= self.deadline_ms()
    }

    /// Convenience : "is the window open at all" — uses [`u64::MAX`] sentinel for "any time".
    /// Practical callers should prefer [`Self::is_open_at`] with a real clock.
    pub const fn is_open(&self) -> bool {
        // Without a clock this is tautologically true ; left for ergonomic checks
        // in test-helpers where a window's mere presence implies "armed".
        true
    }

    /// Try to revert at `now_ms`. Returns the outcome.
    pub const fn try_revert(&self, now_ms: u64) -> RevertOutcome {
        if self.is_open_at(now_ms) {
            RevertOutcome::Reverted
        } else {
            RevertOutcome::WindowExpired
        }
    }
}

/// Revert-attempt outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertOutcome {
    /// Revert succeeded ; sandbox transitioned to AutoReverted/ManualReverted.
    Reverted,
    /// Window already expired ; revert blocked (edit is now Permanent).
    WindowExpired,
    /// No window registered for this edit-id.
    NoWindow,
}
