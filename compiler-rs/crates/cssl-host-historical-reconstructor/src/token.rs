// § token : 30-min default TtlToken.
// § Time is `u64` seconds since unix-epoch. Caller (host) supplies clock.

use serde::{Deserialize, Serialize};

/// § Default tour TTL : 30 minutes (in seconds).
pub const DEFAULT_TTL_SECS: u64 = 30 * 60;

/// § TtlToken — opaque grant to view a tour. Expires deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtlToken {
    pub token_id: [u8; 32],
    pub holder_pubkey: [u8; 32],
    pub issued_at_secs: u64,
    pub expires_at_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// `expires_at_secs ≤ issued_at_secs` is invalid.
    NonPositiveTtl,
    /// `now_secs ≥ expires_at_secs` (token already expired).
    Expired,
    /// `now_secs < issued_at_secs` (clock-skew or fake-future).
    NotYetIssued,
}

impl core::fmt::Display for TokenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NonPositiveTtl => write!(f, "ttl-token: non-positive ttl"),
            Self::Expired        => write!(f, "ttl-token: expired"),
            Self::NotYetIssued   => write!(f, "ttl-token: not yet issued"),
        }
    }
}
impl std::error::Error for TokenError {}

impl TtlToken {
    /// § Mint a token with default 30-min TTL.
    pub fn mint(holder_pubkey: [u8; 32], cohort_id: [u8; 32], now_secs: u64) -> Self {
        Self::mint_with_ttl(holder_pubkey, cohort_id, now_secs, DEFAULT_TTL_SECS)
    }

    /// § Mint a token with explicit TTL (panics is forbidden — caller must pass > 0).
    pub fn mint_with_ttl(
        holder_pubkey: [u8; 32],
        cohort_id: [u8; 32],
        now_secs: u64,
        ttl_secs: u64,
    ) -> Self {
        // Token id = BLAKE3(holder · cohort · issued)
        let mut h = blake3::Hasher::new();
        h.update(&holder_pubkey);
        h.update(&cohort_id);
        h.update(&now_secs.to_le_bytes());
        h.update(&ttl_secs.to_le_bytes());
        let token_id = *h.finalize().as_bytes();

        Self {
            token_id,
            holder_pubkey,
            issued_at_secs: now_secs,
            // saturating_add prevents wrap on absurd inputs.
            expires_at_secs: now_secs.saturating_add(ttl_secs),
        }
    }

    /// § Validate token against `now_secs`.
    pub fn validate(&self, now_secs: u64) -> Result<(), TokenError> {
        if self.expires_at_secs <= self.issued_at_secs {
            return Err(TokenError::NonPositiveTtl);
        }
        if now_secs < self.issued_at_secs {
            return Err(TokenError::NotYetIssued);
        }
        if now_secs >= self.expires_at_secs {
            return Err(TokenError::Expired);
        }
        Ok(())
    }

    pub fn is_expired_at(&self, now_secs: u64) -> bool {
        now_secs >= self.expires_at_secs
    }

    pub fn ttl_remaining(&self, now_secs: u64) -> u64 {
        self.expires_at_secs.saturating_sub(now_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ttl_is_thirty_minutes() {
        assert_eq!(DEFAULT_TTL_SECS, 1800);
    }

    #[test]
    fn mint_default_ttl_30_min() {
        let t = TtlToken::mint([1u8; 32], [2u8; 32], 1_000_000);
        assert_eq!(t.expires_at_secs - t.issued_at_secs, 1800);
    }

    #[test]
    fn fresh_token_validates() {
        let t = TtlToken::mint([1u8; 32], [2u8; 32], 1_000_000);
        assert!(t.validate(1_000_001).is_ok());
        assert!(!t.is_expired_at(1_000_001));
    }

    #[test]
    fn expired_after_30_min() {
        let t = TtlToken::mint([1u8; 32], [2u8; 32], 1_000_000);
        assert!(t.validate(1_000_000 + 1800).is_err());
        assert!(t.is_expired_at(1_000_000 + 1800));
    }

    #[test]
    fn not_yet_issued_rejected() {
        let t = TtlToken::mint([1u8; 32], [2u8; 32], 1_000_000);
        assert_eq!(t.validate(999_999), Err(TokenError::NotYetIssued));
    }

    #[test]
    fn ttl_remaining_decreases() {
        let t = TtlToken::mint([1u8; 32], [2u8; 32], 1_000_000);
        assert_eq!(t.ttl_remaining(1_000_000), 1800);
        assert_eq!(t.ttl_remaining(1_000_900), 900);
        assert_eq!(t.ttl_remaining(1_010_000), 0);
    }

    #[test]
    fn token_ids_distinct_per_holder() {
        let a = TtlToken::mint([1u8; 32], [9u8; 32], 1_000_000);
        let b = TtlToken::mint([2u8; 32], [9u8; 32], 1_000_000);
        assert_ne!(a.token_id, b.token_id);
    }

    #[test]
    fn token_ids_deterministic_same_inputs() {
        let a = TtlToken::mint([1u8; 32], [9u8; 32], 1_000_000);
        let b = TtlToken::mint([1u8; 32], [9u8; 32], 1_000_000);
        assert_eq!(a.token_id, b.token_id);
    }

    #[test]
    fn expires_exactly_at_30_min_boundary() {
        let t = TtlToken::mint([7u8; 32], [8u8; 32], 0);
        // at issued_at + ttl is treated as expired (≥ comparator).
        assert!(t.is_expired_at(1800));
        assert!(!t.is_expired_at(1799));
    }
}
