//! § header — `.ccpkg` binary container format.
//!
//! § BYTE-LEVEL LAYOUT  (HEADER_BYTES = 80, fixed size · little-endian)
//!
//! ```text
//! offset  size  field             notes
//! ──────  ────  ────────────────  ──────────────────────────────────────────
//! 0       4     magic             "CCPK" = 0x43 0x43 0x50 0x4B
//! 4       2     format_version    u16 LE · current = 1 · BREAKING bumps this
//! 6       1     content_kind      u8 · ContentKind discriminant 1..=8
//! 7       1     author_cap_class  u8 · AuthorCapClass discriminant 1..=5
//! 8       2     ver_major         u16 LE · semver
//! 10      2     ver_minor         u16 LE
//! 12      2     ver_patch         u16 LE
//! 14      2     reserved_a        u16 LE · MUST be 0
//! 16      8     created_ts_ns     u64 LE · build wall-clock (epoch ns)
//! 24      8     total_size        u64 LE · bytes of (manifest + archive)
//! 32      32    blake3_payload    BLAKE3-256 of (manifest_bytes || archive_bytes)
//! 64      16    reserved_b        16 bytes · MUST be all-zero · future-use
//! ──────  ────  ────────────────  ──────────────────────────────────────────
//! HEADER_BYTES = 80
//!
//! ──── then ────
//! 80      4     manifest_size     u32 LE · length of manifest_bytes
//! 84      M     manifest_bytes    JSON serialised Manifest
//! 84+M    A     archive_bytes     TARLITE payload (see `archive.rs`)
//! 84+M+A  64    ed25519_sig       Ed25519 signature over (header || manifest_size_le || manifest || archive)
//! +64     32    sigma_chain_anchor BLAKE3-32 of Σ-Chain commit attesting this package
//! ```
//!
//! Total file = `HEADER_BYTES (80) + 4 (manifest_size) + M (manifest) +
//!              A (archive) + 64 (sig) + 32 (anchor)`.
//!
//! § DETERMINISM
//!   The 16 bytes of `reserved_b` MUST be all-zero on write and MUST be
//!   all-zero-or-rejected on read. Future format-version-2 may give those
//!   bytes meaning, but format-version-1 verifiers reject any non-zero
//!   pattern, so existing signers cannot be tricked into signing extension
//!   data.
//!
//! § ENDIANNESS
//!   Little-endian throughout, matching x86_64 native and de-facto modern
//!   target conventions.

use crate::cap::AuthorCapClass;
use crate::kind::ContentKind;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 4-byte magic prefix : ASCII "CCPK" — Cssl-Content-PacKage.
pub const BUNDLE_MAGIC: [u8; 4] = *b"CCPK";

/// Current format version. Bumping is a BREAKING CHANGE.
pub const BUNDLE_FORMAT_VERSION: u16 = 1;

/// Fixed header size in bytes.
pub const HEADER_BYTES: usize = 80;

/// Ed25519 signature size in bytes.
pub const SIGNATURE_BYTES: usize = 64;

/// Σ-Chain anchor size in bytes (BLAKE3-32).
pub const ANCHOR_BYTES: usize = 32;

/// Length of the manifest_size LE u32 prefix.
pub const MANIFEST_SIZE_PREFIX: usize = 4;

/// One-line description of the byte layout, for spec-output and debugging.
pub const HEADER_LAYOUT: &str =
    "magic(4)CCPK | fmt(2) | kind(1) | cap(1) | ver(2+2+2) | rsvA(2) | ts(8) | size(8) | blake3(32) | rsvB(16) || manifest_size(4) | manifest(N) | archive(N) || sig(64) | anchor(32)";

/// § The fixed-size header. Read/written in little-endian, bit-packed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleHeader {
    pub format_version: u16,
    pub content_kind: ContentKind,
    pub author_cap_class: AuthorCapClass,
    pub ver_major: u16,
    pub ver_minor: u16,
    pub ver_patch: u16,
    /// Wall-clock epoch nanoseconds of bundle creation.
    pub created_ts_ns: u64,
    /// Bytes of (manifest_bytes || archive_bytes) following manifest_size.
    pub total_size: u64,
    /// BLAKE3-256 of (manifest_bytes || archive_bytes).
    pub blake3_payload: [u8; 32],
}

