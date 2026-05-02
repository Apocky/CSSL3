//! § verify — Ed25519 signature verification + cap-role enforcement.
//!
//! All apply paths funnel through `verify_bundle` ; the apply pipeline
//! refuses to mutate disk state without a `VerifyOk` token from this module.

use crate::bundle::{Bundle, SIGNATURE_BYTES};
use crate::cap::{CapKey, CapRole};
use crate::channel::Channel;
use crate::manifest::Manifest;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use thiserror::Error;

/// § Marker token returned only on successful verification. Its existence
/// in a function's argument list is the type-level proof that the
/// associated bundle/manifest is authentic.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VerifyOk(());

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("payload bytes blake3 mismatch : header claims {claimed}, actual {actual}")]
    PayloadBlake3Mismatch { claimed: String, actual: String },
    #[error("cap-role {0:?} is not authorised for channel {1}")]
    CapRoleNotAuthorisedForChannel(CapRole, &'static str),
    #[error("no public key on file for cap-role {0:?}")]
    NoPublicKeyForCap(CapRole),
    #[error("invalid public key bytes for cap-role {0:?}")]
    InvalidPublicKey(CapRole),
    #[error("ed25519 signature verification failed")]
    BadSignature,
    #[error("bundle is on the revocation list (channel={0} version={1})")]
    Revoked(&'static str, String),
    #[error("manifest schema version {got} not supported (max {max})")]
    UnsupportedManifestSchema { got: u16, max: u16 },
}

const MAX_MANIFEST_SCHEMA: u16 = 1;

/// § Verify a bundle. Checks (in order) :
///   1. cap-role-of-header matches `channel.required_cap()`
///   2. payload BLAKE3 matches header digest
///   3. signature verifies against the on-file public-key for that cap-role
pub fn verify_bundle(bundle: &Bundle, public_keys: &[CapKey]) -> Result<VerifyOk, VerifyError> {
    let required = bundle.header.channel.required_cap();
    if bundle.header.cap_role != required {
        return Err(VerifyError::CapRoleNotAuthorisedForChannel(
            bundle.header.cap_role,
            bundle.header.channel.name(),
        ));
    }
    let actual = *blake3::hash(&bundle.payload).as_bytes();
    if actual != bundle.header.payload_blake3 {
        return Err(VerifyError::PayloadBlake3Mismatch {
            claimed: hex32(&bundle.header.payload_blake3),
            actual: hex32(&actual),
        });
    }
    let cap_key = public_keys
        .iter()
        .find(|k| k.role == required)
        .ok_or(VerifyError::NoPublicKeyForCap(required))?;
    let vk = VerifyingKey::from_bytes(&cap_key.pubkey)
        .map_err(|_| VerifyError::InvalidPublicKey(required))?;
    let sig = Signature::from_bytes(&bundle.signature);
    let to_check = bundle.signed_bytes();
    vk.verify(&to_check, &sig).map_err(|_| VerifyError::BadSignature)?;
    Ok(VerifyOk(()))
}

/// § Verify a manifest signature against the cap-key it claims (`signed_by`).
pub fn verify_manifest(manifest: &Manifest, public_keys: &[CapKey]) -> Result<VerifyOk, VerifyError> {
    if manifest.schema_version > MAX_MANIFEST_SCHEMA {
        return Err(VerifyError::UnsupportedManifestSchema {
            got: manifest.schema_version,
            max: MAX_MANIFEST_SCHEMA,
        });
    }
    let cap_key = public_keys
        .iter()
        .find(|k| k.role == manifest.signed_by)
        .ok_or(VerifyError::NoPublicKeyForCap(manifest.signed_by))?;
    let vk = VerifyingKey::from_bytes(&cap_key.pubkey)
        .map_err(|_| VerifyError::InvalidPublicKey(manifest.signed_by))?;
    let to_check = manifest.canonical_bytes_for_signing();
    let sig = Signature::from_bytes(&manifest.signature);
    vk.verify(&to_check, &sig).map_err(|_| VerifyError::BadSignature)?;
    Ok(VerifyOk(()))
}

/// § Check whether a (channel, version) pair is revoked by the manifest.
/// Returns `Err(VerifyError::Revoked)` if so.
pub fn check_not_revoked(
    manifest: &Manifest,
    channel: Channel,
    version: &str,
) -> Result<(), VerifyError> {
    for r in &manifest.revocations {
        if r.channel == channel && r.version == version {
            return Err(VerifyError::Revoked(channel.name(), version.to_string()));
        }
    }
    Ok(())
}

fn hex32(b: &[u8; 32]) -> String {
    crate::hex_lower(b)
}

// Small re-export so tests can construct VerifyOk in mocks if needed.
#[cfg(test)]
pub(crate) const fn verify_ok_for_test() -> VerifyOk {
    VerifyOk(())
}

const _: usize = SIGNATURE_BYTES; // ensure module imports stay used.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{BundleHeader, BUNDLE_FORMAT_VERSION};
    use crate::cap::CapKey;
    use crate::channel::Channel;
    use crate::sign::sign_bundle;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn make_keys(role: CapRole) -> (SigningKey, Vec<CapKey>) {
        let key = SigningKey::generate(&mut OsRng);
        let pub_bytes: [u8; 32] = key.verifying_key().to_bytes();
        (
            key,
            vec![CapKey {
                role,
                pubkey: pub_bytes,
            }],
        )
    }

    #[test]
    fn verify_round_trip_succeeds() {
        let (key, pubs) = make_keys(CapRole::CapD);
        let header = BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            channel: Channel::SecurityPatch,
            cap_role: CapRole::CapD,
            ver_major: 1,
            ver_minor: 0,
            ver_patch: 0,
            timestamp_ns: 0,
            payload_size: 0,
            payload_blake3: [0u8; 32],
        };
        let bundle = sign_bundle(header, b"hi".to_vec(), &key).unwrap();
        verify_bundle(&bundle, &pubs).unwrap();
    }

    #[test]
    fn verify_tampered_payload_rejected() {
        let (key, pubs) = make_keys(CapRole::CapD);
        let header = BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            channel: Channel::SecurityPatch,
            cap_role: CapRole::CapD,
            ver_major: 1,
            ver_minor: 0,
            ver_patch: 0,
            timestamp_ns: 0,
            payload_size: 0,
            payload_blake3: [0u8; 32],
        };
        let mut bundle = sign_bundle(header, b"hi".to_vec(), &key).unwrap();
        bundle.payload[0] ^= 0xFF;
        let r = verify_bundle(&bundle, &pubs);
        assert!(matches!(r, Err(VerifyError::PayloadBlake3Mismatch { .. })));
    }

    #[test]
    fn verify_wrong_cap_role_rejected() {
        let (key, _) = make_keys(CapRole::CapE);
        // Make a header that lies about cap-role : channel = SecurityPatch
        // (requires CapD), but we'll set cap_role = CapD so sign succeeds,
        // and then mutate the bytes after signing. To do that we sign a
        // legitimate bundle and then poke the cap_role byte.
        let header = BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            channel: Channel::SecurityPatch,
            cap_role: CapRole::CapD,
            ver_major: 1,
            ver_minor: 0,
            ver_patch: 0,
            timestamp_ns: 0,
            payload_size: 0,
            payload_blake3: [0u8; 32],
        };
        let mut bundle = sign_bundle(header, vec![], &key).unwrap();
        // Tamper : claim CapE in header. Verification must reject.
        bundle.header.cap_role = CapRole::CapE;
        let pubs = vec![CapKey {
            role: CapRole::CapE,
            pubkey: [0u8; 32],
        }];
        let r = verify_bundle(&bundle, &pubs);
        assert!(matches!(
            r,
            Err(VerifyError::CapRoleNotAuthorisedForChannel(CapRole::CapE, _))
        ));
    }

    #[test]
    fn verify_no_public_key_rejected() {
        let (key, _) = make_keys(CapRole::CapD);
        let header = BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            channel: Channel::SecurityPatch,
            cap_role: CapRole::CapD,
            ver_major: 1,
            ver_minor: 0,
            ver_patch: 0,
            timestamp_ns: 0,
            payload_size: 0,
            payload_blake3: [0u8; 32],
        };
        let bundle = sign_bundle(header, vec![], &key).unwrap();
        let pubs: Vec<CapKey> = vec![]; // no key for CapD
        assert!(matches!(
            verify_bundle(&bundle, &pubs),
            Err(VerifyError::NoPublicKeyForCap(CapRole::CapD))
        ));
    }

    #[test]
    fn check_not_revoked_blocks_listed_pair() {
        use crate::manifest::{Manifest, RevocationEntry};
        let m = Manifest {
            schema_version: 1,
            generated_at_ns: 0,
            signed_by: CapRole::CapD,
            channels: Default::default(),
            revocations: vec![RevocationEntry {
                channel: Channel::SecurityPatch,
                version: "1.0.0".to_string(),
                ts_ns: 0,
                reason: "test".to_string(),
            }],
            signature: [0u8; 64],
        };
        assert!(check_not_revoked(&m, Channel::SecurityPatch, "1.0.0").is_err());
        assert!(check_not_revoked(&m, Channel::SecurityPatch, "1.0.1").is_ok());
    }
}
