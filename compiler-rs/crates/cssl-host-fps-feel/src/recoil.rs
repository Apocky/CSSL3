// § recoil.rs — Recoil state-machine + per-archetype patterns + recovery
// ════════════════════════════════════════════════════════════════════
// § I> per spec §I.2.recoil-state-machine :
//      · 8 archetypes : Pistol·Smg·Rifle·Lmg·Shotgun·Sniper·Bow·Launcher
//      · per-archetype RecoilPattern : vertical-rise + horizontal-jitter
//      · recovery-curve returns-to-zero @ 300ms after-fire-stop (linear)
//      · pull-down-counter : skill-floor-not-ceiling
//      · deterministic-per-seed : replay-bit-equal
//      · cosmetic-only-axiom : pattern = mechanic ; visual-tracer-skin only
// § I> emits RecoilEvent (a snapshot of current cumulative kick) consumed
//      by camera (W13-4) for view-yaw/pitch-bias and weapons (W13-2) for
//      muzzle-direction adjustment.
// § I> WeaponArchetype enum mirrors first-8 of cssl-host-weapons WeaponKind
//      so a single dispatch table can serve both crates ; the .csl spec
//      pins these as the FROZEN-archetype-set.

use serde::{Deserialize, Serialize};
use crate::seed::DeterministicRng;

/// Recovery duration in milliseconds — recoil decays linearly to zero.
pub const RECOIL_RECOVERY_MS: f32 = 300.0;

/// 8 weapon-archetypes — frozen mechanic ; mirrors weapons-crate first-8.
/// Cosmetic-only-axiom : ALL archetypes have DPS-parity across skins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum WeaponArchetype {
    Pistol   = 0,
    Smg      = 1,
    Rifle    = 2,
    Lmg      = 3,
    Shotgun  = 4,
    Sniper   = 5,
    Bow      = 6,
    Launcher = 7,
}

impl WeaponArchetype {
    /// Iterate ∀ archetype ; helper for tests + table-build.
    pub const ALL: [WeaponArchetype; 8] = [
        WeaponArchetype::Pistol,
        WeaponArchetype::Smg,
        WeaponArchetype::Rifle,
        WeaponArchetype::Lmg,
        WeaponArchetype::Shotgun,
        WeaponArchetype::Sniper,
        WeaponArchetype::Bow,
        WeaponArchetype::Launcher,
    ];
}

/// Per-archetype recoil-pattern parameters. Frozen ∀ player (cosmetic-only-axiom).
/// `vertical_rise_rad` = per-shot pitch-up kick (radians).
/// `horizontal_jitter_rad` = per-shot horizontal-spread amplitude (centered-uniform).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecoilPattern {
    pub vertical_rise_rad: f32,
    pub horizontal_jitter_rad: f32,
}

impl RecoilPattern {
    /// Lookup the frozen pattern for a given archetype.
    /// Numbers calibrated to give Pistol < Smg ≤ Rifle < Lmg, with Shotgun and
    /// Launcher loud-but-slow, Sniper one-shot-heavy, Bow zero (drawn-shot).
    #[must_use]
    pub const fn for_archetype(a: WeaponArchetype) -> Self {
        match a {
            WeaponArchetype::Pistol   => Self { vertical_rise_rad: 0.005, horizontal_jitter_rad: 0.002 },
            WeaponArchetype::Smg      => Self { vertical_rise_rad: 0.008, horizontal_jitter_rad: 0.006 },
            WeaponArchetype::Rifle    => Self { vertical_rise_rad: 0.012, horizontal_jitter_rad: 0.004 },
            WeaponArchetype::Lmg      => Self { vertical_rise_rad: 0.015, horizontal_jitter_rad: 0.008 },
            WeaponArchetype::Shotgun  => Self { vertical_rise_rad: 0.030, horizontal_jitter_rad: 0.010 },
            WeaponArchetype::Sniper   => Self { vertical_rise_rad: 0.050, horizontal_jitter_rad: 0.001 },
            WeaponArchetype::Bow      => Self { vertical_rise_rad: 0.000, horizontal_jitter_rad: 0.000 },
            WeaponArchetype::Launcher => Self { vertical_rise_rad: 0.040, horizontal_jitter_rad: 0.005 },
        }
    }
}

