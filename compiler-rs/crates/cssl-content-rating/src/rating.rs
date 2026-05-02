//! § rating — 24-byte bit-packed Rating record + pack/unpack + validation.

use crate::tags::TagBitset;
use crate::{CAP_RATE, CAP_RESERVED_MASK};
use serde::{Deserialize, Serialize};

/// § RATING_BYTES — fixed-size on-the-wire layout.
pub const RATING_BYTES: usize = 24;

/// § RatingError — pack/unpack/validation failure modes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RatingError {
    #[error("rating bit-pack length must be {RATING_BYTES} bytes ; got {0}")]
    WrongLength(usize),
    #[error("stars out-of-range : got {0} (must be 0..=5)")]
    StarsOutOfRange(u8),
    #[error("sigma_mask cap-bit CAP_RATE missing : got 0b{0:08b}")]
    CapRateMissing(u8),
    #[error("sigma_mask reserved bits non-zero : got 0b{0:08b}")]
    ReservedBitsSet(u8),
    #[error("reserved trailing bytes non-zero")]
    NonZeroReserved,
}

/// § Rating — fixed 24-byte bit-packed record. Little-endian.
///
/// `stars == 0` is the WITHDRAWN sentinel — written when the rater revokes.
/// `1..=5` are the active rating values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rating {
    pub rater_pubkey_hash: u64,
    pub content_id: u32,
    pub stars: u8,
    pub tags_bitset: TagBitset,
    pub sigma_mask: u8,
    pub ts_minutes_since_epoch: u32,
    pub weight_q8: u8,
}

impl Rating {
    /// § new — construct + validate.
    pub fn new(
        rater_pubkey_hash: u64,
        content_id: u32,
        stars: u8,
        tags_bitset: TagBitset,
        sigma_mask: u8,
        ts_minutes_since_epoch: u32,
        weight_q8: u8,
    ) -> Result<Self, RatingError> {
        if stars > 5 {
            return Err(RatingError::StarsOutOfRange(stars));
        }
        if sigma_mask & CAP_RESERVED_MASK != 0 {
            return Err(RatingError::ReservedBitsSet(sigma_mask));
        }
        if sigma_mask & CAP_RATE == 0 {
            return Err(RatingError::CapRateMissing(sigma_mask));
        }
        Ok(Self {
            rater_pubkey_hash,
            content_id,
            stars,
            tags_bitset,
            sigma_mask,
            ts_minutes_since_epoch,
            weight_q8,
        })
    }

    /// § withdrawn — sentinel emitted on `revoke`. Stars zeroed ; tags zeroed.
    /// Sigma-mask still required to assert (consent-as-OS).
    pub fn withdrawn(
        rater_pubkey_hash: u64,
        content_id: u32,
        sigma_mask: u8,
        ts_minutes_since_epoch: u32,
    ) -> Result<Self, RatingError> {
        Self::new(
            rater_pubkey_hash,
            content_id,
            0,
            TagBitset::EMPTY,
            sigma_mask,
            ts_minutes_since_epoch,
            0,
        )
    }

    /// § is_withdrawn — convenience predicate.
    #[must_use]
    pub fn is_withdrawn(&self) -> bool {
        self.stars == 0
    }

    /// § pack — serialize to 24-byte fixed buffer (little-endian).
    #[must_use]
    pub fn pack(&self) -> [u8; RATING_BYTES] {
        let mut out = [0u8; RATING_BYTES];
        out[0..8].copy_from_slice(&self.rater_pubkey_hash.to_le_bytes());
        out[8..12].copy_from_slice(&self.content_id.to_le_bytes());
        out[12] = self.stars;
        out[13..15].copy_from_slice(&self.tags_bitset.bits().to_le_bytes());
        out[15] = self.sigma_mask;
        out[16..20].copy_from_slice(&self.ts_minutes_since_epoch.to_le_bytes());
        out[20] = self.weight_q8;
        // out[21..24] = reserved zeros (already zeroed)
        out
    }

    /// § unpack — deserialize + validate. Reserved trailing bytes MUST be zero.
    pub fn unpack(buf: &[u8]) -> Result<Self, RatingError> {
        if buf.len() != RATING_BYTES {
            return Err(RatingError::WrongLength(buf.len()));
        }
        if buf[21] != 0 || buf[22] != 0 || buf[23] != 0 {
            return Err(RatingError::NonZeroReserved);
        }
        let mut h = [0u8; 8];
        h.copy_from_slice(&buf[0..8]);
        let rater_pubkey_hash = u64::from_le_bytes(h);
        let mut c = [0u8; 4];
        c.copy_from_slice(&buf[8..12]);
        let content_id = u32::from_le_bytes(c);
        let stars = buf[12];
        let mut tb = [0u8; 2];
        tb.copy_from_slice(&buf[13..15]);
        let tags_bitset = TagBitset::from_bits(u16::from_le_bytes(tb));
        let sigma_mask = buf[15];
        let mut t = [0u8; 4];
        t.copy_from_slice(&buf[16..20]);
        let ts_minutes_since_epoch = u32::from_le_bytes(t);
        let weight_q8 = buf[20];
        // Use new() so all the validation rules apply uniformly. Withdrawn
        // (stars==0) goes through fine since new() permits 0..=5.
        Self::new(
            rater_pubkey_hash,
            content_id,
            stars,
            tags_bitset,
            sigma_mask,
            ts_minutes_since_epoch,
            weight_q8,
        )
    }

