//! § cap.rs — SovereignCap : Ed25519-signed grant-witness for the Σ-runtime.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   When a [`crate::SigmaMask`] is `AUDIENCE_DERIVED` or carries
//!   [`crate::FLAG_ATTESTED`], the [`crate::evaluate`] gate-fn requires a
//!   `cap_witness` — a [`SovereignCap`] proving the caller holds the
//!   sovereign-granted right to perform the requested effect at the
//!   requested audience-class. The cap is Ed25519-signed by the granting
//!   sovereign · the evaluator verifies the signature on every consult.
//!
//! § DESIGN
//!   - Field layout maps 1:1 to the spec :
//!     ```text
//!        holder_pubkey   : [u8; 32]   Ed25519 public-key of cap-holder
//!        grants          : u32        bitset of EFFECT_* the cap permits
//!        audience_class  : u16        bitset of AUDIENCE_* the cap belongs to
//!        expires_at      : u64        seconds-since-epoch · 0 = never
//!        revocation_ref  : Option<[u8;32]>  BLAKE3-hash of revocation-record
//!        signature       : [u8; 64]   Ed25519 signature over canonical-bytes
//!     ```
//!   - The signature covers the canonical-byte-form of (holder · grants ·
//!     audience · expires · revocation_ref-presence-bit + 32-bytes) — see
//!     [`SovereignCap::canonical_signing_bytes`]. Tampering with any field
//!     post-signature is detected by `verify_signature`.
//!   - Caps are NOT bearer tokens : the `holder_pubkey` is the public-key
//!     the cap-holder must demonstrate ownership of by ALSO providing a
//!     fresh signed-challenge in higher-level protocols. The Σ-runtime
//!     itself only verifies the cap's OWN signature (cap-issued-by-sovereign)
//!     — the bound-to-holder check is the caller's responsibility (typically
//!     done at session-establishment-time).
//!
//! § PRIME_DIRECTIVE alignment
//!   - § 0 consent = OS : the cap is the cryptographic witness OF consent.
//!   - § 5 revocability : `revocation_ref` points at the on-chain revocation
//!     record · presence-of-ref ⇒ cap is revoked.
//!   - § 7 INTEGRITY : every cap-grant + cap-revoke routes through the
//!     [`crate::audit::AuditRing`].

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use thiserror::Error;

use crate::mask::EFFECT_READ;

/// Error variants for [`SovereignCap`] verification.
#[derive(Debug, Clone, Error)]
pub enum CapError {
    /// The Ed25519 public-key bytes are malformed (not on the curve).
    #[error("malformed Ed25519 public-key")]
    MalformedPublicKey,
    /// The Ed25519 signature bytes are malformed.
    #[error("malformed Ed25519 signature")]
    MalformedSignature,
    /// Signature verification failed · the cap was tampered with or
    /// signed by a different key than `holder_pubkey`.
    #[error("Ed25519 signature verification failed")]
    SignatureVerifyFailed,
    /// The cap has expired : `expires_at <= now`.
    #[error("cap expired at unix-second {expires_at}")]
    Expired { expires_at: u64 },
    /// The cap has been revoked : `revocation_ref` is `Some`.
    #[error("cap revoked")]
    Revoked,
}

/// Sovereign-cap : Ed25519-signed grant-witness.
///
/// § INVARIANTS
///   - `holder_pubkey` is a valid Ed25519 verifying-key (canonical-bytes form).
///   - `signature` is the Ed25519 signature over [`SovereignCap::canonical_signing_bytes`]
///     produced by the granting sovereign's signing-key. The signing-key is
///     NOT necessarily `holder_pubkey` ; in production the issuing-sovereign
///     is a separate identity (Apocky-Self-Sovereign-Root or a delegate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SovereignCap {
    pub holder_pubkey: [u8; 32],
    pub grants: u32,
    pub audience_class: u16,
    pub expires_at: u64,
    pub revocation_ref: Option<[u8; 32]>,
    pub signature: [u8; 64],
}

/// Domain-separator for cap canonical-bytes hashing.
///
/// § DESIGN : signed-message domain-separation prevents cross-protocol
/// signature replay. A cap signature CANNOT be replayed as e.g. a chat-
/// message signature because the prefix differs.
const CAP_SIGNING_DOMAIN: &[u8; 32] = b"cssl.sigma-runtime.cap.sign.v01\0";

impl SovereignCap {
    /// Build a SovereignCap from raw fields + signature bytes. Does NOT
    /// verify ; use [`SovereignCap::verify_signature`] before trusting.
    pub fn from_raw(
        holder_pubkey: [u8; 32],
        grants: u32,
        audience_class: u16,
        expires_at: u64,
        revocation_ref: Option<[u8; 32]>,
        signature: [u8; 64],
    ) -> Self {
        Self {
            holder_pubkey,
            grants,
            audience_class,
            expires_at,
            revocation_ref,
            signature,
        }
    }

