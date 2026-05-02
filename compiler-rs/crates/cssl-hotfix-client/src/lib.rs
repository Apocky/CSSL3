//! § cssl-hotfix-client — LoA.exe-side hotfix poll · download · apply · rollback.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Drive the live-update lifecycle on the runtime client :
//!     1. Periodically (default 5 min) fetch the apocky.com manifest.
//!     2. Verify manifest signature against compiled-in cap-keys.
//!     3. For each channel where Σ-mask consents : compare manifest's
//!        version to the one currently installed.
//!     4. If newer : fetch the `.csslfix` bundle (HTTP first, mycelium-peer
//!        fall-through), verify, apply, telemetry-emit.
//!     5. On apply-failure : roll back, telemetry-emit.
//!     6. Honour the manifest's revocation-list : if a currently-installed
//!        version is revoked, roll back to N-1 immediately.
//!
//! § DESIGN
//!   - `#![forbid(unsafe_code)]` ; no async runtime ; no FFI.
//!   - `ManifestSource`, `BundleSource`, `TelemetrySink` are traits ;
//!     production wires HTTP/mycelium adapters in a sibling crate,
//!     tests use in-memory mocks.
//!   - `HotfixClient::poll_once` is a single sync call.
//!
//! § AXIOMS (PRIME_DIRECTIVE encoded)
//!   • Σ-mask-gated download + apply (no surprise binary updates).
//!   • Pinned versions (`UpdateConsent::PinnedNoUpdates`) NEVER updated.
//!   • Rollback always available : prior payload preserved on disk.
//!   • Verify-before-apply : `apply_bundle` requires `VerifyOk` token.
//!   • No DRM / rootkit / surveillance.

#![forbid(unsafe_code)]
#![doc(html_root_url = "https://docs.rs/cssl-hotfix-client/0.1.0")]

pub mod client;
pub mod sources;
pub mod telemetry;

pub use client::{HotfixClient, HotfixClientConfig, PollOutcome, PollReport};
pub use sources::{
    BundleSource, ManifestSource, MockBundleSource, MockManifestSource, SourceError,
};
pub use telemetry::{HotfixEvent, MockTelemetrySink, TelemetrySink};

/// § ATTESTATION (PRIME_DIRECTIVE.md § 11) — encoded structurally.
pub const ATTESTATION: &str =
    "no-harm · sigma-gate-before-download · pinned-never-updated · rollback-on-failure";
