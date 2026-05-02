//! § bundle — `.csslfix` binary container format.
//!
//! § BYTE-LEVEL LAYOUT  (HEADER_BYTES = 80, fixed size · little-endian)
//!
//! ```text
//! offset  size  field            notes
//! ──────  ────  ───────────────  ──────────────────────────────────────────
//! 0       4     magic            "CSFX" = 0x43 0x53 0x46 0x58
//! 4       2     format_version   u16 LE · current = 1 · BREAKING bumps this
//! 6       1     channel          u8 · Channel discriminant 1..=9
//! 7       1     cap_role         u8 · CapRole discriminant 1..=5
//! 8       2     ver_major        u16 LE · semver
//! 10      2     ver_minor        u16 LE
//! 12      2     ver_patch        u16 LE
//! 14      2     reserved_a       u16 LE · MUST be 0
//! 16      8     timestamp_ns     u64 LE · build wall-clock (epoch ns)
//! 24      8     payload_size     u64 LE · bytes of payload following header
//! 32      32    payload_blake3   raw BLAKE3-256 of payload bytes
//! 64      16    reserved_b       16 bytes · MUST be all-zero · future-use
//! ──────  ────  ───────────────  ──────────────────────────────────────────
//! HEADER_BYTES = 80
//!
//! ──── then ────
//! 80      N     payload          payload_size bytes
//! 80+N    64    ed25519_sig      Ed25519 signature over header || payload
//! ```
//!
//! Total file size = `HEADER_BYTES + payload_size + 64`.
//!
//! § DETERMINISM
//!   The 16 bytes of `reserved_b` MUST be all-zero on write and MUST be
//!   all-zero-or-rejected on read. This means existing signers cannot be
//!   tricked into signing extension-data : a future format-version-2 may
//!   give those bytes meaning, but format-version-1 verifiers reject any
//!   non-zero pattern.
//!
//! § ENDIANNESS
//!   Little-endian throughout. Matches x86_64 native and is the de-facto
//!   choice for most modern targets.

use crate::cap::CapRole;
use crate::channel::Channel;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 4-byte magic prefix : ASCII "CSFX" — Cssl-Fix.
pub const BUNDLE_MAGIC: [u8; 4] = *b"CSFX";

/// Current format version. Bumping this is a BREAKING CHANGE.
pub const BUNDLE_FORMAT_VERSION: u16 = 1;

/// Fixed header size in bytes.
pub const HEADER_BYTES: usize = 80;

/// Ed25519 signature size in bytes.
pub const SIGNATURE_BYTES: usize = 64;

/// One-line description of the byte layout, for spec-output and debugging.
pub const HEADER_LAYOUT: &str =
    "magic(4) | fmt(2) | chan(1) | cap(1) | ver(2+2+2) | rsvA(2) | ts(8) | size(8) | blake3(32) | rsvB(16)";

/// § The fixed-size header. Read/written in little-endian, bit-packed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleHeader {
    pub format_version: u16,
    pub channel: Channel,
    pub cap_role: CapRole,
    pub ver_major: u16,
    pub ver_minor: u16,
    pub ver_patch: u16,
    /// epoch nanoseconds.
    pub timestamp_ns: u64,
    /// Bytes of payload following header.
    pub payload_size: u64,
    /// Raw 32-byte BLAKE3-256 of payload bytes.
    pub payload_blake3: [u8; 32],
}

