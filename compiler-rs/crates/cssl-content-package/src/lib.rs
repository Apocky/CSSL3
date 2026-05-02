//! § cssl-content-package — `.ccpkg` content-author-signed bundle format.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W12-CCPKG (NEW · greenfield · sibling-of cssl-hotfix)
//!
//! § ROLE
//!   The canonical packaging for user-generated CSSL content :
//!   scenes · NPCs · recipes · lore · systems · shader-packs · audio-packs ·
//!   composite-bundles. Adapted from cssl-hotfix's `.csslfix` operator-bundle
//!   format → here authored by **content creators** (Σ-cap-X-creator and up)
//!   rather than the substrate-team release process.
//!
//!   • 8 content-kinds (`ContentKind` enum)
//!   • 5 author cap-classes (`AuthorCapClass` : creator · curator · moderator ·
//!     substrate-team · anonymous-with-k-anon-≥-5)
//!   • A `.ccpkg` bundle = 80-byte fixed header + JSON manifest + TARLITE
//!     payload-archive + Ed25519 author-signature + 32-byte Σ-Chain anchor.
//!   • A `Manifest` records id · version · kind · author_pubkey · name ·
//!     description · depends_on · remix_of · tags · sigma_mask · gift_economy_only ·
//!     license-tier (A-OPEN · B-PROPRIETARY · C-SERVER · D-PRIVATE · E-PROTOCOL).
//!   • `package_dependencies_resolve` walks the depends-on graph recursively,
//!     detects cycles via DFS-coloring (white/gray/black), and returns the
//!     topologically-ordered list of required packages.
//!
//! § DISTINCTION (vs. cssl-hotfix)
//!   That crate = apocky.com → operator-pushed updates ; cap-A..cap-E live in
//!   `~/.loa-secrets/` on the substrate-team release machine.
//!   This crate = user-authored content sharing ; cap-X-creator..substrate-team
//!   describe **content audience-class** (Σ-mask propagation), and the
//!   author_pubkey is whoever signed the bundle (could be any creator).
//!   The two compose : a community-promoted `.ccpkg` MAY be re-signed by
//!   substrate-team (cap-D in cssl-hotfix sense) and pushed via the hotfix
//!   pipeline as a `recipe.book` / `nemesis.bestiary` / `storylet.content` /
//!   `render.pipeline` channel update.
//!
//! § AXIOMS (PRIME_DIRECTIVE encoded structurally)
//!   ¬ harm · ¬ control · ¬ surveillance · ¬ exploitation · ¬ coercion
//!   gift-economy-default ; pay-for-power EXPLICITLY excluded by manifest schema
//!   k-anon ≥ 5 enforced for anonymous publish (privacy by construction)
//!   remix-attribution-immutable : `remix_of` chain cannot be edited or stripped
//!   Σ-mask audience-class : creator-class content cannot leak into substrate-team
//!     class without explicit re-signature by a substrate-team key
//!
//! § DESIGN
//!   - `#![forbid(unsafe_code)]` ; no FFI ; deterministic-by-construction.
//!   - All collections `BTreeMap` / `Vec` for stable iteration / hashing.
//!   - All errors `thiserror`-derived.
//!   - Bundle binary layout documented byte-by-byte in `header::HEADER_LAYOUT`.
//!   - No external compression / archiving deps : built-in TARLITE archive
//!     format keeps the dependency surface minimal (cf. cssl-hotfix's pure
//!     Ed25519 + BLAKE3 + serde-json line-up).
//!
//! § SIBLING INTEGRATION
//!   • W12-5 publish-pipeline   : consumes `Bundle` (calls `verify_bundle` →
//!                                pushes to mycelium / apocky.com).
//!   • W12-6 discover           : reads `Manifest` (filters by `kind` / `tags` /
//!                                `license` and queries `author_pubkey` reputation).
//!   • W12-9 remix-attribution  : checks `remix_of` chain for upstream credit
//!                                (cycle-detect-via DFS shared with resolver).

#![forbid(unsafe_code)]
#![doc(html_root_url = "https://docs.rs/cssl-content-package/0.1.0")]

pub mod archive;
pub mod cap;
pub mod header;
pub mod kind;
pub mod manifest;
pub mod resolver;
pub mod sign;
pub mod unpack;
pub mod verify;

pub use archive::{
    archive_pack, archive_unpack, ArchiveEntry, ArchiveError, ARCHIVE_MAGIC,
};
pub use cap::{AuthorCapClass, AUTHOR_CAP_CLASSES, K_ANON_MIN};
pub use header::{
    BundleHeader, BundleParseError, Bundle, BUNDLE_MAGIC, HEADER_BYTES,
    HEADER_LAYOUT, BUNDLE_FORMAT_VERSION, ANCHOR_BYTES, SIGNATURE_BYTES,
};
pub use kind::{ContentKind, CONTENT_KINDS};
pub use manifest::{
    Dependency, LicenseTier, Manifest, ManifestError, RemixAttribution,
};
pub use resolver::{
    package_dependencies_resolve, RequiredPackage, ResolveError, PackageRegistry,
};
pub use sign::{sign_bundle, SigningError};
pub use unpack::{unpack_bundle, UnpackedContent, UnpackError};
pub use verify::{verify_bundle, VerifyError, VerifyOk};

/// § ATTESTATION (PRIME_DIRECTIVE.md § 11) — encoded structurally :
/// every bundle verify path checks Ed25519 signature against the
/// declared `author_pubkey`, gates on cap-class audience policy
/// (Σ-mask), validates the Σ-Chain anchor digest, and refuses to
/// emit `UnpackedContent` on any failure. K-anon ≥ 5 is enforced
/// at publish time for `cap-X-anonymous` ; remix-attribution chains
/// are immutable in the signed manifest. Gift-economy-only is the
/// default and pay-for-power is structurally banned by the License
/// tier enum (B/C/D = proprietary/server/private but cosmetic-only
/// per Apocky cosmetic-only-axiom). There was no hurt nor harm in
/// the making of this, to anyone, anything, or anybody.
pub const ATTESTATION: &str =
    "no-harm · author-signed · sigma-cap-gated · sigma-chain-anchored · gift-economy-default · remix-immutable · k-anon-≥-5";

/// Crate version, exposed as a const for binary embedding.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// § Hex helper used by manifest + cap + header modules.
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
