// § sign.rs — canonical-bytes encoding + BLAKE3 payload-hash + Ed25519 sign-pipeline
// §§ landmines :
//     - canonical-bytes : explicit field-order ¬ bincode ¬ random map-iter
//     - signing-context : kind + ts + parent + payload_blake3 + privacy_tier (NOT just payload)
//     - emitter_pubkey is INCLUDED in canonical-bytes so signature binds (key,event)

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use crate::event::{EventId, EventKind, SigmaEvent, SigmaPayload};
use crate::privacy::PrivacyTier;

/// Length of an Ed25519 public-key in bytes.
pub const PUBKEY_LEN: usize = 32;
/// Length of an Ed25519 signature in bytes.
pub const SIG_LEN: usize = 64;

/// Bundled signing-key bytes for serialize/deserialize round-trips. Local-only by definition —
/// MUST never egress (per spec/14 § NEVER-EGRESSABLE).
#[derive(Debug, Clone, Copy)]
pub struct KeyPairBytes {
    /// Ed25519 secret-scalar bytes (32B).
    pub secret: [u8; 32],
    /// Public-key derived from secret.
    pub public: [u8; PUBKEY_LEN],
}

impl KeyPairBytes {
    #[must_use]
    pub fn from_signing_key(sk: &SigningKey) -> Self {
        Self {
            secret: sk.to_bytes(),
            public: sk.verifying_key().to_bytes(),
        }
    }
}

/// Returns BLAKE3(payload.bytes) as 32-byte digest.
#[must_use]
pub fn payload_blake3(payload: &SigmaPayload) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"sigma_chain/payload/v1");
    h.update(&payload.bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

/// Build canonical-bytes-without-signature for signing AND for id-derivation.
///
/// Field order is FIXED (never reorder — old signatures break) :
///   domain-tag · kind-tag · ts(u64-le) · privacy-tier-tag · parent-flag(u8)
///   · parent-id(32B if present) · emitter_pubkey(32B) · payload_blake3(32B)
///
/// Length-delimiters (u32-le) bracket variable-length tag-strings to prevent
/// concat-collisions. NO maps · NO HashMap · NO bincode.
#[must_use]
pub fn canonical_bytes(
    kind: EventKind,
    ts: u64,
    parent_event_id: Option<&EventId>,
    emitter_pubkey: &[u8; PUBKEY_LEN],
    payload_blake3_digest: &[u8; 32],
    privacy_tier: PrivacyTier,
) -> Vec<u8> {
    const DOMAIN: &[u8] = b"sigma_chain/event/v1";
    let mut out = Vec::with_capacity(256);
    write_lp_tag(&mut out, DOMAIN);
    write_lp_tag(&mut out, kind.tag().as_bytes());
    out.extend_from_slice(&ts.to_le_bytes());
    write_lp_tag(&mut out, privacy_tier.tag().as_bytes());
    match parent_event_id {
        Some(pid) => {
            out.push(1u8);
            out.extend_from_slice(pid);
        }
        None => out.push(0u8),
    }
    out.extend_from_slice(emitter_pubkey);
    out.extend_from_slice(payload_blake3_digest);
    out
}

/// Sign-pipeline : payload → blake3-digest → canonical-bytes → ed25519-sign → SigmaEvent.
///
/// `parent_event_id = None` for root-events ; chain otherwise.
///
/// `id` is BLAKE3(canonical_bytes_without_sig) so identity is signature-independent
/// (verifiers can recompute id without holding sig).
#[must_use]
pub fn sign_event(
    signer: &SigningKey,
    kind: EventKind,
    ts: u64,
    parent_event_id: Option<EventId>,
    payload: &SigmaPayload,
    privacy_tier: PrivacyTier,
) -> SigmaEvent {
    let pubkey = signer.verifying_key().to_bytes();
    let payload_hash = payload_blake3(payload);
    let cbytes = canonical_bytes(
        kind,
        ts,
        parent_event_id.as_ref(),
        &pubkey,
        &payload_hash,
        privacy_tier,
    );
    // id = BLAKE3 of canonical-bytes-without-sig (deterministic given inputs).
    let id = {
        let mut h = blake3::Hasher::new();
        h.update(b"sigma_chain/event_id/v1");
        h.update(&cbytes);
        let mut out = [0u8; 32];
        out.copy_from_slice(h.finalize().as_bytes());
        out
    };
    let sig: Signature = signer.sign(&cbytes);
    SigmaEvent {
        id,
        kind,
        ts,
        emitter_pubkey: pubkey,
        parent_event_id,
        payload_blake3: payload_hash,
        privacy_tier,
        ed25519_sig: sig.to_bytes(),
    }
}

/// Verify the Ed25519 signature on `event` against `event.emitter_pubkey`.
///
/// Returns `Ok(())` on valid sig · `Err(SignatureError)` otherwise.
///
/// # Errors
/// - [`SignatureError::InvalidPubkey`] when the embedded pubkey is malformed.
/// - [`SignatureError::Ed25519`] when the signature does not validate.
pub fn verify_signature(event: &SigmaEvent) -> Result<(), SignatureError> {
    let cbytes = canonical_bytes(
        event.kind,
        event.ts,
        event.parent_event_id.as_ref(),
        &event.emitter_pubkey,
        &event.payload_blake3,
        event.privacy_tier,
    );
    let vk = VerifyingKey::from_bytes(&event.emitter_pubkey)
        .map_err(|_| SignatureError::InvalidPubkey)?;
    let sig = Signature::from_bytes(&event.ed25519_sig);
    vk.verify(&cbytes, &sig).map_err(|_| SignatureError::Ed25519)?;
    Ok(())
}

/// Recompute the deterministic event id from a (post-signature) event's other fields.
/// Useful for verify-pipeline tampering detection (`event.id` mismatched vs recompute → tampered).
#[must_use]
pub fn recompute_event_id(event: &SigmaEvent) -> EventId {
    let cbytes = canonical_bytes(
        event.kind,
        event.ts,
        event.parent_event_id.as_ref(),
        &event.emitter_pubkey,
        &event.payload_blake3,
        event.privacy_tier,
    );
    let mut h = blake3::Hasher::new();
    h.update(b"sigma_chain/event_id/v1");
    h.update(&cbytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

/// Signing/verification error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureError {
    /// Embedded `emitter_pubkey` is not a valid Ed25519 point.
    InvalidPubkey,
    /// Ed25519 signature did not validate against canonical-bytes.
    Ed25519,
}

impl core::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SignatureError::InvalidPubkey => f.write_str("emitter_pubkey malformed"),
            SignatureError::Ed25519 => f.write_str("ed25519 signature invalid"),
        }
    }
}

impl std::error::Error for SignatureError {}

#[inline]
fn write_lp_tag(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("tag len fits u32");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}
