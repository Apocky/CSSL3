// § routines.rs — L3 layer ; 12 routine-archetypes ; per-hour schedule + jitter
// ════════════════════════════════════════════════════════════════════
// § I> per GDD § DAILY-ROUTINES ; 12 archetypes ; ±10% jitter per-day-seeded
// § I> determinism : (npc-id × game-day) seed → splitmix64 → schedule
// § I> ¬ overlap with Companion-AI cap-ladders ; this is NPC-only
// ════════════════════════════════════════════════════════════════════

use crate::DetRng;
use serde::{Deserialize, Serialize};

/// Twelve canonical routine-archetypes per GDD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoutineArchetype {
    /// Night-rest · day-light-craft · home-zone-dominant.
    Sleeper,
    /// Dawn-commute · 8h-labor · evening-tavern · night-home.
    Worker,
    /// Morning-prep · long-bench-session · evening-deliver.
    Crafter,
    /// Stall-open 9-17 · midday-restock · evening-tally.
    Merchant,
    /// Shift-rotation · perimeter-walk · barracks-rest.
    PatrolGuard,
    /// Dawn-rite · counsel-block · evening-rite · vigil.
    Priest,
    /// Library-block · field-block · debate-block.
    Scholar,
    /// Dawn-departure · field-roam · dusk-return.
    Forager,
    /// No-fixed-zone · random-walk · open-conv-pool.
    Wanderer,
    /// Market-fringe-day · alley-night · low-trade.
    Beggar,
    /// Late-rise · court-block · feast-block · salon.
    Noble,
    /// Contract-board-check · escort-or-hunt · drink-rest.
    Mercenary,
}

impl RoutineArchetype {
    /// All 12 archetypes — for tests + scheduler-iteration.
    #[must_use]
    pub const fn all() -> [Self; 12] {
        [
            Self::Sleeper,
            Self::Worker,
            Self::Crafter,
            Self::Merchant,
            Self::PatrolGuard,
            Self::Priest,
            Self::Scholar,
            Self::Forager,
            Self::Wanderer,
            Self::Beggar,
            Self::Noble,
            Self::Mercenary,
        ]
    }
}

/// What an NPC is doing during one hour-block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoutineActivity {
    /// Sleep / rest.
    Sleep,
    /// Travel between zones.
    Commute,
    /// Active labor (worker / crafter).
    Work,
    /// Stall-tending / market activity.
    Trade,
    /// Patrol / guard duty.
    Patrol,
    /// Religious rite / prayer.
    Pray,
    /// Study / debate / library.
    Study,
    /// Forage / harvest field.
    Forage,
    /// Drift / unstructured walk.
    Wander,
    /// Beg / loiter at fringe.
    Beg,
    /// Court / salon / feast.
    Court,
    /// Mercenary contract / hunt.
    Hunt,
    /// Tavern / social drinking.
    Tavern,
    /// Idle / unscheduled.
    Idle,
}

/// One hour-block : (start_hour, activity, zone_affinity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HourBlock {
    /// Hour-of-day this block covers (0..24).
    pub hour: u8,
    /// What the NPC is doing.
    pub activity: RoutineActivity,
    /// Preferred zone-id ; host clamps to legal-zones.
    pub zone_affinity: u32,
}

