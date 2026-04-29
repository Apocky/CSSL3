//! [`ErrorFingerprint`] — BLAKE3-derived dedup-key for error events.
//!
//! § SPEC : `_drafts/phase_j/05_l0_l1_error_log_spec.md` § 1.6.
//!
//! § DESIGN
//!   - Fingerprint = BLAKE3((kind || source-loc || frame-bucket)).
//!   - Frame-bucket = frame_n / 60 ; ~1-second windows @ 60fps.
//!   - Two errors with the same (kind, source-loc, frame-bucket) yield the
//!     same fingerprint ⟶ rate-limit policy can dedup them.
//!   - The 32-byte hash is stable + reproducible : same inputs always
//!     yield same fingerprint (replay-friendly).
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - § 4 TRANSPARENCY : the fingerprint algorithm is publicly documented.
//!   - § 7 INTEGRITY : the BLAKE3 derivation is collision-resistant ;
//!     fingerprints cannot be forged to suppress unrelated errors.

use core::fmt;

use crate::context::{KindId, SourceLocation};

/// BLAKE3-derived 32-byte fingerprint for error-event dedup.
///
/// § INVARIANTS
///   - Constructed only via [`ErrorFingerprint::compute`] (no public ctor
///     accepting raw bytes ; sentinel values via [`ErrorFingerprint::zero`]).
///   - Equal fingerprints ⟹ same (kind, source-loc, frame-bucket) inputs.
///   - Different kinds OR different source-locs OR different frame-buckets
///     produce different fingerprints (collision probability ≈ 2^-256).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ErrorFingerprint([u8; 32]);

/// Domain-tag prepended to BLAKE3 input to prevent cross-use of the hash.
const FINGERPRINT_DOMAIN: &[u8] = b"cssl-error-fingerprint-v1";

impl ErrorFingerprint {
    /// Sentinel "zero" fingerprint ; reserved for "no fingerprint computed".
    /// Never produced by [`Self::compute`] (BLAKE3 never returns all-zeros
    /// for non-empty input).
    #[must_use]
    pub const fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Compute the fingerprint from (kind, source-loc, frame-bucket).
    ///
    /// § INPUT-ENCODING (deterministic)
    ///   - 8 bytes : domain-tag length prefix
    ///   - N bytes : domain-tag
    ///   - 4 bytes : kind.as_u32() little-endian
    ///   - 32 bytes : source.file_path_hash bytes
    ///   - 4 bytes : source.line little-endian
    ///   - 4 bytes : source.column little-endian
    ///   - 8 bytes : frame_bucket little-endian
    #[must_use]
    pub fn compute(kind: KindId, source: &SourceLocation, frame_bucket: u64) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(&(FINGERPRINT_DOMAIN.len() as u64).to_le_bytes());
        h.update(FINGERPRINT_DOMAIN);
        h.update(&kind.as_u32().to_le_bytes());
        h.update(&source.file_path_hash.0);
        h.update(&source.line.to_le_bytes());
        h.update(&source.column.to_le_bytes());
        h.update(&frame_bucket.to_le_bytes());
        let out = h.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(out.as_bytes());
        Self(bytes)
    }

    /// Get the 32 bytes of the fingerprint.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Construct from raw bytes. Used by deserializers ; prefer
    /// [`Self::compute`] for live captures.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Short (16-hex) form for terse displays + audit-entry messages.
    #[must_use]
    pub fn short_hex(&self) -> String {
        let mut s = String::with_capacity(19);
        for b in &self.0[..8] {
            s.push_str(&format!("{b:02x}"));
        }
        s.push_str("...");
        s
    }

    /// Full (64-hex) form. Use sparingly ; the short-form is what audit
    /// records carry.
    #[must_use]
    pub fn hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    /// Returns `true` iff this is the zero sentinel.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }

    /// Compute the frame-bucket for a given frame_n (1-second @ 60fps).
    /// Spec § 1.6 : `frame_bucket = frame_n / 60`.
    #[must_use]
    pub const fn frame_bucket_for(frame_n: u64) -> u64 {
        frame_n / 60
    }
}

impl fmt::Display for ErrorFingerprint {
    /// Display = SHORT-form (19 chars) ; matches audit-entry format.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.short_hex())
    }
}

impl Default for ErrorFingerprint {
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorFingerprint;
    use crate::context::{KindId, SourceLocation};
    use cssl_telemetry::PathHasher;

    fn ph() -> PathHasher {
        PathHasher::from_seed([4u8; 32])
    }

    #[test]
    fn zero_sentinel_is_all_zeros() {
        let z = ErrorFingerprint::zero();
        assert!(z.is_zero());
        assert_eq!(z.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn compute_is_deterministic() {
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 12, 3);
        let fp_a = ErrorFingerprint::compute(KindId::new(7), &loc, 0);
        let fp_b = ErrorFingerprint::compute(KindId::new(7), &loc, 0);
        assert_eq!(fp_a, fp_b);
    }

    #[test]
    fn compute_differs_on_kind() {
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 12, 3);
        let fp_a = ErrorFingerprint::compute(KindId::new(7), &loc, 0);
        let fp_b = ErrorFingerprint::compute(KindId::new(8), &loc, 0);
        assert_ne!(fp_a, fp_b);
    }

