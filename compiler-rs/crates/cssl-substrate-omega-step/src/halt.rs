//! Kill-switch — `ω_halt()` semantics.
//!
//! § THESIS
//!   `specs/30_SUBSTRATE.csl § ω_halt()` requires that the scheduler
//!   honor a halt request **within at most 1 tick**. The semantics :
//!     - `ω_halt(token, reason)` consumes the token (linear ⇒ at-most-once).
//!     - The next `omega_step()` observes the kill-flag and degrades :
//!         sim ← skip ; render ← black-frame ; audio ← silence ;
//!         telemetry ← drain ; audit ← final-entry ; save ← checkpoint.
//!     - Step-after-halt returns Omega in HaltedState (no further steps).
//!
//!   Stage-0 form : the linear `iso<KillToken>` is represented as a
//!   single-shot `HaltToken` whose state is held inside `HaltState`.
//!   Calling `consume()` flips the state to `HaltState::Triggered` and
//!   stores the reason ; subsequent `consume()` calls are no-ops with
//!   no error (the spec describes a one-shot effect, not a repeatable one).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One-shot kill-token. Backed by an atomic flag + interior reason string.
///
/// § THREAD-SAFETY
///   `HaltToken` is `Clone` ; cloning shares the atomic flag. Any clone
///   may call `consume()` ; the flag is set exactly once.
///
/// § REASON STRING
///   The reason is stored on the first `consume()` call. Subsequent
///   consume calls do not overwrite the reason — this matches the
///   "linear, one-shot" semantics of `iso<KillToken>` in the spec.
#[derive(Debug, Clone)]
pub struct HaltToken {
    inner: Arc<HaltInner>,
}

#[derive(Debug)]
struct HaltInner {
    triggered: AtomicBool,
    /// Held under a mutex because writing the reason happens at most once
    /// per `HaltToken` lifetime — contention is non-existent in practice.
    reason: std::sync::Mutex<Option<String>>,
}

impl HaltToken {
    /// Construct a fresh, untriggered token.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(HaltInner {
                triggered: AtomicBool::new(false),
                reason: std::sync::Mutex::new(None),
            }),
        }
    }

    /// Trigger the halt with a reason. Idempotent — only the first call
    /// records the reason. Returns `true` if this call triggered the halt
    /// (was the first), `false` if it was already triggered.
    pub fn consume(&self, reason: impl Into<String>) -> bool {
        // Claim the trigger via compare-exchange so only the first caller
        // gets to write the reason.
        let was_unset = self
            .inner
            .triggered
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();
        if was_unset {
            // SAFETY: lock-poison would mean a previous panic in the
            // mutex-guarded scope ; recovering from poison is acceptable
            // here because the only mutation is a single Option-set.
            let mut guard = self
                .inner
                .reason
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *guard = Some(reason.into());
        }
        was_unset
    }

    /// Whether the token has been triggered.
    #[must_use]
    pub fn is_triggered(&self) -> bool {
        self.inner.triggered.load(Ordering::SeqCst)
    }

    /// The recorded reason, if any.
    #[must_use]
    pub fn reason(&self) -> Option<String> {
        self.inner
            .reason
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Default for HaltToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Coarse halt classification reported by the scheduler in its public state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltState {
    /// Scheduler is running normally.
    Running,
    /// `halt()` was called this tick or earlier ; the scheduler will reject
    /// further `step()` calls with `OmegaError::HaltedByKill`.
    Halted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_token_not_triggered() {
        let t = HaltToken::new();
        assert!(!t.is_triggered());
        assert_eq!(t.reason(), None);
    }

    #[test]
    fn consume_triggers_and_records_reason() {
        let t = HaltToken::new();
        let was_first = t.consume("kill-switch");
        assert!(was_first);
        assert!(t.is_triggered());
        assert_eq!(t.reason(), Some("kill-switch".into()));
    }

    #[test]
    fn consume_is_one_shot() {
        let t = HaltToken::new();
        assert!(t.consume("first"));
        // Second call returns false ; first reason preserved.
        assert!(!t.consume("second"));
        assert_eq!(t.reason(), Some("first".into()));
    }

    #[test]
    #[allow(
        clippy::redundant_clone,
        reason = "clone is the SUBJECT of the test : we verify Arc-backed \
        propagation of the trigger flag. The lint can't see through Arc."
    )]
    fn clones_share_state() {
        let t = HaltToken::new();
        let t2 = t.clone();
        assert!(!t.is_triggered());
        assert!(!t2.is_triggered());
        assert!(t.consume("via t"));
        assert!(t2.is_triggered());
        assert_eq!(t2.reason(), Some("via t".into()));
        // t2 cannot trigger again ; t already did.
        assert!(!t2.consume("via t2"));
    }

    #[test]
    fn halt_state_variants() {
        assert_ne!(HaltState::Running, HaltState::Halted);
    }
}
