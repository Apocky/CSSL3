//! R16 C99-anchor reproducibility-attestation hook.
//!
//! § SPEC : `specs/01_BOOTSTRAP.csl` § REPRODUCIBILITY + §§ SYNTHESIS_V2 R16.
//! § GATE : T30 (OG10) ship-gate — C99-compiled stage3 ≡ CSSLv3-compiled stage1 bit-exact.
//! § CHAIN: attestation signed by Apocky-key + CI-key chain per `specs/01_BOOTSTRAP.csl`.
//! § STATUS : T11-phase-2b live (canonical-serialization + BLAKE3/Ed25519 sign + verify
//!            helpers) ; real stage3 rebuild-pipeline still pending stage3 entry.

use cssl_telemetry::audit::{ContentHash, Signature, SigningKey};

/// Single attestation record, Ed25519-signed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attestation {
    /// Semver of the compiler build that produced this attestation.
    pub compiler_version: String,
    /// Git SHA of the source commit (empty in stage0 / non-git contexts).
    pub source_commit: String,
    /// BLAKE3 hash of the emitted C99 tarball.
    pub c99_tarball_blake3: String,
    /// BLAKE3 hash of the stage1 (self-hosted) compiler binary.
    pub stage1_blake3: String,
    /// Ed25519 signature over the canonical serialization of the above fields.
    pub signature: Vec<u8>,
}

impl Attestation {
    /// Canonical byte serialization used as the sign-input.
    /// Format : `compiler_version|source_commit|c99_tarball_blake3|stage1_blake3`
    /// with literal `|` separators ; UTF-8 encoded.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(self.compiler_version.as_bytes());
        v.push(b'|');
        v.extend_from_slice(self.source_commit.as_bytes());
        v.push(b'|');
        v.extend_from_slice(self.c99_tarball_blake3.as_bytes());
        v.push(b'|');
        v.extend_from_slice(self.stage1_blake3.as_bytes());
        v
    }

    /// Build an attestation from raw fields + a signing-key. The signature is a
    /// real Ed25519 signature over `canonical_bytes()`.
    #[must_use]
    pub fn build_signed(
        compiler_version: impl Into<String>,
        source_commit: impl Into<String>,
        c99_tarball_blake3: impl Into<String>,
        stage1_blake3: impl Into<String>,
        key: &SigningKey,
    ) -> Self {
        let mut record = Self {
            compiler_version: compiler_version.into(),
            source_commit: source_commit.into(),
            c99_tarball_blake3: c99_tarball_blake3.into(),
            stage1_blake3: stage1_blake3.into(),
            signature: Vec::new(),
        };
        let sig = Signature::sign(key, &record.canonical_bytes());
        record.signature = sig.0.to_vec();
        record
    }

    /// Verify the attestation's signature against `key`'s verifying-half.
    /// Returns `true` iff the signature is valid for `canonical_bytes()`.
    #[must_use]
    pub fn verify(&self, key: &SigningKey) -> bool {
        if self.signature.len() != 64 {
            return false;
        }
        let mut bytes = [0u8; 64];
        bytes.copy_from_slice(&self.signature);
        let sig = Signature(bytes);
        key.verify(&self.canonical_bytes(), &sig).is_ok()
    }

    /// Compute the BLAKE3 content-hash of the canonical-bytes. Used as a
    /// compact identifier when printing attestations.
    #[must_use]
    pub fn content_hash(&self) -> ContentHash {
        ContentHash::hash(&self.canonical_bytes())
    }
}

/// Outcome of running the R16 attestation pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Stage0 stub — stage3 infrastructure pending.
    Stage0Unimplemented,
    /// C99 rebuild produced byte-exact stage1 binary; attestation signed.
    Attested { record: Attestation },
    /// Rebuild diverged; attestation refused.
    Diverged {
        expected_blake3: String,
        actual_blake3: String,
    },
    /// Attestation key material unavailable in this context (dev workstation sans keys).
    NoSigningKey,
}

/// Attester trait — stage3 implementation drives the rebuild + signing pipeline.
pub trait Attester {
    /// Emit a C99 tarball, rebuild stage1, compare byte-for-byte, sign the attestation.
    fn attest(&self) -> Outcome;
}

/// Stage0 stub attester — always returns `Stage0Unimplemented`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Stage0Stub;