    #[test]
    fn compute_differs_on_source_loc() {
        let p = ph().hash_str("/src/file.rs");
        let loc_a = SourceLocation::new(p, 12, 3);
        let loc_b = SourceLocation::new(p, 13, 3);
        let fp_a = ErrorFingerprint::compute(KindId::new(7), &loc_a, 0);
        let fp_b = ErrorFingerprint::compute(KindId::new(7), &loc_b, 0);
        assert_ne!(fp_a, fp_b);
    }

    #[test]
    fn compute_differs_on_column() {
        let p = ph().hash_str("/src/file.rs");
        let loc_a = SourceLocation::new(p, 12, 3);
        let loc_b = SourceLocation::new(p, 12, 4);
        let fp_a = ErrorFingerprint::compute(KindId::new(7), &loc_a, 0);
        let fp_b = ErrorFingerprint::compute(KindId::new(7), &loc_b, 0);
        assert_ne!(fp_a, fp_b);
    }

    #[test]
    fn compute_differs_on_path_hash() {
        let p1 = ph().hash_str("/src/file_a.rs");
        let p2 = ph().hash_str("/src/file_b.rs");
        let loc_a = SourceLocation::new(p1, 12, 3);
        let loc_b = SourceLocation::new(p2, 12, 3);
        let fp_a = ErrorFingerprint::compute(KindId::new(7), &loc_a, 0);
        let fp_b = ErrorFingerprint::compute(KindId::new(7), &loc_b, 0);
        assert_ne!(fp_a, fp_b);
    }

    #[test]
    fn compute_differs_on_frame_bucket() {
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 12, 3);
        let fp_a = ErrorFingerprint::compute(KindId::new(7), &loc, 0);
        let fp_b = ErrorFingerprint::compute(KindId::new(7), &loc, 1);
        assert_ne!(fp_a, fp_b);
    }

    #[test]
    fn compute_is_not_zero_for_nonempty_input() {
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 1, 1);
        let fp = ErrorFingerprint::compute(KindId::new(0), &loc, 0);
        assert!(!fp.is_zero());
    }

    #[test]
    fn frame_bucket_for_60fps_one_second() {
        assert_eq!(ErrorFingerprint::frame_bucket_for(0), 0);
        assert_eq!(ErrorFingerprint::frame_bucket_for(59), 0);
        assert_eq!(ErrorFingerprint::frame_bucket_for(60), 1);
        assert_eq!(ErrorFingerprint::frame_bucket_for(120), 2);
        assert_eq!(ErrorFingerprint::frame_bucket_for(3599), 59);
        assert_eq!(ErrorFingerprint::frame_bucket_for(3600), 60);
    }

    #[test]
    fn short_hex_is_19_chars() {
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 1, 1);
        let fp = ErrorFingerprint::compute(KindId::new(1), &loc, 0);
        let s = fp.short_hex();
        // 8 bytes * 2 hex chars + "..." = 19.
        assert_eq!(s.len(), 19);
        assert!(s.ends_with("..."));
    }

    #[test]
    fn full_hex_is_64_chars() {
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 1, 1);
        let fp = ErrorFingerprint::compute(KindId::new(1), &loc, 0);
        let s = fp.hex();
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn display_uses_short_form() {
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 1, 1);
        let fp = ErrorFingerprint::compute(KindId::new(1), &loc, 0);
        let s = format!("{fp}");
        assert_eq!(s.len(), 19);
        assert!(s.ends_with("..."));
    }

    #[test]
    fn from_bytes_round_trip() {
        let bytes = [0xAB; 32];
        let fp = ErrorFingerprint::from_bytes(bytes);
        assert_eq!(fp.as_bytes(), &bytes);
    }

    #[test]
    fn default_is_zero() {
        let fp = ErrorFingerprint::default();
        assert!(fp.is_zero());
    }

    #[test]
    fn dedup_within_same_bucket() {
        // 60 frames within bucket-0 should all dedup to the same fingerprint.
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 5, 5);
        let mut all = std::collections::HashSet::new();
        for f in 0..60 {
            let bucket = ErrorFingerprint::frame_bucket_for(f);
            all.insert(ErrorFingerprint::compute(KindId::new(1), &loc, bucket));
        }
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn dedup_across_distinct_buckets() {
        // 600 frames span 10 buckets ⟶ 10 distinct fingerprints.
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 5, 5);
        let mut all = std::collections::HashSet::new();
        for f in 0..600 {
            let bucket = ErrorFingerprint::frame_bucket_for(f);
            all.insert(ErrorFingerprint::compute(KindId::new(1), &loc, bucket));
        }
        assert_eq!(all.len(), 10);
    }

    #[test]
    fn different_kinds_yield_distinct_fingerprints() {
        let p = ph().hash_str("/src/file.rs");
        let loc = SourceLocation::new(p, 1, 1);
        let mut seen = std::collections::HashSet::new();
        for k in 0..256 {
            seen.insert(ErrorFingerprint::compute(KindId::new(k), &loc, 0));
        }
        assert_eq!(seen.len(), 256);
    }
}
