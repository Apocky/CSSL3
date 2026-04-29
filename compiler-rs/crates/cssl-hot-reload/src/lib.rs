//! `CSSLv3` stage-0 — `cssl-hot-reload` : hot-swap surface for assets,
//! shaders, engine config, and KAN weights.
//!
//! § DESIGN
//!
//! This crate exposes the SURFACE for hot-reload. It does NOT contain real
//! OS filesystem-event pumps (Win32 `ReadDirectoryChangesW`, Linux inotify,
//! macOS `FSEvents`) — those land in a follow-up Wave-J slice behind a
//! `feature = "os-pump"` cargo flag. At stage-0 the only driver is the
//! manual `push_event` API, which lets tests + the LLM-iteration loop
//! inject synthetic swap events directly into the queue.
//!
//! The four swap kinds are :
//!
//! - `SwapKind::Asset`      — PNG / GLTF / WAV / TTF reload
//! - `SwapKind::Shader`     — SPIR-V / DXIL / MSL / WGSL pipeline rebuild
//! - `SwapKind::Config`     — engine.toml / render.toml / etc. live re-init
//! - `SwapKind::KanWeight`  — KAN-network weight bundle hot-swap (the
//!   LLM-iteration killer feature ; preserves persistent-kernel residency)
//!
//! The flow for ALL kinds is :
//!
//! 1. ⟨watcher fires⟩  (`push_event` in stage-0 ; OS-pump in stage-1+)
//! 2. ⟨validate schema⟩
//! 3. ⟨stage⟩ alongside old resource
//! 4. ⟨queue swap-event⟩
//! 5. ⟨engine fences current frame⟩
//! 6. ⟨apply at frame boundary⟩
//! 7. ⟨record replay-event⟩  (logical frame N — NEVER wall-clock)
//! 8. ⟨notify subscribers via handler trait⟩
//!
//! § REPLAY-DETERMINISM CONTRACT
//!   Every applied swap event is recorded into a `ReplayLog` keyed on the
//!   logical frame number. A recorded session is bit-equal-replayable IFF the
//!   replay-asset-store carries the post-fingerprint payload bytes. The
//!   replay log NEVER consults wall-clock — `std::time::Instant` is forbidden
//!   in this crate ; logical frame N is the one and only ordinal.
//!
//! § PRIME-DIRECTIVE
//!   Hot-reload is a developer-loop tool. At stage-0 it is inert until a
//!   driver explicitly calls `push_event`. There is no implicit filesystem
//!   subscription, no daemon thread, no telemetry channel. `Drop` on the
//!   `HotReload` releases all pending state silently.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::pedantic)]

pub mod event;
pub mod replay_log;
pub mod swap;

pub use event::{ConfigKind, ShaderKind, SwapEvent, SwapKind};
pub use replay_log::{ReplayLog, ReplayLogError, ReplayRecord};
pub use swap::{HotReload, HotReloadError, SwapHandler, SwapOutcome};

/// Marker string surfaced for crate-identity probes (CI smoke + cssl-trace).
pub const CRATE_TAG: &str = "cssl-hot-reload-stage0";

#[cfg(test)]
mod sanity {
    use super::CRATE_TAG;

    #[test]
    fn crate_tag_present() {
        assert!(CRATE_TAG.starts_with("cssl-hot-reload"));
    }
}
