// § bloom.rs — Accuracy-bloom : sustained-fire cone-growth + grace + decay
// ════════════════════════════════════════════════════════════════════
// § I> per spec §I.3.accuracy-bloom :
//      · cone-of-fire grows linearly per-shot · max-bloom @ N-shots
//      · recovery-grace = 200ms after fire-stop · then exponential-decay
//      · hipfire-bloom-max > ADS-bloom-max (lenient-on-aim-down axiom)
//      · deterministic ; bloom-state checkpointable per WeaponState
// § I> emits AccuracyConeRadiansOverride consumed by cssl-host-weapons (W13-2)
//      to tighten the actual hit-test cone for this player's shots.

use serde::{Deserialize, Serialize};

/// Grace period in milliseconds — bloom does not start decaying until after.
pub const BLOOM_GRACE_MS: f32 = 200.0;
/// Per-shot bloom-cone increment in radians (linear growth toward max).
pub const BLOOM_PER_SHOT_RAD: f32 = 0.005;
/// Max hipfire bloom-cone (radians).
pub const BLOOM_MAX_HIPFIRE_RAD: f32 = 0.080;
/// Max ADS bloom-cone (radians) — strictly less than hipfire (lenient-on-aim-down).
pub const BLOOM_MAX_ADS_RAD: f32 = 0.020;
/// Exponential-decay rate constant (1/sec) once grace expires. Calibrated so
/// 5 seconds of decay reduces the cone to under 5% of its post-grace value :
///   e^(-1.0 × 4.8) ≈ 0.0082 ; comfortably below the 5% threshold.
pub const BLOOM_DECAY_PER_SEC: f32 = 1.0;

/// Bloom state — accumulator over time. `current_rad` is the actual cone-radius
/// the weapon system uses for spread sampling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BloomState {
    pub current_rad: f32,
    /// Time since last fire in milliseconds — drives grace + decay.
    pub time_since_fire_ms: f32,
}

impl Default for BloomState {
    fn default() -> Self {
        Self::new()
    }
}

impl BloomState {
    /// New zero-bloom state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            current_rad: 0.0,
            time_since_fire_ms: BLOOM_GRACE_MS, // start past-grace, fully-recovered
        }
    }

    /// Apply one shot — adds linear increment, capped to `max_rad`.
    /// `is_ads` selects the appropriate max-cap (ADS tighter than hipfire).
    pub fn on_fire(&mut self, is_ads: bool) {
        let max = if is_ads { BLOOM_MAX_ADS_RAD } else { BLOOM_MAX_HIPFIRE_RAD };
        self.current_rad = (self.current_rad + BLOOM_PER_SHOT_RAD).min(max);
        self.time_since_fire_ms = 0.0;
    }

    /// Advance state. During the first `BLOOM_GRACE_MS` after fire-stop, the
    /// cone holds steady. After grace, exponential-decay returns it to zero.
    pub fn step(&mut self, dt_ms: f32) {
        let dt = dt_ms.max(0.0);
        self.time_since_fire_ms += dt;
        if self.time_since_fire_ms <= BLOOM_GRACE_MS {
            return; // grace : no decay
        }
        // Exponential decay : rad *= exp(-rate * dt_sec)
        let dt_sec = dt / 1000.0;
        let factor = (-BLOOM_DECAY_PER_SEC * dt_sec).exp().max(0.0);
        self.current_rad = (self.current_rad * factor).max(0.0);
        // Floor : if effectively zero, snap to zero so f32 underflow doesn't
        // hold a permanent epsilon tail.
        if self.current_rad < 1e-6 {
            self.current_rad = 0.0;
        }
    }

    /// Effective cap given current ADS state — used by external query.
    #[must_use]
    pub fn cap_rad(is_ads: bool) -> f32 {
        if is_ads { BLOOM_MAX_ADS_RAD } else { BLOOM_MAX_HIPFIRE_RAD }
    }

    /// Reset on weapon-swap / death.
    pub fn reset(&mut self) {
        self.current_rad = 0.0;
        self.time_since_fire_ms = BLOOM_GRACE_MS;
    }
}

/// Event emitted to weapons-host (W13-2) per tick — spread-cone radians.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AccuracyConeRadiansOverride {
    pub cone_rad: f32,
    pub is_ads: bool,
}

impl AccuracyConeRadiansOverride {
    pub fn snapshot(state: &BloomState, is_ads: bool) -> Self {
        Self {
            cone_rad: state.current_rad,
            is_ads,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_zero_bloom() {
        let s = BloomState::new();
        assert_eq!(s.current_rad, 0.0);
    }

    #[test]
    fn bloom_grows_with_fire() {
        let mut s = BloomState::new();
        let mut prev = s.current_rad;
        for _ in 0..5 {
            s.on_fire(false);
            assert!(s.current_rad > prev || s.current_rad == BLOOM_MAX_HIPFIRE_RAD);
            prev = s.current_rad;
        }
    }

    #[test]
    fn bloom_caps_at_hipfire_max() {
        let mut s = BloomState::new();
        for _ in 0..1000 {
            s.on_fire(false);
        }
        assert_eq!(s.current_rad, BLOOM_MAX_HIPFIRE_RAD);
    }

    #[test]
    fn ads_max_strictly_less_than_hipfire_max() {
        // Spec invariant : lenient-on-aim-down ; hipfire-bloom-max > ADS-bloom-max.
        assert!(BLOOM_MAX_ADS_RAD < BLOOM_MAX_HIPFIRE_RAD);
        // Concrete : fire 1000 shots ADS, confirm cap at ADS-max.
        let mut s = BloomState::new();
        for _ in 0..1000 {
            s.on_fire(true);
        }
        assert_eq!(s.current_rad, BLOOM_MAX_ADS_RAD);
    }

    #[test]
    fn ads_spread_tighter_than_hipfire() {
        // After equal shot-counts the ADS cone should be ≤ hipfire cone.
        let mut hip = BloomState::new();
        let mut ads = BloomState::new();
        for _ in 0..50 {
            hip.on_fire(false);
            ads.on_fire(true);
        }
        assert!(ads.current_rad <= hip.current_rad);
    }

    #[test]
    fn bloom_recovery_200ms_grace_no_decay() {
        let mut s = BloomState::new();
        s.on_fire(false);
        let after_fire = s.current_rad;
        // 5 × 40ms = 200ms (right at grace boundary)
        for _ in 0..5 {
            s.step(40.0);
        }
        // No decay during grace
        assert_eq!(s.current_rad, after_fire);
    }

    #[test]
    fn bloom_exp_decay_after_grace() {
        let mut s = BloomState::new();
        for _ in 0..10 {
            s.on_fire(false);
        }
        let initial = s.current_rad;
        // 200ms grace + 5 seconds decay
        for _ in 0..520 {
            s.step(10.0);
        }
        assert!(s.current_rad < initial * 0.05);
    }

    #[test]
    fn cap_rad_query() {
        assert_eq!(BloomState::cap_rad(false), BLOOM_MAX_HIPFIRE_RAD);
        assert_eq!(BloomState::cap_rad(true), BLOOM_MAX_ADS_RAD);
    }

    #[test]
    fn reset_clears_bloom() {
        let mut s = BloomState::new();
        s.on_fire(false);
        s.reset();
        assert_eq!(s.current_rad, 0.0);
    }

    #[test]
    fn override_event_snapshot() {
        let mut s = BloomState::new();
        s.on_fire(false);
        let evt = AccuracyConeRadiansOverride::snapshot(&s, false);
        assert_eq!(evt.cone_rad, s.current_rad);
        assert!(!evt.is_ads);
    }
}
