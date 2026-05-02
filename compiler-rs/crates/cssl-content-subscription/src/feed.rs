//! § feed — ContentNotification + per-subscriber feed store.
//!
//! § NOTIFICATION KINDS (5)
//!   - `NewPublished`        : new content from a followed creator/tag/chain
//!   - `RemixCreated`        : someone remixed content the subscriber holds
//!   - `UpdateAvailable`     : new version of a subscribed-content
//!   - `RevokedByCreator`    : creator-revoke cascade · CONSENT-GATED removal
//!   - `RevokedByModeration` : moderation-revoke cascade · w/ reason-tag · APPEALABLE
//!
//! § ANTI-SPAM
//!   - `auto_resurfaceable()` returns `false` for ALL kinds → the engine never
//!     re-surfaces an old notification after the subscriber has read it.
//!   - The store maintains `read_at_ns` ; once set, the row stops appearing
//!     in `unread_for(...)`. Compaction of read rows is the host crate's job.
//!
//! § DAILY DIGEST
//!   `roll_up_daily(...)` collapses N realtime notifications for a single
//!   subscriber into one synthetic `digest` notification covering 24 h.

use crate::subscription::SubscriptionId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// § Notification kinds (5).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum NotificationKind {
    NewPublished = 1,
    RemixCreated = 2,
    UpdateAvailable = 3,
    RevokedByCreator = 4,
    RevokedByModeration = 5,
}

impl NotificationKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NewPublished => "new-published",
            Self::RemixCreated => "remix-created",
            Self::UpdateAvailable => "update-available",
            Self::RevokedByCreator => "revoked-by-creator",
            Self::RevokedByModeration => "revoked-by-moderation",
        }
    }

    /// § ANTI-SPAM ATTESTATION : NEVER auto-resurface old notifications.
    /// Returns `false` for every kind → encoded structurally so a future
    /// well-meaning contributor cannot accidentally enable engagement-bait.
    #[must_use]
    pub const fn auto_resurfaceable(self) -> bool {
        false
    }

    /// Whether this kind triggers a CONSENT-GATED remove-from-installed.
    #[must_use]
    pub const fn cascades_remove(self) -> bool {
        matches!(
            self,
            Self::RevokedByCreator | Self::RevokedByModeration
        )
    }
}

/// § Stable notification identifier : BLAKE3(sub-id · kind · content-id · ts).
pub type NotificationId = [u8; 32];

/// § Notification record · 1:1 with SQL row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentNotification {
    pub id: NotificationId,
    pub subscription_id: SubscriptionId,
    pub subscriber_pubkey: [u8; 32],
    pub kind: NotificationKind,
    /// 32-byte content-id from sibling W12-4 ContentPackage.
    pub content_id: [u8; 32],
    /// Optional moderator/creator reason tag (kind = Revoked*).
    pub reason: Option<String>,
    pub sigma_mask: [u8; 16],
    pub created_at_ns: u64,
    /// Set when the subscriber reads the row ; once set, never re-surfaced.
    pub read_at_ns: Option<u64>,
}

