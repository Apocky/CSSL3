//! § sign — Ed25519 signing of bundles + manifests.
//!
//! Signing keys live ONLY on Apocky's release machine ; this module is
//! used by build-tools, not at runtime. The runtime client only ever
//! verifies (see `verify.rs`).

use crate::bundle::{Bundle, BundleHeader, SIGNATURE_BYTES};
use crate::cap::CapRole;
use crate::manifest::Manifest;
use ed25519_dalek::{Signer, SigningKey};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SigningError {
    #[error("payload size {0} does not match header.payload_size {1}")]
    PayloadSizeMismatch(usize, u64),
    #[error("payload BLAKE3 mismatch (caller-supplied header has stale digest)")]
    PayloadBlake3Mismatch,
    #[error("cap-role {0:?} is not authorised to sign for channel {1}")]
    CapRoleNotAuthorised(CapRole, &'static str),
    #[error("manifest must list at least one channel")]
    EmptyManifest,
}

/// § Sign a payload into a complete `Bundle`.
///
/// This recomputes `payload_blake3` from `payload` (so callers can pass a
/// stub header) and validates that the cap-role matches the channel's
/// `required_cap()`.
pub fn sign_bundle(
    mut header: BundleHeader,
    payload: Vec<u8>,
    signing_key: &SigningKey,
) -> Result<Bundle, SigningError> {
    // Channel-cap coherence : signer must hold the cap-role required by
    // the target channel. This is the LOCAL guard ; the verifier will
    // re-check against the public-key it has on file for that role.
    if header.cap_role != header.channel.required_cap() {
        return Err(SigningError::CapRoleNotAuthorised(
            header.cap_role,
            header.channel.name(),
        ));
    }

    // Recompute BLAKE3 + size from the actual payload (overrides any
    // stale data in the caller-supplied header).
    let blake = *blake3::hash(&payload).as_bytes();
    header.payload_blake3 = blake;
    header.payload_size = payload.len() as u64;

    let header_bytes = header.to_bytes();
    let mut to_sign = Vec::with_capacity(header_bytes.len() + payload.len());
    to_sign.extend_from_slice(&header_bytes);
    to_sign.extend_from_slice(&payload);

    let sig = signing_key.sign(&to_sign);
    let mut signature = [0u8; SIGNATURE_BYTES];
    signature.copy_from_slice(&sig.to_bytes());

    Ok(Bundle {
        header,
        payload,
        signature,
    })
}

/// § Sign a manifest. The signing-key MUST hold the role declared in
/// `manifest.signed_by`. The manifest's `signature` field is replaced
/// with the new signature ; all other fields are preserved.
pub fn sign_manifest(
    mut manifest: Manifest,
    signing_key: &SigningKey,
) -> Result<Manifest, SigningError> {
    if manifest.channels.is_empty() {
        return Err(SigningError::EmptyManifest);
    }
    let to_sign = manifest.canonical_bytes_for_signing();
    let sig = signing_key.sign(&to_sign);
    let mut sig_bytes = [0u8; SIGNATURE_BYTES];
    sig_bytes.copy_from_slice(&sig.to_bytes());
    manifest.signature = sig_bytes;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{BundleHeader, BUNDLE_FORMAT_VERSION};
    use crate::cap::CapRole;
    use crate::channel::Channel;
    use ed25519_dalek::{SigningKey, Verifier, VerifyingKey};
    use rand::rngs::OsRng;

    fn make_header(channel: Channel, cap: CapRole) -> BundleHeader {
        BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            channel,
            cap_role: cap,
            ver_major: 1,
            ver_minor: 0,
            ver_patch: 0,
            timestamp_ns: 1_700_000_000_000_000_000,
            payload_size: 0,
            payload_blake3: [0u8; 32],
        }
    }

    #[test]
    fn sign_bundle_round_trip_verifies() {
        let key = SigningKey::generate(&mut OsRng);
        let pub_bytes: [u8; 32] = key.verifying_key().to_bytes();

        let header = make_header(Channel::SecurityPatch, CapRole::CapD);
        let payload = b"hello-world-security-patch".to_vec();
        let bundle = sign_bundle(header, payload.clone(), &key).unwrap();

        // BLAKE3 was recomputed.
        let expected = *blake3::hash(&payload).as_bytes();
        assert_eq!(bundle.header.payload_blake3, expected);
        assert_eq!(bundle.header.payload_size, payload.len() as u64);

        // Signature verifies via raw ed25519.
        let vk = VerifyingKey::from_bytes(&pub_bytes).unwrap();
        let sig = ed25519_dalek::Signature::from_bytes(&bundle.signature);
        let to_check = bundle.signed_bytes();
        assert!(vk.verify(&to_check, &sig).is_ok());
    }

    #[test]
    fn sign_bundle_rejects_wrong_cap() {
        let key = SigningKey::generate(&mut OsRng);
        // SecurityPatch requires CapD, we'll claim CapE.
        let header = make_header(Channel::SecurityPatch, CapRole::CapE);
        let r = sign_bundle(header, vec![], &key);
        assert!(matches!(r, Err(SigningError::CapRoleNotAuthorised(..))));
    }

    #[test]
    fn sign_manifest_empty_rejected() {
        use crate::manifest::Manifest;
        let key = SigningKey::generate(&mut OsRng);
        let m = Manifest {
            schema_version: 1,
            generated_at_ns: 0,
            signed_by: CapRole::CapA,
            channels: Default::default(),
            revocations: Vec::new(),
            signature: [0u8; 64],
        };
        assert!(matches!(
            sign_manifest(m, &key),
            Err(SigningError::EmptyManifest)
        ));
    }
}
