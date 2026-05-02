//! § sign — Ed25519 author-signing of `.ccpkg` bundles.
//!
//! § FLOW
//!   `sign_bundle(payload_archive_bytes, manifest, author_cap, sigma_anchor,
//!                signing_key, header_skeleton) -> Bundle`
//!
//!   1. Validate manifest invariants (id/version/name + gift-economy-axiom).
//!   2. Serialise manifest to canonical-JSON bytes.
//!   3. Validate author_pubkey-of-manifest matches signing_key.verifying_key().
//!   4. Combine (manifest_bytes || archive_bytes) → recompute BLAKE3 + size.
//!   5. Build BundleHeader with the recomputed digest + author_cap_class.
//!   6. Compute signed_bytes = (header || manifest_size_le || manifest || archive).
//!   7. Sign with Ed25519 → emit Bundle { header · manifest · archive · sig · anchor }.

use crate::archive::ArchiveError;
use crate::cap::AuthorCapClass;
use crate::header::{Bundle, BundleHeader, ANCHOR_BYTES, BUNDLE_FORMAT_VERSION, SIGNATURE_BYTES};
use crate::manifest::{Manifest, ManifestError};
use ed25519_dalek::{Signer, SigningKey};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("manifest validation failed : {0}")]
    Manifest(#[from] ManifestError),
    #[error("archive validation failed : {0}")]
    Archive(#[from] ArchiveError),
    #[error("author_pubkey in manifest does not match signing_key.verifying_key()")]
    PubkeyMismatch,
    #[error("cannot sign content-kind {kind} with cap-class {cap}")]
    CapClassRejectsKind { kind: &'static str, cap: &'static str },
}

/// § Sign a bundle.
///
/// Inputs are :
///   - `archive_bytes`     : pre-packed TARLITE payload (or empty Vec)
///   - `manifest`          : Manifest struct (pre-validated by caller or here)
///   - `author_cap_class`  : the audience-class this bundle is signed at
///   - `sigma_chain_anchor`: BLAKE3-32 of an Σ-Chain commit attesting this package
///   - `signing_key`       : Ed25519 SigningKey (author's private key)
///   - `ver`               : (major, minor, patch) override for the header
///   - `created_ts_ns`     : wall-clock at sign time (caller-supplied for determinism)
///
/// Returns a fully-signed `Bundle`.
#[allow(clippy::too_many_arguments)]
pub fn sign_bundle(
    archive_bytes: Vec<u8>,
    manifest: Manifest,
    author_cap_class: AuthorCapClass,
    sigma_chain_anchor: [u8; ANCHOR_BYTES],
    signing_key: &SigningKey,
    ver: (u16, u16, u16),
    created_ts_ns: u64,
) -> Result<Bundle, SigningError> {
    // 1. Validate manifest.
    manifest.validate()?;

    // 2. Validate cap-class can sign this kind. (Universally true today, but
    //    we keep the gate to allow future per-kind cap-restrictions.)
    if !author_cap_class.can_sign_kind(manifest.kind) {
        return Err(SigningError::CapClassRejectsKind {
            kind: manifest.kind.name(),
            cap: author_cap_class.as_str(),
        });
    }

    // 3. Validate author_pubkey of manifest matches signing_key.
    let signing_pubkey: [u8; 32] = signing_key.verifying_key().to_bytes();
    if signing_pubkey != manifest.author_pubkey {
        return Err(SigningError::PubkeyMismatch);
    }

    // 4. Serialise manifest.
    let manifest_bytes = manifest.to_canonical_bytes()?;

    // 5. Build combined payload + recompute BLAKE3.
    let total_size = (manifest_bytes.len() + archive_bytes.len()) as u64;
    let mut combined = Vec::with_capacity(manifest_bytes.len() + archive_bytes.len());
    combined.extend_from_slice(&manifest_bytes);
    combined.extend_from_slice(&archive_bytes);
    let blake3_payload: [u8; 32] = blake3::hash(&combined).into();

    let header = BundleHeader {
        format_version: BUNDLE_FORMAT_VERSION,
        content_kind: manifest.kind,
        author_cap_class,
        ver_major: ver.0,
        ver_minor: ver.1,
        ver_patch: ver.2,
        created_ts_ns,
        total_size,
        blake3_payload,
    };

    // 6. Build signed-bytes.
    let header_bytes = header.to_bytes();
    let manifest_size_bytes = (manifest_bytes.len() as u32).to_le_bytes();
    let mut to_sign = Vec::with_capacity(
        header_bytes.len() + manifest_size_bytes.len() + manifest_bytes.len() + archive_bytes.len(),
    );
    to_sign.extend_from_slice(&header_bytes);
    to_sign.extend_from_slice(&manifest_size_bytes);
    to_sign.extend_from_slice(&manifest_bytes);
    to_sign.extend_from_slice(&archive_bytes);

    // 7. Sign.
    let sig = signing_key.sign(&to_sign);
    let mut signature = [0u8; SIGNATURE_BYTES];
    signature.copy_from_slice(&sig.to_bytes());

    Ok(Bundle {
        header,
        manifest_bytes,
        archive_bytes,
        signature,
        sigma_chain_anchor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::Bundle;
    use crate::kind::ContentKind;
    use crate::manifest::{Dependency, LicenseTier};
    use ed25519_dalek::{SigningKey, Verifier, VerifyingKey};
    use rand::rngs::OsRng;

    fn fixture_manifest(pubkey: [u8; 32]) -> Manifest {
        Manifest {
            id: "loa.scenes.darkforest".to_string(),
            version: "1.0.0".to_string(),
            kind: ContentKind::Scene,
            author_pubkey: pubkey,
            name: "Dark Forest".to_string(),
            description: "Mossy whisper of ancient trees.".to_string(),
            depends_on: vec![Dependency {
                id: "loa.atmos.fog".to_string(),
                version: "1.0.0".to_string(),
            }],
            remix_of: None,
            tags: vec!["forest".to_string()],
            sigma_mask: 0,
            gift_economy_only: true,
            license: LicenseTier::Open,
        }
    }

    #[test]
    fn sign_bundle_round_trip_verifies_raw_ed25519() {
        let key = SigningKey::generate(&mut OsRng);
        let pubkey: [u8; 32] = key.verifying_key().to_bytes();
        let manifest = fixture_manifest(pubkey);
        let archive = b"TARLITE-stub".to_vec();
        let anchor = [0xAB; ANCHOR_BYTES];

        let bundle = sign_bundle(
            archive,
            manifest,
            AuthorCapClass::Creator,
            anchor,
            &key,
            (1, 0, 0),
            1_700_000_000_000_000_000,
        )
        .unwrap();

        // Header BLAKE3 was recomputed.
        let payload = bundle.payload_bytes();
        let expected = *blake3::hash(&payload).as_bytes();
        assert_eq!(bundle.header.blake3_payload, expected);
        assert_eq!(bundle.header.total_size, payload.len() as u64);

        // Signature verifies via raw ed25519.
        let vk = VerifyingKey::from_bytes(&pubkey).unwrap();
        let sig = ed25519_dalek::Signature::from_bytes(&bundle.signature);
        let to_check = bundle.signed_bytes();
        assert!(vk.verify(&to_check, &sig).is_ok());

        // Anchor preserved.
        assert_eq!(bundle.sigma_chain_anchor, anchor);

        // Roundtrip on disk.
        let bytes = bundle.to_bytes();
        let back = Bundle::from_bytes(&bytes).unwrap();
        assert_eq!(back, bundle);
    }

    #[test]
    fn sign_rejects_pubkey_mismatch() {
        let key = SigningKey::generate(&mut OsRng);
        // Manifest claims a *different* author pubkey than the signer.
        let manifest = fixture_manifest([0xFF; 32]);
        let r = sign_bundle(
            vec![],
            manifest,
            AuthorCapClass::Creator,
            [0u8; ANCHOR_BYTES],
            &key,
            (1, 0, 0),
            0,
        );
        assert!(matches!(r, Err(SigningError::PubkeyMismatch)));
    }

    #[test]
    fn sign_rejects_invalid_manifest() {
        let key = SigningKey::generate(&mut OsRng);
        let pubkey: [u8; 32] = key.verifying_key().to_bytes();
        let mut manifest = fixture_manifest(pubkey);
        manifest.id = String::new(); // invalid
        let r = sign_bundle(
            vec![],
            manifest,
            AuthorCapClass::Creator,
            [0u8; ANCHOR_BYTES],
            &key,
            (1, 0, 0),
            0,
        );
        assert!(matches!(r, Err(SigningError::Manifest(_))));
    }

    #[test]
    fn sign_with_empty_archive_works() {
        let key = SigningKey::generate(&mut OsRng);
        let pubkey: [u8; 32] = key.verifying_key().to_bytes();
        let manifest = fixture_manifest(pubkey);
        let bundle = sign_bundle(
            vec![],
            manifest,
            AuthorCapClass::Creator,
            [0u8; ANCHOR_BYTES],
            &key,
            (1, 0, 0),
            0,
        )
        .unwrap();
        assert!(bundle.archive_bytes.is_empty());
        assert_eq!(bundle.header.total_size as usize, bundle.manifest_bytes.len());
    }

    #[test]
    fn sign_with_anonymous_cap_class_works() {
        // Note : k-anon enforcement is at the publish layer (W12-5), not
        // the sign layer. Sign always succeeds for any cap_class, the gate
        // is at publish.
        let key = SigningKey::generate(&mut OsRng);
        let pubkey: [u8; 32] = key.verifying_key().to_bytes();
        let manifest = fixture_manifest(pubkey);
        let bundle = sign_bundle(
            vec![],
            manifest,
            AuthorCapClass::Anonymous,
            [0u8; ANCHOR_BYTES],
            &key,
            (1, 0, 0),
            0,
        )
        .unwrap();
        assert_eq!(bundle.header.author_cap_class, AuthorCapClass::Anonymous);
    }
}
