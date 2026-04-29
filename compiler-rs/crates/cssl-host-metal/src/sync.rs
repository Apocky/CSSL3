//! `MTLEvent` / `MTLSharedEvent` + `MTLFence` for cross-queue synchronisation.
//!
//! § SPEC : `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § Metal` (sync row).
//!
//! § DESIGN
//!   - [`EventHandle`] wraps `MTLSharedEvent` (the cross-queue sync primitive
//!     introduced in macOS 10.14 / iOS 12). Apple-side : real `MTLSharedEvent`.
//!     Stub-side : a counter-based monotonic-token model that lets tests
//!     observe the encode-then-wait shape without FFI.
//!   - [`FenceHandle`] wraps `MTLFence` for intra-queue producer-consumer
//!     synchronisation between command-encoders. `MTLFence` is the lighter
//!     primitive ; `MTLEvent` is the heavier-weight shared primitive.
//!   - [`SignalToken`] is a strongly-typed monotonic value used to signal +
//!     wait on an `EventHandle`.
//!   - [`FenceUsage`] models intent (BeforeRead / AfterWrite) so the apple
//!     side can call `[encoder waitForFence:]` vs `[encoder updateFence:]`.

use crate::error::MetalResult;

/// Monotonic signal-token used by [`EventHandle::signal`] / [`EventHandle::wait_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SignalToken(pub u64);

impl SignalToken {
    /// Initial signal-token value (zero).
    pub const ZERO: Self = Self(0);

    /// Successor of this token.
    #[must_use]
    pub const fn successor(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// `MTLSharedEvent` handle.
#[derive(Debug)]
pub struct EventHandle {
    /// Event label (debug-info).
    pub label: String,
    /// Last-signaled value (stub-side counter).
    pub last_signaled: SignalToken,
    /// Inner state.
    ///
    /// § The stub-side variant carries no payload ; on Apple hosts the field
    ///   resolves to a pool index. The `dead_code` allow is needed because
    ///   stub-only builds never read `Apple { pool_idx }`.
    #[allow(dead_code)]
    pub(crate) inner: EventInner,
}

#[derive(Debug)]
pub(crate) enum EventInner {
    /// Stub state — counter only.
    Stub,
    /// Apple-side state.
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    ))]
    Apple {
        /// Index into the Apple-session event-pool.
        pool_idx: u32,
    },
}

impl EventHandle {
    /// Stub constructor.
    #[must_use]
    pub fn stub(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            last_signaled: SignalToken::ZERO,
            inner: EventInner::Stub,
        }
    }

    /// Signal this event with the given token. Token must be strictly greater
    /// than the last-signaled value (Metal's monotonic invariant).
    pub fn signal(&mut self, token: SignalToken) -> MetalResult<()> {
        if token <= self.last_signaled {
            return Err(crate::error::MetalError::CommitFailed {
                detail: format!(
                    "signal token {} not greater than last {}",
                    token.0, self.last_signaled.0
                ),
            });
        }
        self.last_signaled = token;
        Ok(())
    }

    /// Wait for `token` to be signaled. Stub-side this checks the counter ;
    /// Apple-side delegates to `[event waitUntilSignaledValue:timeoutMS:]`.
    pub fn wait_for(&self, token: SignalToken) -> MetalResult<()> {
        if self.last_signaled >= token {
            Ok(())
        } else {
            Err(crate::error::MetalError::WaitTimeout { timeout_ms: 0 })
        }
    }
}

/// `MTLFence` usage — what the encoder wants to do with this fence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FenceUsage {
    /// `[encoder updateFence:]` — encoder finishes writing, fence becomes ready.
    AfterWrite,
    /// `[encoder waitForFence:]` — encoder waits until fence is ready before reading.
    BeforeRead,
}

impl FenceUsage {
    /// Short canonical name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AfterWrite => "after_write",
            Self::BeforeRead => "before_read",
        }
    }
}

/// `MTLFence` handle.
#[derive(Debug)]
pub struct FenceHandle {
    /// Fence label (debug-info).
    pub label: String,
    /// Whether this fence has been updated by an encoder.
    pub updated: bool,
    /// Inner state.
    ///
    /// § The stub-side variant carries no payload ; on Apple hosts the field
    ///   resolves to a pool index. `dead_code` allow needed for stub-only builds.
    #[allow(dead_code)]
    pub(crate) inner: FenceInner,
}

#[derive(Debug)]
pub(crate) enum FenceInner {
    /// Stub state.
    Stub,
    /// Apple-side state.
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "visionos"
    ))]
    Apple {
        /// Index into the Apple-session fence-pool.
        pool_idx: u32,
    },
}

impl FenceHandle {
    /// Stub constructor.
    #[must_use]
    pub fn stub(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            updated: false,
            inner: FenceInner::Stub,
        }
    }

    /// Mark this fence as updated (corresponds to `[encoder updateFence:]`).
    pub fn mark_updated(&mut self) {
        self.updated = true;
    }

    /// Reset the updated flag (clear-after-wait pattern).
    pub fn reset(&mut self) {
        self.updated = false;
    }
}

#[cfg(test)]
mod tests {
    use super::{EventHandle, FenceHandle, FenceUsage, MetalResult, SignalToken};

    #[test]
    fn signal_token_zero_and_successor() {
        let z = SignalToken::ZERO;
        assert_eq!(z.0, 0);
        let n = z.successor();
        assert_eq!(n.0, 1);
        assert!(n > z);
    }

    #[test]
    fn signal_token_successor_saturates() {
        let max = SignalToken(u64::MAX);
        assert_eq!(max.successor().0, u64::MAX);
    }

    #[test]
    fn fresh_event_starts_at_zero() {
        let e = EventHandle::stub("test");
        assert_eq!(e.last_signaled, SignalToken::ZERO);
    }

    #[test]
    fn signal_increments_last_signaled() {
        let mut e = EventHandle::stub("test");
        e.signal(SignalToken(1)).unwrap();
        assert_eq!(e.last_signaled, SignalToken(1));
        e.signal(SignalToken(5)).unwrap();
        assert_eq!(e.last_signaled, SignalToken(5));
    }

    #[test]
    fn signal_with_non_increasing_token_errors() {
        let mut e = EventHandle::stub("test");
        e.signal(SignalToken(3)).unwrap();
        let r: MetalResult<()> = e.signal(SignalToken(3));
        assert!(r.is_err());
        let r2: MetalResult<()> = e.signal(SignalToken(2));
        assert!(r2.is_err());
    }

    #[test]
    fn wait_for_already_signaled_succeeds() {
        let mut e = EventHandle::stub("test");
        e.signal(SignalToken(5)).unwrap();
        e.wait_for(SignalToken(3)).unwrap();
        e.wait_for(SignalToken(5)).unwrap();
    }

    #[test]
    fn wait_for_unsignaled_token_times_out() {
        let e = EventHandle::stub("test");
        let r = e.wait_for(SignalToken(1));
        assert!(r.is_err());
    }

    #[test]
    fn fence_usage_names() {
        assert_eq!(FenceUsage::AfterWrite.as_str(), "after_write");
        assert_eq!(FenceUsage::BeforeRead.as_str(), "before_read");
    }

    #[test]
    fn fresh_fence_is_not_updated() {
        let f = FenceHandle::stub("f");
        assert!(!f.updated);
    }

    #[test]
    fn fence_mark_updated_then_reset() {
        let mut f = FenceHandle::stub("f");
        f.mark_updated();
        assert!(f.updated);
        f.reset();
        assert!(!f.updated);
    }
}