impl BundleHeader {
    /// Encode the header to a fixed-size 80-byte buffer.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; HEADER_BYTES] {
        let mut buf = [0u8; HEADER_BYTES];
        buf[0..4].copy_from_slice(&BUNDLE_MAGIC);
        buf[4..6].copy_from_slice(&self.format_version.to_le_bytes());
        buf[6] = self.channel as u8;
        buf[7] = self.cap_role as u8;
        buf[8..10].copy_from_slice(&self.ver_major.to_le_bytes());
        buf[10..12].copy_from_slice(&self.ver_minor.to_le_bytes());
        buf[12..14].copy_from_slice(&self.ver_patch.to_le_bytes());
        // reserved_a (14..16) stays 0.
        buf[16..24].copy_from_slice(&self.timestamp_ns.to_le_bytes());
        buf[24..32].copy_from_slice(&self.payload_size.to_le_bytes());
        buf[32..64].copy_from_slice(&self.payload_blake3);
        // reserved_b (64..80) stays 0.
        buf
    }

    /// Decode a header from a byte slice. Strict on magic, format-version,
    /// channel/cap discriminants, and reserved-byte zero-ness.
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
        let channel = match buf[6] {
            1 => Channel::LoaBinary,
            2 => Channel::CsslBundle,
            3 => Channel::KanWeights,
            4 => Channel::BalanceConfig,
            5 => Channel::RecipeBook,
            6 => Channel::NemesisBestiary,
            7 => Channel::SecurityPatch,
            8 => Channel::StoryletContent,
            9 => Channel::RenderPipeline,
            other => return Err(BundleParseError::UnknownChannel(other)),
        };
        let cap_role = match buf[7] {
            1 => CapRole::CapA,
            2 => CapRole::CapB,
            3 => CapRole::CapC,
            4 => CapRole::CapD,
            5 => CapRole::CapE,
            other => return Err(BundleParseError::UnknownCapRole(other)),
        };
        let ver_major = u16::from_le_bytes([buf[8], buf[9]]);
        let ver_minor = u16::from_le_bytes([buf[10], buf[11]]);
        let ver_patch = u16::from_le_bytes([buf[12], buf[13]]);
        // reserved_a strict-zero.
        if buf[14] != 0 || buf[15] != 0 {
            return Err(BundleParseError::ReservedNonZero("reserved_a"));
        }
        let timestamp_ns = u64::from_le_bytes([
            buf[16], buf[17], buf[18], buf[19], buf[20], buf[21], buf[22], buf[23],
        ]);
        let payload_size = u64::from_le_bytes([
            buf[24], buf[25], buf[26], buf[27], buf[28], buf[29], buf[30], buf[31],
        ]);
        let mut payload_blake3 = [0u8; 32];
        payload_blake3.copy_from_slice(&buf[32..64]);
        // reserved_b strict-zero.
        for &b in &buf[64..80] {
            if b != 0 {
                return Err(BundleParseError::ReservedNonZero("reserved_b"));
            }
        }
        Ok(Self {
            format_version,
            channel,
            cap_role,
            ver_major,
            ver_minor,
            ver_patch,
            timestamp_ns,
            payload_size,
            payload_blake3,
        })
    }

    /// Semver triple as `(major, minor, patch)`.
    #[must_use]
    pub fn version_triple(&self) -> (u16, u16, u16) {
        (self.ver_major, self.ver_minor, self.ver_patch)
    }
}

/// § Full bundle = header + payload + 64-byte signature.
///
/// On disk the wire format is `header_bytes || payload_bytes ||
/// signature_bytes` ; in memory we keep them as separate `Vec` / array
/// fields so signing + verification can stream-hash header || payload
/// without an extra concat allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub header: BundleHeader,
    pub payload: Vec<u8>,
    /// Ed25519 signature over (header_bytes || payload_bytes).
    pub signature: [u8; SIGNATURE_BYTES],
}

impl Bundle {
    /// Encode the full bundle to its on-disk byte representation.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let header_bytes = self.header.to_bytes();
        let mut out = Vec::with_capacity(HEADER_BYTES + self.payload.len() + SIGNATURE_BYTES);
        out.extend_from_slice(&header_bytes);
        out.extend_from_slice(&self.payload);
        out.extend_from_slice(&self.signature);
        out
    }

    /// Decode a full bundle from on-disk bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BundleParseError> {
        let header = BundleHeader::from_bytes(bytes)?;
        let payload_size = header.payload_size as usize;
        let total_needed = HEADER_BYTES
            .checked_add(payload_size)
            .and_then(|x| x.checked_add(SIGNATURE_BYTES))
            .ok_or(BundleParseError::PayloadSizeOverflow)?;
        if bytes.len() < total_needed {
            return Err(BundleParseError::TooShort {
                got: bytes.len(),
                need: total_needed,
            });
        }
        let payload = bytes[HEADER_BYTES..HEADER_BYTES + payload_size].to_vec();
        let mut signature = [0u8; SIGNATURE_BYTES];
        signature.copy_from_slice(
            &bytes[HEADER_BYTES + payload_size..HEADER_BYTES + payload_size + SIGNATURE_BYTES],
        );
        Ok(Self {
            header,
            payload,
            signature,
        })
    }

    /// Canonical "to-be-signed" bytes : header || payload (no signature).
    #[must_use]
    pub fn signed_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_BYTES + self.payload.len());
        buf.extend_from_slice(&self.header.to_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }
}

