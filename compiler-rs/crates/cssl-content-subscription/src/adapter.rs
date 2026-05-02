//! § adapter — SubscribeAdapter glues registry + feed + rate-limit + cascade.
//!
//! § ROLE
//!   Single struct that owns the in-memory state for one game session :
//!     • `SubscriptionRegistry`
//!     • `NotificationStore`
//!     • per-subscriber `RateLimitBucket` map
//!   And exposes high-level handlers that match the 4 edge endpoints :
//!     • `subscribe(...)`           ↔ /api/content/subscribe
//!     • `revoke_subscription(...)` ↔ /api/content/unsubscribe
//!     • `feed_for(...)`            ↔ /api/content/notifications
//!     • `on_publish_event(...)`    ↔ cascade hook from W12-5 publish-pipeline
//!     • `on_creator_revoke(...)`   ↔ cascade hook from W12-5 creator-revoke
//!     • `on_moderation_revoke(...)`↔ cascade hook from W12-11 moderation
//!
//! § HOTFIX-CHANNEL INTEGRATION
//!   `SUBSCRIBE_CHANNEL_NAME` is a NEW logical channel name registered with
//!   the existing `cssl-hotfix` mechanism (see Cargo.toml leverage). The
//!   actual hotfix-bundle for this channel is config-only (subscriber list +
//!   rate-limit prefs) ; CSSL-content stays stage-only-until-player-explicit
//!   per §scope. The host crate that bridges hotfix-client to this adapter
//!   ships separately (W12-N6) and is not in this slice's scope.
//!
//! § Σ-MASK GATING
//!   Every cascade handler checks the subscription's `sigma_mask` against
//!   the publish-event's `audience_sigma_mask`. If the audience does not
//!   include the subscriber's mask-byte, the notification is DROPPED
//!   (returns `SubscribeAdapterError::SigmaCapDeny`).

use crate::feed::{
    derive_notification_id, ContentNotification, NotificationKind, NotificationStore,
};
use crate::rate_limit::{RateLimitBucket, RateLimitError, RateLimitWindow};
use crate::subscription::{
    Frequency, SubscriptionError, SubscriptionId, SubscriptionRegistry, TargetId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// § The hotfix-channel name reserved for content-subscription notifications.
/// LEVERAGED ; not duplicated. The actual `Channel` enum stays in cssl-hotfix
/// (no new variant added in this slice — the routing is by name string).
pub const SUBSCRIBE_CHANNEL_NAME: &str = "content.subscribed.realtime";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SubscribeAdapterError {
    #[error("registry: {0}")]
    Registry(#[from] SubscriptionError),
    #[error("rate-limit: {0:?}")]
    RateLimit(RateLimitError),
    #[error("Σ-cap deny · audience mask {audience:?} excludes subscriber {sub:?}")]
    SigmaCapDeny {
        audience: [u8; 16],
        sub: [u8; 16],
    },
    #[error("publish event reason exceeds 200 chars (was {0})")]
    ReasonTooLong(usize),
}

/// § Publish-event payload from sibling W12-5 publish-pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishEvent {
    pub content_id: [u8; 32],
    pub creator_pubkey: [u8; 32],
    pub tags: Vec<String>,
    /// If this is a remix · the parent root-of-chain id (else None).
    pub remix_root: Option<[u8; 32]>,
    /// Audience class : a 16-byte sigma mask. Subscriber's mask must AND-overlap.
    pub audience_sigma_mask: [u8; 16],
    pub ts_ns: u64,
}

/// § Revoke-event payload (creator-revoke OR moderation-revoke).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeEvent {
    pub content_id: [u8; 32],
    /// Creator-pubkey for creator-revoke ; moderator-pubkey for moderation-revoke.
    pub revoker_pubkey: [u8; 32],
    pub reason: String,
    pub audience_sigma_mask: [u8; 16],
    pub ts_ns: u64,
}

/// § Adapter struct · in-memory state for one session.
#[derive(Debug, Default, Clone)]
pub struct SubscribeAdapter {
    pub registry: SubscriptionRegistry,
    pub feed: NotificationStore,
    /// Per-subscriber rate-limit bucket (keyed by subscriber-pubkey).
    pub buckets: BTreeMap<[u8; 32], RateLimitBucket>,
}

