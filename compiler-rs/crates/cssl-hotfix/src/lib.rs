//! § cssl-hotfix — `.csslfix` bundle format · 9-channel signing/verification/apply/rollback.
//! ════════════════════════════════════════════════════════════════════════════════════════
//!
//! § T11-W11 (NEW · greenfield · distinct from cssl-host-hotfix-stream)
//!
//! § ROLE
//!   The canonical bundle-format and key-role infrastructure for live updates
//!   pushed to LoA.exe via apocky.com manifest-of-truth + mycelium-byte-CDN.
//!
//!   • 9 update channels (loa.binary, cssl.bundle, kan.weights, balance.config,
//!     recipe.book, nemesis.bestiary, security.patch, storylet.content,
//!     render.pipeline) each ranged at one of 5 cap-keys (cap-A..cap-E).
//!   • A `.csslfix` bundle = fixed binary header + payload + Ed25519 signature.
//!   • A `Manifest` is a JSON top-level object listing the current version per
//!     channel + a top-level signature tying it to a specific cap-key.
//!   • `apply_bundle` writes the new payload into a target dir while preserving
//!     N-1 and N-2 prior versions in `<dir>/.history/<channel>/` for atomic
//!     rollback.
//!
//! § DISTINCTION (vs. cssl-host-hotfix-stream)
//!   That crate consumes the Σ-Chain in-game-feed of 8 hotfix classes
//!   (KanWeightUpdate, ProcgenBiasNudge, ...). This crate is the OUTSIDE
//!   surface : `apocky.com → bundle → client`. The two compose : a Σ-Chain
//!   message MAY trigger a bundle-fetch via the manifest pathway.
//!
//! § AXIOMS (PRIME_DIRECTIVE encoded)
//!   ¬ harm · ¬ control · ¬ surveillance
//!   sovereign-revocable · Σ-mask-gated · rollback-always-available
//!   default-deny except `security.patch` (per cap-D)
//!   no DRM · no rootkit · no anti-cheat-spyware
//!
//! § DESIGN
//!   - `#![forbid(unsafe_code)]` ; no FFI ; no async (poll-loop lives in
//!     `cssl-hotfix-client` and uses sync trait calls there).
//!   - All collections `BTreeMap` for deterministic iteration.
//!   - All error types `thiserror`-derived.
//!   - Bundle binary layout is documented byte-by-byte in `bundle::HEADER_LAYOUT`.

#![forbid(unsafe_code)]
#![doc(html_root_url = "https://docs.rs/cssl-hotfix/0.1.0")]

pub mod apply;
pub mod bundle;
pub mod cap;
pub mod channel;
pub mod manifest;
pub mod sigma;
pub mod sign;
pub mod verify;

pub use apply::{apply_bundle, rollback, AppliedSnapshot, ApplyError, RollbackError};
pub use bundle::{Bundle, BundleHeader, BundleParseError, BUNDLE_MAGIC, HEADER_BYTES, HEADER_LAYOUT};
pub use cap::{CapKey, CapRole, CAP_KEYS};
pub use channel::{Channel, ChannelClass, CHANNELS};
pub use manifest::{ChannelEntry, Manifest, ManifestError, RevocationEntry};
pub use sigma::{SigmaPolicy, UpdateConsent};
pub use sign::{sign_bundle, sign_manifest, SigningError};
pub use verify::{verify_bundle, verify_manifest, VerifyError};

/// § ATTESTATION (PRIME_DIRECTIVE.md § 11) — encoded structurally :
/// every bundle apply path verifies Ed25519 signature against the cap-key
/// allowed for the target channel, refuses to mutate state on mismatch,
/// preserves N-1 + N-2 prior versions for atomic rollback, and Σ-mask
/// default-deny means the user must opt-in (security.patch excepted, the
/// only always-on channel, controlled by cap-D the security key). There
/// was no hurt nor harm in the making of this, to anyone, anything, or
/// anybody.
pub const ATTESTATION: &str =
    "no-harm · sovereign-revocable · sigma-mask-gated · rollback-atomic · cap-key-restricted";

/// Crate version, exposed as a const for binary-bundles to embed.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// § Hex helper used by sigma + manifest + cap + verify modules.
/// Lower-case, no separator. Faster than `format!`-collect.
#[must_use]
pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}