    /// Canonical signing-bytes = domain-separator || holder || grants ||
    ///   audience || expires || revocation-presence || revocation-ref-or-zero.
    ///
    /// § STABILITY : byte-order is FROZEN. Reordering = ABI break.
    pub fn canonical_signing_bytes(&self) -> [u8; 32 + 32 + 4 + 2 + 8 + 1 + 32] {
        let mut buf = [0u8; 32 + 32 + 4 + 2 + 8 + 1 + 32];
        let mut o = 0usize;
        buf[o..o + 32].copy_from_slice(CAP_SIGNING_DOMAIN);
        o += 32;
        buf[o..o + 32].copy_from_slice(&self.holder_pubkey);
        o += 32;
        buf[o..o + 4].copy_from_slice(&self.grants.to_le_bytes());
        o += 4;
        buf[o..o + 2].copy_from_slice(&self.audience_class.to_le_bytes());
        o += 2;
        buf[o..o + 8].copy_from_slice(&self.expires_at.to_le_bytes());
        o += 8;
        match self.revocation_ref {
            Some(r) => {
                buf[o] = 1;
                buf[o + 1..o + 1 + 32].copy_from_slice(&r);
            }
            None => {
                buf[o] = 0;
                // remaining 32 bytes left zero · canonical for "no revocation".
            }
        }
        buf
    }

    /// Verify the cap's Ed25519 signature using the issuing-sovereign's
    /// public-key (which is NOT `holder_pubkey` in general).
    ///
    /// § ARG `issuing_sovereign_pk` — Ed25519 public-key of the sovereign
    /// who SIGNED this cap (e.g. Apocky-root or a delegated grantor).
    pub fn verify_signature(&self, issuing_sovereign_pk: &[u8; 32]) -> Result<(), CapError> {
        let vk = VerifyingKey::from_bytes(issuing_sovereign_pk)
            .map_err(|_| CapError::MalformedPublicKey)?;
        let sig = Signature::from_bytes(&self.signature);
        let msg = self.canonical_signing_bytes();
        vk.verify(&msg, &sig).map_err(|_| CapError::SignatureVerifyFailed)
    }

    /// Convenience : verify the holder's pubkey is well-formed (curve check).
    pub fn verify_holder_pubkey(&self) -> Result<(), CapError> {
        VerifyingKey::from_bytes(&self.holder_pubkey)
            .map(|_| ())
            .map_err(|_| CapError::MalformedPublicKey)
    }

    /// Test whether the cap permits the requested `effect` bit.
    pub const fn permits_effect(&self, effect: u32) -> bool {
        (self.grants & effect) == effect
    }

    /// Test whether the cap belongs to the requested audience-class bit.
    pub const fn covers_audience(&self, audience: u16) -> bool {
        (self.audience_class & audience) != 0
    }

    /// Test whether the cap has expired given a wall-clock-second.
    pub const fn is_expired(&self, now_seconds: u64) -> bool {
        if self.expires_at == 0 {
            return false;
        }
        now_seconds >= self.expires_at
    }

    /// Test whether the cap has been revoked.
    pub const fn is_revoked(&self) -> bool {
        self.revocation_ref.is_some()
    }

    /// Convenience : run the canonical pre-flight chain {holder-pubkey-valid →
    /// not-revoked → not-expired → signature-verifies}. Returns the first
    /// failing condition.
    pub fn preflight(
        &self,
        issuing_sovereign_pk: &[u8; 32],
        now_seconds: u64,
    ) -> Result<(), CapError> {
        self.verify_holder_pubkey()?;
        if self.is_revoked() {
            return Err(CapError::Revoked);
        }
        if self.is_expired(now_seconds) {
            return Err(CapError::Expired {
                expires_at: self.expires_at,
            });
        }
        self.verify_signature(issuing_sovereign_pk)
    }
}

// ── safety-prelude : some readers expect a default-deny-all cap value
// for testing the deny-path. We expose a constant zero-filled cap that is
// valid bit-form but WILL fail signature verification — useful for tests.

/// A bit-zero cap that fails every verification check. Use ONLY in tests
/// that want to exercise the deny-path without a real signing-key.
///
/// § INVARIANT : `pubkey` is `[0; 32]` which is on-curve (the identity
/// point) but NOT a real holder-key ; signature is zero-bytes which never
/// verifies under Ed25519. Using this in production = bug + audit-event.
pub const ZERO_DENY_CAP: SovereignCap = SovereignCap {
    holder_pubkey: [0; 32],
    grants: EFFECT_READ, // arbitrary nonzero so callers don't trip "no caps".
    audience_class: 0,
    expires_at: 0,
    revocation_ref: None,
    signature: [0; 64],
};

