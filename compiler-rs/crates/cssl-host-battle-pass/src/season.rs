//! § season — seasonal-cycle catalog (60-90 day · configurable).
//!
//! Mirrors `public.battle_pass_seasons` in
//! `cssl-supabase/migrations/0032_battle_pass.sql`.
//!
//! Anti-FOMO rule : after a season ends, the [`SeasonStatus::Archived`]
//! state still allows reward-redemption via [`crate::reward::Reward::is_re_purchasable_at`]
//! at gift-cost. ¬ scarcity-exclusive. ¬ resetting-progress.

use serde::{Deserialize, Serialize};

/// Newtype around a season identifier. Distinct from
/// `cssl-host-roguelike-run::SeasonId` to keep responsibilities separate
/// (roguelike-run.season tracks permadeath cycles ; this tracks
/// battle-pass cycles ; the two SHOULD share an ID-range in practice
/// but this crate does not depend on roguelike-run).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeasonId(pub u32);

impl SeasonId {
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// Season lifecycle. Mirrors the `status` CHECK in the SQL migration.
///
/// `Upcoming → Active → Archived`. Once `Archived`, the season's rewards
/// remain re-purchasable at gift-cost ; this is THE anti-FOMO mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeasonStatus {
    Upcoming,
    Active,
    Archived,
}

/// Minimum + maximum allowed season-duration in days. Rules per
/// `Labyrinth of Apocalypse/systems/battle_pass.csl § seasonal-cycle`.
pub const SEASON_DURATION_MIN_DAYS: u32 = 60;
pub const SEASON_DURATION_MAX_DAYS: u32 = 90;

/// Per-season catalog row. Times are stored as ISO-8601 epoch-seconds
/// to avoid pulling a chrono dep into the host crate. Endpoints +
/// the SQL layer translate to `timestamptz`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Season {
    pub id: SeasonId,
    pub started_at_unix_s: i64,
    pub ends_at_unix_s: i64,
    pub tier_count: u32,
    pub status: SeasonStatus,
}

impl Season {
    /// Construct + validate. Returns `SeasonErr::*` on invariant breach.
    pub fn new(
        id: SeasonId,
        started_at_unix_s: i64,
        ends_at_unix_s: i64,
        tier_count: u32,
        status: SeasonStatus,
    ) -> Result<Self, SeasonErr> {
        if ends_at_unix_s <= started_at_unix_s {
            return Err(SeasonErr::InvertedWindow);
        }
        if tier_count != crate::xp::MAX_TIER {
            return Err(SeasonErr::TierCountMismatch {
                got: tier_count,
                want: crate::xp::MAX_TIER,
            });
        }
        let duration_secs = ends_at_unix_s.saturating_sub(started_at_unix_s);
        let duration_days = (duration_secs / 86_400) as u32;
        if duration_days < SEASON_DURATION_MIN_DAYS
            || duration_days > SEASON_DURATION_MAX_DAYS
        {
            return Err(SeasonErr::DurationOutOfRange {
                got_days: duration_days,
                min_days: SEASON_DURATION_MIN_DAYS,
                max_days: SEASON_DURATION_MAX_DAYS,
            });
        }
        Ok(Self {
            id,
            started_at_unix_s,
            ends_at_unix_s,
            tier_count,
            status,
        })
    }

    /// `true` when the season has ended (Archived). Rewards remain
    /// re-purchasable per anti-FOMO rule.
    pub const fn is_archived(&self) -> bool {
        matches!(self.status, SeasonStatus::Archived)
    }

    /// Duration in days (computed). Stable per construction.
    pub fn duration_days(&self) -> u32 {
        let d = self.ends_at_unix_s.saturating_sub(self.started_at_unix_s);
        (d / 86_400) as u32
    }
}

/// Season construction errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeasonErr {
    /// `ends_at <= started_at`.
    InvertedWindow,
    /// `tier_count != MAX_TIER`.
    TierCountMismatch { got: u32, want: u32 },
    /// Duration outside `[60, 90]` days.
    DurationOutOfRange {
        got_days: u32,
        min_days: u32,
        max_days: u32,
    },
}

impl core::fmt::Display for SeasonErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvertedWindow => write!(f, "season ends_at must be after started_at"),
            Self::TierCountMismatch { got, want } => {
                write!(f, "tier-count mismatch : got {got} · expected {want}")
            }
            Self::DurationOutOfRange {
                got_days,
                min_days,
                max_days,
            } => write!(
                f,
                "season duration {got_days}d out of range [{min_days}, {max_days}]"
            ),
        }
    }
}
impl std::error::Error for SeasonErr {}

#[cfg(test)]
mod tests {
    use super::*;

    const SEC_PER_DAY: i64 = 86_400;

    #[test]
    fn season_constructs_within_60_to_90_days() {
        let start = 1_700_000_000_i64;
        let s = Season::new(
            SeasonId(1),
            start,
            start + 75 * SEC_PER_DAY,
            crate::xp::MAX_TIER,
            SeasonStatus::Active,
        );
        assert!(s.is_ok());
    }

    #[test]
    fn season_rejects_lt_60_days() {
        let start = 1_700_000_000_i64;
        let s = Season::new(
            SeasonId(1),
            start,
            start + 30 * SEC_PER_DAY,
            crate::xp::MAX_TIER,
            SeasonStatus::Active,
        );
        assert!(matches!(s, Err(SeasonErr::DurationOutOfRange { .. })));
    }

    #[test]
    fn season_rejects_gt_90_days() {
        let start = 1_700_000_000_i64;
        let s = Season::new(
            SeasonId(1),
            start,
            start + 120 * SEC_PER_DAY,
            crate::xp::MAX_TIER,
            SeasonStatus::Active,
        );
        assert!(matches!(s, Err(SeasonErr::DurationOutOfRange { .. })));
    }

    #[test]
    fn season_rejects_inverted_window() {
        let s = Season::new(
            SeasonId(1),
            1_700_000_000,
            1_600_000_000,
            crate::xp::MAX_TIER,
            SeasonStatus::Active,
        );
        assert!(matches!(s, Err(SeasonErr::InvertedWindow)));
    }

    #[test]
    fn season_rejects_wrong_tier_count() {
        let start = 1_700_000_000_i64;
        let s = Season::new(
            SeasonId(1),
            start,
            start + 75 * SEC_PER_DAY,
            50,
            SeasonStatus::Active,
        );
        assert!(matches!(s, Err(SeasonErr::TierCountMismatch { .. })));
    }
}
