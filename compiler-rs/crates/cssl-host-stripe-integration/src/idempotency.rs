// § idempotency.rs — Stripe `Idempotency-Key` mandatory on every mutating call
// 24h TTL · BTreeMap-deterministic · per-process in-memory store ;
// G1 may swap to a persistent store backed by sled / sqlite.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 24 hours in seconds — Stripe's documented retention window for Idempotency-Key reuse.
pub const IDEMPOTENCY_TTL_SECS: u64 = 24 * 60 * 60;

/// § IdempotencyKey — newtype to keep call-sites self-documenting and prevent
/// accidental swap with arbitrary `String`. Construction validates non-empty
/// and ASCII-only (Stripe spec).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Construct from a caller-provided string. Returns `None` if empty or
    /// contains non-ASCII bytes (Stripe rejects those server-side anyway).
    #[must_use]
    pub fn new(k: impl Into<String>) -> Option<Self> {
        let s: String = k.into();
        if s.is_empty() || !s.is_ascii() {
            None
        } else {
            Some(Self(s))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// § IdempotencyEntry — what we cached the first time the key was seen.
///
/// `payload_hash` is a BLAKE3 hash of the canonicalized request body, so we
/// can detect "same key, different payload" — that is `IdempotencyConflict`,
/// not idempotent reuse.
#[derive(Debug, Clone)]
pub struct IdempotencyEntry {
    pub key: IdempotencyKey,
    pub payload_hash: [u8; 32],
    pub stored_at_unix: u64,
    pub method: &'static str,
    pub response_body: String,
    pub response_status: u16,
}

impl IdempotencyEntry {
    #[must_use]
    pub fn is_expired(&self, now_unix: u64) -> bool {
        now_unix.saturating_sub(self.stored_at_unix) > IDEMPOTENCY_TTL_SECS
    }
}

/// § IdempotencyStore — BTreeMap-backed, deterministic iteration.
#[derive(Debug, Default)]
pub struct IdempotencyStore {
    state: Mutex<BTreeMap<IdempotencyKey, IdempotencyEntry>>,
}

impl IdempotencyStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Hash the request body deterministically for conflict detection.
    #[must_use]
    pub fn hash_payload(body: &str) -> [u8; 32] {
        *blake3::hash(body.as_bytes()).as_bytes()
    }

    /// Look up the entry by key. Returns expired entries as `None` (auto-purge).
    pub fn get(&self, key: &IdempotencyKey, now_unix: u64) -> Option<IdempotencyEntry> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = s.get(key) {
            if entry.is_expired(now_unix) {
                s.remove(key);
                None
            } else {
                Some(entry.clone())
            }
        } else {
            None
        }
    }

    /// Insert a new entry. If `key` already exists with a divergent
    /// `payload_hash`, returns `Err(())` — caller maps to `IdempotencyConflict`.
    /// If `key` exists with matching `payload_hash`, this is a no-op (idempotent).
    pub fn insert(&self, entry: IdempotencyEntry) -> Result<(), ()> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = s.get(&entry.key) {
            if existing.payload_hash == entry.payload_hash {
                return Ok(()); // matching reuse · idempotent
            }
            return Err(()); // conflict
        }
        s.insert(entry.key.clone(), entry);
        Ok(())
    }

    /// Manual purge of expired entries (callers may invoke periodically).
    pub fn purge_expired(&self, now_unix: u64) -> usize {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let before = s.len();
        s.retain(|_, v| !v.is_expired(now_unix));
        before - s.len()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Wall-clock seconds since epoch. Used for TTL bookkeeping.
#[must_use]
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_for(key: &str, body: &str, ts: u64) -> IdempotencyEntry {
        IdempotencyEntry {
            key: IdempotencyKey::new(key).expect("key"),
            payload_hash: IdempotencyStore::hash_payload(body),
            stored_at_unix: ts,
            method: "create_checkout_session",
            response_body: String::new(),
            response_status: 200,
        }
    }

    #[test]
    fn key_rejects_empty() {
        assert!(IdempotencyKey::new("").is_none());
    }

    #[test]
    fn key_rejects_non_ascii() {
        assert!(IdempotencyKey::new("kéy").is_none());
        assert!(IdempotencyKey::new("kabc").is_some());
    }

    #[test]
    fn idempotent_reuse_is_ok() {
        let s = IdempotencyStore::new();
        s.insert(entry_for("k1", "body=1", 1000)).unwrap();
        s.insert(entry_for("k1", "body=1", 2000)).unwrap();
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn divergent_payload_is_conflict() {
        let s = IdempotencyStore::new();
        s.insert(entry_for("k1", "body=1", 1000)).unwrap();
        let r = s.insert(entry_for("k1", "body=2", 1100));
        assert!(r.is_err());
    }

    #[test]
    fn ttl_expires_on_lookup() {
        let s = IdempotencyStore::new();
        s.insert(entry_for("k1", "body=1", 1000)).unwrap();
        let later = 1000 + IDEMPOTENCY_TTL_SECS + 1;
        assert!(s.get(&IdempotencyKey::new("k1").unwrap(), later).is_none());
        assert_eq!(s.len(), 0); // purged on lookup
    }
}
