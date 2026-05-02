//! § review — variable-size text review (≤ 512 bytes incl tags + sig).
//!
//! Reviews are AUTHOR-INDEPENDENT : the content-author CANNOT modify them.
//! Only the rater themselves can mutate (overwrite) or revoke their own row.

use crate::tags::TagBitset;
use crate::{CAP_REVIEW_BODY, CAP_RESERVED_MASK};
use serde::{Deserialize, Serialize};

/// § REVIEW_BODY_MAX — maximum body length in bytes (UTF-8).
pub const REVIEW_BODY_MAX: usize = 240;

/// § REVIEW_MAX_BYTES — total cap incl. body + tags + sig + envelope.
pub const REVIEW_MAX_BYTES: usize = 512;

/// § ReviewError — submit/validation failure modes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReviewError {
    #[error("review body length must be ≤ {REVIEW_BODY_MAX} bytes ; got {0}")]
    BodyTooLong(usize),
    #[error("stars out-of-range : got {0} (must be 1..=5)")]
    StarsOutOfRange(u8),
    #[error("sigma_mask cap-bit CAP_REVIEW_BODY missing : got 0b{0:08b}")]
    CapReviewMissing(u8),
    #[error("sigma_mask reserved bits non-zero : got 0b{0:08b}")]
    ReservedBitsSet(u8),
    #[error("signature length must be 64 bytes ; got {0}")]
    SignatureWrongLength(usize),
    #[error("review serialized size exceeds {REVIEW_MAX_BYTES} bytes : got {0}")]
    OverallTooLarge(usize),
}

/// § REVIEW_SIG_LEN — Ed25519 signature length (bytes). Stored as Vec<u8>
/// because serde does not derive `Deserialize` for `[u8; 64]` ; the value
/// MUST be exactly 64 bytes when present.
pub const REVIEW_SIG_LEN: usize = 64;

/// § Review — variable-size review record. Body is plain UTF-8 ; max 240
/// bytes ; tags echo the rating. Signature is opaque 64-byte slot
/// (Ed25519 in production ; left as bytes here so the substrate is impl-agnostic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub rater_pubkey_hash: u64,
    pub content_id: u32,
    pub stars: u8,
    pub body: String,
    pub tags_bitset: TagBitset,
    pub sigma_mask: u8,
    pub ts_minutes_since_epoch: u32,
    /// Ed25519 signature ; MUST be exactly `REVIEW_SIG_LEN` bytes.
    pub sig: Vec<u8>,
}

impl Review {
    /// § new — construct + validate.
    pub fn new(
        rater_pubkey_hash: u64,
        content_id: u32,
        stars: u8,
        body: String,
        tags_bitset: TagBitset,
        sigma_mask: u8,
        ts_minutes_since_epoch: u32,
        sig: Vec<u8>,
    ) -> Result<Self, ReviewError> {
        if body.len() > REVIEW_BODY_MAX {
            return Err(ReviewError::BodyTooLong(body.len()));
        }
        if !(1..=5).contains(&stars) {
            return Err(ReviewError::StarsOutOfRange(stars));
        }
        if sigma_mask & CAP_RESERVED_MASK != 0 {
            return Err(ReviewError::ReservedBitsSet(sigma_mask));
        }
        if sigma_mask & CAP_REVIEW_BODY == 0 {
            return Err(ReviewError::CapReviewMissing(sigma_mask));
        }
        if sig.len() != REVIEW_SIG_LEN {
            return Err(ReviewError::SignatureWrongLength(sig.len()));
        }
        let r = Self {
            rater_pubkey_hash,
            content_id,
            stars,
            body,
            tags_bitset,
            sigma_mask,
            ts_minutes_since_epoch,
            sig,
        };
        // Defensively check JSON size against the documented cap.
        let serialized =
            serde_json::to_vec(&r).map_err(|_| ReviewError::OverallTooLarge(REVIEW_MAX_BYTES))?;
        if serialized.len() > REVIEW_MAX_BYTES {
            return Err(ReviewError::OverallTooLarge(serialized.len()));
        }
        Ok(r)
    }

    /// § signing_payload — bytes the rater signs over (BLAKE3-canonical-form).
    /// Same payload regardless of crate-version : (content_id ‖ rater ‖ stars
    /// ‖ tags ‖ ts ‖ body-bytes).
    #[must_use]
    pub fn signing_payload(&self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"cssl-content-rating/review/v1\0");
        h.update(&self.content_id.to_le_bytes());
        h.update(&self.rater_pubkey_hash.to_le_bytes());
        h.update(&[self.stars]);
        h.update(&self.tags_bitset.bits().to_le_bytes());
        h.update(&self.ts_minutes_since_epoch.to_le_bytes());
        h.update(self.body.as_bytes());
        let f = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&f.as_bytes()[0..32]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid() -> Review {
        Review::new(
            0xAB_CD_EF_12_34_56_78_90,
            7,
            5,
            "great pacing".to_string(),
            TagBitset::from_bits(0x0021),
            CAP_REVIEW_BODY,
            123,
            vec![0u8; 64],
        )
        .expect("valid review")
    }

    #[test]
    fn review_rejects_body_too_long() {
        let body = "x".repeat(REVIEW_BODY_MAX + 1);
        let err = Review::new(
            1,
            1,
            5,
            body,
            TagBitset::EMPTY,
            CAP_REVIEW_BODY,
            1,
            vec![0u8; 64],
        )
        .expect_err("body too long must reject");
        match err {
            ReviewError::BodyTooLong(n) => assert_eq!(n, REVIEW_BODY_MAX + 1),
            _ => panic!("expected BodyTooLong"),
        }
    }

    #[test]
    fn review_rejects_zero_stars() {
        let err = Review::new(
            1,
            1,
            0,
            String::new(),
            TagBitset::EMPTY,
            CAP_REVIEW_BODY,
            1,
            vec![0u8; 64],
        )
        .expect_err("stars=0 must reject (review-only ; ratings can be 0=withdrawn)");
        assert_eq!(err, ReviewError::StarsOutOfRange(0));
    }

    #[test]
    fn review_rejects_missing_cap_review_body() {
        let err = Review::new(
            1,
            1,
            4,
            "ok".to_string(),
            TagBitset::EMPTY,
            0,
            1,
            vec![0u8; 64],
        )
        .expect_err("missing cap must reject");
        match err {
            ReviewError::CapReviewMissing(m) => assert_eq!(m, 0),
            _ => panic!("expected CapReviewMissing"),
        }
    }

    #[test]
    fn review_signing_payload_is_stable_under_content_change() {
        let mut r = make_valid();
        let p1 = r.signing_payload();
        // Mutating sig should NOT change the signing payload (sig isn't input).
        r.sig = vec![1u8; 64];
        let p2 = r.signing_payload();
        assert_eq!(p1, p2);
    }

    #[test]
    fn review_signing_payload_changes_with_body() {
        let r1 = make_valid();
        let mut r2 = r1.clone();
        r2.body = "different".to_string();
        assert_ne!(r1.signing_payload(), r2.signing_payload());
    }
}
