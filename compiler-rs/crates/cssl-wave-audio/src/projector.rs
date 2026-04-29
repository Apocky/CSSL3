//! § WaveAudioProjector — ψ-AUDIO field → listener-position binaural projection.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Per `Omniverse/04_OMEGA_FIELD/04_WAVE_UNITY.csl § XV` :
//!
//!   ```text
//!   | Audio-listener as separate query-path | violates §0 same-substrate
//!   ⊗ AudioListener samples ψ |
//!   ```
//!
//!   `WaveAudioProjector` is the canonical "AudioListener samples ψ"
//!   surface. It :
//!
//!     1. Samples the ψ-AUDIO field at the listener's left + right ear
//!        positions (trilinear interpolation across the sparse-Morton
//!        grid).
//!     2. Applies a Doppler-shift correction based on listener velocity.
//!     3. Forwards to the [`BinauralRender`] for stereo output.
//!     4. Optionally folds in cross-band-coupling shimmer from the
//!        [`CrossBandCoupler`] (when D114's wave-solver lights up the
//!        LIGHT band).
//!
//! § HRTF + RC-DERIVED ITD/ILD (per FIELD_AUDIO § IV)
//!   The wave-unity ITD/ILD emerges from the differential ψ-sampling at
//!   the two ear positions ; the projector does NOT bake an HRTF
//!   database in. Instead :
//!
//!     - **ITD** : two ear-probes sit at different distances from the
//!       source ; the LBM/Helmholtz solver delivers waves at different
//!       times. The projector samples both probes at the SAME wall-clock
//!       sample-time, so the differential delay is automatic.
//!     - **ILD** : the binaural renderer applies a head-shadow gain
//!       reduction to the contralateral ear based on azimuth. This is
//!       a coarse approximation ; a full HRTF database would replace
//!       it (deferred per `cssl-audio-mix § HRTF`).
//!
//! § DOPPLER
//!   A moving listener experiences shifted frequencies. We don't
//!   pre-shift the field (that would violate substrate-locality) ;
//!   instead the projector re-samples with a time-stretched read that
//!   approximates the same effect. For stage-0 we expose the Doppler
//!   ratio in the projection result so the caller can apply it via
//!   their own resampler ; the internal pipeline does NOT alter the
//!   field.
//!
//! § DETERMINISM
//!   All operations are pure functions over the inputs. Two replays
//!   with identical inputs produce bit-equal stereo output.

use crate::binaural::{BinauralRender, StereoSample};
use crate::complex::Complex;
use crate::listener::{AudioListener, SPEED_OF_SOUND};
use crate::psi_field::PsiAudioField;
use crate::vec3::Vec3;

/// Result of one projection step : the per-ear ψ-amplitudes + the
/// rendered stereo sample + the Doppler-shift ratio for caller-side
/// resampling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectionResult {
    /// ψ-amplitude sampled at the LEFT ear.
    pub psi_left: Complex,
    /// ψ-amplitude sampled at the RIGHT ear.
    pub psi_right: Complex,
    /// Stereo `f32` sample after binaural rendering.
    pub stereo: StereoSample,
    /// Doppler-shift ratio (1.0 = no shift). Caller may apply this to
    /// the source's pitch via their own resampler.
    pub doppler_ratio: f32,
    /// Source-azimuth in the listener's horizontal plane (radians).
    pub azimuth_rad: f32,
    /// Inter-aural-time-difference in seconds (computed analytically
    /// from per-ear distance to the source for cross-checking that the
    /// ψ-field carries the correct physical delay).
    pub itd_seconds: f32,
}

impl ProjectionResult {
    /// Silent/neutral projection : zero ψ, zero stereo, unit Doppler.
    pub const SILENCE: ProjectionResult = ProjectionResult {
        psi_left: Complex::ZERO,
        psi_right: Complex::ZERO,
        stereo: StereoSample::SILENCE,
        doppler_ratio: 1.0,
        azimuth_rad: 0.0,
        itd_seconds: 0.0,
    };
}

/// Configuration knobs for the projector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectorConfig {
    /// Voxel size in metres for the ψ-AUDIO field. Default 0.5m per
    /// spec § III BAND-CELL TABLE AUDIO row.
    pub voxel_size: f32,
    /// Speed of sound in m/s for Doppler computation. Default 343.
    pub speed_of_sound: f32,
    /// Doppler-shift clamp range. Default [0.25, 4.0] matching legacy.
    pub doppler_min: f32,
    /// Doppler-shift clamp upper bound.
    pub doppler_max: f32,
    /// Master post-render gain applied AFTER the binaural renderer's
    /// own gain. Useful for per-listener volume in split-screen.
    pub master_gain: f32,
}

