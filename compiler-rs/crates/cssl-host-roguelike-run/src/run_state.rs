// § run-state-machine ← GDDs/ROGUELIKE_LOOP.csl §RUN-STRUCTURE
// ════════════════════════════════════════════════════════════════════
// § I> states : Hub → BiomeSelect → Floor(N=3..12) → BossArena → Reward
//                  → BiomeSelect | Hub | Death(SoftPermaCarryover)
// § I> seed-pinned post-genesis ; immutable mid-run
// § I> echoes_pre = pre-run bank ; echoes_in_run = current-run accumulation
// ════════════════════════════════════════════════════════════════════

use crate::biome_dag::Biome;
use crate::seed::pin_seed;
use serde::{Deserialize, Serialize};

/// § Run-phase enum — discrete states of the run-state-machine.
///
/// `Floor` carries the floor-index (1-based, ≤ floor_count) and current
/// biome. `BossArena` is the terminal floor before Reward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunPhase {
    /// At Hub — between-runs ; meta-progression spend permitted.
    Hub,
    /// Choosing next biome at a DAG-junction.
    BiomeSelect,
    /// On a numbered floor of a biome.
    Floor { idx: u8, biome: Biome },
    /// At the boss-arena terminating the current biome.
    BossArena { biome: Biome },
    /// Reward room post-boss ; awards Echoes + meta-perks.
    Reward { biome: Biome },
    /// Run terminated by death ; soft-perma-carryover applied externally.
    Death,
}

/// § State-machine transition errors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStateErr {
    /// Caller attempted invalid phase-transition (e.g. Hub → BossArena).
    InvalidTransition { from: RunPhase, to_kind: &'static str },
    /// Floor-index exceeds floor_count ceiling.
    FloorIndexOutOfRange { idx: u8, ceiling: u8 },
    /// Seed-mutation attempted mid-run (forbidden per GDD).
    SeedImmutable,
}

/// § Full run-state aggregate.
///
/// `seed` is pinned at genesis and immutable thereafter (enforced via
/// `seed_immutable_after_genesis` invariant in transition methods).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    /// Current phase in the state-machine.
    pub phase: RunPhase,
    /// Currently-active biome (None if at Hub or BiomeSelect-junction).
    pub current_biome: Option<Biome>,
    /// Total floor-count for this run (3..=12 per curve).
    pub floor_count: u8,
    /// Current depth (1-based ; 0 = pre-Floor-1 ; equals floor_count at boss).
    pub depth: u8,
    /// Echoes banked before this run (read-only mirror of MetaProgress for handoff).
    pub echoes_pre: u64,
    /// Echoes accumulated in-run (lost on death except for soft-perma carryover).
    pub echoes_in_run: u64,
    /// Monotonic per-player run-id (for seed-pinning and run-share attestation).
    pub run_id: u64,
    /// Pinned u128 seed — immutable post-genesis.
    pub seed: u128,
}

impl RunState {
    /// § Genesis-construct a new run-state.
    ///
    /// `player_id_hash` and `run_counter` deterministically pin the seed.
    /// All in-run counters start zero. Phase = Hub (caller transitions to
    /// BiomeSelect when the player commits to entering a biome).
    pub fn genesis(player_id_hash: u64, run_counter: u64) -> Self {
        Self {
            phase: RunPhase::Hub,
            current_biome: None,
            floor_count: 3,
            depth: 0,
            echoes_pre: 0,
            echoes_in_run: 0,
            run_id: run_counter,
            seed: pin_seed(player_id_hash, run_counter),
        }
    }

    /// § Transition Hub → BiomeSelect.
    pub fn enter_biome_select(&mut self) -> Result<(), RunStateErr> {
        match self.phase {
            RunPhase::Hub | RunPhase::Reward { .. } => {
                self.phase = RunPhase::BiomeSelect;
                Ok(())
            }
            _ => Err(RunStateErr::InvalidTransition {
                from: self.phase.clone(),
                to_kind: "BiomeSelect",
            }),
        }
    }

    /// § Transition BiomeSelect → Floor(1, biome) ; sets floor_count.
    pub fn descend_into(&mut self, biome: Biome, floor_count: u8) -> Result<(), RunStateErr> {
        match self.phase {
            RunPhase::BiomeSelect => {
                self.phase = RunPhase::Floor { idx: 1, biome };
                self.current_biome = Some(biome);
                self.floor_count = floor_count.clamp(3, 12);
                self.depth = 1;
                Ok(())
            }
            _ => Err(RunStateErr::InvalidTransition {
                from: self.phase.clone(),
                to_kind: "Floor",
            }),
        }
    }

    /// § Advance to next floor ; transitions to BossArena when idx == floor_count.
    pub fn advance_floor(&mut self) -> Result<(), RunStateErr> {
        let RunPhase::Floor { biome, .. } = self.phase else {
            return Err(RunStateErr::InvalidTransition {
                from: self.phase.clone(),
                to_kind: "advance_floor",
            });
        };
        let next_idx = self.depth.saturating_add(1);
        if next_idx > self.floor_count {
            return Err(RunStateErr::FloorIndexOutOfRange {
                idx: next_idx,
                ceiling: self.floor_count,
            });
        }
        if next_idx == self.floor_count {
            self.phase = RunPhase::BossArena { biome };
        } else {
            self.phase = RunPhase::Floor { idx: next_idx, biome };
        }
        self.depth = next_idx;
        Ok(())
    }

    /// § Transition BossArena → Reward.
    pub fn boss_cleared(&mut self) -> Result<(), RunStateErr> {
        match self.phase {
            RunPhase::BossArena { biome } => {
                self.phase = RunPhase::Reward { biome };
                Ok(())
            }
            _ => Err(RunStateErr::InvalidTransition {
                from: self.phase.clone(),
                to_kind: "Reward",
            }),
        }
    }

    /// § Award Echoes for in-run actions (drops + boss-kills).
    pub fn award_echoes(&mut self, amount: u64) {
        self.echoes_in_run = self.echoes_in_run.saturating_add(amount);
    }

    /// § Sanity-check : seed must not change post-genesis.
    pub fn seed_immutable_after_genesis(&self, original_seed: u128) -> Result<(), RunStateErr> {
        if self.seed != original_seed {
            return Err(RunStateErr::SeedImmutable);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_starts_at_hub() {
        let s = RunState::genesis(0xABCD_1234, 1);
        assert!(matches!(s.phase, RunPhase::Hub));
        assert_eq!(s.depth, 0);
        assert_eq!(s.run_id, 1);
        assert_ne!(s.seed, 0);
    }

    #[test]
    fn award_echoes_is_saturating() {
        let mut s = RunState::genesis(1, 1);
        s.echoes_in_run = u64::MAX - 5;
        s.award_echoes(100);
        assert_eq!(s.echoes_in_run, u64::MAX);
    }
}