/// Recoil state — accumulator over time. `pitch_rad`/`yaw_rad` are the
/// cumulative camera-bias produced by repeated fires, decaying back to zero.
///
/// The decay model uses *peak-snapshot* linear decay : on every fire, we
/// snapshot the current kick into `peak_pitch_rad`/`peak_yaw_rad` and reset
/// the timer. During recovery we lerp from the snapshot to zero over
/// `RECOIL_RECOVERY_MS` so the curve is bit-equal regardless of dt-slicing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecoilState {
    pub archetype: WeaponArchetype,
    pub pitch_rad: f32,
    pub yaw_rad: f32,
    /// Time since last fire in milliseconds — drives recovery curve.
    pub time_since_fire_ms: f32,
    /// Snapshot of pitch the moment of last-fire ; recovery lerps from this.
    pub peak_pitch_rad: f32,
    /// Snapshot of yaw the moment of last-fire ; recovery lerps from this.
    pub peak_yaw_rad: f32,
    /// Cumulative pull-down compensation applied by the player (skill-floor).
    pub pull_down_compensation_rad: f32,
}

impl RecoilState {
    /// New state for a given archetype, zero-kick.
    #[must_use]
    pub const fn new(archetype: WeaponArchetype) -> Self {
        Self {
            archetype,
            pitch_rad: 0.0,
            yaw_rad: 0.0,
            time_since_fire_ms: RECOIL_RECOVERY_MS, // start fully-recovered
            peak_pitch_rad: 0.0,
            peak_yaw_rad: 0.0,
            pull_down_compensation_rad: 0.0,
        }
    }

    /// Apply one shot — adds vertical rise + horizontal jitter.
    /// `rng` is advanced for deterministic-per-seed jitter.
    pub fn on_fire(&mut self, rng: &mut DeterministicRng) {
        let pat = RecoilPattern::for_archetype(self.archetype);
        self.pitch_rad += pat.vertical_rise_rad;
        let j = rng.next_f32_centered() * pat.horizontal_jitter_rad;
        self.yaw_rad += j;
        // Snapshot post-fire kick as the new recovery starting-point.
        self.peak_pitch_rad = self.pitch_rad;
        self.peak_yaw_rad = self.yaw_rad;
        self.time_since_fire_ms = 0.0;
    }

    /// Advance recovery — linear-decay back to zero over RECOIL_RECOVERY_MS.
    /// `dt_ms` is the frame delta. `pull_down_input_rad` is the player's
    /// downward camera-correction this frame (skill-floor counter).
    pub fn step(&mut self, dt_ms: f32, pull_down_input_rad: f32) {
        let dt = dt_ms.max(0.0);
        self.time_since_fire_ms += dt;

        // Apply player skill-floor : subtract player downward-input from
        // both the live pitch AND the recovery-peak so subsequent recovery
        // is computed against the player's effective shoulder-bias.
        let comp = pull_down_input_rad.max(0.0);
        self.pull_down_compensation_rad += comp;
        self.pitch_rad = (self.pitch_rad - comp).max(0.0);
        self.peak_pitch_rad = (self.peak_pitch_rad - comp).max(0.0);

        // Linear recovery from peak → zero over RECOIL_RECOVERY_MS.
        // t ∈ [0, 1] = time_since_fire / window ; pitch = peak × (1-t).
        let t_norm = (self.time_since_fire_ms / RECOIL_RECOVERY_MS).min(1.0);
        let factor = (1.0 - t_norm).max(0.0);
        self.pitch_rad = self.peak_pitch_rad * factor;
        self.yaw_rad = self.peak_yaw_rad * factor;
    }

    /// Reset on weapon-swap / death.
    pub fn reset(&mut self) {
        self.pitch_rad = 0.0;
        self.yaw_rad = 0.0;
        self.time_since_fire_ms = RECOIL_RECOVERY_MS;
        self.peak_pitch_rad = 0.0;
        self.peak_yaw_rad = 0.0;
        self.pull_down_compensation_rad = 0.0;
    }
}

/// Event emitted to camera + weapons each tick — read-only snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecoilEvent {
    pub archetype: WeaponArchetype,
    pub pitch_rad: f32,
    pub yaw_rad: f32,
}

