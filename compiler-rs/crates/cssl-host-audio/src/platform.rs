//! Platform-specific audio backend implementations.
//!
//! Selection at compile-time via `cfg(target_os = ...)` — exactly
//! one of `wasapi` / `alsa` / `coreaudio` is the active backend ;
//! the others are stubs returning
//! [`crate::error::AudioError::LoaderMissing`].

#[cfg(target_os = "windows")]
pub mod wasapi;
#[cfg(target_os = "windows")]
pub use wasapi as active;

#[cfg(target_os = "linux")]
pub mod alsa;
#[cfg(target_os = "linux")]
pub use alsa as active;

#[cfg(target_os = "macos")]
pub mod coreaudio;
#[cfg(target_os = "macos")]
pub use coreaudio as active;

// Fallback for unsupported targets — every constructor returns
// `AudioError::LoaderMissing`.
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub mod stub;
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub use stub as active;
