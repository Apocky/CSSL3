//! § verify — Ed25519-sig + BLAKE3-payload-match pipeline.
//!
//! Verification rules :
//!   1. `hotfix.issuer_pubkey` MUST equal the configured master_pubkey
//!      (rejects rogue-issuer forgeries even if their own sig is valid).
//!   2. Ed25519 over `hotfix.envelope_bytes()` MUST validate against
//!      `master_pubkey`.
//!   3. `blake3(hotfix.payload).to_hex()` MUST equal `hotfix.payload_blake3`.
//!   4. `hotfix.class_tier` MUST equal `hotfix.class.tier()`
//!      (defends against a forged-but-misclassified payload escalating
//!      from cosmetic-tier auto-apply to security-tier).

use crate::class::Hotfix;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// § Outcome of verification : explicit success type so callers can
/// pattern-match without leaning on `Result<(), _>`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifyResult {
    /// All four rules passed.
    Verified,
}

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("issuer pubkey does not match the configured master pubkey")]
    IssuerNotMaster,
    #[error("invalid ed25519 master verifying key bytes")]
    InvalidMasterKey,
    #[error("ed25519 signature failed verification")]
    BadSignature,
    #[error("payload blake3 hex `{claimed}` does not match actual `{actual}`")]
    PayloadHashMismatch { claimed: String, actual: String },
    #[error("declared class_tier does not match HotfixClass::tier()")]
    TierMismatch,
}

/// § Pure verification function. Returns `Verified` on success ;
/// otherwise a typed `VerifyError`. Side-effect-free.
pub fn verify(hotfix: &Hotfix, master_pubkey: &[u8; 32]) -> Result<VerifyResult, VerifyError> {
    // (1) issuer === master.
    if &hotfix.issuer_pubkey != master_pubkey {
        return Err(VerifyError::IssuerNotMaster);
    }

    // (4) tier-coherence (cheap check first).
    if hotfix.class_tier != hotfix.class.tier() {
        return Err(VerifyError::TierMismatch);
    }

    // (3) payload BLAKE3 actually matches the claim.
    let actual = blake3::hash(&hotfix.payload).to_hex().to_string();
    if actual != hotfix.payload_blake3 {
        return Err(VerifyError::PayloadHashMismatch {
            claimed: hotfix.payload_blake3.clone(),
            actual,
        });
    }

    // (2) Ed25519 signature.
    let key = VerifyingKey::from_bytes(master_pubkey).map_err(|_| VerifyError::InvalidMasterKey)?;
    let sig = Signature::from_bytes(&hotfix.ed25519_sig);
    key.verify(&hotfix.envelope_bytes(), &sig)
        .map_err(|_| VerifyError::BadSignature)?;

    Ok(VerifyResult::Verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::{Hotfix, HotfixClass, HotfixId};
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn make_signed(class: HotfixClass) -> (Hotfix, [u8; 32]) {
        let mut csprng = OsRng;
        let signing = SigningKey::generate(&mut csprng);
        let pubkey = signing.verifying_key().to_bytes();
        let payload = vec![1u8, 2, 3, 4, 5];
        let payload_hex = blake3::hash(&payload).to_hex().to_string();
        let mut h = Hotfix {
            id: HotfixId::new("hf-test"),
            class,
            payload,
            payload_blake3: payload_hex,
            ed25519_sig: [0u8; 64],
            issuer_pubkey: pubkey,
            ts: 1_700_000_000_000_000_000,
            class_tier: class.tier(),
        };
        let sig = signing.sign(&h.envelope_bytes());
        h.ed25519_sig = sig.to_bytes();
        (h, pubkey)
    }

    /// ed25519-verify good-sig (1).
    #[test]
    fn good_signature_verifies() {
        let (h, pk) = make_signed(HotfixClass::KanWeightUpdate);
        assert_eq!(verify(&h, &pk).unwrap(), VerifyResult::Verified);
    }

    /// ed25519-verify bad-sig (2).
    #[test]
    fn tampered_signature_rejected() {
        let (mut h, pk) = make_signed(HotfixClass::KanWeightUpdate);
        h.ed25519_sig[0] ^= 0xFF;
        assert!(matches!(verify(&h, &pk), Err(VerifyError::BadSignature)));
    }

    /// BLAKE3-mismatch rejected (1).
    #[test]
    fn payload_hash_mismatch_rejected() {
        let (mut h, pk) = make_signed(HotfixClass::KanWeightUpdate);
        // Mutate payload AFTER signing — blake3-claim will diverge.
        h.payload[0] ^= 0x01;
        let err = verify(&h, &pk).unwrap_err();
        assert!(matches!(err, VerifyError::PayloadHashMismatch { .. }));
    }

    #[test]
    fn rogue_issuer_rejected() {
        let (h, _pk) = make_signed(HotfixClass::KanWeightUpdate);
        // Ask verifier to validate against a *different* master.
        let other = [42u8; 32];
        assert!(matches!(verify(&h, &other), Err(VerifyError::IssuerNotMaster)));
    }

    #[test]
    fn tier_mismatch_rejected() {
        let (mut h, pk) = make_signed(HotfixClass::KanWeightUpdate);
        h.class_tier = crate::class::HotfixTier::Security;
        assert!(matches!(verify(&h, &pk), Err(VerifyError::TierMismatch)));
    }
}
