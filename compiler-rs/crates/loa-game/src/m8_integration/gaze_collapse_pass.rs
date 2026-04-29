//! § gaze_collapse_pass — Stage 2 : eye-track → fovea-mask + KAN-detail-budget.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage 2 of the pipeline. Drives `cssl-gaze-collapse::GazeCollapsePass`
//!   with a deterministic synthesized gaze input + produces the per-eye
//!   foveal coefficient + transition count that downstream stages consume.
//!
//! § FALLBACK PATH
//!   When `opt_in` is `false` OR confidence is below threshold, the pass
//!   takes the `FoveationFallback::CenterBias` path : center-bias mask,
//!   no transitions, predicted-saccade=None.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use cssl_gaze_collapse::gaze_input::SensitiveGazeConstructors;
use cssl_gaze_collapse::{
    EyeOpenness, GazeCollapseConfig, GazeCollapsePass, GazeConfidence, GazeDirection, GazeInput,
    SaccadeState, SensitiveGaze,
};

use super::embodiment_pass::BodyPresenceField;
use super::pipeline::PipelineError;

/// Lightweight outputs of the gaze-collapse pass — enough for downstream
/// stages to derive shading-rate + collapse-bias without re-importing the
/// full `cssl-gaze-collapse::GazeCollapseOutputs`.
#[derive(Debug, Clone)]
pub struct GazeCollapseOutputsLite {
    /// Frame this snapshot covers.
    pub frame_idx: u64,
    /// Whether the fallback path was taken (consent revoked / low confidence).
    pub fallback_used: bool,
    /// Per-eye foveal coefficient.
    pub foveal_coef: f32,
    /// Para-foveal coefficient.
    pub para_foveal_coef: f32,
    /// Peripheral coefficient.
    pub peripheral_coef: f32,
    /// Number of foveal pixels (per eye).
    pub foveal_pixels: u32,
    /// Number of region transitions detected this frame.
    pub transitions: u32,
    /// Mock fovea-center in normalized coords [0..1] per left eye.
    pub fovea_center_left: [f32; 2],
    /// Mock fovea-center per right eye.
    pub fovea_center_right: [f32; 2],
}

impl GazeCollapseOutputsLite {
    /// Hash for determinism comparison.
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        self.fallback_used.hash(&mut h);
        for f in [
            self.foveal_coef,
            self.para_foveal_coef,
            self.peripheral_coef,
        ] {
            f.to_bits().hash(&mut h);
        }
        self.foveal_pixels.hash(&mut h);
        self.transitions.hash(&mut h);
        for v in self
            .fovea_center_left
            .iter()
            .chain(self.fovea_center_right.iter())
        {
            v.to_bits().hash(&mut h);
        }
        h.finish()
    }
}

/// Stage 2 driver.
pub struct GazeCollapsePassDriver {
    /// Master seed.
    seed: u64,
    /// View width in pixels (per-eye).
    width: u32,
    /// View height in pixels (per-eye).
    height: u32,
    /// The wrapped `cssl-gaze-collapse::GazeCollapsePass`. Stored to keep
    /// state across frames (saccade history, transition tracking).
    pass: GazeCollapsePass,
}

impl std::fmt::Debug for GazeCollapsePassDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GazeCollapsePassDriver")
            .field("seed", &self.seed)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl GazeCollapsePassDriver {
    /// Construct.
    #[must_use]
    pub fn new(seed: u64, width: u32, height: u32) -> Self {
        let mut cfg = GazeCollapseConfig::quest3_opted_in();
        cfg.render_target_width = width.max(64);
        cfg.render_target_height = height.max(64);
        let pass = GazeCollapsePass::new(cfg).expect("default GazeCollapseConfig");
        Self {
            seed,
            width,
            height,
            pass,
        }
    }

    /// Run the pass for one frame. Synthesizes a deterministic gaze input
    /// based on `(seed, frame_idx)` then drives the underlying pass.
    pub fn run(
        &mut self,
        body: &BodyPresenceField,
        frame_idx: u64,
    ) -> Result<GazeCollapseOutputsLite, PipelineError> {
        // Synthesize deterministic gaze input. Body's frame_idx feeds into
        // the gaze direction so different frames produce different gaze.
        let gaze_input = synth_gaze(self.seed, frame_idx, body);
        let sensitive: SensitiveGaze = SensitiveGaze::from_raw(gaze_input);

        let outs = match self.pass.execute(&sensitive) {
            Ok(o) => o,
            Err(_) => {
                return Ok(fallback_outputs(frame_idx));
            }
        };

        let fallback_used = outs.fallback_used;
        let kan = &outs.kan_budgets[0];
        let foveal_coef = kan.foveal;
        let para_foveal_coef = kan.para_foveal;
        let peripheral_coef = kan.peripheral;
        let foveal_pixels = kan.foveal_pixels;
        let transitions = outs.transitions.len() as u32;

        // Approximate fovea center from the cyclopean direction. We project
        // the gaze direction onto the screen plane (ignoring z) and shift to
        // [0,1] coords. For center-bias this is always (0.5, 0.5).
        let cyc = sensitive.value.cyclopean_direction();
        let cx = cyc.x.mul_add(0.5, 0.5).clamp(0.0, 1.0);
        let cy = cyc.y.mul_add(0.5, 0.5).clamp(0.0, 1.0);

        Ok(GazeCollapseOutputsLite {
            frame_idx,
            fallback_used,
            foveal_coef,
            para_foveal_coef,
            peripheral_coef,
            foveal_pixels,
            transitions,
            fovea_center_left: [cx, cy],
            fovea_center_right: [cx, cy],
        })
    }

    /// Master seed accessor.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// View width.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// View height.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }
}

