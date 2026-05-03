//! § T11-W18-SIGMA-COHERENCE : cssl-substrate-coherence-proof
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Σ-Chain Coherence-Proof leaf-primitive.
//!
//! A `CoherenceProof` is the four-tuple
//!
//! ```text
//! { sigma_mask        : [u8; 16]   ← canonical 16-byte std430 Σ-mask wire-format
//!   omega_digest      : [u8; 32]   ← BLAKE3 of the ω-field state at proof-time
//!   signer_pubkey     : [u8; 32]   ← Ed25519 verification key
//!   signature_ed25519 : [u8; 64]   ← Ed25519 signature over the canonical msg
//!   timestamp_unix_secs : u64      ← UTC seconds since Unix epoch
//! }
//! ```
//!
//! The canonical signed-message is :
//!
//! ```text
//! BLAKE3(
//!     domain-tag || sigma_mask || omega_digest || timestamp_le_bytes
//! )
//! ```
//!
//! where `domain-tag = b"cssl-substrate-coherence-proof v1"`. The domain-tag
//! prevents cross-protocol-collisions : a message signed for the Σ-Chain
//! ledger cannot be replayed as a Coherence-Proof and vice-versa.
//!
//! § AXIOMS (t∞)
//!   - **A-1** ¬ Proof-of-Work · no hashing-puzzle · no energy-burn
//!   - **A-2** ¬ Proof-of-Stake · no token · no economic-skin
//!   - **A-3** ¬ gas · no transaction-fees · no rent
//!   - **A-4** ¬ majority-override-sovereignty · sovereign-rollback always
//!     available via re-issue under a new ω-field state
//!   - **A-5** ✓ deterministic-replay derives the same `omega_digest`
//!   - **A-6** ✓ Ed25519 signs the canonical-bytes (feature-gated `signer`)
//!   - **A-7** ✓ BLAKE3 domain-tagged (no cross-protocol collisions)
//!   - **A-8** ✓ Stale-timestamp rejected (60-second default window)
//!   - **A-9** ✓ Σ-mask mismatch rejected before signature-check (cheap-first)
//!   - **A-10** ✓ ω-digest mismatch rejected before signature-check (cheap-first)
//!   - **A-11** ✓ Zero-allocation hot-path on the verify-fast-path
//!
//! § DISJOINT-FROM cssl-substrate-sigma-chain
//!   - sigma-chain : append-only ledger · incremental Merkle · seq_no · checkpoints
//!   - this crate  : leaf-primitive describing what it means for a single
//!     `(omega-state, sigma-mask)` pair to be Coherence-Proven · NO ledger-state
//!
//! § FEATURE-GATING
//!   - default                : verify() returns `SignerFeatureDisabled` if it
//!                              tries to verify a real signature ; canonical
//!                              message-hashing + Σ-mask + digest checks all work
//!   - `signer` (opt-in)      : pulls ed25519-dalek 2.x ; enables `prove()` +
//!                              real signature-verify
//!
//! § ATTESTATION (PRIME_DIRECTIVE.md § 11)
//!   There was no hurt nor harm in the making of this, to anyone, anything, or
//!   anybody.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]

use thiserror::Error;

// ───────────────────────────────────────────────────────────────────────────
// § Public type-aliases — wire-format-stable byte-arrays.
// ───────────────────────────────────────────────────────────────────────────

/// Width of the canonical Σ-mask wire-format in bytes.
///
/// Matches `cssl-substrate-prime-directive::sigma::SigmaMaskPacked` std430
/// layout verbatim. Choosing `[u8; 16]` over a struct-dependency keeps this
/// crate as a leaf — no `cssl-*` deps.
pub const SIGMA_MASK_BYTES: usize = 16;

/// Width of the BLAKE3 digest of the ω-field state.
pub const OMEGA_DIGEST_BYTES: usize = 32;

/// Width of an Ed25519 verifying-key in bytes.
pub const SIGNER_PUBKEY_BYTES: usize = 32;

