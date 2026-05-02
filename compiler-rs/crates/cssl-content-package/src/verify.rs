//! § verify — Ed25519 author-signature + Σ-Chain-anchor + audience-class verification.
//!
//! All unpack paths funnel through `verify_bundle` ; the unpack pipeline
//! refuses to surface `UnpackedContent` without a `VerifyOk` token from this
//! module.

use crate::cap::{AuthorCapClass, K_ANON_MIN};
use crate::header::Bundle;
use crate::manifest::Manifest;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use thiserror::Error;

/// § Marker token returned only on successful verification.
/// Its existence in a function's argument list is type-level proof
/// that the associated bundle is authentic.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VerifyOk(());

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("manifest JSON deserialise failed : {0}")]
    ManifestDeserialise(String),
    #[error("bundle.header.content_kind ({h}) does not match manifest.kind ({m})")]
    KindMismatch { h: &'static str, m: &'static str },
    #[error("payload BLAKE3 mismatch : header claims {claimed}, actual {actual}")]
    PayloadBlake3Mismatch { claimed: String, actual: String },
    #[error("payload size mismatch : header claims {claimed}, actual {actual}")]
    PayloadSizeMismatch { claimed: u64, actual: u64 },
    #[error("invalid author public-key bytes")]
    InvalidAuthorPubkey,
    #[error("ed25519 signature verification failed")]
    BadSignature,
    #[error("sigma-chain anchor mismatch")]
    AnchorMismatch,
    #[error("audience-class deny : bundle signed at {bundle_class} cannot propagate to {viewer_class}")]
    AudienceClassDeny {
        bundle_class: &'static str,
        viewer_class: &'static str,
    },
    #[error("k-anonymity cohort {got} below minimum {min}")]
    KAnonShort { got: usize, min: usize },
}

/// § Verify a bundle.
///
/// Checks (in order) :
///   1. Manifest deserialises.
///   2. `header.content_kind` == `manifest.kind`.
///   3. (manifest_bytes || archive_bytes) length == `header.total_size`.
///   4. BLAKE3-256 of (manifest || archive) == `header.blake3_payload`.
///   5. Ed25519 signature verifies against `manifest.author_pubkey`.
///
/// Σ-Chain anchor verification + audience-class propagation are SEPARATE
/// optional gates (see `verify_anchor_against`, `verify_audience_class`).
pub fn verify_bundle(bundle: &Bundle) -> Result<VerifyOk, VerifyError> {
    // 1. Deserialise manifest.
    let manifest_str = std::str::from_utf8(&bundle.manifest_bytes)
        .map_err(|e| VerifyError::ManifestDeserialise(e.to_string()))?;
    let manifest: Manifest = serde_json::from_str(manifest_str)
        .map_err(|e| VerifyError::ManifestDeserialise(e.to_string()))?;

    // 2. Kind agreement.
    if bundle.header.content_kind != manifest.kind {
        return Err(VerifyError::KindMismatch {
            h: bundle.header.content_kind.name(),
            m: manifest.kind.name(),
        });
    }

    // 3. Size agreement.
    let actual_size = (bundle.manifest_bytes.len() + bundle.archive_bytes.len()) as u64;
    if actual_size != bundle.header.total_size {
        return Err(VerifyError::PayloadSizeMismatch {
            claimed: bundle.header.total_size,
            actual: actual_size,
        });
    }

    // 4. BLAKE3 agreement.
    let payload = bundle.payload_bytes();
    let actual: [u8; 32] = blake3::hash(&payload).into();
    if actual != bundle.header.blake3_payload {
        return Err(VerifyError::PayloadBlake3Mismatch {
            claimed: hex32(&bundle.header.blake3_payload),
            actual: hex32(&actual),
        });
    }

    // 5. Signature verification.
    let vk = VerifyingKey::from_bytes(&manifest.author_pubkey)
        .map_err(|_| VerifyError::InvalidAuthorPubkey)?;
    let sig = Signature::from_bytes(&bundle.signature);
    let to_check = bundle.signed_bytes();
    vk.verify(&to_check, &sig)
        .map_err(|_| VerifyError::BadSignature)?;

    Ok(VerifyOk(()))
}

