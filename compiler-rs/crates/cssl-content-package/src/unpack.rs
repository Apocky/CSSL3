//! § unpack — bundle → CSSL source + asset bytes + manifest.
//!
//! § FLOW
//!   `unpack_bundle(bundle) -> Result<UnpackedContent, UnpackError>`
//!
//!   1. `verify_bundle(bundle)` (gates the surface ; UnpackError wraps VerifyError).
//!   2. Deserialise manifest from `bundle.manifest_bytes`.
//!   3. Decode archive → `Vec<ArchiveEntry>` via `archive_unpack`.
//!   4. Split entries by extension : `.cssl` / `.csl` → CSSL source ; rest → assets.
//!   5. Emit `UnpackedContent { manifest · cssl_sources · assets }`.

use crate::archive::{archive_unpack, ArchiveError};
use crate::header::Bundle;
use crate::manifest::Manifest;
use crate::verify::{verify_bundle, VerifyError};
use std::collections::BTreeMap;
use thiserror::Error;

/// § The unpacked surface of a verified `.ccpkg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpackedContent {
    /// Verified manifest (struct-decoded from the signed JSON bytes).
    pub manifest: Manifest,
    /// CSSL source files, keyed by archive path.
    /// Path → raw UTF-8 source text.
    pub cssl_sources: BTreeMap<String, String>,
    /// Non-CSSL assets, keyed by archive path.
    /// Path → raw bytes.
    pub assets: BTreeMap<String, Vec<u8>>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UnpackError {
    #[error("verify failed : {0}")]
    Verify(#[from] VerifyError),
    #[error("archive decode failed : {0}")]
    Archive(#[from] ArchiveError),
    #[error("manifest deserialise failed : {0}")]
    ManifestDeserialise(String),
    #[error("CSSL source at '{path}' is not valid UTF-8")]
    SourceNotUtf8 { path: String },
}

/// § Unpack a verified bundle.
///
/// Refuses to surface contents on any verification failure.
pub fn unpack_bundle(bundle: &Bundle) -> Result<UnpackedContent, UnpackError> {
    verify_bundle(bundle)?;

    // Deserialise manifest.
    let manifest_str = std::str::from_utf8(&bundle.manifest_bytes)
        .map_err(|e| UnpackError::ManifestDeserialise(e.to_string()))?;
    let manifest: Manifest = serde_json::from_str(manifest_str)
        .map_err(|e| UnpackError::ManifestDeserialise(e.to_string()))?;

    // Unpack archive.
    let entries = archive_unpack(&bundle.archive_bytes)?;

    let mut cssl_sources = BTreeMap::new();
    let mut assets = BTreeMap::new();
    for e in entries {
        if is_cssl_path(&e.path) {
            let src = String::from_utf8(e.content)
                .map_err(|_| UnpackError::SourceNotUtf8 { path: e.path.clone() })?;
            cssl_sources.insert(e.path, src);
        } else {
            assets.insert(e.path, e.content);
        }
    }

    Ok(UnpackedContent {
        manifest,
        cssl_sources,
        assets,
    })
}

/// Is the path a CSSL source file ? (`.cssl` or `.csl` extension.)
#[must_use]
fn is_cssl_path(p: &str) -> bool {
    p.ends_with(".cssl") || p.ends_with(".csl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::{archive_pack, ArchiveEntry};
    use crate::cap::AuthorCapClass;
    use crate::header::ANCHOR_BYTES;
    use crate::kind::ContentKind;
    use crate::manifest::LicenseTier;
    use crate::sign::sign_bundle;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn fixture_manifest(pubkey: [u8; 32]) -> Manifest {
        Manifest {
            id: "test.scene".to_string(),
            version: "1.0.0".to_string(),
            kind: ContentKind::Scene,
            author_pubkey: pubkey,
            name: "Test Scene".to_string(),
            description: "test".to_string(),
            depends_on: vec![],
            remix_of: None,
            tags: vec![],
            sigma_mask: 0,
            gift_economy_only: true,
            license: LicenseTier::Open,
        }
    }

    fn build_signed_bundle() -> Bundle {
        let key = SigningKey::generate(&mut OsRng);
        let pubkey: [u8; 32] = key.verifying_key().to_bytes();
        let manifest = fixture_manifest(pubkey);
        let entries = vec![
            ArchiveEntry {
                path: "scenes/main.cssl".to_string(),
                content: b"§ scene main\n  ¬ harm".to_vec(),
            },
            ArchiveEntry {
                path: "assets/torch.gltf".to_string(),
                content: vec![0x67, 0x6c, 0x54, 0x46],
            },
        ];
        let archive = archive_pack(&entries).unwrap();
        sign_bundle(
            archive,
            manifest,
            AuthorCapClass::Creator,
            [0u8; ANCHOR_BYTES],
            &key,
            (1, 0, 0),
            0,
        )
        .unwrap()
    }

    #[test]
    fn unpack_round_trip_succeeds() {
        let bundle = build_signed_bundle();
        let u = unpack_bundle(&bundle).unwrap();
        assert_eq!(u.manifest.id, "test.scene");
        assert_eq!(u.cssl_sources.len(), 1);
        assert!(u.cssl_sources.contains_key("scenes/main.cssl"));
        assert_eq!(u.assets.len(), 1);
        assert!(u.assets.contains_key("assets/torch.gltf"));
    }

    #[test]
    fn unpack_tampered_bundle_rejected() {
        let mut bundle = build_signed_bundle();
        bundle.archive_bytes[bundle.archive_bytes.len() - 1] ^= 0xFF;
        assert!(unpack_bundle(&bundle).is_err());
    }

    #[test]
    fn unpack_csl_extension_treated_as_cssl_source() {
        let key = SigningKey::generate(&mut OsRng);
        let pubkey: [u8; 32] = key.verifying_key().to_bytes();
        let manifest = fixture_manifest(pubkey);
        let entries = vec![ArchiveEntry {
            path: "core.csl".to_string(),
            content: b"§ S T11".to_vec(),
        }];
        let archive = archive_pack(&entries).unwrap();
        let bundle = sign_bundle(
            archive,
            manifest,
            AuthorCapClass::Creator,
            [0u8; ANCHOR_BYTES],
            &key,
            (1, 0, 0),
            0,
        )
        .unwrap();
        let u = unpack_bundle(&bundle).unwrap();
        assert_eq!(u.cssl_sources.len(), 1);
        assert!(u.cssl_sources.contains_key("core.csl"));
    }

    #[test]
    fn unpack_empty_archive_works() {
        let key = SigningKey::generate(&mut OsRng);
        let pubkey: [u8; 32] = key.verifying_key().to_bytes();
        let manifest = fixture_manifest(pubkey);
        let archive = archive_pack(&[]).unwrap();
        let bundle = sign_bundle(
            archive,
            manifest,
            AuthorCapClass::Creator,
            [0u8; ANCHOR_BYTES],
            &key,
            (1, 0, 0),
            0,
        )
        .unwrap();
        let u = unpack_bundle(&bundle).unwrap();
        assert!(u.cssl_sources.is_empty());
        assert!(u.assets.is_empty());
    }
}