fn fallback_outputs(frame_idx: u64) -> GazeCollapseOutputsLite {
    GazeCollapseOutputsLite {
        frame_idx,
        fallback_used: true,
        foveal_coef: 1.0,
        para_foveal_coef: 0.5,
        peripheral_coef: 0.25,
        foveal_pixels: 0,
        transitions: 0,
        fovea_center_left: [0.5, 0.5],
        fovea_center_right: [0.5, 0.5],
    }
}

fn synth_gaze(seed: u64, frame_idx: u64, body: &BodyPresenceField) -> GazeInput {
    // Map (seed, frame_idx, body_handle) into a small deterministic offset
    // around forward.
    let mix = (seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(frame_idx)
        .wrapping_add(body.sdf_handle))
        & 0x0FFF_FFFF_FFFF_FFFF;
    let u = ((mix & 0x7FFF) as f32 / 32_767.0) * 0.1 - 0.05; // small offset
    let v = (((mix >> 16) & 0x7FFF) as f32 / 32_767.0) * 0.1 - 0.05;
    // Form a unit vector with z dominant so length is ~1.0 within tolerance.
    let z = 1.0_f32 - (u * u + v * v).min(0.99);
    let mag = (u * u + v * v + z * z).sqrt().max(1e-6);
    let dir = GazeDirection::new(u / mag, v / mag, z / mag).unwrap_or(GazeDirection::FORWARD);

    GazeInput {
        left_direction: dir,
        right_direction: dir,
        left_confidence: GazeConfidence::new(0.95).unwrap(),
        right_confidence: GazeConfidence::new(0.95).unwrap(),
        left_openness: EyeOpenness::new(0.9).unwrap(),
        right_openness: EyeOpenness::new(0.9).unwrap(),
        saccade_state: SaccadeState::Fixation,
        frame_counter: (frame_idx & 0xFFFF_FFFF) as u32,
        convergence_meters: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver() -> GazeCollapsePassDriver {
        GazeCollapsePassDriver::new(0xC551_F00D, 64, 64)
    }

    fn body(frame_idx: u64) -> BodyPresenceField {
        BodyPresenceField {
            frame_idx,
            cells: vec![[0, 0, 0]],
            aura_density: vec![0.5],
            sdf_handle: 1,
        }
    }

    #[test]
    fn gaze_pass_constructs() {
        let d = driver();
        assert_eq!(d.width(), 64);
        assert_eq!(d.height(), 64);
    }

    #[test]
    fn gaze_pass_runs_one_frame() {
        let mut d = driver();
        let o = d.run(&body(0), 0).unwrap();
        assert_eq!(o.frame_idx, 0);
    }

    #[test]
    fn gaze_pass_replay_bit_equal() {
        let mut d1 = driver();
        let mut d2 = driver();
        let a = d1.run(&body(7), 7).unwrap();
        let b = d2.run(&body(7), 7).unwrap();
        assert_eq!(a.determinism_hash(), b.determinism_hash());
    }

    #[test]
    fn gaze_pass_advances_frame_idx() {
        let mut d = driver();
        let a = d.run(&body(0), 0).unwrap();
        let b = d.run(&body(1), 1).unwrap();
        assert_eq!(a.frame_idx, 0);
        assert_eq!(b.frame_idx, 1);
    }

    #[test]
    fn gaze_outputs_carry_coef_in_unit_interval() {
        let mut d = driver();
        let o = d.run(&body(0), 0).unwrap();
        for c in [o.foveal_coef, o.para_foveal_coef, o.peripheral_coef] {
            assert!((0.0..=1.0).contains(&c), "coef {} not in [0,1]", c);
        }
    }
}
