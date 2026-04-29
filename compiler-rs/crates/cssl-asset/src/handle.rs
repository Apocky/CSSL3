//! `AssetHandle<T>` — async-loading scaffold.
//!
//! § DESIGN
//!   At stage-0 the asset pipeline is fundamentally synchronous : every
//!   load fn returns `Result<Asset, AssetError>` directly. `AssetHandle`
//!   is the SURFACE for the eventual async story — once cssl-rt grows a
//!   real async runtime, the handle's internal state machine will track
//!   in-flight work and a polling consumer can await completion.
//!
//!   At stage-0 the handle wraps an already-resolved value (or an error)
//!   and exposes a poll-style API that always returns `Ready`. This lets
//!   downstream code be written in terms of the handle today and switch
//!   to real async loading without an API break.
//!
//! § STATES
//!   - `Pending`   — work in flight (only producible by a future async
//!                   runtime ; the synchronous fns never return Pending)
//!   - `Ready(T)`  — load succeeded, asset available
//!   - `Failed(AssetError)` — load failed, error available
//!   - `Cancelled` — caller dropped a load future before completion
//!
//! § PROGRESS
//!   `LoadProgress` reports raw bytes-consumed / bytes-total for a load
//!   in flight. At stage-0 every progress reading is `(total, total)`
//!   on success or `(0, total)` on failure. Real chunked progress lands
//!   when chunked decoding does.
//!
//! § PRIME-DIRECTIVE
//!   The handle never reveals information about the asset it carries
//!   without an explicit method call by the holder. No telemetry, no
//!   listeners, no shared mutable state. The `Drop` impl is silent.

use crate::error::{AssetError, Result};

/// State of an `AssetHandle<T>` at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetState<T> {
    /// Load in flight (stage-0 synchronous loads never produce this).
    Pending,
    /// Load succeeded ; asset available.
    Ready(T),
    /// Load failed ; error available.
    Failed(AssetError),
    /// Caller dropped the load future before completion.
    Cancelled,
}

/// Progress reporter for an in-flight load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadProgress {
    /// Bytes consumed so far.
    pub bytes_done: u64,
    /// Total bytes the load expects to consume (0 if unknown).
    pub bytes_total: u64,
}

impl LoadProgress {
    /// Build a progress sample from `bytes_done / bytes_total`.
    #[must_use]
    pub const fn new(bytes_done: u64, bytes_total: u64) -> Self {
        Self {
            bytes_done,
            bytes_total,
        }
    }

    /// Build a "fresh start" progress sample (0 / total).
    #[must_use]
    pub const fn starting(bytes_total: u64) -> Self {
        Self {
            bytes_done: 0,
            bytes_total,
        }
    }

    /// Build a "complete" progress sample (total / total).
    #[must_use]
    pub const fn complete(bytes_total: u64) -> Self {
        Self {
            bytes_done: bytes_total,
            bytes_total,
        }
    }

    /// Fraction in `[0.0, 1.0]`. Returns `0.0` if `bytes_total == 0`.
    #[must_use]
    pub fn fraction(self) -> f64 {
        if self.bytes_total == 0 {
            0.0
        } else {
            (self.bytes_done as f64) / (self.bytes_total as f64)
        }
    }

    /// Is the load complete (`bytes_done >= bytes_total > 0`) ?
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.bytes_total > 0 && self.bytes_done >= self.bytes_total
    }
}

/// `AssetHandle<T>` — placeholder for the eventual async-loading future.
///
/// At stage-0 this wraps an already-resolved value or error ; the API
/// shape matches what a real async future will expose so downstream
/// code is forward-compatible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetHandle<T> {
    state: AssetState<T>,
    progress: LoadProgress,
}

impl<T> AssetHandle<T> {
    /// Build a handle in the `Pending` state with the given total size.
    #[must_use]
    pub const fn pending(bytes_total: u64) -> Self {
        Self {
            state: AssetState::Pending,
            progress: LoadProgress::starting(bytes_total),
        }
    }

    /// Build a handle in the `Ready` state from an already-loaded asset.
    #[must_use]
    pub const fn ready(value: T, bytes_total: u64) -> Self {
        Self {
            state: AssetState::Ready(value),
            progress: LoadProgress::complete(bytes_total),
        }
    }

    /// Build a handle in the `Failed` state.
    #[must_use]
    pub const fn failed(error: AssetError) -> Self {
        Self {
            state: AssetState::Failed(error),
            progress: LoadProgress::starting(0),
        }
    }

    /// Build a handle in the `Cancelled` state.
    #[must_use]
    pub const fn cancelled() -> Self {
        Self {
            state: AssetState::Cancelled,
            progress: LoadProgress::starting(0),
        }
    }