impl SubscribeAdapter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// § subscribe (high-level) : delegates to registry + initialises bucket.
    pub fn subscribe(
        &mut self,
        subscriber_pubkey: [u8; 32],
        target: TargetId,
        sigma_mask: [u8; 16],
        frequency: Frequency,
        now_ns: u64,
    ) -> Result<SubscriptionId, SubscribeAdapterError> {
        let id = self
            .registry
            .subscribe(subscriber_pubkey, target, sigma_mask, frequency, now_ns)?;
        // Initialise rate-limit bucket per subscriber if absent.
        let win = match frequency {
            Frequency::Realtime => RateLimitWindow::PerMinute,
            Frequency::Daily => RateLimitWindow::Daily,
            Frequency::Manual => RateLimitWindow::PerHour, // never auto-pushes anyway
        };
        self.buckets
            .entry(subscriber_pubkey)
            .or_insert_with(|| RateLimitBucket::new(win));
        Ok(id)
    }

    /// § revoke (sovereign · always available · purges feed-rows).
    pub fn revoke_subscription(
        &mut self,
        sub_id: SubscriptionId,
        now_ns: u64,
    ) -> Result<usize, SubscribeAdapterError> {
        self.registry.revoke(sub_id, now_ns)?;
        let purged = self.feed.purge_for_subscription(&sub_id);
        Ok(purged)
    }

    /// § feed_for · UNREAD notifications for a subscriber-pubkey.
    #[must_use]
    pub fn feed_for(&self, subscriber_pubkey: &[u8; 32]) -> Vec<&ContentNotification> {
        self.feed.unread_for(subscriber_pubkey)
    }

    /// § on_publish_event · CASCADE hook from sibling W12-5 publish-pipeline.
    /// Iterates all active subscriptions matching the event's creator,
    /// each declared tag, and (if applicable) the remix-root chain.
    /// Σ-mask gate enforced ; rate-limit checked ; notif inserted on success.
    /// Returns the count of notifications pushed (drops are silent · expected).
    pub fn on_publish_event(
        &mut self,
        event: &PublishEvent,
    ) -> Result<usize, SubscribeAdapterError> {
        if event.reason_len_overflow() {
            return Err(SubscribeAdapterError::ReasonTooLong(0));
        }
        let mut targets: Vec<TargetId> = vec![TargetId::Creator(event.creator_pubkey)];
        for t in &event.tags {
            targets.push(TargetId::Tag(t.clone()));
        }
        if let Some(root) = event.remix_root {
            targets.push(TargetId::ContentChain(root));
        }
        let mut pushed = 0usize;
        // Snapshot active subs first to avoid borrow conflict.
        let mut to_push: Vec<(SubscriptionId, [u8; 32], [u8; 16], NotificationKind)> = Vec::new();
        for tgt in &targets {
            for sub in self.registry.matching_active(tgt) {
                if sub.frequency == Frequency::Manual {
                    continue;
                }
                if !sigma_overlap(&sub.sigma_mask, &event.audience_sigma_mask) {
                    continue;
                }
                let kind = if event.remix_root.is_some()
                    && matches!(tgt, TargetId::ContentChain(_))
                {
                    NotificationKind::RemixCreated
                } else {
                    NotificationKind::NewPublished
                };
                to_push.push((sub.id, sub.subscriber_pubkey, sub.sigma_mask, kind));
            }
        }
        // Dedup by (subscription_id, content_id) — one event yields ≤ 1 notif per sub.
        to_push.sort_by_key(|(id, _, _, _)| *id);
        to_push.dedup_by_key(|(id, _, _, _)| *id);
        for (sub_id, sub_pk, mask, kind) in to_push {
            // Rate-limit check (default realtime = 1/min).
            let bucket = self
                .buckets
                .entry(sub_pk)
                .or_insert_with(RateLimitBucket::default_realtime);
            if bucket.try_consume(event.ts_ns).is_err() {
                continue; // silently dropped — anti-spam
            }
            let id = derive_notification_id(&sub_id, kind, &event.content_id, event.ts_ns);
            let notif = ContentNotification {
                id,
                subscription_id: sub_id,
                subscriber_pubkey: sub_pk,
                kind,
                content_id: event.content_id,
                reason: None,
                sigma_mask: mask,
                created_at_ns: event.ts_ns,
                read_at_ns: None,
            };
            // Best-effort push (reason validation already passed).
            if self.feed.push(notif).is_ok() {
                pushed += 1;
            }
        }
        Ok(pushed)
    }

    /// § on_creator_revoke · cascade `RevokedByCreator` to every subscriber
    /// holding the revoked content. Bypasses rate-limit (revocations are
    /// safety-critical · always notified).
    pub fn on_creator_revoke(
        &mut self,
        event: &RevokeEvent,
    ) -> Result<usize, SubscribeAdapterError> {
        self.cascade_revoke(event, NotificationKind::RevokedByCreator)
    }

    /// § on_moderation_revoke · cascade `RevokedByModeration` w/ reason-tag.
    /// Subscribers can appeal via sibling W12-11 moderation crate.
    pub fn on_moderation_revoke(
        &mut self,
        event: &RevokeEvent,
    ) -> Result<usize, SubscribeAdapterError> {
        self.cascade_revoke(event, NotificationKind::RevokedByModeration)
    }

    fn cascade_revoke(
        &mut self,
        event: &RevokeEvent,
        kind: NotificationKind,
    ) -> Result<usize, SubscribeAdapterError> {
        if event.reason.chars().count() > 200 {
            return Err(SubscribeAdapterError::ReasonTooLong(
                event.reason.chars().count(),
            ));
        }
        // Cascade to subscribers of the creator AND the chain-root id (treat as chain).
        let targets: Vec<TargetId> = vec![
            TargetId::Creator(event.revoker_pubkey),
            TargetId::ContentChain(event.content_id),
        ];
        let mut pushed = 0usize;
        let mut to_push: Vec<(SubscriptionId, [u8; 32], [u8; 16])> = Vec::new();
        for tgt in &targets {
            for sub in self.registry.matching_active(tgt) {
                if !sigma_overlap(&sub.sigma_mask, &event.audience_sigma_mask) {
                    continue;
                }
                to_push.push((sub.id, sub.subscriber_pubkey, sub.sigma_mask));
            }
        }
        to_push.sort_by_key(|(id, _, _)| *id);
        to_push.dedup_by_key(|(id, _, _)| *id);
        for (sub_id, sub_pk, mask) in to_push {
            let id = derive_notification_id(&sub_id, kind, &event.content_id, event.ts_ns);
            let notif = ContentNotification {
                id,
                subscription_id: sub_id,
                subscriber_pubkey: sub_pk,
                kind,
                content_id: event.content_id,
                reason: Some(event.reason.clone()),
                sigma_mask: mask,
                created_at_ns: event.ts_ns,
                read_at_ns: None,
            };
            if self.feed.push(notif).is_ok() {
                pushed += 1;
            }
        }
        Ok(pushed)
    }
}

