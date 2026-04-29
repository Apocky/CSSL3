//! § Stability prediction + adaptive substep selection.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § THESIS (Wave-Unity §VI.2 + §VI.3)
//!   The wave-solver picks a per-frame substep count `n_substeps ∈
//!   [1, MAX_SUBSTEPS]` based on a KAN-predicted stable Δt :
//!
//!     `n_substeps = clamp(ceil(dt_frame / dt_kan), 1, 16)`
//!
//!   The KAN inference is mocked at this slice via [`MockStabilityKan`]
//!   — it returns a deterministic estimate based on the maximum |ψ|²
//!   in the field summary. The real D115 KAN runtime evaluates a
//!   small spline network ; the surface is identical so swap is one
//!   trait-impl change.
//!
//! § STABILITY HEURISTIC
//!   Stage-0 mock heuristic :
//!
//!     `dt_stable = min(CFL_dt) · safety_factor / max(1, sqrt(max_norm_sqr))`
//!
//!   - `CFL_dt` is the per-band CFL bound `Δx / c`.
//!   - `safety_factor = 0.5` for explicit bands ; 0.9 for implicit bands.
//!   - High-norm regions request smaller Δt.
//!
//! § DETERMINISM
//!   The mock heuristic is a pure function of the field summary +
//!   safety constants. No RNG.

use crate::band::BandClass;
use crate::psi_field::WaveField;

/// § Substep ceiling per frame. Matches Wave-Unity §VI.2 + UPDATE_RULE
///   §III adaptive-substeps.
pub const MAX_SUBSTEPS: u32 = 16;
/// § Substep floor — at least one update per frame.
pub const MIN_SUBSTEPS: u32 = 1;

/// § The stability-predictor trait. Real D115 KAN runtime impls this ;
///   Stage-0 ships [`MockStabilityKan`] as the deterministic fallback.
pub trait KanStability {
    /// § Predict the stable Δt for the current field state.
    ///   `summary` carries the per-band max-norm + cell-count statistics
    ///   the predictor consumes. Result is in seconds.
    fn predict_stable_dt(&self, summary: &FieldSummary) -> f64;
}

/// § Lightweight summary the KAN-stability predictor reads. Built on
///   demand from a `WaveField` snapshot.
#[derive(Debug, Clone, Copy)]
pub struct FieldSummary {
    /// § Total cell count across bands.
    pub total_cells: usize,
    /// § Maximum |ψ|² across bands. Used by the safety scaling.
    pub max_norm_sqr: f64,
    /// § Minimum CFL Δt across bands (seconds).
    pub min_cfl_dt: f64,
}

impl FieldSummary {
    /// § Build a summary from a `WaveField`.
    #[must_use]
    pub fn from_field<const C: usize>(field: &WaveField<C>) -> Self {
        let mut max_norm = 0.0_f64;
        let mut min_cfl = f64::INFINITY;
        for b in 0..field.band_count() {
            let n = field.band_norm_sqr(b);
            if n > max_norm {
                max_norm = n;
            }
            // CFL = Δx / c per band ; we don't know `c` from the field
            // metadata alone — class is the proxy.
            let dx = field.dx_m(b);
            let c = match field.class(b) {
                BandClass::FastDirect => 343.0_f64,
                BandClass::FastEnvelope => 2.997_924_58e8,
                BandClass::SlowEnvelope => 1.0e-3,
            };
            let cfl = dx / c;
            if cfl < min_cfl {
                min_cfl = cfl;
            }
        }
        if !min_cfl.is_finite() {
            min_cfl = 1.0e-3;
        }
        Self {
            total_cells: field.total_cell_count(),
            max_norm_sqr: max_norm,
            min_cfl_dt: min_cfl,
        }
    }
}

/// § The Stage-0 deterministic mock. Returns a heuristic stable-Δt.
#[derive(Debug, Clone, Copy, Default)]
pub struct MockStabilityKan {
    /// § Safety factor multiplied into the CFL bound. Defaults to 0.5
    ///   (explicit bands).
    pub safety_factor: f64,
}

impl MockStabilityKan {
    /// § Default safety factor 0.5.
    #[must_use]
    pub const fn new() -> Self {
        Self { safety_factor: 0.5 }
    }

    /// § Custom safety factor.
    #[must_use]
    pub const fn with_safety(safety: f64) -> Self {
        Self {
            safety_factor: safety,
        }
    }
}

impl KanStability for MockStabilityKan {
    fn predict_stable_dt(&self, summary: &FieldSummary) -> f64 {
        let safety = if self.safety_factor <= 0.0 {
            0.5
        } else {
            self.safety_factor
        };
        let scale = 1.0 / (1.0 + summary.max_norm_sqr.sqrt());
        // Floor at 1 fs ; no upper cap. The light-band CFL is ~3e-11 s ;
        // a 1ns floor would dominate the prediction. The 1 fs floor is
        // safe (no field can reasonably demand sub-fs dt).
        (summary.min_cfl_dt * safety * scale).max(1.0e-15)
    }
}

