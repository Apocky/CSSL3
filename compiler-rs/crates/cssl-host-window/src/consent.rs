//! PRIME-DIRECTIVE consent-arch enforcement for window close-requests.
//!
//! § PRIME-DIRECTIVE BINDING
//!   Per `PRIME_DIRECTIVE.md § 1 PROHIBITIONS § entrapment` the window MUST
//!   never trap the user. The close-button on a window is the most common
//!   "kill switch" affordance the user has — overriding it without explicit
//!   consent is a directive violation, not a feature.
//!
//! § STATE-MACHINE
//!   ```text
//!   Idle                                    ← no close pending
//!     │  WM_CLOSE / Alt-F4 / system-menu
//!     ▼
//!   Pending(t0)                             ← user requested close
//!     │
//!     ├── user-code calls
//!     │      Window::dismiss_close_request  → Idle (acknowledged)
//!     │      Window::request_destroy        → Destroyed (granted)
//!     │
//!     └── if (t - t0) ≥ grace_window_ms :
//!         GraceExceeded → forced shutdown
//!   ```
//!
//! § ANTI-TRAP GRACE WINDOW
//!   `GraceWindowConfig::ms` defines how long user-code may delay before the
//!   pump auto-grants the close. Default = 5_000 ms (5 seconds). Set to 0
//!   to opt out of auto-grant — but the policy enum still requires explicit
//!   acknowledgement on every Close, otherwise `WindowError::ConsentViolation`
//!   fires.
//!
//! § DEFAULT POLICY
//!   `CloseDispositionPolicy::AutoGrantAfterGrace { 5_000 }` — fits the
//!   PRIME-DIRECTIVE intent for "kill-switch always works" while still
//!   allowing well-behaved apps to confirm-quit dialogs.

use core::fmt;

/// Caller-visible state of an in-flight close request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseRequestState {
    /// No close has been requested.
    Idle,

    /// Close was requested at the given monotonic timestamp (ms since
    /// window construction). User-code observes this and must either
    /// acknowledge or dismiss it.
    Pending {
        /// Monotonic ms-since-window-open at which Close was requested.
        requested_at_ms: u64,
    },

    /// User-code dismissed the close (either via `dismiss_close_request` or
    /// via no-op past the grace-window when policy = `AutoGrantAfterGrace`).
    Dismissed,

    /// Close was granted ; window is being torn down.
    Granted,
}

impl Default for CloseRequestState {
    fn default() -> Self {
        Self::Idle
    }
}

impl fmt::Display for CloseRequestState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Pending { requested_at_ms } => write!(f, "pending @ {requested_at_ms}ms"),
            Self::Dismissed => write!(f, "dismissed"),
            Self::Granted => write!(f, "granted"),
        }
    }
}

/// Disposition policy : what the pump does with an unacknowledged close.
///
/// PRIME-DIRECTIVE-mandated : the only legal way for a window to silently
/// suppress a Close is the explicit `RequireExplicit` policy paired with
/// user-code that ALWAYS observes the Close event. Any "default-suppress"
/// path is a directive violation per § entrapment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDispositionPolicy {
    /// Auto-grant the close after the grace-window elapses.
    ///
    /// This is the DEFAULT and only universally-PRIME-DIRECTIVE-safe policy.
    /// User-code may confirm-quit during the grace window ; if no decision
    /// is made the pump grants the close.
    AutoGrantAfterGrace { grace: GraceWindowConfig },

    /// Require user-code to explicitly call dismiss-or-grant.
    ///
    /// Permitted ONLY when user-code always observes Close events on every
    /// pump_events drain. The pump enforces this by requiring acknowledgement
    /// within `consent_arch_audit_window_ms` ms ; missing acknowledgement
    /// fires [`crate::WindowError::ConsentViolation`].
    RequireExplicit { consent_arch_audit_window_ms: u64 },
}

impl Default for CloseDispositionPolicy {
    fn default() -> Self {
        Self::AutoGrantAfterGrace {
            grace: GraceWindowConfig::default(),
        }
    }
}

/// Anti-trap grace-window length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraceWindowConfig {
    /// Milliseconds the user-code has to dismiss-or-grant before the pump
    /// auto-grants the close. Default = 5_000 ms (5 seconds) ; 0 disables
    /// auto-grant (then policy MUST be `RequireExplicit`).
    pub ms: u64,
}

impl Default for GraceWindowConfig {
    fn default() -> Self {
        Self { ms: 5_000 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_state_default_is_idle() {
        assert_eq!(CloseRequestState::default(), CloseRequestState::Idle);
    }

    #[test]
    fn close_state_display_round_trips() {
        let p = CloseRequestState::Pending {
            requested_at_ms: 123,
        };
        let s = format!("{p}");
        assert!(s.contains("pending"), "display = {s}");
        assert!(s.contains("123"), "display = {s}");

        assert_eq!(format!("{}", CloseRequestState::Idle), "idle");
        assert_eq!(format!("{}", CloseRequestState::Granted), "granted");
        assert_eq!(format!("{}", CloseRequestState::Dismissed), "dismissed");
    }

    #[test]
    fn default_policy_is_auto_grant_5s() {
        let p = CloseDispositionPolicy::default();
        match p {
            CloseDispositionPolicy::AutoGrantAfterGrace { grace } => {
                assert_eq!(grace.ms, 5_000);
            }
            CloseDispositionPolicy::RequireExplicit { .. } => {
                panic!("default policy is not AutoGrantAfterGrace");
            }
        }
    }

    #[test]
    fn grace_window_default_is_5s() {
        let g = GraceWindowConfig::default();
        assert_eq!(g.ms, 5_000);
    }

    #[test]
    fn require_explicit_policy_is_constructible() {
        let p = CloseDispositionPolicy::RequireExplicit {
            consent_arch_audit_window_ms: 30_000,
        };
        match p {
            CloseDispositionPolicy::RequireExplicit {
                consent_arch_audit_window_ms,
            } => assert_eq!(consent_arch_audit_window_ms, 30_000),
            CloseDispositionPolicy::AutoGrantAfterGrace { .. } => panic!("policy mismatch"),
        }
    }
}