    /// Lift a `Result<T>` into a handle. The result encodes the load
    /// outcome — Ok → Ready, Err → Failed.
    pub fn from_result(result: Result<T>, bytes_total: u64) -> Self {
        match result {
            Ok(v) => Self::ready(v, bytes_total),
            Err(e) => Self::failed(e),
        }
    }

    /// Current state ; the borrowing form for inspection.
    #[must_use]
    pub const fn state(&self) -> &AssetState<T> {
        &self.state
    }

    /// Current load progress.
    #[must_use]
    pub const fn progress(&self) -> LoadProgress {
        self.progress
    }

    /// Is the handle in `Pending` ?
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self.state, AssetState::Pending)
    }

    /// Is the handle in `Ready` ?
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self.state, AssetState::Ready(_))
    }

    /// Is the handle in `Failed` ?
    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self.state, AssetState::Failed(_))
    }

    /// Is the handle in `Cancelled` ?
    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        matches!(self.state, AssetState::Cancelled)
    }

    /// Borrow the loaded value (Some only when `Ready`).
    #[must_use]
    pub const fn as_ready(&self) -> Option<&T> {
        if let AssetState::Ready(v) = &self.state {
            Some(v)
        } else {
            None
        }
    }

    /// Borrow the failure error (Some only when `Failed`).
    #[must_use]
    pub const fn as_failed(&self) -> Option<&AssetError> {
        if let AssetState::Failed(e) = &self.state {
            Some(e)
        } else {
            None
        }
    }

    /// Consume the handle and return the inner Result. `Pending` and
    /// `Cancelled` lift to a synthetic error since they cannot produce
    /// a value.
    pub fn into_result(self) -> Result<T> {
        match self.state {
            AssetState::Ready(v) => Ok(v),
            AssetState::Failed(e) => Err(e),
            AssetState::Pending => Err(AssetError::watcher(
                "AssetHandle::into_result",
                "still pending",
            )),
            AssetState::Cancelled => Err(AssetError::watcher(
                "AssetHandle::into_result",
                "load cancelled",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_starting_is_zero_done() {
        let p = LoadProgress::starting(1024);
        assert_eq!(p.bytes_done, 0);
        assert_eq!(p.bytes_total, 1024);
        assert!(!p.is_complete());
        assert!((p.fraction() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_complete_is_done_eq_total() {
        let p = LoadProgress::complete(1024);
        assert_eq!(p.bytes_done, 1024);
        assert!(p.is_complete());
        assert!((p.fraction() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_fraction_handles_zero_total() {
        let p = LoadProgress::new(10, 0);
        assert!((p.fraction() - 0.0).abs() < f64::EPSILON);
        assert!(!p.is_complete());
    }

    #[test]
    fn progress_fraction_partial() {
        let p = LoadProgress::new(256, 1024);
        let frac = p.fraction();
        assert!((frac - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn handle_pending_state() {
        let h: AssetHandle<u32> = AssetHandle::pending(4096);
        assert!(h.is_pending());
        assert!(!h.is_ready());
        assert!(h.as_ready().is_none());
        assert_eq!(h.progress().bytes_total, 4096);
    }

    #[test]
    fn handle_ready_state_and_into_result_ok() {
        let h: AssetHandle<u32> = AssetHandle::ready(42, 16);
        assert!(h.is_ready());
        assert_eq!(h.as_ready(), Some(&42));
        assert_eq!(h.progress().bytes_done, 16);
        assert_eq!(h.into_result().unwrap(), 42);
    }

    #[test]
    fn handle_failed_state_and_into_result_err() {
        let h: AssetHandle<u32> = AssetHandle::failed(AssetError::io("x", "y"));
        assert!(h.is_failed());
        assert!(h.as_failed().is_some());
        assert!(h.into_result().is_err());
    }

    #[test]
    fn handle_cancelled_state() {
        let h: AssetHandle<u32> = AssetHandle::cancelled();
        assert!(h.is_cancelled());
        let r = h.into_result();
        assert!(r.is_err());
    }

    #[test]
    fn handle_from_result_lifts_ok() {
        let h: AssetHandle<u32> = AssetHandle::from_result(Ok(7), 4);
        assert!(h.is_ready());
        assert_eq!(h.as_ready(), Some(&7));
    }

    #[test]
    fn handle_from_result_lifts_err() {
        let h: AssetHandle<u32> = AssetHandle::from_result(Err(AssetError::io("a", "b")), 0);
        assert!(h.is_failed());
    }

    #[test]
    fn handle_pending_into_result_yields_error() {
        let h: AssetHandle<u32> = AssetHandle::pending(0);
        let r = h.into_result();
        assert!(r.is_err());
    }

    #[test]
    fn progress_starting_complete_round_trip() {
        let starting = LoadProgress::starting(1000);
        let complete = LoadProgress::complete(1000);
        assert_eq!(starting.bytes_total, complete.bytes_total);
        assert!(!starting.is_complete());
        assert!(complete.is_complete());
    }
}