    /// § content_addressable_id — BLAKE3-trunc of (content_id, rater_hash).
    /// Stable across replays ; used as the storage primary-key.
    #[must_use]
    pub fn storage_key(&self) -> u64 {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"rating-storage-key\0");
        hasher.update(&self.content_id.to_le_bytes());
        hasher.update(&self.rater_pubkey_hash.to_le_bytes());
        let h = hasher.finalize();
        let bytes = h.as_bytes();
        let mut out = [0u8; 8];
        out.copy_from_slice(&bytes[0..8]);
        u64::from_le_bytes(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::{tag_index, TagBitset};

    fn make_valid() -> Rating {
        let mut tags = TagBitset::EMPTY;
        tags.set(tag_index("fun").expect("fun is a tag"));
        tags.set(tag_index("balanced").expect("balanced is a tag"));
        Rating::new(0xDEAD_BEEF_F00D_BABE, 42, 4, tags, CAP_RATE, 1_000_000, 200)
            .expect("valid rating")
    }

    #[test]
    fn rating_size_is_24_bytes() {
        let r = make_valid();
        assert_eq!(r.pack().len(), 24);
        assert_eq!(RATING_BYTES, 24);
    }

    #[test]
    fn rating_pack_unpack_roundtrip_preserves_all_fields() {
        let r = make_valid();
        let buf = r.pack();
        let r2 = Rating::unpack(&buf).expect("unpack ok");
        assert_eq!(r, r2);
    }

    #[test]
    fn rating_rejects_stars_above_5() {
        let err = Rating::new(1, 1, 6, TagBitset::EMPTY, CAP_RATE, 1, 0)
            .expect_err("stars=6 must reject");
        assert_eq!(err, RatingError::StarsOutOfRange(6));
    }

    #[test]
    fn rating_rejects_missing_cap_rate() {
        let err = Rating::new(1, 1, 4, TagBitset::EMPTY, 0, 1, 0)
            .expect_err("sigma_mask=0 must reject");
        match err {
            RatingError::CapRateMissing(m) => assert_eq!(m, 0),
            _ => panic!("expected CapRateMissing"),
        }
    }

    #[test]
    fn rating_rejects_reserved_bits_set() {
        let err = Rating::new(1, 1, 4, TagBitset::EMPTY, CAP_RATE | 0x80, 1, 0)
            .expect_err("reserved bit must reject");
        match err {
            RatingError::ReservedBitsSet(m) => assert_eq!(m, CAP_RATE | 0x80),
            _ => panic!("expected ReservedBitsSet"),
        }
    }

    #[test]
    fn rating_unpack_rejects_wrong_length() {
        let err = Rating::unpack(&[0u8; 23]).expect_err("23 bytes must reject");
        assert_eq!(err, RatingError::WrongLength(23));
    }

    #[test]
    fn rating_unpack_rejects_non_zero_reserved() {
        let r = make_valid();
        let mut buf = r.pack();
        buf[22] = 0x01;
        let err = Rating::unpack(&buf).expect_err("non-zero reserved must reject");
        assert_eq!(err, RatingError::NonZeroReserved);
    }

    #[test]
    fn rating_storage_key_is_stable() {
        let r = make_valid();
        let k1 = r.storage_key();
        let k2 = r.storage_key();
        assert_eq!(k1, k2);
    }

    #[test]
    fn rating_storage_key_is_distinct_per_rater() {
        let r1 = make_valid();
        let r2 = Rating::new(
            0xCAFE_BABE_DEAD_BEEF,
            42,
            5,
            TagBitset::EMPTY,
            CAP_RATE,
            1_000_001,
            255,
        )
        .expect("valid");
        assert_ne!(r1.storage_key(), r2.storage_key());
    }

    #[test]
    fn rating_withdrawn_has_zero_stars_and_zero_tags() {
        let w = Rating::withdrawn(1, 1, CAP_RATE, 5).expect("withdrawn ok");
        assert_eq!(w.stars, 0);
        assert!(w.is_withdrawn());
        assert_eq!(w.tags_bitset.bits(), 0);
    }
}
