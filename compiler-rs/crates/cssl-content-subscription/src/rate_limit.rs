//! § rate_limit — token-bucket rate-limit per subscriber.
//!
//! § DEFAULT
//!   `RateLimitWindow::PerMinute` · 1 token per 60 s · capacity = 1.
//!   Daily-digest mode (`RateLimitWindow::Daily`) collapses to 1 / 24 h.
//!
//! § DESIGN
//!   - Pure data ; deterministic ; no clock-of-its-own.
//!   - `try_consume(now_ns)` returns Ok if a token is available, refilling
//!     based on elapsed time vs. window. Timestamps are u64 ns.
//!   - Sovereign : the player can opt to a higher rate via UI ; this is
//!     intentionally NOT exposed via push-from-server (no creator can
//!     turn up their own followers' rate-limit · only the SUBSCRIBER can).

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// § Rate-limit window enum.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum RateLimitWindow {
    /// 1 / 60 s · default for `Frequency::Realtime`.
    PerMinute = 1,
    /// 1 / 1 h · opt-in for high-volume creators.
    PerHour = 2,
    /// 1 / 24 h · default for `Frequency::Daily`.
    Daily = 3,
}

impl RateLimitWindow {
    #[must_use]
    pub const fn nanos(self) -> u64 {
        match self {
            Self::PerMinute => 60 * 1_000_000_000,
            Self::PerHour => 60 * 60 * 1_000_000_000,
            Self::Daily => 24 * 60 * 60 * 1_000_000_000,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PerMinute => "per-minute",
            Self::PerHour => "per-hour",
            Self::Daily => "daily",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RateLimitError {
    #[error("rate-limit-exceeded · retry after {retry_after_ns} ns")]
    Exceeded { retry_after_ns: u64 },
}

/// § Token-bucket · capacity = 1 · refill = 1 token per window.
/// Holds last-emit timestamp ; on `try_consume`, if elapsed ≥ window,
/// emits and updates timestamp ; else returns `Exceeded` with retry-after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitBucket {
    pub window: RateLimitWindow,
    /// `0` means "never emitted yet · next try_consume always succeeds".
    pub last_emit_ns: u64,
}

impl RateLimitBucket {
    #[must_use]
    pub fn new(window: RateLimitWindow) -> Self {
        Self {
            window,
            last_emit_ns: 0,
        }
    }

    /// Default : 1 notification per minute (matches CSSL spec default).
    #[must_use]
    pub fn default_realtime() -> Self {
        Self::new(RateLimitWindow::PerMinute)
    }

    /// Daily-digest : 1 notification per 24 h.
    #[must_use]
    pub fn daily_digest() -> Self {
        Self::new(RateLimitWindow::Daily)
    }

    /// Try to consume a token. Mutates `last_emit_ns` on success.
    pub fn try_consume(&mut self, now_ns: u64) -> Result<(), RateLimitError> {
        if self.last_emit_ns == 0 {
            self.last_emit_ns = now_ns;
            return Ok(());
        }
        let elapsed = now_ns.saturating_sub(self.last_emit_ns);
        let win = self.window.nanos();
        if elapsed >= win {
            self.last_emit_ns = now_ns;
            Ok(())
        } else {
            Err(RateLimitError::Exceeded {
                retry_after_ns: win - elapsed,
            })
        }
    }

    /// How long until the next token would be available, in ns. Zero if available now.
    #[must_use]
    pub fn time_until_next_ns(&self, now_ns: u64) -> u64 {
        if self.last_emit_ns == 0 {
            return 0;
        }
        let elapsed = now_ns.saturating_sub(self.last_emit_ns);
        let win = self.window.nanos();
        if elapsed >= win {
            0
        } else {
            win - elapsed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_consume_always_succeeds() {
        let mut b = RateLimitBucket::default_realtime();
        assert!(b.try_consume(0).is_ok());
    }

    #[test]
    fn second_consume_within_window_blocked() {
        let mut b = RateLimitBucket::default_realtime();
        b.try_consume(1_000_000_000).unwrap();
        let err = b.try_consume(1_500_000_000).unwrap_err();
        match err {
            RateLimitError::Exceeded { retry_after_ns } => {
                assert!(retry_after_ns > 0 && retry_after_ns < 60 * 1_000_000_000);
            }
        }
    }

    #[test]
    fn after_window_consume_succeeds_again() {
        let mut b = RateLimitBucket::default_realtime();
        b.try_consume(1_000_000_000).unwrap();
        // Exactly window-boundary → OK.
        assert!(b.try_consume(1_000_000_000 + 60 * 1_000_000_000).is_ok());
    }

    #[test]
    fn daily_digest_window_is_24h() {
        let b = RateLimitBucket::daily_digest();
        assert_eq!(b.window.nanos(), 24 * 60 * 60 * 1_000_000_000);
    }

    #[test]
    fn time_until_next_drops_to_zero_on_first_call() {
        let b = RateLimitBucket::default_realtime();
        assert_eq!(b.time_until_next_ns(123), 0);
    }

    #[test]
    fn rate_limit_window_names() {
        assert_eq!(RateLimitWindow::PerMinute.name(), "per-minute");
        assert_eq!(RateLimitWindow::PerHour.name(), "per-hour");
        assert_eq!(RateLimitWindow::Daily.name(), "daily");
    }
}
