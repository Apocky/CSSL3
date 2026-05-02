//! § record — FlagRecord 32-byte bit-packed wire-stable record.
//! ════════════════════════════════════════════════════════════════════════
//!
//! § BIT-PACK LAYOUT (LE · 32 bytes total)
//!   ┌───────────┬────────┬────────────────────────────────────────────────┐
//!   │ offset    │ bytes  │ field                                          │
//!   ├───────────┼────────┼────────────────────────────────────────────────┤
//!   │  0..8     │   8    │ flagger_pubkey_hash    (BLAKE3-trunc · u64 LE) │
//!   │  8..12    │   4    │ content_id             (u32 LE)                │
//!   │ 12        │   1    │ flag_kind              (FlagKind disc · u8)    │
//!   │ 13        │   1    │ severity               (0..=100 · u8)          │
//!   │ 14        │   1    │ sigma_mask             (Σ-cap-bits · u8)       │
//!   │ 15        │   1    │ reserved               (must be 0)             │
//!   │ 16..20    │   4    │ ts                     (epoch-seconds · u32 LE)│
//!   │ 20..28    │   8    │ rationale_short        (BLAKE3-trunc · u64 LE) │
//!   │ 28..32    │   4    │ sig_trunc              (Ed25519-trunc · u32 LE)│
//!   └───────────┴────────┴────────────────────────────────────────────────┘
//!
//! Wire-stable. flagger_pubkey_hash is non-recoverable-to-pubkey.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// § FlagKind — narrow taxonomy. Adding a variant requires PRIME-DIRECTIVE
/// review. Discriminants are wire-stable.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum FlagKind {
    PrimeDirectiveViolation = 0,
    HarmTowardOthers = 1,
    SurveillanceMechanism = 2,
    PayForPowerLeak = 3,
    AttributionFraud = 4,
    ProhibitedContent = 5,
    Spam = 6,
    Other = 7,
}

impl FlagKind {
    /// Try-from u8 discriminant. Returns None for invalid bytes (defense).
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::PrimeDirectiveViolation),
            1 => Some(Self::HarmTowardOthers),
            2 => Some(Self::SurveillanceMechanism),
            3 => Some(Self::PayForPowerLeak),
            4 => Some(Self::AttributionFraud),
            5 => Some(Self::ProhibitedContent),
            6 => Some(Self::Spam),
            7 => Some(Self::Other),
            _ => None,
        }
    }
}

/// Reserved-bits mask for sigma_mask byte. Reserved bits MUST be zero.
pub const SIGMA_MASK_RESERVED: u8 = 0b1100_0000;

/// Errors during FlagRecord pack/unpack.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum RecordError {
    #[error("severity out of range : {0} (must be 0..=100)")]
    SeverityOutOfRange(u8),
    #[error("invalid flag_kind discriminant : {0}")]
    InvalidFlagKind(u8),
    #[error("reserved-byte non-zero : 0x{0:02x}")]
    ReservedNonZero(u8),
    #[error("reserved sigma_mask bits non-zero : 0x{0:02x}")]
    ReservedSigmaBits(u8),
}

/// § FlagRecord — 32-byte canonical packed form.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlagRecord {
    /// § raw — 32-byte canonical packed form.
    pub raw: [u8; 32],
}

impl std::fmt::Debug for FlagRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlagRecord")
            .field("flagger_pubkey_hash", &format_args!("{:016x}", self.flagger_pubkey_hash()))
            .field("content_id", &self.content_id())
            .field("flag_kind", &self.flag_kind())
            .field("severity", &self.severity())
            .field("sigma_mask", &format_args!("0b{:08b}", self.sigma_mask()))
            .field("ts", &self.ts())
            .field("rationale_short", &format_args!("{:016x}", self.rationale_short()))
            .field("sig_trunc", &format_args!("0x{:08x}", self.sig_trunc()))
            .finish()
    }
}

impl FlagRecord {
    /// Pack the eight fields into the wire-stable 32-byte record.
    pub fn pack(
        flagger_pubkey_hash: u64,
        content_id: u32,
        kind: FlagKind,
        severity: u8,
        sigma_mask: u8,
        ts: u32,
        rationale_short: u64,
        sig_trunc: u32,
    ) -> Result<Self, RecordError> {
        if severity > 100 {
            return Err(RecordError::SeverityOutOfRange(severity));
        }
        if sigma_mask & SIGMA_MASK_RESERVED != 0 {
            return Err(RecordError::ReservedSigmaBits(sigma_mask));
        }
        let mut raw = [0u8; 32];
        raw[0..8].copy_from_slice(&flagger_pubkey_hash.to_le_bytes());
        raw[8..12].copy_from_slice(&content_id.to_le_bytes());
        raw[12] = kind as u8;
        raw[13] = severity;
        raw[14] = sigma_mask;
        raw[15] = 0; // reserved
        raw[16..20].copy_from_slice(&ts.to_le_bytes());
        raw[20..28].copy_from_slice(&rationale_short.to_le_bytes());
        raw[28..32].copy_from_slice(&sig_trunc.to_le_bytes());
        Ok(Self { raw })
    }