/// § Predict the stable Δt for the current field state via the supplied
///   KAN-stability impl. Returns a positive Δt in seconds.
#[must_use]
pub fn predict_stable_dt<K: KanStability, const C: usize>(
    kan: &K,
    field: &WaveField<C>,
) -> f64 {
    let summary = FieldSummary::from_field(field);
    kan.predict_stable_dt(&summary)
}

/// § Compute the adaptive substep count given a frame `dt` + the
///   KAN-predicted stable Δt. Always clamped to `[MIN_SUBSTEPS, MAX_SUBSTEPS]`.
#[must_use]
pub fn adaptive_substep_count(dt_frame: f64, dt_stable: f64) -> u32 {
    if dt_stable <= 0.0 || !dt_stable.is_finite() {
        return MAX_SUBSTEPS;
    }
    let raw = (dt_frame / dt_stable).ceil();
    if !raw.is_finite() || raw < 1.0 {
        return MIN_SUBSTEPS;
    }
    let capped = raw.min(f64::from(MAX_SUBSTEPS)) as u32;
    capped.clamp(MIN_SUBSTEPS, MAX_SUBSTEPS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::band::Band;
    use crate::complex::C32;
    use cssl_substrate_omega_field::MortonKey;

    fn key(x: u64, y: u64, z: u64) -> MortonKey {
        MortonKey::encode(x, y, z).unwrap()
    }

    #[test]
    fn substep_constants_make_sense() {
        assert_eq!(MIN_SUBSTEPS, 1);
        assert_eq!(MAX_SUBSTEPS, 16);
    }

    #[test]
    fn empty_field_summary_safe_defaults() {
        let f = WaveField::<5>::with_default_bands();
        let s = FieldSummary::from_field(&f);
        assert_eq!(s.total_cells, 0);
        assert_eq!(s.max_norm_sqr, 0.0);
        assert!(s.min_cfl_dt > 0.0);
    }

    #[test]
    fn mock_stability_predicts_positive_dt() {
        let mock = MockStabilityKan::new();
        let f = WaveField::<5>::with_default_bands();
        let dt = predict_stable_dt(&mock, &f);
        assert!(dt > 0.0);
        assert!(dt.is_finite());
    }

    #[test]
    fn mock_stability_smaller_dt_for_high_amplitude() {
        let mock = MockStabilityKan::new();
        let mut f_low = WaveField::<5>::with_default_bands();
        let mut f_high = WaveField::<5>::with_default_bands();
        f_low.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(0.1, 0.0));
        f_high.set_band(Band::AudioSubKHz, key(0, 0, 0), C32::new(100.0, 0.0));
        let dt_low = predict_stable_dt(&mock, &f_low);
        let dt_high = predict_stable_dt(&mock, &f_high);
        assert!(dt_high < dt_low);
    }

    #[test]
    fn substep_count_clamps_to_min() {
        // dt_frame = 1 ms, dt_stable = 100 s ⇒ raw = ceil(1e-5) = 1.
        let n = adaptive_substep_count(1.0e-3, 100.0);
        assert_eq!(n, 1);
    }

    #[test]
    fn substep_count_clamps_to_max() {
        // dt_frame = 1 s, dt_stable = 1 us ⇒ raw = 1e6.
        let n = adaptive_substep_count(1.0, 1.0e-6);
        assert_eq!(n, MAX_SUBSTEPS);
    }

    #[test]
    fn substep_count_typical_case() {
        // dt_frame = 16 ms, dt_stable = 4 ms ⇒ raw = 4.
        let n = adaptive_substep_count(16.0e-3, 4.0e-3);
        assert_eq!(n, 4);
    }

    #[test]
    fn substep_count_zero_stable_dt_returns_max() {
        let n = adaptive_substep_count(1.0e-3, 0.0);
        assert_eq!(n, MAX_SUBSTEPS);
    }

    #[test]
    fn substep_count_negative_stable_dt_returns_max() {
        let n = adaptive_substep_count(1.0e-3, -1.0);
        assert_eq!(n, MAX_SUBSTEPS);
    }

    #[test]
    fn mock_stability_safety_factor_scales_dt() {
        let mock_default = MockStabilityKan::new();
        let mock_aggressive = MockStabilityKan::with_safety(0.9);
        let f = WaveField::<5>::with_default_bands();
        let dt_default = predict_stable_dt(&mock_default, &f);
        let dt_aggressive = predict_stable_dt(&mock_aggressive, &f);
        assert!(dt_aggressive > dt_default);
    }

    #[test]
    fn mock_stability_zero_safety_falls_back_to_default() {
        let mock = MockStabilityKan::with_safety(0.0);
        let f = WaveField::<5>::with_default_bands();
        let dt = predict_stable_dt(&mock, &f);
        assert!(dt > 0.0);
    }

    #[test]
    fn substep_replay_deterministic() {
        let n1 = adaptive_substep_count(16.0e-3, 4.0e-3);
        let n2 = adaptive_substep_count(16.0e-3, 4.0e-3);
        assert_eq!(n1, n2);
    }
}
