// § floor-count-curve ← GDDs/ROGUELIKE_LOOP.csl §FLOOR-COUNT-CURVE
// ════════════════════════════════════════════════════════════════════
// § I> early-game (run 1..10)   : 3..5 floors
// § I> mid-game   (run 11..30)  : 5..8 floors
// § I> late-game  (run 31+)     : 8..12 floors
// § I> rule : floor_count(run_n) = clamp(3 + ⌊run_n / 5⌋ , 3 , 12)
// ════════════════════════════════════════════════════════════════════

use crate::biome_dag::Biome;

/// Compute floor-count for a given biome and run-depth (run_n).
///
/// Per GDD curve. Endless-Spire intentionally caps at 12 here ; the
/// compounding-affix scaling is owned by cssl-host-procgen-rooms ; this
/// crate only serves the *count*.
pub fn floor_count_for(biome: Biome, run_n: u32) -> u8 {
    let base = 3u32 + (run_n / 5);
    let clamped = base.clamp(3, 12) as u8;
    // Biome-specific minor skews per ROGUELIKE_LOOP §DIFFICULTY-CURVE
    // intent (not punitive ; cosmetic-pacing only).
    match biome {
        // Crypt + Forest are entry-tier — lean shorter side of the band.
        Biome::Crypt | Biome::ForestPath => clamped.saturating_sub(0).max(3),
        // Mid-tier biomes use baseline.
        Biome::Citadel | Biome::Sanctum | Biome::Forge => clamped,
        // Late-tier biomes lean longer when run-depth allows.
        Biome::Abyss | Biome::Maelstrom => clamped.saturating_add(u8::from(run_n >= 20)).min(12),
        // Endless caps at the curve max — endlessness is in compounding-affixes,
        // not floor-count (per GDD-spec separation-of-concerns).
        Biome::EndlessSpire => 12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_game_band_3_to_5() {
        let c = floor_count_for(Biome::Crypt, 1);
        assert!((3..=5).contains(&c), "got {c}");
    }

    #[test]
    fn late_game_caps_at_12() {
        let c = floor_count_for(Biome::EndlessSpire, 999);
        assert_eq!(c, 12);
    }

    #[test]
    fn curve_monotonic_across_run_n() {
        let early = floor_count_for(Biome::Citadel, 1);
        let mid = floor_count_for(Biome::Citadel, 20);
        let late = floor_count_for(Biome::Citadel, 60);
        assert!(early <= mid);
        assert!(mid <= late);
    }
}
