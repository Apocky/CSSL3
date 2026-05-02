// training_pair.rs — fixed-size ring-buffer log of (prompt, gen-CSSL, score, decision)
// ══════════════════════════════════════════════════════════════════
// § ROLE
//   Every author-cycle (success or failure) emits a TrainingPairRecord. The
//   ring-buffer holds N records (default 4096) ; oldest evicted on overflow.
//   W12-3 KAN-loop reads these via `as_slice()` for self-improvement training.
//
// § BIT-PACKED BIT-EFFICIENT (per Sawyer/Pokémon-OG memory file)
//   - prompt + generated_cssl stored as Strings (variable-length payload), but
//     score / decision-tag / timestamp packed into 4 bytes total.
//   - source_blake3 stored as 32 raw bytes (canonical id ; cheap-to-hash for KAN).
//   - record-on-disk size = serde_json::to_vec(&record).len() ; record-in-memory
//     = sum-of-string-bytes + 4 (packed) + 32 (hash) + 8 (ts) + 32 (target_path).
//
// § WIRE-FORMAT (deterministic JSON for cross-version stability)
//   {
//     "ts_unix":         u64,
//     "prompt":          String,
//     "kind":            "scene"|"npc_line"|...,
//     "generated_cssl":  String,
//     "source_blake3":   "<64 hex>",
//     "score":           u8,
//     "decision":        "allow"|"deny_*",
//     "target_path":     String,
//   }
// ══════════════════════════════════════════════════════════════════

use crate::live_mutate::LiveMutateDecision;
use crate::request::SelfAuthorKind;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Sub-byte-packed mutate-decision tag for compact in-memory storage.
/// Mirrors `LiveMutateDecision::as_str` 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutateDecision {
    Allow,
    DenyScoreBelowThreshold,
    DenySandboxFailed,
    DenyForbiddenTarget,
    DenyCapInvalid,
    DenyCapRevoked,
    DenySovereignBitMissing,
    /// No mutate attempted (compile-failed-early or request-rejected).
    NotAttempted,
}

impl From<&LiveMutateDecision> for MutateDecision {
    fn from(d: &LiveMutateDecision) -> Self {
        match d {
            LiveMutateDecision::Allow => Self::Allow,
            LiveMutateDecision::DenyScoreBelowThreshold { .. } => Self::DenyScoreBelowThreshold,
            LiveMutateDecision::DenySandboxFailed => Self::DenySandboxFailed,
            LiveMutateDecision::DenyForbiddenTarget(_) => Self::DenyForbiddenTarget,
            LiveMutateDecision::DenyCapInvalid(_) => Self::DenyCapInvalid,
            LiveMutateDecision::DenyCapRevoked => Self::DenyCapRevoked,
            LiveMutateDecision::DenySovereignBitMissing => Self::DenySovereignBitMissing,
        }
    }
}

/// Single training-pair record. JSON-stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainingPairRecord {
    /// Wall-clock seconds when this cycle completed.
    pub ts_unix: u64,
    /// The original GM-prompt.
    pub prompt: String,
    /// Authoring-kind.
    pub kind: SelfAuthorKind,
    /// LLM-emitted CSSL source. May be empty on early-reject (validation-fail).
    pub generated_cssl: String,
    /// BLAKE3 of `generated_cssl` bytes (hex, 64-char). Canonical id.
    pub source_blake3_hex: String,
    /// Quality-score 0..100.
    pub score: u8,
    /// Mutate decision.
    pub decision: MutateDecision,
    /// Target path (may be empty if request did not specify).
    pub target_path: String,
}

impl TrainingPairRecord {
    /// Construct a record. `source_blake3` is auto-hex-encoded.
    #[must_use]
    pub fn new(
        ts_unix: u64,
        prompt: String,
        kind: SelfAuthorKind,
        generated_cssl: String,
        source_blake3: [u8; 32],
        score: u8,
        decision: MutateDecision,
        target_path: String,
    ) -> Self {
        let mut hex = String::with_capacity(64);
        for b in source_blake3 {
            hex.push_str(&format!("{b:02x}"));
        }
        Self {
            ts_unix,
            prompt,
            kind,
            generated_cssl,
            source_blake3_hex: hex,
            score,
            decision,
            target_path,
        }
    }
}