impl Default for ProjectorConfig {
    fn default() -> ProjectorConfig {
        ProjectorConfig {
            voxel_size: 0.5,
            speed_of_sound: SPEED_OF_SOUND,
            doppler_min: 0.25,
            doppler_max: 4.0,
            master_gain: 1.0,
        }
    }
}

/// WaveAudioProjector — the ψ-AUDIO sampler + binaural renderer.
#[derive(Debug, Clone)]
pub struct WaveAudioProjector {
    config: ProjectorConfig,
    binaural: BinauralRender,
}

impl Default for WaveAudioProjector {
    fn default() -> WaveAudioProjector {
        WaveAudioProjector::new(ProjectorConfig::default(), BinauralRender::default())
    }
}

impl WaveAudioProjector {
    /// Construct a new projector.
    #[must_use]
    pub const fn new(config: ProjectorConfig, binaural: BinauralRender) -> WaveAudioProjector {
        WaveAudioProjector { config, binaural }
    }

    /// Read the active configuration.
    #[must_use]
    pub const fn config(&self) -> ProjectorConfig {
        self.config
    }

    /// Update the configuration in place.
    pub fn set_config(&mut self, config: ProjectorConfig) {
        self.config = config;
    }

    /// Read the binaural renderer.
    #[must_use]
    pub const fn binaural(&self) -> &BinauralRender {
        &self.binaural
    }

    /// Mutate the binaural renderer config.
    pub fn binaural_mut(&mut self) -> &mut BinauralRender {
        &mut self.binaural
    }

    /// Project the ψ-AUDIO field to the listener's ears.
    ///
    /// `source_pos` is OPTIONAL : when `Some(pos)` the projector
    /// computes the source-azimuth + Doppler ratio relative to that
    /// position ; when `None` the projector falls back to looking-
    /// straight-ahead and unit Doppler.
    #[must_use]
    pub fn project(
        &self,
        psi: &PsiAudioField,
        listener: &AudioListener,
        source_pos: Option<Vec3>,
        source_velocity: Option<Vec3>,
    ) -> ProjectionResult {
        // Sample ψ at the per-ear positions.
        let le = listener.left_ear().to_array();
        let re = listener.right_ear().to_array();
        let voxel = self.config.voxel_size.max(1e-3);
        let psi_left = psi.sample_world(le, voxel);
        let psi_right = psi.sample_world(re, voxel);

        // Compute azimuth + Doppler relative to the source (when known).
        let (azimuth_rad, doppler_ratio, itd_seconds) = match source_pos {
            Some(sp) => {
                let dir = sp.sub(listener.position).normalize();
                let right_axis = listener.orientation.right();
                let pan = dir.dot(right_axis).clamp(-1.0, 1.0);
                let az = pan.asin();
                let dop = compute_doppler_ratio(
                    listener.velocity,
                    source_velocity.unwrap_or(Vec3::ZERO),
                    dir,
                    self.config,
                );
                let dl = listener.left_ear().sub(sp).length();
                let dr = listener.right_ear().sub(sp).length();
                let c = self.config.speed_of_sound.max(1.0);
                let itd = (dr - dl) / c;
                (az, dop, itd)
            }
            None => (0.0, 1.0, 0.0),
        };

        let pre_master = self
            .binaural
            .render_sample(psi_left, psi_right, azimuth_rad);
        let g = self.config.master_gain * listener.gain;
        let stereo = pre_master.scale(g);

        ProjectionResult {
            psi_left,
            psi_right,
            stereo,
            doppler_ratio,
            azimuth_rad,
            itd_seconds,
        }
    }

    /// Project a block of `n_samples` ; the field is sampled once per
    /// projection call (the ψ-AUDIO band is updated by the LBM solver,
    /// so for fixed-listener scenarios all samples in a block share the
    /// same ψ snapshot). Output is `[L, R, L, R, ...]`.
    pub fn project_block(
        &self,
        psi: &PsiAudioField,
        listener: &AudioListener,
        source_pos: Option<Vec3>,
        source_velocity: Option<Vec3>,
        out: &mut [f32],
    ) -> usize {
        let r = self.project(psi, listener, source_pos, source_velocity);
        let cap = out.len() / 2;
        for i in 0..cap {
            out[2 * i] = r.stereo.left;
            out[2 * i + 1] = r.stereo.right;
        }
        cap
    }

