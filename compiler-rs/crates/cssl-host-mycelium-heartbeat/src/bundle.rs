//! § bundle — `FederationBundle` wire-blob shipped per-heartbeat
//! ════════════════════════════════════════════════════════════════════════
//!
//! § THESIS
//!   `FederationBundle` is the heartbeat wire-blob. One bundle per tick ;
//!   contains all `FederationPattern` records observed since the previous
//!   tick, plus tick-bookkeeping for replay-safety + Σ-Chain anchor.
//!   Bandwidth = O(k) per-tick, not O(N) ; the differential-style emit
//!   keeps per-peer cost at the 1 KB/min target documented in the spec.
//!
//! § ANCHOR
//!   Each bundle carries a `bundle_blake3` over (sorted-pattern-bytes ‖
//!   tick_id ‖ emitter_handle ‖ ts_bucketed). This is the Σ-Chain anchor
//!   for the federation broadcast — replay-stable + tamper-evident.

use crate::pattern::{FederationPattern, PatternError, FEDERATION_PATTERN_SIZE};
use serde::{Deserialize, Serialize};

/// § `BUNDLE_PROTOCOL_VERSION` — wire-format-version of the bundle blob.
pub const BUNDLE_PROTOCOL_VERSION: u32 = 1;

/// § `MAX_PATTERNS_PER_BUNDLE` — soft cap. Bundles exceeding this split
/// at the service-tick layer (the bundle struct itself is unbounded).
pub const MAX_PATTERNS_PER_BUNDLE: usize = 256;

/// § `FederationBundle` — the per-tick wire blob.
///
/// Serialized via serde-JSON for the heartbeat HTTP endpoint ; the JSON
/// envelope is then zstd-compressed by the `compress` module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederationBundle {
    /// Wire-format version (currently 1).
    pub protocol_version: u32,
    /// Monotonic tick-id per service-instance.
    pub tick_id: u64,
    /// Emitter pubkey-trunc (8-byte handle of the EMITTING NODE).
    pub emitter_handle: u64,
    /// Wall-clock ts of the tick (epoch seconds, post-bucketing to /60).
    pub ts_bucketed: u32,
    /// Patterns emitted in this tick (post-Σ-mask filtering).
    pub patterns: Vec<FederationPattern>,
    /// `bundle_blake3` — Σ-Chain anchor for this bundle. 32-byte hex.
    pub bundle_blake3: String,
}

/// § `BundleStats` — observability snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleStats {
    pub bundles_built: u64,
    pub patterns_in_last_bundle: u32,
    pub bytes_in_last_bundle: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("malformed pattern in bundle : {0}")]
    Malformed(#[from] PatternError),
    #[error("anchor mismatch — bundle was tampered with")]
    AnchorMismatch,
    #[error("invalid protocol version {found} (expected {expected})")]
    BadProtocolVersion { found: u32, expected: u32 },
    #[error("bundle empty (no patterns to emit)")]
    Empty,
}

impl FederationBundle {
    /// § build — assemble + anchor a bundle from the drained ring.
    /// Returns `Err(BundleError::Empty)` if `patterns` is empty (caller
    /// should skip emit on empty bundles).
    pub fn build(
        tick_id: u64,
        emitter_handle: u64,
        ts_bucketed: u32,
        mut patterns: Vec<FederationPattern>,
    ) -> Result<Self, BundleError> {
        if patterns.is_empty() {
            return Err(BundleError::Empty);
        }
        // Sort for replay-stability ; ordering by (kind, payload_hash, sig)
        // gives a total order independent of insertion-time scheduling.
        patterns.sort_by_key(|p| (p.kind() as u8, p.payload_hash(), p.sig()));

        let bundle_blake3 = compute_anchor(tick_id, emitter_handle, ts_bucketed, &patterns);

        Ok(Self {
            protocol_version: BUNDLE_PROTOCOL_VERSION,
            tick_id,
            emitter_handle,
            ts_bucketed,
            patterns,
            bundle_blake3,
        })
    }

    /// § verify_anchor — recompute and compare. Returns `true` iff stable.
    #[must_use]
    pub fn verify_anchor(&self) -> bool {
        let recomputed =
            compute_anchor(self.tick_id, self.emitter_handle, self.ts_bucketed, &self.patterns);
        recomputed == self.bundle_blake3
    }

    /// § validate — verify protocol version, anchor, and per-pattern sigs.
    /// Called at cloud-side ingest as the second-line gate.
    pub fn validate(&self) -> Result<(), BundleError> {
        if self.protocol_version != BUNDLE_PROTOCOL_VERSION {
            return Err(BundleError::BadProtocolVersion {
                found: self.protocol_version,
                expected: BUNDLE_PROTOCOL_VERSION,
            });
        }
        if !self.verify_anchor() {
            return Err(BundleError::AnchorMismatch);
        }
        for p in &self.patterns {
            p.validate()?;
        }
        Ok(())
    }