// ───────────────────────────────────────────────────────────────────────────
// § Tests
// ───────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn fresh_keypair() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn sign_a_cap(
        sovereign: &SigningKey,
        holder_pk: [u8; 32],
        grants: u32,
        audience: u16,
        expires_at: u64,
        revocation_ref: Option<[u8; 32]>,
    ) -> SovereignCap {
        let mut cap = SovereignCap::from_raw(
            holder_pk,
            grants,
            audience,
            expires_at,
            revocation_ref,
            [0u8; 64],
        );
        let msg = cap.canonical_signing_bytes();
        let sig = sovereign.sign(&msg);
        cap.signature = sig.to_bytes();
        cap
    }

    #[test]
    fn t01_signature_round_trip_verifies() {
        let sovereign = fresh_keypair();
        let holder = fresh_keypair();
        let cap = sign_a_cap(
            &sovereign,
            holder.verifying_key().to_bytes(),
            EFFECT_READ,
            crate::mask::AUDIENCE_DERIVED,
            0,
            None,
        );
        cap.verify_signature(&sovereign.verifying_key().to_bytes()).unwrap();
    }

    #[test]
    fn t02_verify_with_wrong_sovereign_pk_fails() {
        let sovereign = fresh_keypair();
        let imposter = fresh_keypair();
        let holder = fresh_keypair();
        let cap = sign_a_cap(
            &sovereign,
            holder.verifying_key().to_bytes(),
            EFFECT_READ,
            crate::mask::AUDIENCE_PUBLIC,
            0,
            None,
        );
        let r = cap.verify_signature(&imposter.verifying_key().to_bytes());
        assert!(matches!(r, Err(CapError::SignatureVerifyFailed)));
    }

    #[test]
    fn t03_field_tamper_breaks_signature() {
        let sovereign = fresh_keypair();
        let holder = fresh_keypair();
        let mut cap = sign_a_cap(
            &sovereign,
            holder.verifying_key().to_bytes(),
            EFFECT_READ,
            crate::mask::AUDIENCE_PUBLIC,
            0,
            None,
        );
        // tamper post-sign : escalate grants
        cap.grants |= crate::mask::EFFECT_PURGE;
        let r = cap.verify_signature(&sovereign.verifying_key().to_bytes());
        assert!(matches!(r, Err(CapError::SignatureVerifyFailed)));
    }

    #[test]
    fn t04_expired_cap_rejected_by_preflight() {
        let sovereign = fresh_keypair();
        let holder = fresh_keypair();
        let cap = sign_a_cap(
            &sovereign,
            holder.verifying_key().to_bytes(),
            EFFECT_READ,
            crate::mask::AUDIENCE_DERIVED,
            500,
            None,
        );
        let r = cap.preflight(&sovereign.verifying_key().to_bytes(), 1_000);
        assert!(matches!(r, Err(CapError::Expired { expires_at: 500 })));
    }

    #[test]
    fn t05_revoked_cap_rejected_by_preflight() {
        let sovereign = fresh_keypair();
        let holder = fresh_keypair();
        let cap = sign_a_cap(
            &sovereign,
            holder.verifying_key().to_bytes(),
            EFFECT_READ,
            crate::mask::AUDIENCE_DERIVED,
            0,
            Some([0xAB; 32]),
        );
        let r = cap.preflight(&sovereign.verifying_key().to_bytes(), 0);
        assert!(matches!(r, Err(CapError::Revoked)));
    }

    #[test]
    fn t06_zero_deny_cap_signature_always_fails() {
        let any = fresh_keypair();
        let r = ZERO_DENY_CAP.verify_signature(&any.verifying_key().to_bytes());
        assert!(matches!(r, Err(CapError::SignatureVerifyFailed)));
    }

    #[test]
    fn t07_permits_and_covers_helpers() {
        let sovereign = fresh_keypair();
        let holder = fresh_keypair();
        let cap = sign_a_cap(
            &sovereign,
            holder.verifying_key().to_bytes(),
            crate::mask::EFFECT_READ | crate::mask::EFFECT_DERIVE,
            crate::mask::AUDIENCE_DERIVED | crate::mask::AUDIENCE_CIRCLE,
            0,
            None,
        );
        assert!(cap.permits_effect(crate::mask::EFFECT_READ));
        assert!(cap.permits_effect(crate::mask::EFFECT_READ | crate::mask::EFFECT_DERIVE));
        assert!(!cap.permits_effect(crate::mask::EFFECT_PURGE));
        assert!(cap.covers_audience(crate::mask::AUDIENCE_DERIVED));
        assert!(cap.covers_audience(crate::mask::AUDIENCE_CIRCLE));
        assert!(!cap.covers_audience(crate::mask::AUDIENCE_PUBLIC));
    }
}