/// § Verify the Σ-Chain anchor against a known-good 32-byte digest.
/// Used by clients to confirm that the package was attested to a specific
/// Σ-Chain commit. (The sigma-chain crate provides the `expected` digest.)
pub fn verify_anchor_against(
    bundle: &Bundle,
    expected: [u8; 32],
) -> Result<VerifyOk, VerifyError> {
    if bundle.sigma_chain_anchor == expected {
        Ok(VerifyOk(()))
    } else {
        Err(VerifyError::AnchorMismatch)
    }
}

/// § Verify that a bundle's audience-class can propagate to the viewer.
/// Wraps `AuthorCapClass::can_propagate_to`.
pub fn verify_audience_class(
    bundle: &Bundle,
    viewer_class: AuthorCapClass,
) -> Result<VerifyOk, VerifyError> {
    let bundle_class = bundle.header.author_cap_class;
    if bundle_class.can_propagate_to(viewer_class) {
        Ok(VerifyOk(()))
    } else {
        Err(VerifyError::AudienceClassDeny {
            bundle_class: bundle_class.as_str(),
            viewer_class: viewer_class.as_str(),
        })
    }
}

/// § Verify k-anonymity cohort for `cap-X-anonymous`-signed bundles.
/// Returns `Ok` if the cohort meets `K_ANON_MIN`, error otherwise.
pub fn verify_k_anon(cohort_size: usize) -> Result<VerifyOk, VerifyError> {
    if cohort_size >= K_ANON_MIN {
        Ok(VerifyOk(()))
    } else {
        Err(VerifyError::KAnonShort {
            got: cohort_size,
            min: K_ANON_MIN,
        })
    }
}

