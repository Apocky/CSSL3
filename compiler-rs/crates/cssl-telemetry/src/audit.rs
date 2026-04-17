//! Audit-chain : BLAKE3 content-hash + Ed25519-signed append chain.
//!
//! § SPEC : `specs/22_TELEMETRY.csl` + `specs/11_IFC.csl` + R18 audit-chain invariant.
//!
//! § STAGE-0
//!   Hash + signature are represented as fixed-size byte arrays ; the actual
//!   `blake3` + `ed25519-dalek` integrations land at T11-phase-2. Phase-1 produces
//!   a stable API + structural tests — the crypto is swapped in without churn.

use thiserror::Error;

/// 32-byte BLAKE3 content-hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// Zero-hash placeholder.
    #[must_use]
    pub const fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Stage-0 stub hasher : XOR-fold bytes into 32-byte output. Phase-2 swaps for
    /// real `blake3::hash`. Deterministic so tests can pin expected values.
    #[must_use]
    pub fn stub_hash(bytes: &[u8]) -> Self {
        let mut out = [0u8; 32];
        for (i, b) in bytes.iter().enumerate() {
            out[i % 32] ^= b.rotate_left(u32::try_from(i % 8).unwrap_or(0));
        }
        Self(out)
    }

    /// Hex-encode (lowercase).
    #[must_use]
    pub fn hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

/// 64-byte Ed25519 signature stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature(pub [u8; 64]);

impl Signature {
    /// Zero-signature placeholder.
    #[must_use]
    pub const fn zero() -> Self {
        Self([0u8; 64])
    }

    /// Stage-0 stub : hash the contents twice into a 64-byte pattern. Phase-2
    /// swaps for `ed25519-dalek::SigningKey::sign`.
    #[must_use]
    pub fn stub_sign(message: &[u8]) -> Self {
        let a = ContentHash::stub_hash(message).0;
        let mut doubled = Vec::with_capacity(64);
        doubled.extend_from_slice(&a);
        doubled.extend_from_slice(&a);
        let b = ContentHash::stub_hash(&doubled).0;
        let mut out = [0u8; 64];
        out[..32].copy_from_slice(&a);
        out[32..].copy_from_slice(&b);
        Self(out)
    }
}

/// One audit-chain entry : content-hash + prev-hash + signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// Monotonic sequence index in the chain.
    pub seq: u64,
    /// Unix timestamp (seconds).
    pub timestamp_s: u64,
    /// BLAKE3 hash of the message body.
    pub content_hash: ContentHash,
    /// Hash of the previous entry (zero for genesis).
    pub prev_hash: ContentHash,
    /// Ed25519 signature over (seq + timestamp + content_hash + prev_hash).
    pub signature: Signature,
    /// Short tag / category (e.g., `"power-breach"`, `"declassify"`).
    pub tag: String,
    /// Inline UTF-8 message.
    pub message: String,
}

impl AuditEntry {
    /// Build the to-be-signed byte-vector for this entry.
    #[must_use]
    pub fn sign_input(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(32 + 32 + 8 + 8 + self.tag.len() + self.message.len());
        v.extend_from_slice(&self.seq.to_le_bytes());
        v.extend_from_slice(&self.timestamp_s.to_le_bytes());
        v.extend_from_slice(&self.content_hash.0);
        v.extend_from_slice(&self.prev_hash.0);
        v.extend_from_slice(self.tag.as_bytes());
        v.push(b'|');
        v.extend_from_slice(self.message.as_bytes());
        v
    }
}

/// Audit-chain : append-only BLAKE3 hash-chain with Ed25519 signatures per entry.
#[derive(Debug, Clone, Default)]
pub struct AuditChain {
    entries: Vec<AuditEntry>,
}

impl AuditChain {
    /// New empty chain with a zero genesis-prev-hash.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an entry with the given tag + message. The content-hash is derived
    /// from the stub-hash of `message.as_bytes()` ; phase-2 uses real BLAKE3.
    pub fn append(&mut self, tag: impl Into<String>, message: impl Into<String>, timestamp_s: u64) {
        let tag = tag.into();
        let message = message.into();
        let content_hash = ContentHash::stub_hash(message.as_bytes());
        let prev_hash = self
            .entries
            .last()
            .map_or(ContentHash::zero(), |e| e.content_hash);
        let seq = self.entries.len() as u64;
        let entry_for_sign = AuditEntry {
            seq,
            timestamp_s,
            content_hash,
            prev_hash,
            signature: Signature::zero(),
            tag: tag.clone(),
            message: message.clone(),
        };
        let signature = Signature::stub_sign(&entry_for_sign.sign_input());
        self.entries.push(AuditEntry {
            seq,
            timestamp_s,
            content_hash,
            prev_hash,
            signature,
            tag,
            message,
        });
    }