impl ContentNotification {
    #[must_use]
    pub fn is_unread(&self) -> bool {
        self.read_at_ns.is_none()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NotificationFeedError {
    #[error("notification not found")]
    NotFound,
    #[error("digest window must be > 0 ns")]
    BadWindow,
    #[error("reason exceeds 200 chars (was {0})")]
    ReasonTooLong(usize),
}

/// § In-memory feed store · 1:1 with SQL `content_notifications` table.
#[derive(Debug, Default, Clone)]
pub struct NotificationStore {
    rows: BTreeMap<NotificationId, ContentNotification>,
}

impl NotificationStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a notification. Reason ≤ 200 chars (matches SQL constraint).
    pub fn push(
        &mut self,
        notif: ContentNotification,
    ) -> Result<NotificationId, NotificationFeedError> {
        if let Some(r) = &notif.reason {
            if r.chars().count() > 200 {
                return Err(NotificationFeedError::ReasonTooLong(r.chars().count()));
            }
        }
        let id = notif.id;
        self.rows.insert(id, notif);
        Ok(id)
    }

    /// Mark a row read. Once set, `unread_for` never returns it.
    pub fn mark_read(
        &mut self,
        id: &NotificationId,
        now_ns: u64,
    ) -> Result<(), NotificationFeedError> {
        let r = self.rows.get_mut(id).ok_or(NotificationFeedError::NotFound)?;
        if r.read_at_ns.is_none() {
            r.read_at_ns = Some(now_ns);
        }
        Ok(())
    }

    /// All UNREAD notifications for a subscriber, sorted by created_at_ns asc.
    #[must_use]
    pub fn unread_for(&self, subscriber_pubkey: &[u8; 32]) -> Vec<&ContentNotification> {
        let mut v: Vec<&ContentNotification> = self
            .rows
            .values()
            .filter(|n| &n.subscriber_pubkey == subscriber_pubkey && n.is_unread())
            .collect();
        v.sort_by_key(|n| n.created_at_ns);
        v
    }

    /// Sovereign-purge : remove EVERY row tied to a subscription (by id).
    /// Used by `revoke_subscription` to enforce "purges-feed-row".
    pub fn purge_for_subscription(&mut self, sub_id: &SubscriptionId) -> usize {
        let to_remove: Vec<_> = self
            .rows
            .iter()
            .filter(|(_, n)| &n.subscription_id == sub_id)
            .map(|(k, _)| *k)
            .collect();
        let n = to_remove.len();
        for k in to_remove {
            self.rows.remove(&k);
        }
        n
    }

    /// § Daily-digest rollup : compresses N realtime notifications for a
    /// subscriber covering [now-window, now] into one digest payload.
    /// Returns the digest's `content_id` set (sorted, dedup) + count ;
    /// caller emits one `NewPublished` digest with these bundled in `reason`.
    /// Pure function : does not mutate the store.
    pub fn roll_up_daily(
        &self,
        subscriber_pubkey: &[u8; 32],
        now_ns: u64,
        window_ns: u64,
    ) -> Result<DigestRollup, NotificationFeedError> {
        if window_ns == 0 {
            return Err(NotificationFeedError::BadWindow);
        }
        let lo = now_ns.saturating_sub(window_ns);
        let mut content_ids: Vec<[u8; 32]> = self
            .rows
            .values()
            .filter(|n| {
                &n.subscriber_pubkey == subscriber_pubkey
                    && n.is_unread()
                    && n.created_at_ns >= lo
                    && n.kind == NotificationKind::NewPublished
            })
            .map(|n| n.content_id)
            .collect();
        content_ids.sort();
        content_ids.dedup();
        Ok(DigestRollup {
            window_lo_ns: lo,
            window_hi_ns: now_ns,
            content_ids,
        })
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &ContentNotification> {
        self.rows.values()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// § Output of `roll_up_daily` : a list of distinct content-ids the digest covers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestRollup {
    pub window_lo_ns: u64,
    pub window_hi_ns: u64,
    pub content_ids: Vec<[u8; 32]>,
}

/// Stable notification-id : BLAKE3(sub-id · kind · content-id · ts-le8).
#[must_use]
pub fn derive_notification_id(
    sub_id: &SubscriptionId,
    kind: NotificationKind,
    content_id: &[u8; 32],
    ts_ns: u64,
) -> NotificationId {
    let mut h = blake3::Hasher::new();
    h.update(sub_id);
    h.update(&[kind as u8]);
    h.update(content_id);
    h.update(&ts_ns.to_le_bytes());
    *h.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(s: u8) -> [u8; 32] {
        [s; 32]
    }
    fn mask(s: u8) -> [u8; 16] {
        [s; 16]
    }

    fn make_notif(
        sub_id: SubscriptionId,
        sub_pk: [u8; 32],
        kind: NotificationKind,
        content_id: [u8; 32],
        ts_ns: u64,
    ) -> ContentNotification {
        ContentNotification {
            id: derive_notification_id(&sub_id, kind, &content_id, ts_ns),
            subscription_id: sub_id,
            subscriber_pubkey: sub_pk,
            kind,
            content_id,
            reason: None,
            sigma_mask: mask(0),
            created_at_ns: ts_ns,
            read_at_ns: None,
        }
    }

    #[test]
    fn anti_spam_no_kind_is_resurfaceable() {
        // Encoded ATTESTATION : never auto-resurface old content.
        for k in [
            NotificationKind::NewPublished,
            NotificationKind::RemixCreated,
            NotificationKind::UpdateAvailable,
            NotificationKind::RevokedByCreator,
            NotificationKind::RevokedByModeration,
        ] {
            assert!(!k.auto_resurfaceable(), "{} must NOT be auto-resurfaceable", k.name());
        }
    }

    #[test]
    fn unread_then_mark_read_disappears() {
        let mut store = NotificationStore::new();
        let sub_id = [9u8; 32];
        let n = make_notif(sub_id, pk(1), NotificationKind::NewPublished, pk(2), 100);
        let id = store.push(n).unwrap();
        assert_eq!(store.unread_for(&pk(1)).len(), 1);
        store.mark_read(&id, 200).unwrap();
        assert_eq!(store.unread_for(&pk(1)).len(), 0);
        // Double mark-read is idempotent.
        store.mark_read(&id, 300).unwrap();
        assert_eq!(store.rows.get(&id).unwrap().read_at_ns, Some(200));
    }

    #[test]
    fn purge_for_subscription_removes_only_matching() {
        let mut store = NotificationStore::new();
        let s1 = [1u8; 32];
        let s2 = [2u8; 32];
        let n1 = make_notif(s1, pk(0), NotificationKind::NewPublished, pk(1), 1);
        let n2 = make_notif(s2, pk(0), NotificationKind::NewPublished, pk(2), 2);
        store.push(n1).unwrap();
        store.push(n2).unwrap();
        let removed = store.purge_for_subscription(&s1);
        assert_eq!(removed, 1);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn daily_digest_dedups_content_ids_within_window() {
        let mut store = NotificationStore::new();
        let sub_id = [7u8; 32];
        let pkr = pk(5);
        let c1 = pk(10);
        let c2 = pk(11);
        // Same content seen twice within window → digest dedups.
        store
            .push(make_notif(sub_id, pkr, NotificationKind::NewPublished, c1, 1_000))
            .unwrap();
        store
            .push(make_notif(sub_id, pkr, NotificationKind::NewPublished, c1, 2_000))
            .unwrap();
        store
            .push(make_notif(sub_id, pkr, NotificationKind::NewPublished, c2, 3_000))
            .unwrap();
        let d = store.roll_up_daily(&pkr, 4_000, 86_400 * 1_000_000_000).unwrap();
        assert_eq!(d.content_ids.len(), 2);
        assert!(d.content_ids.contains(&c1));
        assert!(d.content_ids.contains(&c2));
    }

    #[test]
    fn cascades_remove_only_for_revoked_kinds() {
        assert!(NotificationKind::RevokedByCreator.cascades_remove());
        assert!(NotificationKind::RevokedByModeration.cascades_remove());
        assert!(!NotificationKind::NewPublished.cascades_remove());
        assert!(!NotificationKind::RemixCreated.cascades_remove());
        assert!(!NotificationKind::UpdateAvailable.cascades_remove());
    }
}