impl BundleHeader {
    /// Encode header to a fixed-size 80-byte buffer.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; HEADER_BYTES] {
        let mut buf = [0u8; HEADER_BYTES];
        buf[0..4].copy_from_slice(&BUNDLE_MAGIC);
        buf[4..6].copy_from_slice(&self.format_version.to_le_bytes());
        buf[6] = self.content_kind as u8;
        buf[7] = self.author_cap_class as u8;
        buf[8..10].copy_from_slice(&self.ver_major.to_le_bytes());
        buf[10..12].copy_from_slice(&self.ver_minor.to_le_bytes());
        buf[12..14].copy_from_slice(&self.ver_patch.to_le_bytes());
        // reserved_a (14..16) stays 0.
        buf[16..24].copy_from_slice(&self.created_ts_ns.to_le_bytes());
        buf[24..32].copy_from_slice(&self.total_size.to_le_bytes());
        buf[32..64].copy_from_slice(&self.blake3_payload);
        // reserved_b (64..80) stays 0.
        buf
    }

    /// Decode header from a byte slice. Strict on magic, format-version,
    /// kind/cap discriminants, and reserved-byte zero-ness.
    pub fn from_bytes(buf: &[u8]) -> Result<Self, BundleParseError> {
        if buf.len() < HEADER_BYTES {
            return Err(BundleParseError::TooShort {
                got: buf.len(),
                need: HEADER_BYTES,
            });
        }
        if buf[0..4] != BUNDLE_MAGIC {
            return Err(BundleParseError::BadMagic);
        }
        let format_version = u16::from_le_bytes([buf[4], buf[5]]);
        if format_version != BUNDLE_FORMAT_VERSION {
            return Err(BundleParseError::UnsupportedFormatVersion(format_version));
        }
        let content_kind = match buf[6] {
            1 => ContentKind::Scene,
            2 => ContentKind::Npc,
            3 => ContentKind::Recipe,
            4 => ContentKind::Lore,
            5 => ContentKind::System,
            6 => ContentKind::ShaderPack,
            7 => ContentKind::AudioPack,
            8 => ContentKind::Bundle,
            other => return Err(BundleParseError::UnknownContentKind(other)),
        };
        let author_cap_class = match buf[7] {
            1 => AuthorCapClass::Creator,
            2 => AuthorCapClass::Curator,
            3 => AuthorCapClass::Moderator,
            4 => AuthorCapClass::SubstrateTeam,
            5 => AuthorCapClass::Anonymous,
            other => return Err(BundleParseError::UnknownAuthorCapClass(other)),
        };
        let ver_major = u16::from_le_bytes([buf[8], buf[9]]);
        let ver_minor = u16::from_le_bytes([buf[10], buf[11]]);
        let ver_patch = u16::from_le_bytes([buf[12], buf[13]]);
        // reserved_a strict-zero.
        if buf[14] != 0 || buf[15] != 0 {
            return Err(BundleParseError::ReservedNonZero("reserved_a"));
        }
        let created_ts_ns = u64::from_le_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]);
        let total_size = u64::from_le_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]);
        let mut blake3_payload = [0u8; 32];
        blake3_payload.copy_from_slice(&buf[32..64]);
        // reserved_b strict-zero.
        for &b in &buf[64..80] {
            if b != 0 {
                return Err(BundleParseError::ReservedNonZero("reserved_b"));
            }
        }
        Ok(Self {
            format_version,
            content_kind,
            author_cap_class,
            ver_major,
            ver_minor,
            ver_patch,
            created_ts_ns,
            total_size,
            blake3_payload,
        })
    }

    /// Semver triple as `(major, minor, patch)`.
    #[must_use]
    pub fn version_triple(&self) -> (u16, u16, u16) {
        (self.ver_major, self.ver_minor, self.ver_patch)
    }
}

/// § Full `.ccpkg` bundle = header + manifest + archive + sig + anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub header: BundleHeader,
    /// JSON-serialised Manifest bytes.
    pub manifest_bytes: Vec<u8>,
    /// TARLITE archive bytes (CSSL source + assets).
    pub archive_bytes: Vec<u8>,
    /// Ed25519 signature over (header || manifest_size_le || manifest || archive).
    pub signature: [u8; SIGNATURE_BYTES],
    /// BLAKE3-32 of Σ-Chain commit attesting this package.
    pub sigma_chain_anchor: [u8; ANCHOR_BYTES],
}

