//! Schema version + digest.

use core::fmt;

/// Monotonic schema version identifier.
///
/// The `digest` is a 32-byte BLAKE3 hash of the canonical schema representation
/// (stage-0 uses a stub hash ; phase-2 swaps for real `blake3::hash`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SchemaVersion {
    /// Monotonic major-version (bump on incompatible changes).
    pub major: u32,
    /// Monotonic minor-version (bump on backward-compatible changes).
    pub minor: u32,
    /// 32-byte schema-digest.
    pub digest: [u8; 32],
}

impl SchemaVersion {
    /// New version with all-zero digest.
    #[must_use]
    pub const fn new(major: u32, minor: u32) -> Self {
        Self {
            major,
            minor,
            digest: [0u8; 32],
        }
    }

    /// Genesis version : 1.0 / zero-digest.
    #[must_use]
    pub const fn genesis() -> Self {
        Self::new(1, 0)
    }

    /// Stage-0 stub-hash of `canonical_bytes`. Phase-2 swaps for BLAKE3.
    #[must_use]
    pub fn with_digest_from(mut self, canonical_bytes: &[u8]) -> Self {
        for (i, b) in canonical_bytes.iter().enumerate() {
            self.digest[i % 32] ^= b.rotate_left(u32::try_from(i % 8).unwrap_or(0));
        }
        self
    }

    /// True iff `other` is a backward-compatible successor (same major, strictly-greater minor).
    #[must_use]
    pub fn is_minor_upgrade_of(self, other: Self) -> bool {
        self.major == other.major && self.minor > other.minor
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::SchemaVersion;

    #[test]
    fn new_has_zero_digest() {
        let v = SchemaVersion::new(2, 3);
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 3);
        assert_eq!(v.digest, [0u8; 32]);
    }

    #[test]
    fn genesis_is_1_0() {
        let v = SchemaVersion::genesis();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
    }

    #[test]
    fn with_digest_stable_for_same_input() {
        let a = SchemaVersion::new(1, 0).with_digest_from(b"schema-A");
        let b = SchemaVersion::new(1, 0).with_digest_from(b"schema-A");
        assert_eq!(a.digest, b.digest);
    }

    #[test]
    fn with_digest_distinguishes_inputs() {
        let a = SchemaVersion::new(1, 0).with_digest_from(b"schema-A");
        let b = SchemaVersion::new(1, 0).with_digest_from(b"schema-B");
        assert_ne!(a.digest, b.digest);
    }

    #[test]
    fn minor_upgrade_detected() {
        let v1_0 = SchemaVersion::new(1, 0);
        let v1_3 = SchemaVersion::new(1, 3);
        let v2_0 = SchemaVersion::new(2, 0);
        assert!(v1_3.is_minor_upgrade_of(v1_0));
        assert!(!v2_0.is_minor_upgrade_of(v1_0)); // major diff → not minor
        assert!(!v1_0.is_minor_upgrade_of(v1_3)); // older → not upgrade
    }

    #[test]
    fn display_format() {
        assert_eq!(format!("{}", SchemaVersion::new(2, 4)), "2.4");
    }
}
