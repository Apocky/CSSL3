//! Non-Apple stub backend.
//!
//! § Compiles on every non-Apple host (Windows / Linux / Android / WASM). The
//!   public API surface mirrors the apple module's `session_ops` so user code
//!   compiles unchanged across hosts ; every fallible entry-point returns
//!   [`crate::error::MetalError::HostNotApple`].
//!
//! § The cssl-host-metal crate's *user-visible* API
//!   (`MetalSession::open_stub`, `BufferHandle::stub`, etc.) is identical
//!   on Apple + non-Apple — those are the cross-cfg paths intended for
//!   tests + non-Apple production code. `MetalSession::open` (no `_stub`
//!   suffix) is what dispatches between the two backends ; on non-Apple
//!   it returns `HostNotApple`.

#![cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_os = "visionos"
)))]

#[cfg(test)]
mod tests {
    use crate::error::MetalError;
    use crate::session::{MetalSession, SessionConfig};

    #[test]
    fn open_returns_host_not_apple_on_non_apple_host() {
        let r = MetalSession::open(SessionConfig::default());
        assert!(matches!(r, Err(MetalError::HostNotApple { .. })));
    }

    #[test]
    fn open_stub_succeeds_on_non_apple_host() {
        // § The stub-open path is the cross-cfg test fallback.
        let s = MetalSession::open_stub(SessionConfig::default());
        assert!(s.is_stub());
    }
}
