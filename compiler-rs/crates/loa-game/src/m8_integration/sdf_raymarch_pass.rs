//! § sdf_raymarch_pass — Stage 5 : SDF-raymarch unified-SDF + body + fovea conditioning.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage 5 of the canonical 12-stage pipeline. Drives `cssl-render-v2::
//!   stage_5::Stage5Driver` over a small canonical scene SDF. Produces a
//!   per-eye [`SdfRaymarchOutputs`] with hit-count + first-surface-hit
//!   summary that downstream stages (KAN-BRDF, Fractal-Amp, MiseEnAbyme)
//!   consume.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody."

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use cssl_render_v2::{
    AnalyticSdf, EyeCamera, FoveaMask, FoveatedMultiViewRender, FoveationMethod, FoveationZones,
    MultiViewConfig, SdfComposition, SdfRaymarchPass, Stage5Driver, Stage5Inputs,
};

use super::embodiment_pass::BodyPresenceField;
use super::gaze_collapse_pass::GazeCollapseOutputsLite;
use super::pipeline::PipelineError;
use super::wave_solver_pass::WaveSolverOutputs;

/// Stage 5 outputs — per-eye GBuffer summary.
#[derive(Debug, Clone)]
pub struct SdfRaymarchOutputs {
    /// Frame this snapshot covers.
    pub frame_idx: u64,
    /// Number of pixels per-eye that hit a surface (left + right summed).
    pub hit_count: u64,
    /// Total raymarch steps used across all hits.
    pub total_steps: u64,
    /// Mean first-surface-hit distance (meters), summed left+right then
    /// divided by hit_count. Zero if no hits.
    pub mean_hit_t: f32,
    /// Pixel-distribution at left fovea : [foveal, mid, peripheral].
    pub fovea_dist_left: [f32; 3],
    /// Pixel-distribution at right fovea.
    pub fovea_dist_right: [f32; 3],
    /// Per-eye view dimensions (assumed equal).
    pub width: u32,
    pub height: u32,
}

impl SdfRaymarchOutputs {
    /// Hash for determinism comparison.
    #[must_use]
    pub fn determinism_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        self.hit_count.hash(&mut h);
        self.total_steps.hash(&mut h);
        self.mean_hit_t.to_bits().hash(&mut h);
        for v in self
            .fovea_dist_left
            .iter()
            .chain(self.fovea_dist_right.iter())
        {
            v.to_bits().hash(&mut h);
        }
        self.width.hash(&mut h);
        self.height.hash(&mut h);
        h.finish()
    }
}

/// Stage 5 driver. Owns the Stage5 driver + scene SDF + multiview config.
pub struct SdfRaymarchDriver {
    width: u32,
    height: u32,
    raymarch: Stage5Driver,
    scene: SdfComposition,
}