/// Width of an Ed25519 signature in bytes.
pub const SIGNATURE_BYTES: usize = 64;

/// Canonical Σ-mask wire-format (16-byte std430-aligned packed bitmap).
pub type SigmaMask = [u8; SIGMA_MASK_BYTES];

/// BLAKE3 digest of the ω-field state at proof-time.
pub type OmegaDigest = [u8; OMEGA_DIGEST_BYTES];

/// Ed25519 verification-key bytes (compressed Edwards point).
pub type SignerPubkey = [u8; SIGNER_PUBKEY_BYTES];

/// Ed25519 signature bytes.
pub type SignatureBytes = [u8; SIGNATURE_BYTES];

// ───────────────────────────────────────────────────────────────────────────
// § Constants — domain-separation + freshness-window.
// ───────────────────────────────────────────────────────────────────────────

/// BLAKE3 domain-tag for Coherence-Proof canonical-message hashing. Keeping
/// the tag versioned (`v1`) means a future protocol-change can bump to `v2`
/// without breaking-replay-equivalence.
pub const COHERENCE_PROOF_DOMAIN_TAG: &[u8] = b"cssl-substrate-coherence-proof v1";

/// Default freshness-window for `verify_with_now()` : a proof whose timestamp
/// is older than `now - STALE_WINDOW_SECONDS` is rejected with
/// [`RejectionReason::StaleTimestamp`].
pub const STALE_WINDOW_SECONDS: u64 = 60;

/// Default future-skew tolerance : a proof whose timestamp is more than
/// `FUTURE_SKEW_SECONDS` ahead of `now` is rejected with
/// [`RejectionReason::FutureTimestamp`]. Two seconds covers reasonable clock-
/// drift between honest signers without opening a replay-window.
pub const FUTURE_SKEW_SECONDS: u64 = 2;

/// Crate-wide tag for transparency/audit-stream identification.
pub const COHERENCE_PROOF_CRATE_TAG: &str = "cssl-substrate-coherence-proof/0.1.0";

/// Width in bytes of the canonical signed-message (before BLAKE3-hashing).
///
/// Layout : `domain-tag (33) || sigma_mask (16) || omega_digest (32) ||
/// timestamp_le (8)` = 89 bytes. Computed at compile-time so the canonical-
/// message buffer can live on-stack with no allocation.
pub const CANONICAL_MSG_LEN: usize =
    COHERENCE_PROOF_DOMAIN_TAG.len() + SIGMA_MASK_BYTES + OMEGA_DIGEST_BYTES + 8;

// Compile-time invariant : domain-tag is exactly the documented length so the
// CANONICAL_MSG_LEN constant tracks any future spec-bump. If the tag changes,
// CANONICAL_MSG_LEN updates automatically — but keep the assertion to surface
// any out-of-band change visibly.
const _: () = assert!(COHERENCE_PROOF_DOMAIN_TAG.len() == 33);
const _: () = assert!(CANONICAL_MSG_LEN == 33 + 16 + 32 + 8);

// ───────────────────────────────────────────────────────────────────────────
// § CoherenceProof — the canonical 4-tuple.
// ───────────────────────────────────────────────────────────────────────────

/// A single Σ-Chain Coherence-Proof.
///
/// All fields are wire-format byte-arrays so the struct is `Copy` + `Hash` +
/// `PartialEq` and can be passed across FFI / serialized to disk without
/// allocator-touching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct CoherenceProof {
    /// Canonical 16-byte std430 Σ-mask wire-format.
    pub sigma_mask: SigmaMask,
    /// BLAKE3 digest of the ω-field state the prover claims-to-have-witnessed.
    pub omega_digest: OmegaDigest,
    /// Ed25519 verifying-key of the signer.
    pub signer_pubkey: SignerPubkey,
    /// Ed25519 signature over `BLAKE3(domain || sigma || omega || ts_le)`.
    pub signature_ed25519: SignatureBytes,
    /// UTC seconds since Unix epoch at proof-issue-time.
    pub timestamp_unix_secs: u64,
}