impl Bundle {
    /// Encode bundle to its on-disk byte representation.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let header_bytes = self.header.to_bytes();
        let manifest_size_bytes = (self.manifest_bytes.len() as u32).to_le_bytes();
        let mut out = Vec::with_capacity(
            HEADER_BYTES
                + MANIFEST_SIZE_PREFIX
                + self.manifest_bytes.len()
                + self.archive_bytes.len()
                + SIGNATURE_BYTES
                + ANCHOR_BYTES,
        );
        out.extend_from_slice(&header_bytes);
        out.extend_from_slice(&manifest_size_bytes);
        out.extend_from_slice(&self.manifest_bytes);
        out.extend_from_slice(&self.archive_bytes);
        out.extend_from_slice(&self.signature);
        out.extend_from_slice(&self.sigma_chain_anchor);
        out
    }

    /// Decode bundle from on-disk bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BundleParseError> {
        let header = BundleHeader::from_bytes(bytes)?;
        let total_size = header.total_size as usize;
        // Layout : HEADER (80) | manifest_size(4) | manifest+archive (total_size) | sig(64) | anchor(32)
        let total_needed = HEADER_BYTES
            .checked_add(MANIFEST_SIZE_PREFIX)
            .and_then(|x| x.checked_add(total_size))
            .and_then(|x| x.checked_add(SIGNATURE_BYTES))
            .and_then(|x| x.checked_add(ANCHOR_BYTES))
            .ok_or(BundleParseError::SizeOverflow)?;
        if bytes.len() < total_needed {
            return Err(BundleParseError::TooShort {
                got: bytes.len(),
                need: total_needed,
            });
        }
        // Read manifest_size LE u32.
        let manifest_size_off = HEADER_BYTES;
        let manifest_size = u32::from_le_bytes([
            bytes[manifest_size_off],
            bytes[manifest_size_off + 1],
            bytes[manifest_size_off + 2],
            bytes[manifest_size_off + 3],
        ]) as usize;
        if manifest_size > total_size {
            return Err(BundleParseError::ManifestSizeExceedsTotal {
                manifest: manifest_size,
                total: total_size,
            });
        }
        let manifest_off = manifest_size_off + MANIFEST_SIZE_PREFIX;
        let archive_off = manifest_off + manifest_size;
        let archive_len = total_size - manifest_size;
        let manifest_bytes = bytes[manifest_off..archive_off].to_vec();
        let archive_bytes = bytes[archive_off..archive_off + archive_len].to_vec();
        let sig_off = archive_off + archive_len;
        let mut signature = [0u8; SIGNATURE_BYTES];
        signature.copy_from_slice(&bytes[sig_off..sig_off + SIGNATURE_BYTES]);
        let anchor_off = sig_off + SIGNATURE_BYTES;
        let mut sigma_chain_anchor = [0u8; ANCHOR_BYTES];
        sigma_chain_anchor.copy_from_slice(&bytes[anchor_off..anchor_off + ANCHOR_BYTES]);
        Ok(Self {
            header,
            manifest_bytes,
            archive_bytes,
            signature,
            sigma_chain_anchor,
        })
    }

    /// Canonical "to-be-signed" bytes : header || manifest_size_le ||
    /// manifest || archive. Excludes the signature itself and the
    /// Σ-Chain anchor (which is appended as supplementary attestation).
    #[must_use]
    pub fn signed_bytes(&self) -> Vec<u8> {
        let header_bytes = self.header.to_bytes();
        let manifest_size_bytes = (self.manifest_bytes.len() as u32).to_le_bytes();
        let mut buf = Vec::with_capacity(
            HEADER_BYTES
                + MANIFEST_SIZE_PREFIX
                + self.manifest_bytes.len()
                + self.archive_bytes.len(),
        );
        buf.extend_from_slice(&header_bytes);
        buf.extend_from_slice(&manifest_size_bytes);
        buf.extend_from_slice(&self.manifest_bytes);
        buf.extend_from_slice(&self.archive_bytes);
        buf
    }

    /// Combined payload bytes : (manifest_bytes || archive_bytes).
    /// Used to compute / verify `header.blake3_payload`.
    #[must_use]
    pub fn payload_bytes(&self) -> Vec<u8> {
        let mut buf =
            Vec::with_capacity(self.manifest_bytes.len() + self.archive_bytes.len());
        buf.extend_from_slice(&self.manifest_bytes);
        buf.extend_from_slice(&self.archive_bytes);
        buf
    }
}

