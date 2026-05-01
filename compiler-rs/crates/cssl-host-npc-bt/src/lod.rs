// § lod.rs — 4-tier real-scale-city LOD scheduler
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § REAL-SCALE-CITY-PERFORMANCE ; 4096 NPCs sustained @ 60fps
// § I> tiers : Close=60Hz · Mid=10Hz · Far=1Hz · Sleep=0.1Hz
// § I> should_tick(tier, frame_idx) → bool ; deterministic over frame-counter
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

/// LOD tier — coarser tier = fewer ticks per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LodTier {
    /// ≤ 32 cells ; full BT+GOAP+L4 ; 60Hz.
    Close,
    /// 32-128 cells ; BT+GOAP-cached ; 10Hz.
    Mid,
    /// 128-512 cells ; routine-only ; 1Hz.
    Far,
    /// > 512 cells ; macro-sim only ; 0.1Hz.
    Sleep,
}

/// Distance-to-camera (or to-player) → LodTier per GDD-table.
#[must_use]
pub fn tier_for_distance(d: f32) -> LodTier {
    if d < 0.0 {
        return LodTier::Close;
    }
    if d <= 32.0 {
        LodTier::Close
    } else if d <= 128.0 {
        LodTier::Mid
    } else if d <= 512.0 {
        LodTier::Far
    } else {
        LodTier::Sleep
    }
}

/// Per-tier tick frequency in Hz.
#[must_use]
pub fn tick_freq_hz_for_tier(t: LodTier) -> f32 {
    match t {
        LodTier::Close => 60.0,
        LodTier::Mid => 10.0,
        LodTier::Far => 1.0,
        LodTier::Sleep => 0.1,
    }
}

/// Should this tier tick on the given 60Hz-frame index?
///
/// § I> assumes host runs at 60fps ; period = round(60 / tier-Hz).
/// § I> Sleep-tier ticks every 600 frames (~10s).
#[must_use]
pub fn should_tick(t: LodTier, frame_idx: u64) -> bool {
    let period: u64 = match t {
        LodTier::Close => 1,
        LodTier::Mid => 6,    // 60/10
        LodTier::Far => 60,   // 60/1
        LodTier::Sleep => 600, // 60/0.1
    };
    frame_idx % period == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_buckets() {
        assert_eq!(tier_for_distance(10.0), LodTier::Close);
        assert_eq!(tier_for_distance(64.0), LodTier::Mid);
        assert_eq!(tier_for_distance(256.0), LodTier::Far);
        assert_eq!(tier_for_distance(2048.0), LodTier::Sleep);
    }

    #[test]
    fn negative_distance_clamps_close() {
        assert_eq!(tier_for_distance(-5.0), LodTier::Close);
    }

    #[test]
    fn tier_freq_per_spec() {
        assert!((tick_freq_hz_for_tier(LodTier::Close) - 60.0).abs() < 1e-6);
        assert!((tick_freq_hz_for_tier(LodTier::Mid) - 10.0).abs() < 1e-6);
        assert!((tick_freq_hz_for_tier(LodTier::Far) - 1.0).abs() < 1e-6);
        assert!((tick_freq_hz_for_tier(LodTier::Sleep) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn should_tick_close_every_frame() {
        for f in 0..60 {
            assert!(should_tick(LodTier::Close, f));
        }
    }

    #[test]
    fn should_tick_far_one_in_sixty() {
        assert!(should_tick(LodTier::Far, 0));
        assert!(!should_tick(LodTier::Far, 59));
        assert!(should_tick(LodTier::Far, 60));
    }
}
