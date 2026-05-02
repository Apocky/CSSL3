//! § verify — re-derive Σ-Chain anchor + check Ed25519 signature.

use crate::link::RemixLink;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("pubkey hex invalid (must be 32 bytes lower-hex)")]
    BadPubkeyHex,
    #[error("signature hex invalid (must be 64 bytes lower-hex)")]
    BadSignatureHex,
    #[error("Σ-Chain anchor mismatch (canonical-bytes mutated post-anchor)")]
    AnchorMismatch,
    #[error("Ed25519 signature invalid for declared pubkey")]
    SignatureInvalid,
    #[error("canonical-bytes encode failed (attribution-text too long)")]
    CanonicalEncode,
    #[error("link draft incomplete (anchor or signature empty)")]
    DraftIncomplete,
}

/// Successful-verify witness. Returned to the caller as proof the link is
/// attribution-immutable (recomputed-anchor matched + signature held).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedLink<'a> {
    pub link: &'a RemixLink,
}

/// Verify a RemixLink. Re-derives the Σ-Chain anchor from canonical-bytes,
/// confirms the stored anchor matches, then verifies the Ed25519 signature
/// over (DOMAIN_TAG || anchor). Refuses to emit Verified on any failure.
pub fn verify_remix_link(link: &RemixLink) -> Result<VerifiedLink<'_>, VerifyError> {
    if link.sigma_chain_anchor.is_empty() || link.remix_signature.is_empty() {
        return Err(VerifyError::DraftIncomplete);
    }
    crate::sign::verify_signature_and_anchor(link)?;
    Ok(VerifiedLink { link })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::RemixKind;
    use crate::link::RemixLink;
    use crate::royalty::RoyaltyShareGift;
    use crate::sign::sign_remix_link;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn fresh_signed_link() -> (SigningKey, RemixLink) {
        let signer = SigningKey::generate(&mut OsRng);
        let pubkey_hex = crate::hex_lower(&signer.verifying_key().to_bytes());
        let mut link = RemixLink::new_draft(
            "child-001".to_string(),
            "parent-genesis".to_string(),
            "1.0.0".to_string(),
            RemixKind::Fork,
            "fork to add new ending".to_string(),
            1_700_000_000,
            pubkey_hex,
            RoyaltyShareGift::pledged(15).unwrap(),
        )
        .unwrap();
        sign_remix_link(&signer, &mut link).unwrap();
        (signer, link)
    }

    #[test]
    fn roundtrip_sign_then_verify_ok() {
        let (_sk, link) = fresh_signed_link();
        link.ensure_signed().unwrap();
        verify_remix_link(&link).unwrap();
    }

    #[test]
    fn mutating_attribution_text_breaks_anchor() {
        let (_sk, mut link) = fresh_signed_link();
        link.attribution_text.push_str(" SNUCK-IN-EDIT");
        assert_eq!(
            verify_remix_link(&link).unwrap_err(),
            VerifyError::AnchorMismatch
        );
    }

    #[test]
    fn mutating_kind_breaks_anchor() {
        let (_sk, mut link) = fresh_signed_link();
        link.remix_kind = RemixKind::Improvement;
        assert_eq!(
            verify_remix_link(&link).unwrap_err(),
            VerifyError::AnchorMismatch
        );
    }

    #[test]
    fn forged_signature_with_other_key_rejected() {
        let (_sk, mut link) = fresh_signed_link();
        // Replace pubkey with a fresh-different keypair without re-signing
        let other = SigningKey::generate(&mut OsRng);
        link.remix_creator_pubkey = crate::hex_lower(&other.verifying_key().to_bytes());
        // Anchor still matches against original-pubkey input → re-derive will
        // produce a different anchor (canonical-bytes include pubkey), so the
        // anchor check fires first.
        let err = verify_remix_link(&link).unwrap_err();
        assert!(matches!(
            err,
            VerifyError::AnchorMismatch | VerifyError::SignatureInvalid
        ));
    }

    #[test]
    fn empty_anchor_caught_before_crypto() {
        let (_sk, mut link) = fresh_signed_link();
        link.sigma_chain_anchor.clear();
        assert_eq!(
            verify_remix_link(&link).unwrap_err(),
            VerifyError::DraftIncomplete
        );
    }
}