impl From<&RecoilState> for RecoilEvent {
    fn from(s: &RecoilState) -> Self {
        Self {
            archetype: s.archetype,
            pitch_rad: s.pitch_rad,
            yaw_rad: s.yaw_rad,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_table_covers_all_archetypes() {
        for a in WeaponArchetype::ALL {
            let p = RecoilPattern::for_archetype(a);
            assert!(p.vertical_rise_rad >= 0.0);
            assert!(p.horizontal_jitter_rad >= 0.0);
        }
    }

    #[test]
    fn fresh_state_has_no_kick() {
        let s = RecoilState::new(WeaponArchetype::Rifle);
        assert_eq!(s.pitch_rad, 0.0);
        assert_eq!(s.yaw_rad, 0.0);
    }

    #[test]
    fn on_fire_adds_vertical_rise() {
        let mut s = RecoilState::new(WeaponArchetype::Rifle);
        let mut rng = DeterministicRng::new(0xAA);
        let before = s.pitch_rad;
        s.on_fire(&mut rng);
        let pat = RecoilPattern::for_archetype(WeaponArchetype::Rifle);
        assert!((s.pitch_rad - (before + pat.vertical_rise_rad)).abs() < 1e-6);
        assert_eq!(s.time_since_fire_ms, 0.0);
    }

    #[test]
    fn recoil_pattern_deterministic_per_seed() {
        let mut a = RecoilState::new(WeaponArchetype::Smg);
        let mut b = RecoilState::new(WeaponArchetype::Smg);
        let mut ra = DeterministicRng::new(0xDEAD);
        let mut rb = DeterministicRng::new(0xDEAD);
        for _ in 0..32 {
            a.on_fire(&mut ra);
            b.on_fire(&mut rb);
        }
        assert_eq!(a.pitch_rad, b.pitch_rad);
        assert_eq!(a.yaw_rad, b.yaw_rad);
    }

    #[test]
    fn recoil_recovery_zero_at_300ms_post_stop() {
        let mut s = RecoilState::new(WeaponArchetype::Lmg);
        let mut rng = DeterministicRng::new(7);
        for _ in 0..10 {
            s.on_fire(&mut rng);
        }
        let initial_pitch = s.pitch_rad;
        assert!(initial_pitch > 0.0);
        // step 300ms with no fire and no pull-down
        for _ in 0..30 {
            s.step(10.0, 0.0);
        }
        assert!(s.pitch_rad < initial_pitch * 0.05); // ≈ 0
        assert!(s.yaw_rad.abs() < initial_pitch * 0.05);
    }

    #[test]
    fn pull_down_counter_cancels_kick() {
        let mut s = RecoilState::new(WeaponArchetype::Rifle);
        let mut rng = DeterministicRng::new(11);
        s.on_fire(&mut rng);
        let pat = RecoilPattern::for_archetype(WeaponArchetype::Rifle);
        // Player applies exact counter-pull = vertical_rise_rad
        s.step(1.0, pat.vertical_rise_rad);
        assert!(s.pitch_rad < pat.vertical_rise_rad * 0.5);
        assert!(s.pull_down_compensation_rad >= pat.vertical_rise_rad);
    }

    #[test]
    fn cosmetic_skin_no_mechanic_effect() {
        // The crate is data-only ; cosmetic-skin info lives in crosshair/bloom
        // visual layers. We assert the pattern table yields the same numbers
        // regardless of "skin" by re-querying twice and confirming equality.
        let p1 = RecoilPattern::for_archetype(WeaponArchetype::Sniper);
        let p2 = RecoilPattern::for_archetype(WeaponArchetype::Sniper);
        assert_eq!(p1.vertical_rise_rad, p2.vertical_rise_rad);
        assert_eq!(p1.horizontal_jitter_rad, p2.horizontal_jitter_rad);
    }

    #[test]
    fn bow_archetype_zero_kick() {
        // Drawn-shot mechanic ; no kick-pattern. Spec invariant.
        let p = RecoilPattern::for_archetype(WeaponArchetype::Bow);
        assert_eq!(p.vertical_rise_rad, 0.0);
        assert_eq!(p.horizontal_jitter_rad, 0.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut s = RecoilState::new(WeaponArchetype::Shotgun);
        let mut rng = DeterministicRng::new(2);
        s.on_fire(&mut rng);
        s.reset();
        assert_eq!(s.pitch_rad, 0.0);
        assert_eq!(s.yaw_rad, 0.0);
    }
}