    /// Decode from raw bytes with validation.
    pub fn from_raw_validated(raw: [u8; 32]) -> Result<Self, RecordError> {
        if raw[15] != 0 {
            return Err(RecordError::ReservedNonZero(raw[15]));
        }
        if FlagKind::from_u8(raw[12]).is_none() {
            return Err(RecordError::InvalidFlagKind(raw[12]));
        }
        if raw[13] > 100 {
            return Err(RecordError::SeverityOutOfRange(raw[13]));
        }
        if raw[14] & SIGMA_MASK_RESERVED != 0 {
            return Err(RecordError::ReservedSigmaBits(raw[14]));
        }
        Ok(Self { raw })
    }

    pub fn flagger_pubkey_hash(&self) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.raw[0..8]);
        u64::from_le_bytes(b)
    }
    pub fn content_id(&self) -> u32 {
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.raw[8..12]);
        u32::from_le_bytes(b)
    }
    pub fn flag_kind(&self) -> FlagKind {
        FlagKind::from_u8(self.raw[12]).unwrap_or(FlagKind::Other)
    }
    pub fn severity(&self) -> u8 {
        self.raw[13]
    }
    pub fn sigma_mask(&self) -> u8 {
        self.raw[14]
    }
    pub fn ts(&self) -> u32 {
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.raw[16..20]);
        u32::from_le_bytes(b)
    }
    pub fn rationale_short(&self) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.raw[20..28]);
        u64::from_le_bytes(b)
    }
    pub fn sig_trunc(&self) -> u32 {
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.raw[28..32]);
        u32::from_le_bytes(b)
    }

    /// BLAKE3-truncate a pubkey (32 bytes) into a non-recoverable u64 handle.
    pub fn pubkey_handle(pubkey_bytes: &[u8]) -> u64 {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"content-moderation\0flagger\0v1");
        hasher.update(pubkey_bytes);
        let mut b = [0u8; 8];
        b.copy_from_slice(&hasher.finalize().as_bytes()[0..8]);
        u64::from_le_bytes(b)
    }

    /// BLAKE3-truncate rationale-text into a u64 handle (non-recoverable).
    pub fn rationale_hash(text: &str) -> u64 {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"content-moderation\0rationale\0v1");
        hasher.update(text.as_bytes());
        let mut b = [0u8; 8];
        b.copy_from_slice(&hasher.finalize().as_bytes()[0..8]);
        u64::from_le_bytes(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let r = FlagRecord::pack(
            0xDEAD_BEEF_CAFE_BABE,
            42,
            FlagKind::HarmTowardOthers,
            85,
            0b0000_0011,
            1_700_000_000,
            0x1122_3344_5566_7788,
            0xAABB_CCDD,
        )
        .expect("pack ok");
        assert_eq!(r.flagger_pubkey_hash(), 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(r.content_id(), 42);
        assert_eq!(r.flag_kind(), FlagKind::HarmTowardOthers);
        assert_eq!(r.severity(), 85);
        assert_eq!(r.sigma_mask(), 0b0000_0011);
        assert_eq!(r.ts(), 1_700_000_000);
        assert_eq!(r.rationale_short(), 0x1122_3344_5566_7788);
        assert_eq!(r.sig_trunc(), 0xAABB_CCDD);
    }

    #[test]
    fn severity_out_of_range_rejected() {
        let err = FlagRecord::pack(0, 0, FlagKind::Spam, 200, 0, 0, 0, 0).unwrap_err();
        assert_eq!(err, RecordError::SeverityOutOfRange(200));
    }

    #[test]
    fn reserved_sigma_bits_rejected() {
        let err = FlagRecord::pack(0, 0, FlagKind::Spam, 5, 0xC0, 0, 0, 0).unwrap_err();
        assert!(matches!(err, RecordError::ReservedSigmaBits(_)));
    }

    #[test]
    fn from_raw_invalid_flagkind_rejected() {
        let mut raw = [0u8; 32];
        raw[12] = 99;
        let err = FlagRecord::from_raw_validated(raw).unwrap_err();
        assert!(matches!(err, RecordError::InvalidFlagKind(99)));
    }

    #[test]
    fn pubkey_handle_deterministic() {
        let a = FlagRecord::pubkey_handle(b"alice-pubkey");
        let b = FlagRecord::pubkey_handle(b"alice-pubkey");
        assert_eq!(a, b, "same input → same handle");
        let c = FlagRecord::pubkey_handle(b"bob-pubkey");
        assert_ne!(a, c, "different input → different handle");
    }
}