/// § Parsing errors. Disjoint from sign / verify / apply.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum BundleParseError {
    #[error("bundle too short : got {got} bytes, need at least {need}")]
    TooShort { got: usize, need: usize },
    #[error("bad magic prefix : not 'CSFX'")]
    BadMagic,
    #[error("unsupported bundle format version {0}")]
    UnsupportedFormatVersion(u16),
    #[error("unknown channel discriminant {0}")]
    UnknownChannel(u8),
    #[error("unknown cap-role discriminant {0}")]
    UnknownCapRole(u8),
    #[error("reserved bytes ({0}) must be all-zero")]
    ReservedNonZero(&'static str),
    #[error("payload-size overflow")]
    PayloadSizeOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_header() -> BundleHeader {
        BundleHeader {
            format_version: BUNDLE_FORMAT_VERSION,
            channel: Channel::SecurityPatch,
            cap_role: CapRole::CapD,
            ver_major: 1,
            ver_minor: 2,
            ver_patch: 3,
            timestamp_ns: 1_700_000_000_000_000_000,
            payload_size: 4,
            payload_blake3: blake3::hash(&[0xAAu8, 0xBB, 0xCC, 0xDD]).into(),
        }
    }

    #[test]
    fn header_size_is_80() {
        let h = fixture_header();
        assert_eq!(h.to_bytes().len(), HEADER_BYTES);
    }

    #[test]
    fn header_roundtrip() {
        let h = fixture_header();
        let buf = h.to_bytes();
        let back = BundleHeader::from_bytes(&buf).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn bad_magic_rejected() {
        let mut buf = fixture_header().to_bytes();
        buf[0] = 0x00;
        assert_eq!(BundleHeader::from_bytes(&buf), Err(BundleParseError::BadMagic));
    }

    #[test]
    fn unsupported_format_version_rejected() {
        let mut buf = fixture_header().to_bytes();
        buf[4] = 99; // format_version low byte
        let r = BundleHeader::from_bytes(&buf);
        assert!(matches!(r, Err(BundleParseError::UnsupportedFormatVersion(_))));
    }

    #[test]
    fn unknown_channel_rejected() {
        let mut buf = fixture_header().to_bytes();
        buf[6] = 99;
        let r = BundleHeader::from_bytes(&buf);
        assert!(matches!(r, Err(BundleParseError::UnknownChannel(99))));
    }

    #[test]
    fn unknown_cap_rejected() {
        let mut buf = fixture_header().to_bytes();
        buf[7] = 99;
        let r = BundleHeader::from_bytes(&buf);
        assert!(matches!(r, Err(BundleParseError::UnknownCapRole(99))));
    }

    #[test]
    fn reserved_non_zero_rejected() {
        let mut buf = fixture_header().to_bytes();
        buf[14] = 0x42;
        assert!(matches!(
            BundleHeader::from_bytes(&buf),
            Err(BundleParseError::ReservedNonZero("reserved_a"))
        ));

        let mut buf = fixture_header().to_bytes();
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
        let payload = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let header = BundleHeader {
            payload_blake3: blake3::hash(&payload).into(),
            payload_size: payload.len() as u64,
            ..fixture_header()
        };
        let bundle = Bundle {
            header,
            payload,
            signature: [0x77; SIGNATURE_BYTES],
        };
        let bytes = bundle.to_bytes();
        assert_eq!(bytes.len(), HEADER_BYTES + 4 + SIGNATURE_BYTES);
        let back = Bundle::from_bytes(&bytes).unwrap();
        assert_eq!(bundle, back);
    }

    #[test]
    fn full_bundle_truncated_rejected() {
        let payload = vec![1u8, 2, 3];
        let header = BundleHeader {
            payload_blake3: blake3::hash(&payload).into(),
            payload_size: payload.len() as u64,
            ..fixture_header()
        };
        let bundle = Bundle {
            header,
            payload,
            signature: [0u8; SIGNATURE_BYTES],
        };
        let bytes = bundle.to_bytes();
        let truncated = &bytes[..bytes.len() - 5];
        assert!(matches!(
            Bundle::from_bytes(truncated),
            Err(BundleParseError::TooShort { .. })
        ));
    }

    #[test]
    fn signed_bytes_excludes_signature() {
        let bundle = Bundle {
            header: BundleHeader {
                payload_size: 0,
                payload_blake3: blake3::hash(&[]).into(),
                ..fixture_header()
            },
            payload: vec![],
            signature: [0xAB; SIGNATURE_BYTES],
        };
        let signed = bundle.signed_bytes();
        assert_eq!(signed.len(), HEADER_BYTES);
    }
}