    /// § wire_size_bytes — approximate JSON wire size. Used for the
    /// 1 KB/min/peer bandwidth observability metric.
    #[must_use]
    pub fn wire_size_bytes(&self) -> usize {
        // Estimate : per-pattern 32 bytes raw + JSON-encoding overhead
        // (~40% for hex+wrapping). Header overhead ≈ 80 bytes.
        80 + self.patterns.len() * (FEDERATION_PATTERN_SIZE * 2 + 8)
    }
}

fn compute_anchor(
    tick_id: u64,
    emitter_handle: u64,
    ts_bucketed: u32,
    patterns: &[FederationPattern],
) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"federation\0bundle\0v1");
    h.update(&tick_id.to_le_bytes());
    h.update(&emitter_handle.to_le_bytes());
    h.update(&ts_bucketed.to_le_bytes());
    h.update(&(patterns.len() as u32).to_le_bytes());
    for p in patterns {
        h.update(p.as_bytes());
    }
    let bytes = h.finalize();
    let mut out = String::with_capacity(64);
    for b in bytes.as_bytes() {
        // Lowercase hex without alloc-per-byte ; 64 chars total.
        let hi = b >> 4;
        let lo = b & 0x0F;
        out.push(hex_digit(hi));
        out.push(hex_digit(lo));
    }
    out
}

const fn hex_digit(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + (n - 10)) as char,
        _ => '?',
    }
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{FederationKind, FederationPatternBuilder, CAP_FED_FLAGS_ALL};

    fn mk_pattern(seed: u8) -> FederationPattern {
        FederationPatternBuilder {
            kind: FederationKind::CellState,
            cap_flags: CAP_FED_FLAGS_ALL,
            k_anon_cohort_size: 12,
            confidence: 0.5,
            ts_unix: 60 * u64::from(seed),
            payload: vec![seed; 16],
            emitter_pubkey: [seed; 32],
        }
        .build()
        .unwrap()
    }

    #[test]
    fn build_and_verify_anchor() {
        let patterns = (1..=4).map(mk_pattern).collect::<Vec<_>>();
        let b = FederationBundle::build(42, 0xDEAD_BEEF, 1000, patterns).unwrap();
        assert_eq!(b.tick_id, 42);
        assert_eq!(b.protocol_version, BUNDLE_PROTOCOL_VERSION);
        assert!(b.verify_anchor());
        assert_eq!(b.bundle_blake3.len(), 64); // 32 bytes hex
    }

    #[test]
    fn build_empty_rejected() {
        let r = FederationBundle::build(1, 0, 0, vec![]);
        assert!(matches!(r, Err(BundleError::Empty)));
    }

    #[test]
    fn validate_passes_well_formed() {
        let patterns = (1..=3).map(mk_pattern).collect::<Vec<_>>();
        let b = FederationBundle::build(1, 0, 1, patterns).unwrap();
        assert!(b.validate().is_ok());
    }

    #[test]
    fn validate_rejects_bad_protocol_version() {
        let patterns = (1..=3).map(mk_pattern).collect::<Vec<_>>();
        let mut b = FederationBundle::build(1, 0, 1, patterns).unwrap();
        b.protocol_version = 999;
        assert!(matches!(
            b.validate(),
            Err(BundleError::BadProtocolVersion { .. })
        ));
    }

    #[test]
    fn validate_rejects_anchor_tamper() {
        let patterns = (1..=3).map(mk_pattern).collect::<Vec<_>>();
        let mut b = FederationBundle::build(1, 0, 1, patterns).unwrap();
        b.tick_id = 999; // anchor was computed against tick_id=1
        assert!(matches!(b.validate(), Err(BundleError::AnchorMismatch)));
    }

    #[test]
    fn anchor_is_replay_stable() {
        let patterns = (1..=4).map(mk_pattern).collect::<Vec<_>>();
        // Build twice with different insertion orders ; sort makes them equal.
        let mut a_in = patterns.clone();
        a_in.reverse();
        let b1 = FederationBundle::build(1, 7, 99, patterns).unwrap();
        let b2 = FederationBundle::build(1, 7, 99, a_in).unwrap();
        assert_eq!(b1.bundle_blake3, b2.bundle_blake3);
    }

    #[test]
    fn wire_size_bytes_estimates_reasonably() {
        let patterns = (1..=10).map(mk_pattern).collect::<Vec<_>>();
        let b = FederationBundle::build(1, 0, 0, patterns).unwrap();
        let est = b.wire_size_bytes();
        // 10 patterns * (32 hex-doubled = 64 + overhead) ≈ 720+, plus header.
        assert!((600..=2000).contains(&est));
    }

    #[test]
    fn json_round_trips() {
        let patterns = (1..=3).map(mk_pattern).collect::<Vec<_>>();
        let b = FederationBundle::build(7, 0xC0FFEE, 100, patterns).unwrap();
        let j = serde_json::to_string(&b).unwrap();
        let b2: FederationBundle = serde_json::from_str(&j).unwrap();
        assert_eq!(b, b2);
        assert!(b2.validate().is_ok());
    }
}