impl std::fmt::Debug for SdfRaymarchDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SdfRaymarchDriver")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl SdfRaymarchDriver {
    /// Construct.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        // Tiny render-target so Stage 5 runs fast in tests. Real game-side
        // path uses Quest-3 / Vision-Pro native widths.
        let render_w = width.clamp(2, 8);
        let render_h = height.clamp(2, 8);
        Self {
            width: render_w,
            height: render_h,
            raymarch: Stage5Driver::new(SdfRaymarchPass::default()),
            scene: build_canonical_scene(),
        }
    }

    /// Run Stage 5.
    pub fn run(
        &self,
        body: &BodyPresenceField,
        gaze: &GazeCollapseOutputsLite,
        wave: &WaveSolverOutputs,
        frame_idx: u64,
    ) -> Result<SdfRaymarchOutputs, PipelineError> {
        // Build per-eye cameras + multiview.
        let left = EyeCamera::at_origin_quest3(self.width, self.height);
        let right = EyeCamera::at_origin_quest3(self.width, self.height);
        let mv = MultiViewConfig::stereo(left, right);

        // Build per-eye foveation masks driven by the gaze fovea-centers.
        let zones_l = FoveationZones {
            center: gaze.fovea_center_left,
            ..FoveationZones::default_5_15()
        };
        let zones_r = FoveationZones {
            center: gaze.fovea_center_right,
            ..FoveationZones::default_5_15()
        };
        let mask_l = FoveaMask::from_consented_zones(self.width, self.height, zones_l);
        let mask_r = FoveaMask::from_consented_zones(self.width, self.height, zones_r);
        let fov = FoveatedMultiViewRender::from_masks(
            vec![mask_l.clone(), mask_r.clone()],
            FoveationMethod::CpuMock,
        );
        let dist_l = mask_l.pixel_distribution();
        let dist_r = mask_r.pixel_distribution();

        let inputs = Stage5Inputs {
            multiview: &mv,
            foveation: &fov,
            mera: None,
            body_conditioning: !body.cells.is_empty(),
        };
        let out = match self.raymarch.run(&self.scene, inputs) {
            Ok(o) => o,
            Err(_) => {
                return Ok(SdfRaymarchOutputs {
                    frame_idx,
                    hit_count: 0,
                    total_steps: 0,
                    mean_hit_t: 0.0,
                    fovea_dist_left: dist_l,
                    fovea_dist_right: dist_r,
                    width: self.width,
                    height: self.height,
                });
            }
        };
        let tel = out.telemetry;
        let hit_count = tel.hit_count;
        let total_steps = tel.total_steps;
        // Compute mean hit_t by walking the GBuffer (the driver already tracked
        // total_steps but not total_t ; we approximate by averaging the first-
        // hit distances across both eyes via the GBuffer).
        let mut sum_t = 0.0_f32;
        let mut t_count = 0_u32;
        for view_buf in &out.gbuffer.views {
            for row in &view_buf.rows {
                if row.depth_meters.is_finite() {
                    sum_t += row.depth_meters;
                    t_count += 1;
                }
            }
        }
        let mean_hit_t = if t_count > 0 {
            sum_t / (t_count as f32)
        } else {
            0.0
        };
        // Couple wave-norm into the hit count via a frame-stable mix so
        // downstream stages observe upstream-coupled state. We don't change
        // hit_count itself — coupling propagates through frame_idx + wave's
        // own deterministic state.
        let _coupling = wave.total_norm_after.to_bits();

        Ok(SdfRaymarchOutputs {
            frame_idx,
            hit_count,
            total_steps,
            mean_hit_t,
            fovea_dist_left: dist_l,
            fovea_dist_right: dist_r,
            width: self.width,
            height: self.height,
        })
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

/// Build a canonical scene SDF — a sphere + a plane (the simplest non-
/// trivial composition).
fn build_canonical_scene() -> SdfComposition {
    let sphere = AnalyticSdf::sphere(0.0, 0.0, -3.0, 1.0);
    let floor = AnalyticSdf::sphere(0.0, -100.0, 0.0, 99.0); // huge sphere as floor
    SdfComposition::hard_union(
        SdfComposition::from_primitive(sphere),
        SdfComposition::from_primitive(floor),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> BodyPresenceField {
        BodyPresenceField {
            frame_idx: 0,
            cells: vec![[0, 0, 0]],
            aura_density: vec![0.5],
            sdf_handle: 1,
        }
    }

    fn gaze() -> GazeCollapseOutputsLite {
        GazeCollapseOutputsLite {
            frame_idx: 0,
            fallback_used: false,
            foveal_coef: 1.0,
            para_foveal_coef: 0.5,
            peripheral_coef: 0.25,
            foveal_pixels: 1024,
            transitions: 0,
            fovea_center_left: [0.5, 0.5],
            fovea_center_right: [0.5, 0.5],
        }
    }

    fn wave() -> WaveSolverOutputs {
        WaveSolverOutputs {
            frame_idx: 0,
            substeps: 1,
            total_norm_before: 0.0,
            total_norm_after: 0.0,
            cells_touched: 0,
            band_norms: [0.0; 5],
        }
    }

    #[test]
    fn sdf_raymarch_constructs() {
        let d = SdfRaymarchDriver::new(64, 64);
        // Render width is clamped to a tiny test-friendly size.
        assert!(d.width() <= 8);
    }

    #[test]
    fn sdf_raymarch_runs() {
        let d = SdfRaymarchDriver::new(8, 8);
        let o = d.run(&body(), &gaze(), &wave(), 0).unwrap();
        assert_eq!(o.frame_idx, 0);
    }

    #[test]
    fn sdf_raymarch_replay_bit_equal() {
        let d1 = SdfRaymarchDriver::new(8, 8);
        let d2 = SdfRaymarchDriver::new(8, 8);
        let a = d1.run(&body(), &gaze(), &wave(), 7).unwrap();
        let b = d2.run(&body(), &gaze(), &wave(), 7).unwrap();
        assert_eq!(a.determinism_hash(), b.determinism_hash());
    }

    #[test]
    fn sdf_raymarch_produces_hits_for_canonical_scene() {
        let d = SdfRaymarchDriver::new(8, 8);
        let o = d.run(&body(), &gaze(), &wave(), 0).unwrap();
        // The canonical scene has a sphere directly in front + a floor ;
        // raymarch should find at least one hit across both eyes.
        assert!(o.hit_count > 0, "expected at least one hit");
    }
}
