// § shards.rs · AethericShards balance newtype · checked-arithmetic · audit-emit

use serde::{Deserialize, Serialize};

use crate::imprint::AkashicError;

/// Premium-currency · Aetheric-Shards balance newtype.
///
/// All arithmetic is checked · `add`/`sub` return [`AkashicError`] rather than
/// panicking · NEVER overflow-panic in-release (per task-spec § 5).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AethericShards(pub u64);

impl AethericShards {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub fn new(amount: u64) -> Self {
        Self(amount)
    }

    #[must_use]
    pub fn amount(self) -> u64 {
        self.0
    }

    /// Checked-add · returns `BalanceOverflow` if overflow.
    ///
    /// # Errors
    /// Returns [`AkashicError::BalanceOverflow`] on `u64` overflow.
    pub fn checked_add(self, rhs: Self) -> Result<Self, AkashicError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(AkashicError::BalanceOverflow)
    }

    /// Checked-sub · returns `InsufficientShards` if underflow.
    ///
    /// # Errors
    /// Returns [`AkashicError::InsufficientShards`] when `rhs > self`.
    pub fn checked_sub(self, rhs: Self) -> Result<Self, AkashicError> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or(AkashicError::InsufficientShards {
                have: self.0,
                need: rhs.0,
            })
    }

    /// Sufficient-balance check.
    #[must_use]
    pub fn covers(self, cost: u32) -> bool {
        self.0 >= u64::from(cost)
    }
}

impl From<u32> for AethericShards {
    fn from(v: u32) -> Self {
        Self(u64::from(v))
    }
}

impl From<u64> for AethericShards {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shards_zero_default() {
        let s = AethericShards::default();
        assert_eq!(s, AethericShards::ZERO);
        assert_eq!(s.amount(), 0);
    }

    #[test]
    fn shards_checked_add_ok() {
        let a = AethericShards::new(100);
        let b = AethericShards::new(50);
        assert_eq!(a.checked_add(b).unwrap(), AethericShards::new(150));
    }

    #[test]
    fn shards_checked_add_overflow() {
        let a = AethericShards::new(u64::MAX);
        let b = AethericShards::new(1);
        let err = a.checked_add(b).unwrap_err();
        assert!(matches!(err, AkashicError::BalanceOverflow));
    }

    #[test]
    fn shards_checked_sub_ok() {
        let a = AethericShards::new(100);
        let b = AethericShards::new(50);
        assert_eq!(a.checked_sub(b).unwrap(), AethericShards::new(50));
    }

    #[test]
    fn shards_checked_sub_underflow() {
        let a = AethericShards::new(50);
        let b = AethericShards::new(100);
        let err = a.checked_sub(b).unwrap_err();
        assert!(matches!(
            err,
            AkashicError::InsufficientShards {
                have: 50,
                need: 100
            }
        ));
    }

    #[test]
    fn shards_covers_check() {
        let a = AethericShards::new(50);
        assert!(a.covers(50));
        assert!(a.covers(0));
        assert!(!a.covers(51));
    }

    #[test]
    fn shards_serde_roundtrip() {
        let s = AethericShards::new(12345);
        let json = serde_json::to_string(&s).unwrap();
        // transparent → bare integer
        assert_eq!(json, "12345");
        let back: AethericShards = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