    /// Entry count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Empty check.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate entries.
    pub fn iter(&self) -> impl Iterator<Item = &AuditEntry> {
        self.entries.iter()
    }

    /// Verify the chain-invariant : each entry's `prev_hash` matches the preceding
    /// entry's `content_hash`, and the genesis `prev_hash` is zero.
    ///
    /// # Errors
    /// Returns [`AuditError::GenesisPrevNonZero`] / [`AuditError::ChainBreak`] /
    /// [`AuditError::InvalidSequence`] on failure.
    pub fn verify_chain(&self) -> Result<(), AuditError> {
        for (i, e) in self.entries.iter().enumerate() {
            if e.seq != i as u64 {
                return Err(AuditError::InvalidSequence {
                    expected: i as u64,
                    actual: e.seq,
                });
            }
            if i == 0 {
                if e.prev_hash != ContentHash::zero() {
                    return Err(AuditError::GenesisPrevNonZero);
                }
            } else if e.prev_hash != self.entries[i - 1].content_hash {
                return Err(AuditError::ChainBreak { seq: e.seq });
            }
        }
        Ok(())
    }
}

/// Audit-chain failure modes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuditError {
    /// Genesis entry had a non-zero prev-hash.
    #[error("genesis entry had non-zero prev-hash")]
    GenesisPrevNonZero,
    /// Chain linkage broken at a specific sequence.
    #[error("chain-break at seq {seq} (prev-hash does not match previous entry's content-hash)")]
    ChainBreak { seq: u64 },
    /// Sequence index not monotonic.
    #[error("invalid sequence : expected {expected}, found {actual}")]
    InvalidSequence { expected: u64, actual: u64 },
}

#[cfg(test)]
mod tests {
    use super::{AuditChain, AuditError, ContentHash, Signature};

    #[test]
    fn content_hash_zero_is_all_zeroes() {
        let h = ContentHash::zero();
        assert_eq!(h.0, [0u8; 32]);
    }

    #[test]
    fn content_hash_stub_deterministic() {
        let a = ContentHash::stub_hash(b"hello");
        let b = ContentHash::stub_hash(b"hello");
        assert_eq!(a, b);
        assert_ne!(a, ContentHash::zero());
    }

    #[test]
    fn content_hash_different_inputs_different_outputs() {
        let a = ContentHash::stub_hash(b"hello");
        let b = ContentHash::stub_hash(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn content_hash_hex_is_64_chars() {
        let h = ContentHash::stub_hash(b"hi");
        let hex = h.hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn signature_stub_deterministic() {
        let a = Signature::stub_sign(b"msg");
        let b = Signature::stub_sign(b"msg");
        assert_eq!(a, b);
        assert_ne!(a, Signature::zero());
    }

    #[test]
    fn empty_chain_verifies() {
        let c = AuditChain::new();
        assert_eq!(c.len(), 0);
        assert!(c.is_empty());
        c.verify_chain().unwrap();
    }

    #[test]
    fn append_builds_sequential_chain() {
        let mut c = AuditChain::new();
        c.append("declassify", "released employee record 1", 1_000);
        c.append("power-breach", "exceeded 225 W limit", 2_000);
        c.append("audit-signed", "attestation from CI", 3_000);
        assert_eq!(c.len(), 3);
        c.verify_chain().unwrap();
    }

    #[test]
    fn chain_verify_detects_break() {
        let mut c = AuditChain::new();
        c.append("a", "first", 1);
        c.append("b", "second", 2);
        // Corrupt entry 1's prev-hash.
        c.entries[1].prev_hash = ContentHash([0xFFu8; 32]);
        let err = c.verify_chain().unwrap_err();
        assert!(matches!(err, AuditError::ChainBreak { seq: 1 }));
    }

    #[test]
    fn chain_verify_detects_bad_genesis() {
        let mut c = AuditChain::new();
        c.append("a", "first", 1);
        c.entries[0].prev_hash = ContentHash([0x01u8; 32]);
        let err = c.verify_chain().unwrap_err();
        assert_eq!(err, AuditError::GenesisPrevNonZero);
    }

    #[test]
    fn chain_verify_detects_bad_seq() {
        let mut c = AuditChain::new();
        c.append("a", "first", 1);
        c.entries[0].seq = 7;
        let err = c.verify_chain().unwrap_err();
        assert!(matches!(err, AuditError::InvalidSequence { .. }));
    }

    #[test]
    fn entry_sign_input_includes_seq_and_hash() {
        let mut c = AuditChain::new();
        c.append("t", "m", 1_234);
        let e = &c.entries[0];
        let si = e.sign_input();
        // seq 8 bytes + ts 8 bytes + content_hash 32 bytes + prev_hash 32 bytes
        // + tag 1 byte ("t") + '|' + message 1 byte ("m") = 83.
        assert_eq!(si.len(), 83);
    }
}