/// § Parsing errors. Disjoint from sign / verify / unpack.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BundleParseError {
    #[error("bundle too short : got {got} bytes, need at least {need}")]
    TooShort { got: usize, need: usize },
    #[error("bad magic prefix : not 'CCPK'")]
    BadMagic,
    #[error("unsupported bundle format version {0}")]
    UnsupportedFormatVersion(u16),
    #[error("unknown content-kind discriminant {0}")]
    UnknownContentKind(u8),
    #[error("unknown author-cap-class discriminant {0}")]
    UnknownAuthorCapClass(u8),
    #[error("reserved bytes ({0}) must be all-zero")]
    ReservedNonZero(&'static str),
    #[error("size overflow")]
    SizeOverflow,
    #[error("declared manifest size {manifest} exceeds total payload size {total}")]
    ManifestSizeExceedsTotal { manifest: usize, total: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_header(payload: &[u8]) -> BundleHeader {
        BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            content_kind: ContentKind::Scene,
            author_cap_class: AuthorCapClass::Creator,
            ver_major: 1,
            ver_minor: 2,
            ver_patch: 3,
            created_ts_ns: 1_700_000_000_000_000_000,
            total_size: payload.len() as u64,
            blake3_payload: blake3::hash(payload).into(),
        }
    }

    #[test]
    fn header_size_is_80() {
        let h = fixture_header(&[]);
        assert_eq!(h.to_bytes().len(), HEADER_BYTES);
    }

    #[test]
    fn header_roundtrip() {
        let payload = b"manifestjson|archivebytes";
        let h = fixture_header(payload);
        let buf = h.to_bytes();
        let back = BundleHeader::from_bytes(&buf).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn bad_magic_rejected() {
        let mut buf = fixture_header(&[]).to_bytes();
        buf[0] = 0x00;
        assert_eq!(BundleHeader::from_bytes(&buf), Err(BundleParseError::BadMagic));
    }

    #[test]
    fn magic_is_ccpk() {
        assert_eq!(BUNDLE_MAGIC, *b"CCPK");
    }

    #[test]
    fn unsupported_format_version_rejected() {
        let mut buf = fixture_header(&[]).to_bytes();
        buf[4] = 99;
        let r = BundleHeader::from_bytes(&buf);
        assert!(matches!(r, Err(BundleParseError::UnsupportedFormatVersion(_))));
    }

    #[test]
    fn unknown_content_kind_rejected() {
        let mut buf = fixture_header(&[]).to_bytes();
        buf[6] = 99;
        let r = BundleHeader::from_bytes(&buf);
        assert!(matches!(r, Err(BundleParseError::UnknownContentKind(99))));
    }

    #[test]
    fn unknown_cap_class_rejected() {
        let mut buf = fixture_header(&[]).to_bytes();
        buf[7] = 99;
        let r = BundleHeader::from_bytes(&buf);
        assert!(matches!(r, Err(BundleParseError::UnknownAuthorCapClass(99))));
    }

    #[test]
    fn reserved_a_non_zero_rejected() {
        let mut buf = fixture_header(&[]).to_bytes();
        buf[14] = 0x42;
        assert!(matches!(
            BundleHeader::from_bytes(&buf),
            Err(BundleParseError::ReservedNonZero("reserved_a"))
        ));
    }

    #[test]
    fn reserved_b_non_zero_rejected() {
        let mut buf = fixture_header(&[]).to_bytes();
        buf[64] = 0x99;
        assert!(matches!(
            BundleHeader::from_bytes(&buf),
            Err(BundleParseError::ReservedNonZero("reserved_b"))
        ));
    }

    #[test]
    fn too_short_rejected() {
        let buf = vec![0u8; 10];
        assert!(matches!(
            BundleHeader::from_bytes(&buf),
            Err(BundleParseError::TooShort { got: 10, need: 80 })
        ));
    }

    #[test]
    fn full_bundle_roundtrip() {
        let manifest = br#"{"id":"test","ver":"1.0.0"}"#.to_vec();
        let archive = vec![0xAAu8, 0xBB, 0xCC, 0xDD, 0xEE];
        let combined = {
            let mut buf = manifest.clone();
            buf.extend_from_slice(&archive);
            buf
        };
        let header = BundleHeader {
            blake3_payload: blake3::hash(&combined).into(),
            total_size: combined.len() as u64,
            ..fixture_header(&combined)
        };
        let bundle = Bundle {
            header,
            manifest_bytes: manifest,
            archive_bytes: archive,
            signature: [0x77; SIGNATURE_BYTES],
            sigma_chain_anchor: [0x11; ANCHOR_BYTES],
        };
        let bytes = bundle.to_bytes();
        let expected_size =
            HEADER_BYTES + MANIFEST_SIZE_PREFIX + bundle.manifest_bytes.len()
                + bundle.archive_bytes.len() + SIGNATURE_BYTES + ANCHOR_BYTES;
        assert_eq!(bytes.len(), expected_size);
        let back = Bundle::from_bytes(&bytes).unwrap();
        assert_eq!(bundle, back);
    }

    #[test]
    fn full_bundle_truncated_rejected() {
        let manifest = b"{}".to_vec();
        let archive = vec![1u8, 2, 3];
        let combined = {
            let mut buf = manifest.clone();
            buf.extend_from_slice(&archive);
            buf
        };
        let header = BundleHeader {
            blake3_payload: blake3::hash(&combined).into(),
            total_size: combined.len() as u64,
            ..fixture_header(&combined)
        };
        let bundle = Bundle {
            header,
            manifest_bytes: manifest,
            archive_bytes: archive,
            signature: [0u8; SIGNATURE_BYTES],
            sigma_chain_anchor: [0u8; ANCHOR_BYTES],
        };
        let bytes = bundle.to_bytes();
        let truncated = &bytes[..bytes.len() - 5];
        assert!(matches!(
            Bundle::from_bytes(truncated),
            Err(BundleParseError::TooShort { .. })
        ));
    }

    #[test]
    fn signed_bytes_excludes_signature_and_anchor() {
        let manifest = b"{}".to_vec();
        let archive: Vec<u8> = vec![];
        let combined = manifest.as_slice();
        let header = BundleHeader {
            blake3_payload: blake3::hash(combined).into(),
            total_size: combined.len() as u64,
            ..fixture_header(combined)
        };
        let bundle = Bundle {
            header,
            manifest_bytes: manifest.clone(),
            archive_bytes: archive,
            signature: [0xAB; SIGNATURE_BYTES],
            sigma_chain_anchor: [0xCD; ANCHOR_BYTES],
        };
        let signed = bundle.signed_bytes();
        // Should be HEADER + 4 (manifest_size) + manifest_len + archive_len.
        assert_eq!(signed.len(), HEADER_BYTES + MANIFEST_SIZE_PREFIX + manifest.len());
    }

    #[test]
    fn payload_bytes_excludes_header() {
        let manifest = b"abc".to_vec();
        let archive = vec![1u8, 2];
        let bundle = Bundle {
            header: fixture_header(&[]),
            manifest_bytes: manifest.clone(),
            archive_bytes: archive.clone(),
            signature: [0u8; SIGNATURE_BYTES],
            sigma_chain_anchor: [0u8; ANCHOR_BYTES],
        };
        let p = bundle.payload_bytes();
        let mlen = manifest.len();
        assert_eq!(p.len(), mlen + archive.len());
        assert_eq!(&p[..mlen], &manifest[..]);
        assert_eq!(&p[mlen..], &archive[..]);
    }

    #[test]
    fn manifest_size_exceeds_total_rejected() {
        // Hand-craft : header.total_size = 5, but manifest_size prefix says 99.
        let header = BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            content_kind: ContentKind::Scene,
            author_cap_class: AuthorCapClass::Creator,
            ver_major: 1,
            ver_minor: 0,
            ver_patch: 0,
            created_ts_ns: 0,
            total_size: 5,
            blake3_payload: [0u8; 32],
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&header.to_bytes());
        // Manifest_size = 99 (LIE).
        bytes.extend_from_slice(&99u32.to_le_bytes());
        // Add 5 bytes of payload + sig + anchor to satisfy the length check.
        bytes.extend_from_slice(&[0u8; 5]);
        bytes.extend_from_slice(&[0u8; SIGNATURE_BYTES]);
        bytes.extend_from_slice(&[0u8; ANCHOR_BYTES]);
        let r = Bundle::from_bytes(&bytes);
        assert!(matches!(r, Err(BundleParseError::ManifestSizeExceedsTotal { .. })));
    }
}