/// Generate a 24-hour schedule for the given archetype, jittered by `day_seed`.
///
/// § I> seed = (npc-id × game-day) per GDD ; ±10% block-shuffle within archetype-pattern.
/// § I> deterministic : same seed → identical schedule.
#[must_use]
pub fn daily_schedule(arch: RoutineArchetype, day_seed: u64) -> [HourBlock; 24] {
    let base = base_pattern(arch);
    let mut rng = DetRng::new(day_seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let mut out = base;
    // ±10% jitter : ~2 random adjacent-hour swaps per day.
    let swaps = 2;
    for _ in 0..swaps {
        let i = rng.range_u32(23) as usize;
        let j = i + 1;
        // Keep sleep-block (0..6) and the morning-anchor (hour=6) stable for archetype-identity.
        if i >= 6 && j < 24 {
            // Swap activities only ; zone_affinity stays.
            let act_i = out[i].activity;
            let act_j = out[j].activity;
            out[i].activity = act_j;
            out[j].activity = act_i;
        }
    }
    out
}

/// Base 24-hour pattern for an archetype — pre-jitter.
#[must_use]
fn base_pattern(arch: RoutineArchetype) -> [HourBlock; 24] {
    let zone_home: u32 = 1;
    let zone_work: u32 = 2;
    let zone_market: u32 = 3;
    let zone_field: u32 = 4;
    let zone_temple: u32 = 5;
    let zone_court: u32 = 6;
    let zone_tavern: u32 = 7;

    let pat: [(RoutineActivity, u32); 24] = match arch {
        RoutineArchetype::Sleeper => sleeper_pattern(zone_home, zone_work),
        RoutineArchetype::Worker => worker_pattern(zone_home, zone_work, zone_tavern),
        RoutineArchetype::Crafter => crafter_pattern(zone_home, zone_work),
        RoutineArchetype::Merchant => merchant_pattern(zone_home, zone_market),
        RoutineArchetype::PatrolGuard => patrol_guard_pattern(zone_home, zone_work),
        RoutineArchetype::Priest => priest_pattern(zone_home, zone_temple),
        RoutineArchetype::Scholar => scholar_pattern(zone_home, zone_work, zone_field),
        RoutineArchetype::Forager => forager_pattern(zone_home, zone_field),
        RoutineArchetype::Wanderer => wanderer_pattern(zone_home),
        RoutineArchetype::Beggar => beggar_pattern(zone_home, zone_market),
        RoutineArchetype::Noble => noble_pattern(zone_home, zone_court),
        RoutineArchetype::Mercenary => mercenary_pattern(zone_home, zone_work, zone_tavern),
    };

    let mut out = [HourBlock {
        hour: 0,
        activity: RoutineActivity::Idle,
        zone_affinity: 0,
    }; 24];
    for (i, (act, z)) in pat.iter().enumerate() {
        out[i] = HourBlock {
            hour: i as u8,
            activity: *act,
            zone_affinity: *z,
        };
    }
    out
}

// Macro-like constructor helper avoids repetition without macro_rules.
const fn fill24(default: (RoutineActivity, u32)) -> [(RoutineActivity, u32); 24] {
    [default; 24]
}

/// Fill a slice-range with a given (activity, zone) tuple ; tiny helper that
/// keeps clippy `needless_range_loop` happy by routing through `iter_mut`.
fn fill_range(p: &mut [(RoutineActivity, u32); 24], start: usize, end: usize, val: (RoutineActivity, u32)) {
    for slot in p.iter_mut().take(end).skip(start) {
        *slot = val;
    }
}

fn sleeper_pattern(home: u32, work: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 8, 18, (RoutineActivity::Work, work));
    p
}

fn worker_pattern(home: u32, work: u32, tavern: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    p[6] = (RoutineActivity::Commute, home);
    fill_range(&mut p, 7, 15, (RoutineActivity::Work, work));
    p[15] = (RoutineActivity::Commute, home);
    fill_range(&mut p, 16, 20, (RoutineActivity::Tavern, tavern));
    p
}

fn crafter_pattern(home: u32, work: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 7, 18, (RoutineActivity::Work, work));
    p
}

fn merchant_pattern(home: u32, market: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 9, 17, (RoutineActivity::Trade, market));
    p
}

fn patrol_guard_pattern(home: u32, work: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 6, 14, (RoutineActivity::Patrol, work));
    p
}

fn priest_pattern(home: u32, temple: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 5, 21, (RoutineActivity::Pray, temple));
    p
}

fn scholar_pattern(home: u32, work: u32, field: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 8, 14, (RoutineActivity::Study, work));
    fill_range(&mut p, 14, 18, (RoutineActivity::Study, field));
    p
}

fn forager_pattern(home: u32, field: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 5, 18, (RoutineActivity::Forage, field));
    p
}

fn wanderer_pattern(home: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Wander, home));
    fill_range(&mut p, 0, 6, (RoutineActivity::Sleep, home));
    p
}

fn beggar_pattern(home: u32, market: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 9, 18, (RoutineActivity::Beg, market));
    p
}

fn noble_pattern(home: u32, court: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 10, 22, (RoutineActivity::Court, court));
    p
}

fn mercenary_pattern(home: u32, work: u32, tavern: u32) -> [(RoutineActivity, u32); 24] {
    let mut p = fill24((RoutineActivity::Sleep, home));
    fill_range(&mut p, 7, 15, (RoutineActivity::Hunt, work));
    fill_range(&mut p, 18, 22, (RoutineActivity::Tavern, tavern));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_archetypes() {
        assert_eq!(RoutineArchetype::all().len(), 12);
    }

    #[test]
    fn schedule_is_24_hour() {
        let s = daily_schedule(RoutineArchetype::Worker, 0xCAFE);
        assert_eq!(s.len(), 24);
        for (i, b) in s.iter().enumerate() {
            assert_eq!(b.hour, i as u8);
        }
    }
}