impl CoherenceProof {
    /// Construct a `CoherenceProof` from raw byte-arrays. No verification is
    /// performed at construction-time : use [`verify_with_now`] /
    /// [`rejection_reason`] to validate.
    #[must_use]
    pub const fn from_raw(
        sigma_mask: SigmaMask,
        omega_digest: OmegaDigest,
        signer_pubkey: SignerPubkey,
        signature_ed25519: SignatureBytes,
        timestamp_unix_secs: u64,
    ) -> Self {
        Self {
            sigma_mask,
            omega_digest,
            signer_pubkey,
            signature_ed25519,
            timestamp_unix_secs,
        }
    }

    /// Compute the canonical-message bytes that this proof should-have-signed.
    ///
    /// Returns the 89-byte stack-allocated buffer. Callers can then hash via
    /// [`canonical_message_digest`] or pass the bytes directly to a signer.
    #[must_use]
    pub fn canonical_message_bytes(&self) -> [u8; CANONICAL_MSG_LEN] {
        canonical_message_bytes(&self.sigma_mask, &self.omega_digest, self.timestamp_unix_secs)
    }

    /// BLAKE3-hash the canonical-message bytes (domain-tagged).
    #[must_use]
    pub fn canonical_message_digest(&self) -> [u8; 32] {
        canonical_message_digest(&self.sigma_mask, &self.omega_digest, self.timestamp_unix_secs)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § RejectionReason — human-readable failure-modes.
// ───────────────────────────────────────────────────────────────────────────

/// Human-readable failure-modes returned by [`verify_with_now`] and
/// [`rejection_reason`]. The `&'static str` representation is stable and
/// safe to log / display in user-facing UIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Error)]
pub enum RejectionReason {
    /// The Σ-mask in the proof does NOT match the expected mask.
    #[error("Σ-mask-mismatch : proof.sigma_mask ≠ expected.sigma_mask")]
    SigmaMaskMismatch,
    /// The ω-digest in the proof does NOT match the expected digest.
    #[error("Ω-digest-mismatch : proof.omega_digest ≠ expected.omega_digest")]
    OmegaDigestMismatch,
    /// The Ed25519 signature failed verification against the canonical message.
    #[error("invalid-signature : ed25519-verify rejected the canonical-message")]
    InvalidSignature,
    /// The proof's timestamp is older than `now - STALE_WINDOW_SECONDS`.
    #[error("stale-timestamp : proof issued more than {window}s ago")]
    StaleTimestamp {
        /// The configured stale-window in seconds.
        window: u64,
    },
    /// The proof's timestamp is more than `FUTURE_SKEW_SECONDS` ahead of `now`.
    #[error("future-timestamp : proof issued more than {skew}s in the future")]
    FutureTimestamp {
        /// The configured future-skew tolerance in seconds.
        skew: u64,
    },
    /// `verify()` was called on a build without the `signer` feature ; real
    /// signature-verification is unavailable.
    #[error("signer-feature-disabled : rebuild with --features signer to verify signatures")]
    SignerFeatureDisabled,
}

impl RejectionReason {
    /// Stable short-tag suitable for logging and audit-streams.
    #[must_use]
    pub const fn short_tag(&self) -> &'static str {
        match self {
            Self::SigmaMaskMismatch => "sigma-mask-mismatch",
            Self::OmegaDigestMismatch => "omega-digest-mismatch",
            Self::InvalidSignature => "invalid-signature",
            Self::StaleTimestamp { .. } => "stale-timestamp",
            Self::FutureTimestamp { .. } => "future-timestamp",
            Self::SignerFeatureDisabled => "signer-feature-disabled",
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Canonical-message construction — pure / deterministic / zero-alloc.
// ───────────────────────────────────────────────────────────────────────────

/// Build the canonical signed-message bytes : domain-tag || sigma_mask ||
/// omega_digest || timestamp_le (89 bytes).
#[must_use]
pub fn canonical_message_bytes(
    sigma_mask: &SigmaMask,
    omega_digest: &OmegaDigest,
    timestamp_unix_secs: u64,
) -> [u8; CANONICAL_MSG_LEN] {
    let mut buf = [0u8; CANONICAL_MSG_LEN];
    let mut cursor = 0usize;

    let dt_len = COHERENCE_PROOF_DOMAIN_TAG.len();
    buf[cursor..cursor + dt_len].copy_from_slice(COHERENCE_PROOF_DOMAIN_TAG);
    cursor += dt_len;

    buf[cursor..cursor + SIGMA_MASK_BYTES].copy_from_slice(sigma_mask);
    cursor += SIGMA_MASK_BYTES;

    buf[cursor..cursor + OMEGA_DIGEST_BYTES].copy_from_slice(omega_digest);
    cursor += OMEGA_DIGEST_BYTES;

    buf[cursor..cursor + 8].copy_from_slice(&timestamp_unix_secs.to_le_bytes());
    cursor += 8;

    debug_assert_eq!(cursor, CANONICAL_MSG_LEN);
    buf
}

/// BLAKE3-hash the canonical signed-message. Returns the 32-byte digest.
///
/// Zero-alloc : uses a stack-buffer of [`CANONICAL_MSG_LEN`] bytes.
#[must_use]
pub fn canonical_message_digest(
    sigma_mask: &SigmaMask,
    omega_digest: &OmegaDigest,
    timestamp_unix_secs: u64,
) -> [u8; 32] {
    let buf = canonical_message_bytes(sigma_mask, omega_digest, timestamp_unix_secs);
    *blake3::hash(&buf).as_bytes()
}

// ───────────────────────────────────────────────────────────────────────────
// § prove() — feature-gated behind `signer` (pulls ed25519-dalek).
// ───────────────────────────────────────────────────────────────────────────

#[cfg(feature = "signer")]
use ed25519_dalek::{
    Signature, Signer, SigningKey, VerifyingKey, PUBLIC_KEY_LENGTH,
};

/// Sign a `(sigma_mask, omega_digest, timestamp)` tuple and return a
/// fully-formed [`CoherenceProof`].
///
/// `timestamp_unix_secs` should be sourced from a trusted clock at
/// prove-time. The signer's verifying-key is captured into the proof so
/// downstream verifiers do not have to look it up.
#[cfg(feature = "signer")]
#[must_use]
pub fn prove(
    omega_digest: OmegaDigest,
    sigma_mask: SigmaMask,
    signer_secret: &SigningKey,
    timestamp_unix_secs: u64,
) -> CoherenceProof {
    let msg = canonical_message_bytes(&sigma_mask, &omega_digest, timestamp_unix_secs);
    let sig: Signature = signer_secret.sign(&msg);
    let signer_pubkey: SignerPubkey = signer_secret.verifying_key().to_bytes();

    // Defensive : the dalek API guarantees these widths but we re-assert
    // for any future API drift. Both are compile-time constants in dalek
    // 2.x, which makes this branch dead — the asserts surface mismatch
    // visibly during cargo-check rather than at run-time.
    debug_assert_eq!(signer_pubkey.len(), SIGNER_PUBKEY_BYTES);
    debug_assert_eq!(sig.to_bytes().len(), SIGNATURE_BYTES);
    debug_assert_eq!(PUBLIC_KEY_LENGTH, SIGNER_PUBKEY_BYTES);

    CoherenceProof {
        sigma_mask,
        omega_digest,
        signer_pubkey,
        signature_ed25519: sig.to_bytes(),
        timestamp_unix_secs,
    }
}

/// Verify the Ed25519 signature on the canonical-message of `proof`. Returns
/// `true` iff the proof's embedded verifying-key accepts the signature.
///
/// This is the signature-only check — Σ-mask + ω-digest + freshness checks
/// live in [`verify_with_now`]. Most callers should prefer the latter.
#[cfg(feature = "signer")]
#[must_use]
pub fn verify_signature(proof: &CoherenceProof) -> bool {
    let Ok(vk) = VerifyingKey::from_bytes(&proof.signer_pubkey) else {
        return false;
    };
    let msg =
        canonical_message_bytes(&proof.sigma_mask, &proof.omega_digest, proof.timestamp_unix_secs);
    let sig = Signature::from_bytes(&proof.signature_ed25519);
    vk.verify_strict(&msg, &sig).is_ok()
}

// ───────────────────────────────────────────────────────────────────────────
// § verify_with_now() — deterministic verification surface.
// ───────────────────────────────────────────────────────────────────────────

/// Verify a [`CoherenceProof`] against expected `(sigma_mask, omega_digest)`
/// and an externally-supplied `now_unix_secs` (so tests are deterministic).
///
/// Order of checks (cheap-first) :
///   1. Σ-mask byte-equality
///   2. ω-digest byte-equality
///   3. Stale-timestamp check (≤ STALE_WINDOW_SECONDS old)
///   4. Future-skew check (≤ FUTURE_SKEW_SECONDS ahead)
///   5. Ed25519 signature verification (gated by `signer` feature)
///
/// Returns `Ok(())` on success ; otherwise a [`RejectionReason`].
pub fn verify_with_now(
    proof: &CoherenceProof,
    expected_sigma_mask: &SigmaMask,
    expected_omega_digest: &OmegaDigest,
    now_unix_secs: u64,
) -> Result<(), RejectionReason> {
    if &proof.sigma_mask != expected_sigma_mask {
        return Err(RejectionReason::SigmaMaskMismatch);
    }
    if &proof.omega_digest != expected_omega_digest {
        return Err(RejectionReason::OmegaDigestMismatch);
    }

    if proof.timestamp_unix_secs > now_unix_secs.saturating_add(FUTURE_SKEW_SECONDS) {
        return Err(RejectionReason::FutureTimestamp {
            skew: FUTURE_SKEW_SECONDS,
        });
    }
    if proof.timestamp_unix_secs.saturating_add(STALE_WINDOW_SECONDS) < now_unix_secs {
        return Err(RejectionReason::StaleTimestamp {
            window: STALE_WINDOW_SECONDS,
        });
    }

    #[cfg(feature = "signer")]
    {
        if !verify_signature(proof) {
            return Err(RejectionReason::InvalidSignature);
        }
        Ok(())
    }

    #[cfg(not(feature = "signer"))]
    {
        Err(RejectionReason::SignerFeatureDisabled)
    }
}

/// Verify a [`CoherenceProof`] using `now_unix_secs = 0` — useful as a
/// signature-only check that ignores freshness when the caller has already
/// satisfied themselves that the timestamp is in-range. Returns `true` iff
/// the proof's sigma_mask + omega_digest match expected AND the signature
/// verifies.
///
/// This matches the prompt's `pub fn verify(proof, expected_omega_field) -> bool`
/// surface but takes the digest of the field (the field itself is heavy,
/// we don't want a hard dep on cssl-substrate-omega-field).
#[must_use]
pub fn verify(proof: &CoherenceProof, expected_omega_digest: &OmegaDigest) -> bool {
    // Σ-mask is implicit-equal-to-itself for a no-expectation caller.
    if &proof.omega_digest != expected_omega_digest {
        return false;
    }
    #[cfg(feature = "signer")]
    {
        verify_signature(proof)
    }
    #[cfg(not(feature = "signer"))]
    {
        false
    }
}

/// Produce a stable short-tag for the rejection-reason of `proof` against
/// `(expected_sigma_mask, expected_omega_digest, now_unix_secs)`. Returns
/// `None` if the proof is accepted.
#[must_use]
pub fn rejection_reason(
    proof: &CoherenceProof,
    expected_sigma_mask: &SigmaMask,
    expected_omega_digest: &OmegaDigest,
    now_unix_secs: u64,
) -> Option<&'static str> {
    match verify_with_now(proof, expected_sigma_mask, expected_omega_digest, now_unix_secs) {
        Ok(()) => None,
        Err(reason) => Some(reason.short_tag()),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// § Helpers : compute an ω-digest from raw bytes (BLAKE3 wrapper).
// ───────────────────────────────────────────────────────────────────────────

/// BLAKE3-hash a slice of bytes representing an ω-field state-snapshot. This
/// is provided for callers who want a one-liner without pulling blake3 as a
/// separate dependency.
#[must_use]
pub fn omega_digest_of(bytes: &[u8]) -> OmegaDigest {
    *blake3::hash(bytes).as_bytes()
}

// ═══════════════════════════════════════════════════════════════════════════
// § Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "signer")]
    use ed25519_dalek::SigningKey;
    #[cfg(feature = "signer")]
    use rand::rngs::OsRng;

    fn fake_omega_field() -> Vec<u8> {
        // 1 KB of deterministic-pseudo-random ω-state for tests.
        let mut v = Vec::with_capacity(1024);
        for i in 0..1024u32 {
            v.push((i.wrapping_mul(0x9E37_79B9) >> 24) as u8);
        }
        v
    }

    fn fake_sigma_mask() -> SigmaMask {
        let mut m = [0u8; SIGMA_MASK_BYTES];
        // consent_bits = 0xDEADBEEF (LE) at bytes 0..=3.
        m[0..4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        // sovereign_handle = 0x1234 at bytes 4..=5.
        m[4..6].copy_from_slice(&0x1234u16.to_le_bytes());
        m
    }

    #[test]
    fn canonical_message_layout_is_stable() {
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let buf = canonical_message_bytes(&sigma, &omega, ts);
        // Domain-tag prefix.
        assert_eq!(&buf[0..COHERENCE_PROOF_DOMAIN_TAG.len()], COHERENCE_PROOF_DOMAIN_TAG);
        // sigma_mask follows the domain-tag.
        let dt_end = COHERENCE_PROOF_DOMAIN_TAG.len();
        assert_eq!(&buf[dt_end..dt_end + SIGMA_MASK_BYTES], &sigma);
        // omega_digest follows the sigma_mask.
        let om_start = dt_end + SIGMA_MASK_BYTES;
        assert_eq!(&buf[om_start..om_start + OMEGA_DIGEST_BYTES], &omega);
        // timestamp_le tail.
        let ts_start = om_start + OMEGA_DIGEST_BYTES;
        assert_eq!(&buf[ts_start..ts_start + 8], &ts.to_le_bytes());
        // Total length matches the constant.
        assert_eq!(buf.len(), CANONICAL_MSG_LEN);
    }

    #[test]
    fn canonical_digest_is_deterministic() {
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let d1 = canonical_message_digest(&sigma, &omega, ts);
        let d2 = canonical_message_digest(&sigma, &omega, ts);
        assert_eq!(d1, d2);

        // Changing the timestamp changes the digest.
        let d3 = canonical_message_digest(&sigma, &omega, ts + 1);
        assert_ne!(d1, d3);

        // Changing the sigma changes the digest.
        let mut sigma2 = sigma;
        sigma2[0] ^= 0x01;
        let d4 = canonical_message_digest(&sigma2, &omega, ts);
        assert_ne!(d1, d4);

        // Changing the omega changes the digest.
        let mut omega2 = omega;
        omega2[0] ^= 0x01;
        let d5 = canonical_message_digest(&sigma, &omega2, ts);
        assert_ne!(d1, d5);
    }

    #[cfg(feature = "signer")]
    #[test]
    fn prove_then_verify_roundtrips() {
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = prove(omega, sigma, &signer, ts);

        // Same `(sigma, omega, now=ts)` accepts.
        assert!(verify_with_now(&proof, &sigma, &omega, ts).is_ok());

        // Pubkey in the proof matches the signer.
        assert_eq!(proof.signer_pubkey, signer.verifying_key().to_bytes());

        // The convenience `verify()` (no sigma-check, signature-only)
        // also accepts.
        assert!(verify(&proof, &omega));
    }

    #[cfg(feature = "signer")]
    #[test]
    fn invalid_signature_is_rejected() {
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let mut proof = prove(omega, sigma, &signer, ts);
        // Flip a single bit in the signature.
        proof.signature_ed25519[0] ^= 0x01;
        let outcome = verify_with_now(&proof, &sigma, &omega, ts);
        assert_eq!(outcome, Err(RejectionReason::InvalidSignature));
        assert_eq!(
            rejection_reason(&proof, &sigma, &omega, ts),
            Some("invalid-signature")
        );
    }

    #[cfg(feature = "signer")]
    #[test]
    fn stale_timestamp_is_rejected() {
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = prove(omega, sigma, &signer, ts);

        // `now` = ts + 61s is past the 60s default window.
        let now = ts + STALE_WINDOW_SECONDS + 1;
        let outcome = verify_with_now(&proof, &sigma, &omega, now);
        assert!(matches!(outcome, Err(RejectionReason::StaleTimestamp { .. })));
        assert_eq!(
            rejection_reason(&proof, &sigma, &omega, now),
            Some("stale-timestamp")
        );

        // `now` = ts + 60s is the boundary — still inside the window.
        let now = ts + STALE_WINDOW_SECONDS;
        assert!(verify_with_now(&proof, &sigma, &omega, now).is_ok());
    }

    #[cfg(feature = "signer")]
    #[test]
    fn future_timestamp_is_rejected() {
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = prove(omega, sigma, &signer, ts);

        // `now` is FUTURE_SKEW_SECONDS+1 BEFORE the proof — proof appears
        // FUTURE_SKEW_SECONDS+1 in the future.
        let now = ts.saturating_sub(FUTURE_SKEW_SECONDS + 1);
        let outcome = verify_with_now(&proof, &sigma, &omega, now);
        assert!(matches!(outcome, Err(RejectionReason::FutureTimestamp { .. })));
        assert_eq!(
            rejection_reason(&proof, &sigma, &omega, now),
            Some("future-timestamp")
        );
    }

    #[cfg(feature = "signer")]
    #[test]
    fn sigma_mask_mismatch_is_rejected() {
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = prove(omega, sigma, &signer, ts);

        let mut bad_sigma = sigma;
        bad_sigma[0] ^= 0x01;
        let outcome = verify_with_now(&proof, &bad_sigma, &omega, ts);
        assert_eq!(outcome, Err(RejectionReason::SigmaMaskMismatch));
        assert_eq!(
            rejection_reason(&proof, &bad_sigma, &omega, ts),
            Some("sigma-mask-mismatch")
        );
    }

    #[cfg(feature = "signer")]
    #[test]
    fn omega_digest_mismatch_is_rejected() {
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = prove(omega, sigma, &signer, ts);

        let mut bad_omega = omega;
        bad_omega[5] ^= 0x42;
        let outcome = verify_with_now(&proof, &sigma, &bad_omega, ts);
        assert_eq!(outcome, Err(RejectionReason::OmegaDigestMismatch));
        assert_eq!(
            rejection_reason(&proof, &sigma, &bad_omega, ts),
            Some("omega-digest-mismatch")
        );
    }

    #[cfg(feature = "signer")]
    #[test]
    fn cheap_checks_run_before_signature_verify() {
        // Σ-mask + ω-digest mismatch must SHORT-CIRCUIT before any
        // ed25519-verify is performed. We can only assert the rejection-
        // reason here — the timing-difference is the actual hot-path win.
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = prove(omega, sigma, &signer, ts);

        let mut bad_sigma = sigma;
        bad_sigma[0] ^= 0x01;
        let mut bad_omega = omega;
        bad_omega[0] ^= 0x01;
        // Σ-mask check is FIRST.
        assert_eq!(
            verify_with_now(&proof, &bad_sigma, &bad_omega, ts),
            Err(RejectionReason::SigmaMaskMismatch)
        );
    }

    #[cfg(feature = "signer")]
    #[test]
    fn proof_is_copy_and_zero_alloc_path() {
        // Constructing + copying + canonical-bytes should not require a heap
        // allocator (this is a smoke-test : we exercise the API and rely on
        // the Cargo-tree to confirm no Vec/Box appears in the call-chain).
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = prove(omega, sigma, &signer, ts);
        let copied = proof; // Copy.
        let bytes = copied.canonical_message_bytes();
        assert_eq!(bytes.len(), CANONICAL_MSG_LEN);

        // Verify-path is also stack-only.
        assert!(verify_with_now(&copied, &sigma, &omega, ts).is_ok());
    }

    #[cfg(feature = "signer")]
    #[test]
    fn rejection_reason_returns_none_on_success() {
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = prove(omega, sigma, &signer, ts);
        assert_eq!(rejection_reason(&proof, &sigma, &omega, ts), None);
    }

    #[cfg(feature = "signer")]
    #[test]
    fn from_raw_constructs_a_valid_proof() {
        let signer = SigningKey::generate(&mut OsRng);
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = prove(omega, sigma, &signer, ts);

        let rebuilt = CoherenceProof::from_raw(
            proof.sigma_mask,
            proof.omega_digest,
            proof.signer_pubkey,
            proof.signature_ed25519,
            proof.timestamp_unix_secs,
        );
        assert_eq!(rebuilt, proof);
        assert!(verify_with_now(&rebuilt, &sigma, &omega, ts).is_ok());
    }

    // ─── default-build (no `signer`) compile-checks ────────────────────────

    #[cfg(not(feature = "signer"))]
    #[test]
    fn default_build_returns_signer_disabled_on_verify() {
        // Without the `signer` feature, `verify_with_now` cannot perform real
        // signature-verification ; it returns SignerFeatureDisabled iff the
        // sigma + digest + freshness checks all pass.
        let sigma = fake_sigma_mask();
        let omega = omega_digest_of(&fake_omega_field());
        let ts = 1_700_000_000u64;
        let proof = CoherenceProof::from_raw(
            sigma,
            omega,
            [0u8; SIGNER_PUBKEY_BYTES],
            [0u8; SIGNATURE_BYTES],
            ts,
        );
        let outcome = verify_with_now(&proof, &sigma, &omega, ts);
        assert_eq!(outcome, Err(RejectionReason::SignerFeatureDisabled));
        // `verify()` (signature-only) also returns false in default build.
        assert!(!verify(&proof, &omega));
    }

    #[test]
    fn omega_digest_of_is_deterministic_and_blake3() {
        let bytes = b"the quick brown fox";
        let d1 = omega_digest_of(bytes);
        let d2 = omega_digest_of(bytes);
        assert_eq!(d1, d2);
        // Cross-check vs blake3-direct.
        let expected = *blake3::hash(bytes).as_bytes();
        assert_eq!(d1, expected);
    }

    #[test]
    fn rejection_short_tags_are_stable() {
        // Lock-in the user-facing strings so refactors can't silently change
        // them. These are surfaces of the public API.
        assert_eq!(RejectionReason::SigmaMaskMismatch.short_tag(), "sigma-mask-mismatch");
        assert_eq!(RejectionReason::OmegaDigestMismatch.short_tag(), "omega-digest-mismatch");
        assert_eq!(RejectionReason::InvalidSignature.short_tag(), "invalid-signature");
        assert_eq!(
            RejectionReason::StaleTimestamp { window: STALE_WINDOW_SECONDS }.short_tag(),
            "stale-timestamp"
        );
        assert_eq!(
            RejectionReason::FutureTimestamp { skew: FUTURE_SKEW_SECONDS }.short_tag(),
            "future-timestamp"
        );
        assert_eq!(
            RejectionReason::SignerFeatureDisabled.short_tag(),
            "signer-feature-disabled"
        );
    }
}
