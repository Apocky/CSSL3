// § accuracy.rs — bloom + recovery model
// ════════════════════════════════════════════════════════════════════
// § I> Per-shot bloom-add ; per-second linear recovery (configurable per-kind).
//      Bloom = current accuracy-cone (radians). 0 = laser-perfect ; max = wide.
// § I> NaN-safe ; saturating ; pure-fn (state mutated via `&mut self`).
// § I> Deterministic-RNG used for cone-jitter sampling.
// ════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

use crate::seed::DeterministicRng;

/// Per-weapon accuracy parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AccuracyParams {
    /// Base cone (no bloom ; "ADS-laser" floor).
    pub base_cone_rad: f32,
    /// Cone added per shot.
    pub bloom_per_shot_rad: f32,
    /// Cone removed per second of no-fire.
    pub recovery_per_sec_rad: f32,
    /// Hard cap on cone (prevents arbitrary growth).
    pub max_cone_rad: f32,
}

impl AccuracyParams {
    /// Default-pistol params (sub-MOA-ish baseline).
    pub const PISTOL_DEFAULT: Self = Self {
        base_cone_rad: 0.005,
        bloom_per_shot_rad: 0.020,
        recovery_per_sec_rad: 0.15,
        max_cone_rad: 0.20,
    };
    /// Default-shotgun params (wider base + faster bloom).
    pub const SHOTGUN_DEFAULT: Self = Self {
        base_cone_rad: 0.080,
        bloom_per_shot_rad: 0.030,
        recovery_per_sec_rad: 0.25,
        max_cone_rad: 0.30,
    };
    /// Default-sniper params (laser baseline + heavy bloom).
    pub const SNIPER_DEFAULT: Self = Self {
        base_cone_rad: 0.001,
        bloom_per_shot_rad: 0.050,
        recovery_per_sec_rad: 0.40,
        max_cone_rad: 0.40,
    };
}

/// Mutable accuracy-state ; instantiated per-weapon-instance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AccuracyState {
    pub params: AccuracyParams,
    pub current_cone_rad: f32,
}

impl AccuracyState {
    #[must_use]
    pub const fn new(params: AccuracyParams) -> Self {
        Self {
            params,
            current_cone_rad: params.base_cone_rad,
        }
    }

    /// Apply per-shot bloom (saturating to max).
    pub fn on_shot(&mut self) {
        let next = self.current_cone_rad + self.params.bloom_per_shot_rad;
        self.current_cone_rad = if next.is_finite() {
            next.min(self.params.max_cone_rad)
        } else {
            self.params.max_cone_rad
        };
    }

    /// Decay cone toward base over `dt_secs`.
    pub fn tick(&mut self, dt_secs: f32) {
        let safe_dt = if dt_secs.is_finite() && dt_secs >= 0.0 {
            dt_secs
        } else {
            0.0
        };
        let decay = self.params.recovery_per_sec_rad * safe_dt;
        let next = self.current_cone_rad - decay;
        self.current_cone_rad = if next.is_finite() {
            next.max(self.params.base_cone_rad)
        } else {
            self.params.base_cone_rad
        };
    }

    /// Sample a 2D jitter offset (radians) drawn uniformly from current cone.
    pub fn sample_jitter(&self, rng: &mut DeterministicRng) -> (f32, f32) {
        let r = self.current_cone_rad * rng.next_f32().sqrt();
        let theta = rng.next_f32() * std::f32::consts::TAU;
        (r * theta.cos(), r * theta.sin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shot_blooms() {
        let mut s = AccuracyState::new(AccuracyParams::PISTOL_DEFAULT);
        let pre = s.current_cone_rad;
        s.on_shot();
        assert!(s.current_cone_rad > pre);
    }

    #[test]
    fn recovery_returns_to_base() {
        let mut s = AccuracyState::new(AccuracyParams::PISTOL_DEFAULT);
        for _ in 0..10 {
            s.on_shot();
        }
        let bloomed = s.current_cone_rad;
        s.tick(10.0); // long recovery
        assert!(s.current_cone_rad < bloomed);
        assert!((s.current_cone_rad - s.params.base_cone_rad).abs() < 1e-5);
    }

    #[test]
    fn bloom_capped_at_max() {
        let mut s = AccuracyState::new(AccuracyParams::PISTOL_DEFAULT);
        for _ in 0..1000 {
            s.on_shot();
        }
        assert!(s.current_cone_rad <= s.params.max_cone_rad + 1e-6);
    }

    #[test]
    fn jitter_within_cone() {
        let mut rng = DeterministicRng::new(12345);
        let s = AccuracyState::new(AccuracyParams::SHOTGUN_DEFAULT);
        for _ in 0..256 {
            let (jx, jy) = s.sample_jitter(&mut rng);
            let mag = (jx * jx + jy * jy).sqrt();
            assert!(mag <= s.current_cone_rad + 1e-5);
        }
    }
}
