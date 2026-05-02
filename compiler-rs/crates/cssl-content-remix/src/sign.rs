//! § sign — Ed25519 signing of canonical-bytes for RemixLink.
//!
//! Σ-Chain anchor = BLAKE3(canonical_bytes_without_signature). Signature
//! covers (remixed_id · parent_id · attribution_text · ts · anchor) as
//! the prompt requires plus the kind-tag for safety against kind-mutation.
//!
//! Stable canonical-bytes layout (UTF-8 strings null-byte separated) :
//!
//!   "remix:v1\0"
//!   <remixed_id>"\0"
//!   <parent_id>"\0"
//!   <parent_version>"\0"
//!   <kind_tag_u8>
//!   <created_at_u64_le>
//!   <attribution_text_len_u32_le><attribution_text_bytes>
//!   <remix_creator_pubkey_32_bytes>
//!   <royalty_pct_u8>
//!
//! The Σ-Chain anchor is BLAKE3 of the above. The signature is Ed25519
//! over `domain_tag || anchor_bytes` to bind the signature to the anchor.

use crate::link::{ContentId, RemixLink, BLAKE3_LEN};
use crate::royalty::RoyaltyShareGift;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use thiserror::Error;

pub const PUBKEY_LEN: usize = 32;
pub const SIG_LEN: usize = 64;

const DOMAIN_TAG: &[u8] = b"cssl-content-remix/sign/v1";
const ANCHOR_TAG: &[u8] = b"remix:v1\0";

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("attribution text exceeds u32 byte-length")]
    AttributionTooLong,
    #[error("invalid pubkey hex (must be 64 lower-hex chars)")]
    BadPubkeyHex,
    #[error("signing pubkey does not match link.remix_creator_pubkey")]
    PubkeyMismatch,
}

/// Decode a 64-char lower-hex string into 32 bytes. Returns None if shape
/// is wrong. Used by both signer + verifier.
pub fn hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks_exact(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Encode a 64-byte signature into 128-char lower-hex.
fn hex_encode(bytes: &[u8]) -> String {
    crate::hex_lower(bytes)
}

/// Build canonical-bytes layout described above. Used by signer + verifier.
pub fn canonical_link_bytes(
    remixed_id: &ContentId,
    parent_id: &ContentId,
    parent_version: &str,
    kind_tag: u8,
    created_at: u64,
    attribution_text: &str,
    remix_creator_pubkey_32: &[u8; 32],
    royalty: &RoyaltyShareGift,
) -> Result<Vec<u8>, SigningError> {
    let attr_bytes = attribution_text.as_bytes();
    if attr_bytes.len() > u32::MAX as usize {
        return Err(SigningError::AttributionTooLong);
    }
    let mut buf = Vec::with_capacity(
        ANCHOR_TAG.len()
            + remixed_id.len() + 1
            + parent_id.len() + 1
            + parent_version.len() + 1
            + 1
            + 8
            + 4 + attr_bytes.len()
            + 32
            + 1,
    );
    buf.extend_from_slice(ANCHOR_TAG);
    buf.extend_from_slice(remixed_id.as_bytes());
    buf.push(0);
    buf.extend_from_slice(parent_id.as_bytes());
    buf.push(0);
    buf.extend_from_slice(parent_version.as_bytes());
    buf.push(0);
    buf.push(kind_tag);
    buf.extend_from_slice(&created_at.to_le_bytes());
    buf.extend_from_slice(&(attr_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(attr_bytes);
    buf.extend_from_slice(remix_creator_pubkey_32);
    buf.push(royalty.pledged_pct);
    Ok(buf)
}

/// Compute the Σ-Chain anchor digest = BLAKE3(canonical-bytes).
fn compute_anchor(canonical: &[u8]) -> [u8; BLAKE3_LEN] {
    *blake3::hash(canonical).as_bytes()
}

/// Sign a draft RemixLink in-place : populates `sigma_chain_anchor` and
/// `remix_signature`. The signing-key MUST correspond to the
/// `remix_creator_pubkey` already-set on the draft.
pub fn sign_remix_link(
    signer: &SigningKey,
    link: &mut RemixLink,
) -> Result<(), SigningError> {
    let sk_pub = signer.verifying_key();
    let declared = hex32(&link.remix_creator_pubkey).ok_or(SigningError::BadPubkeyHex)?;
    if sk_pub.to_bytes() != declared {
        return Err(SigningError::PubkeyMismatch);
    }

    let canonical = canonical_link_bytes(
        &link.remixed_id,
        &link.parent_id,
        &link.parent_version,
        link.remix_kind.tag(),
        link.created_at,
        &link.attribution_text,
        &declared,
        &link.royalty_share_gift,
    )?;
    let anchor = compute_anchor(&canonical);

    // Signature covers (DOMAIN_TAG || anchor) — binds sig to anchor.
    let mut sig_input = Vec::with_capacity(DOMAIN_TAG.len() + 32);
    sig_input.extend_from_slice(DOMAIN_TAG);
    sig_input.extend_from_slice(&anchor);
    let sig: Signature = signer.sign(&sig_input);

    link.sigma_chain_anchor = hex_encode(&anchor);
    link.remix_signature = hex_encode(&sig.to_bytes());
    Ok(())
}

/// Verify-side helper exposed in the verify module : recomputes anchor +
/// checks signature against the declared pubkey. Returns Ok on success.
pub(crate) fn verify_signature_and_anchor(
    link: &RemixLink,
) -> Result<(), crate::verify::VerifyError> {
    use crate::verify::VerifyError;
    let pubkey_bytes =
        hex32(&link.remix_creator_pubkey).ok_or(VerifyError::BadPubkeyHex)?;
    let canonical = canonical_link_bytes(
        &link.remixed_id,
        &link.parent_id,
        &link.parent_version,
        link.remix_kind.tag(),
        link.created_at,
        &link.attribution_text,
        &pubkey_bytes,
        &link.royalty_share_gift,
    )
    .map_err(|_| VerifyError::CanonicalEncode)?;
    let anchor = compute_anchor(&canonical);
    let anchor_hex = hex_encode(&anchor);
    if anchor_hex != link.sigma_chain_anchor {
        return Err(VerifyError::AnchorMismatch);
    }
    let mut sig_input = Vec::with_capacity(DOMAIN_TAG.len() + 32);
    sig_input.extend_from_slice(DOMAIN_TAG);
    sig_input.extend_from_slice(&anchor);

    let sig_bytes_vec = decode_hex(&link.remix_signature).ok_or(VerifyError::BadSignatureHex)?;
    if sig_bytes_vec.len() != SIG_LEN {
        return Err(VerifyError::BadSignatureHex);
    }
    let mut sig_arr = [0u8; SIG_LEN];
    sig_arr.copy_from_slice(&sig_bytes_vec);
    let sig = Signature::from_bytes(&sig_arr);

    let vk = VerifyingKey::from_bytes(&pubkey_bytes).map_err(|_| VerifyError::BadPubkeyHex)?;
    vk.verify_strict(&sig_input, &sig).map_err(|_| VerifyError::SignatureInvalid)?;
    Ok(())
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in s.as_bytes().chunks_exact(2) {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Some(out)
}