    /// Project from a TIME-SERIES of ψ-snapshots. Used by the binaural
    /// integration test to verify that a swept-frequency source
    /// produces the expected azimuth-stable stereo output.
    pub fn project_series(
        &self,
        psi_left_series: &[Complex],
        psi_right_series: &[Complex],
        azimuth_rad: f32,
        out: &mut [f32],
    ) -> usize {
        let n = psi_left_series.len().min(psi_right_series.len());
        let cap = out.len() / 2;
        let n = n.min(cap);
        for i in 0..n {
            let s =
                self.binaural
                    .render_sample(psi_left_series[i], psi_right_series[i], azimuth_rad);
            out[2 * i] = s.left * self.config.master_gain;
            out[2 * i + 1] = s.right * self.config.master_gain;
        }
        n
    }
}

/// Compute the Doppler-shift ratio for a moving listener + moving source
/// along the source-direction axis. Ratio is clamped to the configured
/// range to avoid extreme pitch-shifts on the audio thread.
#[must_use]
pub fn compute_doppler_ratio(
    listener_velocity: Vec3,
    source_velocity: Vec3,
    source_direction: Vec3,
    config: ProjectorConfig,
) -> f32 {
    let dir = source_direction;
    if dir.length_squared() < 1e-12 {
        return 1.0;
    }
    let v_l_along = listener_velocity.dot(dir);
    let v_s_along = source_velocity.dot(dir);
    let c = config.speed_of_sound;
    let denom = c - v_s_along;
    if denom.abs() < 1e-3 {
        return 1.0;
    }
    let ratio = (c - v_l_along) / denom;
    ratio.clamp(config.doppler_min, config.doppler_max)
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::{compute_doppler_ratio, ProjectionResult, ProjectorConfig, WaveAudioProjector};
    use crate::binaural::BinauralRender;
    use crate::complex::Complex;
    use crate::listener::AudioListener;
    use crate::psi_field::PsiAudioField;
    use crate::vec3::Vec3;
    use cssl_substrate_omega_field::morton::MortonKey;

    #[test]
    fn projection_silence_constants() {
        let s = ProjectionResult::SILENCE;
        assert_eq!(s.psi_left, Complex::ZERO);
        assert_eq!(s.psi_right, Complex::ZERO);
        assert_eq!(s.doppler_ratio, 1.0);
        assert_eq!(s.itd_seconds, 0.0);
    }

    #[test]
    fn projector_default_config() {
        let p = WaveAudioProjector::default();
        assert_eq!(p.config().voxel_size, 0.5);
        assert!((p.config().speed_of_sound - 343.0).abs() < 0.1);
    }

    #[test]
    fn project_silent_field_returns_silence_at_listener() {
        let psi = PsiAudioField::new();
        let l = AudioListener::at_origin();
        let p = WaveAudioProjector::default();
        let r = p.project(&psi, &l, None, None);
        assert!(r.psi_left.norm() < 1e-6);
        assert!(r.psi_right.norm() < 1e-6);
        assert!(r.stereo.left.abs() < 1e-6);
        assert!(r.stereo.right.abs() < 1e-6);
    }

    #[test]
    fn project_active_field_emits_stereo() {
        let mut psi = PsiAudioField::new();
        // Place a non-trivial ψ-amplitude at the cell containing the
        // listener's head-center. With voxel_size=0.5 the listener at
        // origin sits at cell (0,0,0).
        let k = MortonKey::encode(0, 0, 0).unwrap();
        psi.set(k, Complex::new(0.5, 0.0)).unwrap();

        let l = AudioListener::at_origin();
        let p = WaveAudioProjector::default();
        let r = p.project(&psi, &l, None, None);
        assert!(r.psi_left.norm() > 0.0);
        assert!(r.psi_right.norm() > 0.0);
    }

    #[test]
    fn project_with_right_source_pans_right() {
        // Source at +X (right of listener). The ψ-field is uniform at the
        // listener's two ear positions ; the binaural renderer's ILD
        // attenuates the LEFT ear → right > left.
        let mut psi = PsiAudioField::new();
        // Fill several cells around origin to cover both ears.
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if x < 0 || y < 0 || z < 0 {
                        continue;
                    }
                    let k = MortonKey::encode(x as u64, y as u64, z as u64).unwrap();
                    psi.set(k, Complex::new(0.5, 0.0)).unwrap();
                }
            }
        }
        let l = AudioListener::at_origin();
        let source = Vec3::new(10.0, 0.0, 0.0);
        let p = WaveAudioProjector::default();
        let r = p.project(&psi, &l, Some(source), None);
        // Source at +X → azimuth π/2 → ILD attenuates LEFT.
        assert!(r.azimuth_rad > 0.0);
        assert!(r.stereo.right > r.stereo.left);
    }

    #[test]
    fn project_with_left_source_pans_left() {
        let mut psi = PsiAudioField::new();
        for x in 0..2 {
            let k = MortonKey::encode(x, 0, 0).unwrap();
            psi.set(k, Complex::new(0.5, 0.0)).unwrap();
        }
        let l = AudioListener::at_origin();
        let source = Vec3::new(-10.0, 0.0, 0.0);
        let p = WaveAudioProjector::default();
        let r = p.project(&psi, &l, Some(source), None);
        assert!(r.azimuth_rad < 0.0);
        assert!(r.stereo.left > r.stereo.right);
    }

    #[test]
    fn project_with_source_computes_itd() {
        // Source on the right at 1 m ; ITD should be sub-ms.
        let psi = PsiAudioField::new();
        let l = AudioListener::at_origin();
        let source = Vec3::new(1.0, 0.0, 0.0);
        let p = WaveAudioProjector::default();
        let r = p.project(&psi, &l, Some(source), None);
        // ITD should be in the human-realistic range : less than ~0.8 ms.
        assert!(r.itd_seconds.abs() < 0.001);
    }

    #[test]
    fn project_centered_source_zero_itd() {
        let psi = PsiAudioField::new();
        let l = AudioListener::at_origin();
        // Source directly ahead.
        let source = Vec3::new(0.0, 0.0, -10.0);
        let p = WaveAudioProjector::default();
        let r = p.project(&psi, &l, Some(source), None);
        assert!(r.itd_seconds.abs() < 1e-5);
    }

    #[test]
    fn project_block_fills_buffer() {
        let psi = PsiAudioField::new();
        let l = AudioListener::at_origin();
        let p = WaveAudioProjector::default();
        let mut out = vec![0.0_f32; 16];
        let n = p.project_block(&psi, &l, None, None, &mut out);
        assert_eq!(n, 8); // 16 / 2 = 8 stereo frames
    }

    #[test]
    fn doppler_ratio_static_is_unity() {
        let r = compute_doppler_ratio(
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::FORWARD,
            ProjectorConfig::default(),
        );
        assert_eq!(r, 1.0);
    }

    #[test]
    fn doppler_ratio_clamped_to_max() {
        let r = compute_doppler_ratio(
            Vec3::new(0.0, 0.0, 1000.0), // listener moving fast
            Vec3::ZERO,
            Vec3::FORWARD,
            ProjectorConfig::default(),
        );
        assert!(r <= 4.0);
    }

    #[test]
    fn doppler_ratio_clamped_to_min() {
        let r = compute_doppler_ratio(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 1000.0),
            Vec3::FORWARD,
            ProjectorConfig::default(),
        );
        assert!(r >= 0.25);
    }

    #[test]
    fn projector_doppler_in_result() {
        let psi = PsiAudioField::new();
        let mut l = AudioListener::at_origin();
        l.set_velocity(Vec3::new(0.0, 0.0, -50.0)); // moving forward at 50 m/s
        let source = Vec3::new(0.0, 0.0, -10.0);
        let p = WaveAudioProjector::default();
        let r = p.project(&psi, &l, Some(source), None);
        // Listener moving toward source (-Z) ; direction = source-listener = -Z ;
        // v_l . direction = -50 * -1 = 50 ; (c-50)/(c-0) = 293/343 ≈ 0.85.
        assert!(r.doppler_ratio < 1.0);
    }

    #[test]
    fn project_series_emits_frames() {
        let p = WaveAudioProjector::default();
        let l = vec![Complex::new(0.5, 0.0); 4];
        let rr = vec![Complex::new(0.5, 0.0); 4];
        let mut out = vec![0.0_f32; 8];
        let n = p.project_series(&l, &rr, 0.0, &mut out);
        assert_eq!(n, 4);
    }

    #[test]
    fn projector_zero_voxel_clamps_to_min() {
        let p = WaveAudioProjector::new(
            ProjectorConfig {
                voxel_size: 0.0,
                ..ProjectorConfig::default()
            },
            BinauralRender::default(),
        );
        let psi = PsiAudioField::new();
        let l = AudioListener::at_origin();
        // Should not panic ; voxel is clamped internally.
        let r = p.project(&psi, &l, None, None);
        assert_eq!(r.psi_left, Complex::ZERO);
    }

    #[test]
    fn project_same_listener_same_field_bit_equal() {
        // Determinism : two projections with identical inputs are
        // bit-equal.
        let mut psi = PsiAudioField::new();
        psi.set(MortonKey::encode(0, 0, 0).unwrap(), Complex::new(0.5, 0.3))
            .unwrap();
        let l = AudioListener::at_origin();
        let p = WaveAudioProjector::default();
        let r1 = p.project(&psi, &l, None, None);
        let r2 = p.project(&psi, &l, None, None);
        assert_eq!(r1.stereo.left.to_bits(), r2.stereo.left.to_bits());
        assert_eq!(r1.stereo.right.to_bits(), r2.stereo.right.to_bits());
    }
}