fn hex32(b: &[u8; 32]) -> String {
    crate::hex_lower(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cap::AuthorCapClass;
    use crate::header::ANCHOR_BYTES;
    use crate::kind::ContentKind;
    use crate::manifest::{LicenseTier, Manifest};
    use crate::sign::sign_bundle;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn fixture_manifest(pubkey: [u8; 32], kind: ContentKind) -> Manifest {
        Manifest {
            id: "test.pkg".to_string(),
            version: "1.0.0".to_string(),
            kind,
            author_pubkey: pubkey,
            name: "Test Pkg".to_string(),
            description: "test".to_string(),
            depends_on: vec![],
            remix_of: None,
            tags: vec![],
            sigma_mask: 0,
            gift_economy_only: true,
            license: LicenseTier::Open,
        }
    }

    fn make_bundle(
        cap_class: AuthorCapClass,
    ) -> (crate::header::Bundle, [u8; 32], [u8; ANCHOR_BYTES]) {
        let key = SigningKey::generate(&mut OsRng);
        let pubkey: [u8; 32] = key.verifying_key().to_bytes();
        let manifest = fixture_manifest(pubkey, ContentKind::Scene);
        let anchor = [0xCC; ANCHOR_BYTES];
        let bundle = sign_bundle(
            vec![1, 2, 3, 4, 5],
            manifest,
            cap_class,
            anchor,
            &key,
            (1, 0, 0),
            1_700_000_000_000_000_000,
        )
        .unwrap();
        (bundle, pubkey, anchor)
    }

    #[test]
    fn verify_round_trip_succeeds() {
        let (bundle, _, _) = make_bundle(AuthorCapClass::Creator);
        verify_bundle(&bundle).unwrap();
    }

    #[test]
    fn verify_tampered_archive_rejected() {
        let (mut bundle, _, _) = make_bundle(AuthorCapClass::Creator);
        bundle.archive_bytes[0] ^= 0xFF;
        let r = verify_bundle(&bundle);
        assert!(matches!(r, Err(VerifyError::PayloadBlake3Mismatch { .. })));
    }

    #[test]
    fn verify_tampered_manifest_rejected() {
        let (mut bundle, _, _) = make_bundle(AuthorCapClass::Creator);
        bundle.manifest_bytes[bundle.manifest_bytes.len() - 2] ^= 0x01;
        let r = verify_bundle(&bundle);
        // Could fail at JSON-deserialise, blake3, or signature — any of those
        // is a valid rejection.
        assert!(r.is_err());
    }

    #[test]
    fn verify_tampered_signature_rejected() {
        let (mut bundle, _, _) = make_bundle(AuthorCapClass::Creator);
        bundle.signature[0] ^= 0xFF;
        let r = verify_bundle(&bundle);
        assert!(matches!(r, Err(VerifyError::BadSignature)));
    }

    #[test]
    fn verify_kind_mismatch_rejected() {
        let (mut bundle, _, _) = make_bundle(AuthorCapClass::Creator);
        bundle.header.content_kind = ContentKind::Recipe; // lie
        let r = verify_bundle(&bundle);
        // Header content_kind disagreement triggers Kind mismatch OR
        // signature failure (since signature covers the header bytes).
        assert!(matches!(
            r,
            Err(VerifyError::KindMismatch { .. } | VerifyError::BadSignature)
        ));
    }

    #[test]
    fn verify_anchor_match_accepts() {
        let (bundle, _, anchor) = make_bundle(AuthorCapClass::Creator);
        verify_anchor_against(&bundle, anchor).unwrap();
    }

    #[test]
    fn verify_anchor_mismatch_rejects() {
        let (bundle, _, _) = make_bundle(AuthorCapClass::Creator);
        let r = verify_anchor_against(&bundle, [0xFF; ANCHOR_BYTES]);
        assert!(matches!(r, Err(VerifyError::AnchorMismatch)));
    }

    #[test]
    fn verify_audience_class_creator_to_creator() {
        let (bundle, _, _) = make_bundle(AuthorCapClass::Creator);
        verify_audience_class(&bundle, AuthorCapClass::Creator).unwrap();
    }

    #[test]
    fn verify_audience_class_creator_to_substrate_team_denied() {
        let (bundle, _, _) = make_bundle(AuthorCapClass::Creator);
        let r = verify_audience_class(&bundle, AuthorCapClass::SubstrateTeam);
        assert!(matches!(r, Err(VerifyError::AudienceClassDeny { .. })));
    }

    #[test]
    fn verify_audience_class_substrate_team_to_creator_succeeds() {
        let (bundle, _, _) = make_bundle(AuthorCapClass::SubstrateTeam);
        // Higher cap-class flows DOWN to lower viewer-class.
        verify_audience_class(&bundle, AuthorCapClass::Creator).unwrap();
    }

    #[test]
    fn verify_audience_class_anonymous_quarantined() {
        let (bundle, _, _) = make_bundle(AuthorCapClass::Anonymous);
        // Anonymous → Creator denied (quarantine).
        let r = verify_audience_class(&bundle, AuthorCapClass::Creator);
        assert!(matches!(r, Err(VerifyError::AudienceClassDeny { .. })));
        // Anonymous → Anonymous allowed.
        verify_audience_class(&bundle, AuthorCapClass::Anonymous).unwrap();
    }

    #[test]
    fn verify_k_anon_cohort_gate() {
        verify_k_anon(5).unwrap();
        verify_k_anon(99).unwrap();
        let r = verify_k_anon(4);
        assert!(matches!(r, Err(VerifyError::KAnonShort { .. })));
    }

    #[test]
    fn verify_truncated_payload_size_mismatch() {
        let (mut bundle, _, _) = make_bundle(AuthorCapClass::Creator);
        // Lie about size.
        bundle.header.total_size = 999_999;
        let r = verify_bundle(&bundle);
        // Either size-mismatch (caught first) or signature failure.
        assert!(matches!(
            r,
            Err(VerifyError::PayloadSizeMismatch { .. } | VerifyError::BadSignature)
        ));
    }
}
