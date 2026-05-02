//! § cssl-content-remix — fork-chain attribution + Σ-Chain-anchor + gift-royalty.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W12-REMIX-CRATE (NEW · greenfield)
//!
//! § ROLE
//!   Sibling of cssl-content-package (W12-4) + cssl-content-rating (W12-7) ·
//!   composes with cssl-host-sigma-chain (W11-17) for attestation-anchor ·
//!   composes with cssl-host-stripe-integration for gift-tip channel.
//!
//!   • 6 RemixKind variants : Fork · Extension · Translation · Adaptation ·
//!     Improvement · Bundle.
//!   • RemixLink = Ed25519-signed · Σ-Chain-anchored · IMMUTABLE post-anchor.
//!   • RoyaltyShareGift = creator-pledged tip-pct · GIFT-ONLY · ¬ enforced ·
//!     sovereign-revocable always-true.
//!   • RemixChain = BTreeMap<ContentId,RemixLink> walked-to-genesis ·
//!     cycle-detect via visited-set · complexity O(depth).
//!   • OptOutRegistry = creator-pubkey-set · NEW remixes blocked when
//!     creator opts-out · EXISTING remixes preserved (sovereignty-irrevocable).
//!
//! § AXIOMS (PRIME_DIRECTIVE.md encoded structurally)
//!   ¬ harm · ¬ control · ¬ surveillance · ¬ exploitation · ¬ coercion
//!   ¬ remix without-Σ-cap (creator-cap REQUIRED at fork-init)
//!   attribution-immutable post-anchor (signature-mutation rejected)
//!   royalty-share-gift-only (¬ enforced-extraction · ¬ pay-for-power)
//!   sovereign-opt-out (creator-can-block-future-remixes ; preserves-past)
//!   100% to-tipped-creator (minus Stripe-fee · ¬ platform-tax)
//!
//! § ATTESTATION (PRIME_DIRECTIVE.md § 11)
//!   Every RemixLink verify path checks Ed25519 signature against the
//!   declared remix_creator_pubkey, validates the Σ-Chain anchor digest,
//!   refuses to emit Verified on any failure. Opt-out registry is checked
//!   at fork-init time (NEW-blocking) but never retroactively revokes
//!   existing chain-links. Royalty-share-gift is structurally banned from
//!   being binding : sovereign_revocable defaults true and cannot be
//!   set false through any public API. Tips flow only via Stripe-gift
//!   channel ; platform never appears in money-path.
//!   There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody.

#![forbid(unsafe_code)]
#![doc(html_root_url = "https://docs.rs/cssl-content-remix/0.1.0")]

pub mod attribution;
pub mod chain;
pub mod kind;
pub mod link;
pub mod opt_out;
pub mod royalty;
pub mod sign;
pub mod tip;
pub mod verify;

pub use attribution::{get_attribution_chain, AttributionError, AttributionWalk};
pub use chain::{RemixChain, RemixChainError};
pub use kind::{RemixKind, REMIX_KINDS};
pub use link::{ContentId, RemixLink, RemixLinkError, SemVer};
pub use opt_out::{OptOutDecision, OptOutRegistry, OPT_OUT_AXIOM};
pub use royalty::{RoyaltyShareGift, RoyaltyShareGiftError, ROYALTY_SHARE_AXIOM};
pub use sign::{
    canonical_link_bytes, sign_remix_link, SigningError, PUBKEY_LEN, SIG_LEN,
};
pub use tip::{TipFlow, TipFlowError, TipReceipt, STRIPE_TIP_AXIOM};
pub use verify::{verify_remix_link, VerifiedLink, VerifyError};

/// § ATTESTATION constant — embedded in binaries for transparency-audit.
pub const ATTESTATION: &str =
    "no-harm · attribution-immutable · sigma-chain-anchored · gift-royalty-only · sovereign-opt-out · 100%-to-tipped-creator-minus-stripe-fee";

/// Crate version, exposed as a const for binary embedding.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// § Hex helper — lower-case · no separator. Faster than `format!`-collect.
#[must_use]
pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}
