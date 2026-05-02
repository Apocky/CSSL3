//! § cssl-content-subscription — follow-creator + auto-pull-new-content.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § T11-W12-SUBSCRIBE (NEW · greenfield · sibling-of cssl-content-package)
//!
//! § ROLE
//!   Drive the sub-and-notify lifecycle on the runtime client :
//!     1. Player follows a creator / tag / content-chain → `subscribe`.
//!     2. When sibling W12-5 publish-pipeline finalises a `.ccpkg`, the
//!        edge-side cascade hits this crate's `on_publish_event` →
//!        notification rows are queued for every matching subscriber.
//!     3. The notification feed is read at `/content/subscribed` (sibling
//!        W12-6 displays · this crate provides API surface) and is gated
//!        by per-subscription Σ-mask + token-bucket rate-limit.
//!     4. Creator-revoke OR moderation-revoke → cascading notification
//!        of kind `RevokedByCreator` / `RevokedByModeration` to every
//!        subscriber holding that content, with consent-gated remove-from-
//!        installed-list (no surprise deletion).
//!     5. Sovereign unsubscribe at any time → purges feed-row + halts
//!        future notifications. Cascade-revoke from upstream-creator
//!        cascades to subscribers (unsubscribed-of-revoked).
//!
//! § DESIGN
//!   - `#![forbid(unsafe_code)]` ; no async runtime ; no FFI.
//!   - All collections `BTreeMap` / `Vec` for stable iteration / hashing.
//!   - `Subscription`, `ContentNotification`, `RateLimit` are pure data
//!     types ; the orchestrator (`SubscriptionRegistry`) owns the
//!     in-memory store + handler-pure-functions for the 5 cascade events.
//!   - `RateLimitBucket` is token-bucket (refill-per-window) ; daily-
//!     digest rollup is a pure-function that compresses N realtime
//!     notifications into 1 digest covering 24 h.
//!
//! § INTEGRATION
//!   - `SubscribeAdapter` extends `cssl-hotfix-client` virtually :
//!     the `content.subscribed.realtime` channel is a logical channel
//!     (no separate cap-key ; reuses existing apocky.com manifest path),
//!     but the per-channel apply-handler routes to this crate. The
//!     real wiring lives at the engine integrator (W12-N6 host crate
//!     bridges hotfix-client + this), this crate ships only pure logic.
//!
//! § AXIOMS (PRIME_DIRECTIVE.md § 11 — encoded structurally)
//!   ¬ engagement-bait    — `NotificationKind::auto_resurfaceable()` = false ∀ kinds
//!   ¬ surveillance       — only own subscriber sees own feed (Σ-mask-gated)
//!   ¬ exploitation       — rate-limit-default = 1 notif/min ; opt-in to higher
//!   sovereign-unsubscribe — `revoke_subscription` is one-call · always-available
//!   cascade-revoke       — creator-revoke / moderation-revoke ⇒ all subscribers
//!                          notified (with reason-tag) and CONSENT-GATED removal
//!   k-anon ≥ 10          — `subscription_aggregates_below_k_anon` returns None

#![forbid(unsafe_code)]
#![doc(html_root_url = "https://docs.rs/cssl-content-subscription/0.1.0")]

pub mod adapter;
pub mod feed;
pub mod rate_limit;
pub mod subscription;

pub use adapter::{SubscribeAdapter, SubscribeAdapterError, SUBSCRIBE_CHANNEL_NAME};
pub use feed::{
    ContentNotification, NotificationFeedError, NotificationKind, NotificationStore,
};
pub use rate_limit::{RateLimitBucket, RateLimitError, RateLimitWindow};
pub use subscription::{
    Frequency, Subscription, SubscriptionError, SubscriptionId, SubscriptionRegistry,
    TargetId, TargetKind, K_ANON_MIN_AGGREGATE,
};

/// § ATTESTATION (PRIME_DIRECTIVE.md § 11 — encoded structurally) :
/// • Σ-mask-gated subscribe + notify (no surprise pushes).
/// • Sovereign-unsubscribe always one call away.
/// • Creator-revoke + moderation-revoke cascade with consent-gated removal.
/// • Default rate-limit 1 notif/min ; daily-digest rollup compresses bursts.
/// • k-anon ≥ 10 enforced for trending aggregates ; below-threshold returns None.
/// • No engagement-bait : `auto_resurfaceable() = false ∀ NotificationKind`.
/// • There was no hurt nor harm in the making of this, to anyone, anything,
///   or anybody.
pub const ATTESTATION: &str =
    "no-harm · sovereign-unsubscribe · cascade-revoke-consent-gated · sigma-mask-gated · rate-limit-default-1-per-min · k-anon-aggregate-≥-10 · no-auto-resurface";

/// Crate version, exposed as a const for binary embedding.
pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