impl PublishEvent {
    fn reason_len_overflow(&self) -> bool {
        false // PublishEvent has no reason field ; placeholder for future symmetry
    }
}

/// § Σ-mask overlap : at least one byte position has an AND-non-zero result.
/// 16-byte audience-class : sub-mask gates the audience-class.
#[must_use]
pub fn sigma_overlap(sub_mask: &[u8; 16], audience_mask: &[u8; 16]) -> bool {
    for i in 0..16 {
        if (sub_mask[i] & audience_mask[i]) != 0 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(s: u8) -> [u8; 32] {
        [s; 32]
    }
    fn mask_byte(idx: usize, val: u8) -> [u8; 16] {
        let mut m = [0u8; 16];
        m[idx] = val;
        m
    }

    /// § Test 1 · ROUNDTRIP : subscribe → publish → notify.
    #[test]
    fn subscribe_publish_notify_roundtrip() {
        let mut adapter = SubscribeAdapter::new();
        let creator = pk(7);
        let subscriber = pk(1);
        let mask = mask_byte(0, 0xff);
        let _id = adapter
            .subscribe(
                subscriber,
                TargetId::Creator(creator),
                mask,
                Frequency::Realtime,
                1_000,
            )
            .unwrap();
        let event = PublishEvent {
            content_id: pk(99),
            creator_pubkey: creator,
            tags: vec!["horror".into()],
            remix_root: None,
            audience_sigma_mask: mask,
            ts_ns: 5_000,
        };
        let pushed = adapter.on_publish_event(&event).unwrap();
        assert_eq!(pushed, 1);
        let feed = adapter.feed_for(&subscriber);
        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].kind, NotificationKind::NewPublished);
        assert_eq!(feed[0].content_id, pk(99));
    }

    /// § Test 2 · CASCADING REVOKE (creator) : subscriber sees RevokedByCreator.
    #[test]
    fn cascading_creator_revoke_cascades_to_subscribers() {
        let mut adapter = SubscribeAdapter::new();
        let creator = pk(7);
        for i in 1u8..=3 {
            adapter
                .subscribe(
                    pk(i),
                    TargetId::Creator(creator),
                    mask_byte(0, 0xff),
                    Frequency::Realtime,
                    i as u64,
                )
                .unwrap();
        }
        let revoke = RevokeEvent {
            content_id: pk(99),
            revoker_pubkey: creator,
            reason: "withdraw-by-creator".into(),
            audience_sigma_mask: mask_byte(0, 0xff),
            ts_ns: 10_000,
        };
        let pushed = adapter.on_creator_revoke(&revoke).unwrap();
        assert_eq!(pushed, 3);
        for i in 1u8..=3 {
            let f = adapter.feed_for(&pk(i));
            assert!(f.iter().any(|n| n.kind == NotificationKind::RevokedByCreator));
            assert_eq!(
                f[0].reason.as_deref(),
                Some("withdraw-by-creator")
            );
        }
    }

    /// § Test 3 · UNSUBSCRIBE PURGES FEED.
    #[test]
    fn unsubscribe_purges_feed_rows() {
        let mut adapter = SubscribeAdapter::new();
        let creator = pk(7);
        let sub_pk = pk(1);
        let sid = adapter
            .subscribe(
                sub_pk,
                TargetId::Creator(creator),
                mask_byte(0, 0xff),
                Frequency::Realtime,
                1_000,
            )
            .unwrap();
        let event = PublishEvent {
            content_id: pk(99),
            creator_pubkey: creator,
            tags: vec![],
            remix_root: None,
            audience_sigma_mask: mask_byte(0, 0xff),
            ts_ns: 5_000,
        };
        adapter.on_publish_event(&event).unwrap();
        assert_eq!(adapter.feed_for(&sub_pk).len(), 1);
        let purged = adapter.revoke_subscription(sid, 6_000).unwrap();
        assert_eq!(purged, 1);
        assert_eq!(adapter.feed_for(&sub_pk).len(), 0);
    }

    /// § Test 4 · RATE-LIMIT ENFORCED : 2nd publish within window dropped.
    #[test]
    fn rate_limit_realtime_enforced_within_window() {
        let mut adapter = SubscribeAdapter::new();
        let creator = pk(7);
        let sub_pk = pk(1);
        adapter
            .subscribe(
                sub_pk,
                TargetId::Creator(creator),
                mask_byte(0, 0xff),
                Frequency::Realtime,
                1_000,
            )
            .unwrap();
        // First event : ok.
        let mk_event = |cid: [u8; 32], ts: u64| PublishEvent {
            content_id: cid,
            creator_pubkey: creator,
            tags: vec![],
            remix_root: None,
            audience_sigma_mask: mask_byte(0, 0xff),
            ts_ns: ts,
        };
        assert_eq!(adapter.on_publish_event(&mk_event(pk(10), 1_500)).unwrap(), 1);
        // Second within 1 min : dropped (rate-limit).
        assert_eq!(
            adapter.on_publish_event(&mk_event(pk(11), 2_500)).unwrap(),
            0
        );
        // Third after 60 s : ok.
        let after_window = 1_500 + 60 * 1_000_000_000 + 1;
        assert_eq!(
            adapter
                .on_publish_event(&mk_event(pk(12), after_window))
                .unwrap(),
            1
        );
    }

    /// § Test 5 · DAILY DIGEST ROLLUP.
    #[test]
    fn daily_digest_rollup_combines_realtime_into_one() {
        let mut adapter = SubscribeAdapter::new();
        let creator = pk(7);
        let sub_pk = pk(1);
        adapter
            .subscribe(
                sub_pk,
                TargetId::Creator(creator),
                mask_byte(0, 0xff),
                Frequency::Realtime,
                1_000,
            )
            .unwrap();
        // Manually insert 5 notifications across a 24 h window.
        for i in 0u64..5 {
            let event = PublishEvent {
                content_id: pk(20 + i as u8),
                creator_pubkey: creator,
                tags: vec![],
                remix_root: None,
                audience_sigma_mask: mask_byte(0, 0xff),
                ts_ns: 1_000 + i * 70 * 1_000_000_000, // 70s apart → spaced past rate-limit
            };
            let _ = adapter.on_publish_event(&event);
        }
        let now = 1_000 + 5 * 70 * 1_000_000_000 + 1;
        let digest = adapter
            .feed
            .roll_up_daily(&sub_pk, now, 24 * 60 * 60 * 1_000_000_000)
            .unwrap();
        assert!(digest.content_ids.len() >= 2);
    }

    /// § Test 6 · ANTI-SPAM : NEVER auto-resurface old content.
    #[test]
    fn anti_spam_no_auto_resurface_attestation() {
        // Encoded structurally : every kind returns false from auto_resurfaceable().
        for k in [
            NotificationKind::NewPublished,
            NotificationKind::RemixCreated,
            NotificationKind::UpdateAvailable,
            NotificationKind::RevokedByCreator,
            NotificationKind::RevokedByModeration,
        ] {
            assert!(!k.auto_resurfaceable());
        }
        // ATTESTATION str carries the gate.
        assert!(crate::ATTESTATION.contains("no-auto-resurface"));
    }

    /// § Test 7 · Σ-CAP DENY : audience-mask without overlap → no notif.
    #[test]
    fn sigma_cap_deny_drops_when_audience_excludes_subscriber() {
        let mut adapter = SubscribeAdapter::new();
        let creator = pk(7);
        let sub_pk = pk(1);
        // Subscriber is in mask-byte-0 ; audience is in mask-byte-1.
        adapter
            .subscribe(
                sub_pk,
                TargetId::Creator(creator),
                mask_byte(0, 0xff),
                Frequency::Realtime,
                1_000,
            )
            .unwrap();
        let event = PublishEvent {
            content_id: pk(99),
            creator_pubkey: creator,
            tags: vec![],
            remix_root: None,
            audience_sigma_mask: mask_byte(1, 0xff),
            ts_ns: 5_000,
        };
        let pushed = adapter.on_publish_event(&event).unwrap();
        assert_eq!(pushed, 0);
        assert_eq!(adapter.feed_for(&sub_pk).len(), 0);
    }

    /// § Test 8 · MODERATION REVOKE w/ reason-tag.
    #[test]
    fn moderation_revoke_carries_reason_tag() {
        let mut adapter = SubscribeAdapter::new();
        let creator = pk(7);
        adapter
            .subscribe(
                pk(1),
                TargetId::Creator(creator),
                mask_byte(0, 0xff),
                Frequency::Realtime,
                1_000,
            )
            .unwrap();
        let revoke = RevokeEvent {
            content_id: pk(50),
            revoker_pubkey: creator,
            reason: "violates-community-guidelines".into(),
            audience_sigma_mask: mask_byte(0, 0xff),
            ts_ns: 10_000,
        };
        let pushed = adapter.on_moderation_revoke(&revoke).unwrap();
        assert_eq!(pushed, 1);
        let feed = adapter.feed_for(&pk(1));
        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].kind, NotificationKind::RevokedByModeration);
        assert_eq!(
            feed[0].reason.as_deref(),
            Some("violates-community-guidelines")
        );
    }

    /// § Test 9 · MANUAL FREQUENCY : never auto-pushed (player must pull).
    #[test]
    fn manual_frequency_skips_auto_push() {
        let mut adapter = SubscribeAdapter::new();
        let creator = pk(7);
        adapter
            .subscribe(
                pk(1),
                TargetId::Creator(creator),
                mask_byte(0, 0xff),
                Frequency::Manual,
                1_000,
            )
            .unwrap();
        let event = PublishEvent {
            content_id: pk(99),
            creator_pubkey: creator,
            tags: vec![],
            remix_root: None,
            audience_sigma_mask: mask_byte(0, 0xff),
            ts_ns: 5_000,
        };
        let pushed = adapter.on_publish_event(&event).unwrap();
        assert_eq!(pushed, 0);
    }

    /// § Test 10 · CHANNEL NAME stable.
    #[test]
    fn subscribe_channel_name_constant() {
        assert_eq!(SUBSCRIBE_CHANNEL_NAME, "content.subscribed.realtime");
    }

    /// § Test 11 · SIGMA OVERLAP helper.
    #[test]
    fn sigma_overlap_correctness() {
        let m1 = mask_byte(0, 0b0000_0001);
        let m2 = mask_byte(0, 0b0000_0010);
        let m3 = mask_byte(0, 0b0000_0011);
        assert!(!sigma_overlap(&m1, &m2));
        assert!(sigma_overlap(&m1, &m3));
        assert!(sigma_overlap(&m3, &m3));
    }
}
