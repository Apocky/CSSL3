//! § cssl-content-rating — sovereign-cap-gated content rating + review
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W12-7-RATING (NEW · greenfield · sibling W12-3 cssl-self-authoring-kan)
//!
//! § ROLE
//!   Cap-gated rating + review system for user-generated CSSL content
//!   (`.ccpkg` bundles from W12-4). 24-byte bit-packed Rating record + a
//!   variable-size Review (≤ 512 bytes) carrying body + tags + signature.
//!   Aggregates k-anonymized ; trending-pages use ONLY-aggregate-counts
//!   (k ≥ 10) so a single malicious rater cannot influence rank.
//!
//! § ARCHITECTURE
//!   ┌──────────────────┐                   ┌──────────────────────┐
//!   │  rate(content)   │ ───emit──────►    │  RatingStore         │
//!   │  by player       │                   │  (in-mem · BTreeMap) │
//!   └──────────────────┘                   └─────────┬────────────┘
//!                                                    │ aggregate
//!                                                    ▼
//!                                          ┌──────────────────────┐
//!                                          │  AggregateView       │
//!                                          │  (k ≥ 5 single ·     │
//!                                          │   k ≥ 10 trending)   │
//!                                          └─────────┬────────────┘
//!                                                    │ derive
//!                                                    ▼
//!                                          ┌──────────────────────┐
//!                                          │  QualitySignal       │
//!                                          │  → KAN-bias-loop     │
//!                                          │   (sibling W12-3)    │
//!                                          └──────────────────────┘
//!
//! § BIT-PACK LAYOUT (24 bytes ; little-endian)
//!   [ rater_pubkey_hash : u64           ] · 8 bytes (BLAKE3-trunc(pubkey))
//!   [ content_id        : u32           ] · 4 bytes
//!   [ stars             : u8            ] · 1 byte  (0..=5 ; 0 = withdrawn)
//!   [ tags_bitset       : u16           ] · 2 bytes (16 player-selectable)
//!   [ sigma_mask        : u8            ] · 1 byte  (cap-policy-snapshot)
//!   [ ts                : u32           ] · 4 bytes (minutes-since-epoch)
//!   [ weight_q8         : u8            ] · 1 byte  (0..=255 ; for KAN)
//!   [ reserved          : [u8; 3]       ] · 3 bytes (zeroed ; future-use)
//!
//! § TAG BITSET (16 bits ; player-selectable per rating)
//!   bit 0  : fun
//!   bit 1  : balanced
//!   bit 2  : creative
//!   bit 3  : accessible
//!   bit 4  : sovereign-respectful
//!   bit 5  : remix-worthy
//!   bit 6  : documentation-clear
//!   bit 7  : runtime-stable
//!   bit 8  : audio-quality
//!   bit 9  : visual-polish
//!   bit 10 : narrative-depth
//!   bit 11 : educational
//!   bit 12 : welcoming
//!   bit 13 : novel
//!   bit 14 : meditative
//!   bit 15 : tense
//!
//! § Σ-MASK GATING (defense-in-depth)
//!   ─ submit-side : `RatingStore::submit` checks (sigma_mask & CAP_RATE) != 0
//!   ─ aggregate-side : `RatingStore::aggregate_for` checks (sigma_mask &
//!     CAP_AGGREGATE_PUBLIC) for any rating-row contributing to k-floor count
//!
//! § K-ANONYMITY ENFORCEMENT
//!   Default `K_FLOOR_SINGLE = 5` — aggregate visible to ANY non-rater only
//!   when ≥ 5 distinct raters have rated the content.
//!   Default `K_FLOOR_TRENDING = 10` — content cannot influence trending-rank
//!   until ≥ 10 distinct raters.
//!
//! § SOVEREIGN-REVOKE FLOW
//!   `RatingStore::revoke(rater_pubkey_hash, content_id)` :
//!     1. removes the (rater_pubkey_hash, content_id) row from `ratings`
//!     2. removes the matching review-body if present
//!     3. recomputes the affected `AggregateView` lazily on next read
//!     4. if recompute drops k-count below `K_FLOOR_SINGLE`, the aggregate
//!        becomes `Hidden` until new raters arrive
//!
//! § QUALITY-SIGNAL → KAN-BIAS-LOOP
//!   `QualitySignal::from_aggregate` distills the AggregateView into a
//!   compact axes-vector for sibling W12-3 (cssl-self-authoring-kan) :
//!     ─ stars_q8           ∈ [0, 255] (mean stars * 51)
//!     ─ remix_worthy_count ∈ u32      (raters who tagged remix-worthy)
//!     ─ runtime_stable_q8  ∈ [0, 255] (proportion tagged runtime-stable)
//!     ─ welcoming_q8       ∈ [0, 255] (proportion tagged welcoming)
//!     ─ warning_count      ∈ u32      (raters who left ≤ 2 stars + ¬ tagged
//!                                      runtime-stable ; → recalibrate-source)
//!
//! § PRIME-DIRECTIVE
//!   `#![forbid(unsafe_code)]`. ¬ surveillance ; ¬ scroll-tracking ; ¬
//!   time-on-card ; ¬ paid-promotion ranking ; cosmetic-axiom-enforced.
//!   Author cannot mutate ratings ; only the rater themselves can revoke.
//!
//! § PARENT spec : `Labyrinth of Apocalypse/systems/content_rating.csl`

#![forbid(unsafe_code)]
#![doc(html_no_source)]

pub mod aggregate;
pub mod kan_bridge;
pub mod rating;
pub mod review;
pub mod store;
pub mod tags;

pub use aggregate::{AggregateView, AggregateVisibility};
pub use kan_bridge::QualitySignal;
pub use rating::{Rating, RatingError, RATING_BYTES};
pub use review::{Review, ReviewError, REVIEW_BODY_MAX, REVIEW_MAX_BYTES};
pub use store::{RatingStore, StoreError};
pub use tags::{tag_index, TagBitset, TAG_NAMES, TAG_TOTAL};

/// § Cap-bit : permission-to-submit-rating.
///
/// Mirrors `cssl-edge/lib/cap.ts CONTENT_RATE_CAP`. A rater's
/// `Rating::sigma_mask` MUST have this bit set ; otherwise `submit` rejects.
pub const CAP_RATE: u8 = 0x01;

/// § Cap-bit : permission-for-row-to-contribute-to-public-aggregate.
///
/// A rating with this bit clear is held in the store but never appears in
/// the `AggregateView` or `QualitySignal`. Used for moderation-suppressed
/// rows that the rater themselves can still see.
pub const CAP_AGGREGATE_PUBLIC: u8 = 0x02;

/// § Cap-bit : permission-to-leave-text-review (vs star-only).
///
/// Distinct from `CAP_RATE` so a host can offer star-only opt-in by default
/// and require a higher trust-tier for free-text.
pub const CAP_REVIEW_BODY: u8 = 0x04;

/// § Cap-bit : reserved / forbidden bits ; non-zero = malformed-tampered.
pub const CAP_RESERVED_MASK: u8 = 0xF0;

/// § K_FLOOR_SINGLE — distinct-rater floor for content-page aggregate.
pub const K_FLOOR_SINGLE: u32 = 5;

/// § K_FLOOR_TRENDING — distinct-rater floor for trending-rank influence.
pub const K_FLOOR_TRENDING: u32 = 10;

/// Crate version stamp ; surfaced in audit lines + observability.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// § PROTOCOL_VERSION — wire-format version of `Rating::pack`. Bumped only
/// when the bit-pack layout changes. Currently 1.
pub const PROTOCOL_VERSION: u32 = 1;