/// Approximate serialized-record size in bytes. Conservative estimate for
/// memory-budgeting (actual JSON may vary slightly with field-shape).
/// Used by KAN-loop sibling to size its ingestion-buffer.
#[must_use]
pub fn serialized_record_size(rec: &TrainingPairRecord) -> usize {
    serde_json::to_vec(rec).map(|v| v.len()).unwrap_or(0)
}

/// Bounded ring-buffer log. Oldest evicted on overflow ; `capacity` configurable.
#[derive(Debug, Clone)]
pub struct TrainingPairLog {
    capacity: usize,
    records: VecDeque<TrainingPairRecord>,
}

impl TrainingPairLog {
    /// Construct a new log with `capacity` slots.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            records: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    /// Append a record. Evicts the oldest when `len` would exceed `capacity`.
    /// Returns the evicted record (if any).
    pub fn push(&mut self, rec: TrainingPairRecord) -> Option<TrainingPairRecord> {
        let evicted = if self.records.len() == self.capacity {
            self.records.pop_front()
        } else {
            None
        };
        self.records.push_back(rec);
        evicted
    }

    /// Borrow the records as a contiguous slice (when possible). For zero-copy
    /// readers, prefer `as_slices` which returns `(front, back)` halves.
    #[must_use]
    pub fn as_slices(&self) -> (&[TrainingPairRecord], &[TrainingPairRecord]) {
        self.records.as_slices()
    }

    /// Number of records currently held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns `true` iff log is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Configured capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Iterate records in append-order (oldest → newest).
    pub fn iter(&self) -> impl Iterator<Item = &TrainingPairRecord> {
        self.records.iter()
    }

    /// Drain into a Vec (consumes the log).
    #[must_use]
    pub fn drain_all(self) -> Vec<TrainingPairRecord> {
        self.records.into_iter().collect()
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ts: u64) -> TrainingPairRecord {
        TrainingPairRecord::new(
            ts,
            "p".into(),
            SelfAuthorKind::Scene,
            "c".into(),
            [0xAB; 32],
            80,
            MutateDecision::Allow,
            "t".into(),
        )
    }

    #[test]
    fn t01_push_under_capacity_no_eviction() {
        let mut log = TrainingPairLog::new(4);
        let e = log.push(rec(1));
        assert!(e.is_none());
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn t02_push_over_capacity_evicts_oldest() {
        let mut log = TrainingPairLog::new(2);
        log.push(rec(1));
        log.push(rec(2));
        let e = log.push(rec(3));
        let evicted = e.expect("oldest evicted");
        assert_eq!(evicted.ts_unix, 1);
        assert_eq!(log.len(), 2);
        let mut iter = log.iter();
        assert_eq!(iter.next().unwrap().ts_unix, 2);
        assert_eq!(iter.next().unwrap().ts_unix, 3);
    }

    #[test]
    fn t03_blake3_hex_encoded() {
        let r = rec(1);
        assert_eq!(r.source_blake3_hex.len(), 64);
        assert!(r.source_blake3_hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn t04_serialized_record_size_positive() {
        let s = serialized_record_size(&rec(1));
        assert!(s > 0);
    }

    #[test]
    fn t05_decision_round_trip_from_live_mutate() {
        let cases = [
            (LiveMutateDecision::Allow, MutateDecision::Allow),
            (
                LiveMutateDecision::DenyScoreBelowThreshold {
                    score: 1,
                    threshold: 75,
                },
                MutateDecision::DenyScoreBelowThreshold,
            ),
            (LiveMutateDecision::DenySandboxFailed, MutateDecision::DenySandboxFailed),
            (LiveMutateDecision::DenyForbiddenTarget("x".into()), MutateDecision::DenyForbiddenTarget),
            (LiveMutateDecision::DenyCapInvalid("x"), MutateDecision::DenyCapInvalid),
            (LiveMutateDecision::DenyCapRevoked, MutateDecision::DenyCapRevoked),
            (LiveMutateDecision::DenySovereignBitMissing, MutateDecision::DenySovereignBitMissing),
        ];
        for (l, m) in cases {
            assert_eq!(MutateDecision::from(&l), m);
        }
    }
}