impl Attester for Stage0Stub {
    fn attest(&self) -> Outcome {
        Outcome::Stage0Unimplemented
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § Decision-helper : compares BLAKE3-hashes of a claimed-rebuild vs target.
// ─────────────────────────────────────────────────────────────────────────

/// Decide the attestation outcome from the raw inputs. Given an `expected`
/// hash (the stage1 CSSLv3-emitted binary) and an `actual` hash (the C99
/// rebuild), returns `Attested { record }` if they match + a signing-key is
/// present, `Diverged` if they differ, `NoSigningKey` if the key is None.
#[must_use]
pub fn decide_attestation(
    expected_blake3: &str,
    actual_blake3: &str,
    compiler_version: &str,
    source_commit: &str,
    signing_key: Option<&SigningKey>,
) -> Outcome {
    if expected_blake3 != actual_blake3 {
        return Outcome::Diverged {
            expected_blake3: expected_blake3.to_string(),
            actual_blake3: actual_blake3.to_string(),
        };
    }
    let Some(key) = signing_key else {
        return Outcome::NoSigningKey;
    };
    // On match : "c99-tarball-hash" and "stage1-hash" are both equal to expected_blake3
    // from the R16-anchor perspective — the tarball rebuilds to the same binary.
    let record = Attestation::build_signed(
        compiler_version,
        source_commit,
        expected_blake3,
        expected_blake3,
        key,
    );
    Outcome::Attested { record }
}

#[cfg(test)]
mod tests {
    use super::{decide_attestation, Attestation, Attester, Outcome, Stage0Stub};
    use cssl_telemetry::audit::SigningKey;

    #[test]
    fn stub_returns_unimplemented() {
        assert_eq!(Stage0Stub.attest(), Outcome::Stage0Unimplemented);
    }

    #[test]
    fn canonical_bytes_has_expected_shape() {
        let a = Attestation {
            compiler_version: "1.0.0".into(),
            source_commit: "deadbeef".into(),
            c99_tarball_blake3: "hash-a".into(),
            stage1_blake3: "hash-b".into(),
            signature: Vec::new(),
        };
        let bytes = a.canonical_bytes();
        let as_str = String::from_utf8(bytes).unwrap();
        assert_eq!(as_str, "1.0.0|deadbeef|hash-a|hash-b");
    }

    #[test]
    fn sign_then_verify_roundtrip() {
        let key = SigningKey::from_seed([42u8; 32]);
        let record =
            Attestation::build_signed("1.0.0", "abc123", "tarball-hash", "stage1-hash", &key);
        assert!(record.verify(&key));
    }

    #[test]
    fn signature_tampered_fails_verify() {
        let key = SigningKey::from_seed([42u8; 32]);
        let mut record =
            Attestation::build_signed("1.0.0", "abc123", "tarball-hash", "stage1-hash", &key);
        record.signature[0] ^= 0xff; // flip a bit
        assert!(!record.verify(&key));
    }

    #[test]
    fn content_hash_is_deterministic() {
        let a = Attestation {
            compiler_version: "1.0.0".into(),
            source_commit: "deadbeef".into(),
            c99_tarball_blake3: "hash-a".into(),
            stage1_blake3: "hash-b".into(),
            signature: Vec::new(),
        };
        let h1 = a.content_hash();
        let h2 = a.content_hash();
        assert_eq!(h1, h2);
        // Non-zero : real BLAKE3 on non-empty input is vanishingly unlikely to be zero.
        assert_ne!(h1.0, [0u8; 32]);
    }

    #[test]
    fn decide_attestation_matching_hashes_produces_attested() {
        let key = SigningKey::from_seed([7u8; 32]);
        let outcome = decide_attestation("hash-X", "hash-X", "1.0.0", "abc123", Some(&key));
        match outcome {
            Outcome::Attested { record } => {
                assert_eq!(record.c99_tarball_blake3, "hash-X");
                assert_eq!(record.stage1_blake3, "hash-X");
                assert!(record.verify(&key));
            }
            other => panic!("expected Attested, got {other:?}"),
        }
    }

    #[test]
    fn decide_attestation_divergent_hashes_produces_diverged() {
        let key = SigningKey::from_seed([7u8; 32]);
        let outcome = decide_attestation("hash-A", "hash-B", "1.0.0", "abc123", Some(&key));
        match outcome {
            Outcome::Diverged {
                expected_blake3,
                actual_blake3,
            } => {
                assert_eq!(expected_blake3, "hash-A");
                assert_eq!(actual_blake3, "hash-B");
            }
            other => panic!("expected Diverged, got {other:?}"),
        }
    }

    #[test]
    fn decide_attestation_no_key_produces_no_signing_key() {
        let outcome = decide_attestation("hash-X", "hash-X", "1.0.0", "abc123", None);
        assert_eq!(outcome, Outcome::NoSigningKey);
    }

    #[test]
    fn cross_key_signature_fails_verify() {
        let key_a = SigningKey::from_seed([1u8; 32]);
        let key_b = SigningKey::from_seed([2u8; 32]);
        let record =
            Attestation::build_signed("1.0.0", "abc", "tarball-hash", "stage1-hash", &key_a);
        assert!(record.verify(&key_a));
        assert!(!record.verify(&key_b));
    }
}
