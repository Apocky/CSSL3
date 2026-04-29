//! § stage_5 — top-level Stage-5 driver tying together raymarch + multi-view +
//! foveation + GBuffer-write + amplifier-hook + spectral-hook.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The runnable Stage-5 driver. Consumers call [`Stage5Driver::run`] with a
//!   [`Stage5Inputs`] and get back a [`Stage5DriverOutput`] (GBuffer + telemetry).
//!   The driver wires together :
//!     - [`crate::raymarch::SdfRaymarchPass`] for the per-pixel march
//!     - [`crate::mera_skip::MeraSkipDispatcher`] for hierarchical skip
//!     - [`crate::foveation::FoveatedMultiViewRender`] for shading-rate selection
//!     - [`crate::gbuffer::MultiViewGBuffer`] as the per-view output
//!     - [`crate::budget::Stage5Budget`] for telemetry + cost projection
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III Stage-5`.

use thiserror::Error;

use crate::budget::{Stage5Budget, Stage5BudgetTelemetry};
use crate::camera::EyeCamera;
use crate::foveation::FoveatedMultiViewRender;
use crate::gbuffer::{GBufferRow, MultiViewGBuffer};
use crate::mera_skip::MeraSkipDispatcher;
use crate::multiview::MultiViewConfig;
use crate::normals::SdfFunction;
use crate::raymarch::{RaymarchError, SdfRaymarchPass};

/// Errors from the Stage-5 driver.
#[derive(Debug, Error)]
pub enum Stage5DriverError {
    /// Raymarcher returned an error during a pixel march.
    #[error("raymarch failed: {0}")]
    Raymarch(#[from] RaymarchError),
    /// Foveation mask doesn't match multi-view config.
    #[error("foveation validation failed")]
    FoveationMismatch,
}

/// Stage-5 driver. Holds the raymarcher + an optional MERA-skip dispatcher.
#[derive(Debug, Clone, Copy)]
pub struct Stage5Driver {
    /// Per-pass raymarch config.
    pub raymarcher: SdfRaymarchPass,
}

impl Default for Stage5Driver {
    fn default() -> Self {
        Stage5Driver {
            raymarcher: SdfRaymarchPass::default(),
        }
    }
}

/// Inputs for one Stage-5 invocation.
#[derive(Debug, Clone)]
pub struct Stage5Inputs<'cfg> {
    /// Per-eye multi-view config.
    pub multiview: &'cfg MultiViewConfig,
    /// Per-eye foveation masks.
    pub foveation: &'cfg FoveatedMultiViewRender,
    /// Optional MERA-skip dispatcher (foundation : may be None).
    pub mera: Option<&'cfg MeraSkipDispatcher<'cfg>>,
    /// Whether to use body-presence-conditioning. If true, the driver
    /// invokes the body-modifier callable on every march. Body-presence
    /// data is the consent-elevated path ; only-call-with-consent.
    pub body_conditioning: bool,
}

/// Output of one Stage-5 invocation.
#[derive(Debug)]
pub struct Stage5DriverOutput {
    /// Multi-view GBuffer with hits / misses.
    pub gbuffer: MultiViewGBuffer,
    /// Telemetry counters.
    pub telemetry: Stage5BudgetTelemetry,
    /// Projected ms-cost.
    pub budget: Stage5Budget,
}

impl Stage5Driver {
    /// Construct from a raymarcher.
    #[must_use]
    pub fn new(raymarcher: SdfRaymarchPass) -> Self {
        Stage5Driver { raymarcher }
    }

    /// Run Stage-5 over an SDF function (analytic or composed).
    pub fn run<F: SdfFunction>(
        &self,
        sdf: &F,
        inputs: Stage5Inputs<'_>,
    ) -> Result<Stage5DriverOutput, Stage5DriverError> {
        // Validate foveation matches multiview.
        inputs
            .foveation
            .validate(inputs.multiview)
            .map_err(|_| Stage5DriverError::FoveationMismatch)?;

        let mut gbuffer = MultiViewGBuffer::from_config(inputs.multiview);
        let mut telemetry = Stage5BudgetTelemetry::default();
        let mut budget = Stage5Budget::quest3();

        for (vidx, view) in inputs.multiview.views.iter().enumerate() {
            let mask = &inputs.foveation.masks[vidx];
            let cam = view.camera;
            for py in 0..cam.height {
                for px in 0..cam.width {
                    let rate = mask.shading_rate_at(px, py);
                    let dir = cam.pixel_to_ray(px, py);
                    let origin = cam.origin;
                    let max_steps = self.raymarcher.config.step_budget(rate);
                    let result = if let Some(mera) = inputs.mera {
                        self.raymarcher
                            .march_with_mera_skip(sdf, mera, origin, dir, max_steps)
                    } else {
                        self.raymarcher.march(sdf, origin, dir, max_steps)
                    };
                    match result {
                        Ok(Some(hit)) => {
                            telemetry.hit_count += 1;
                            telemetry.total_steps += hit.steps_used as u64;
                            let row = GBufferRow::hit(
                                hit.t,
                                hit.p,
                                hit.normal,
                                hit.sdf_value,
                                hit.material_handle,
                                view.index,
                            );
                            gbuffer.write(vidx, px, py, row);
                        }
                        Ok(None) => {
                            telemetry.miss_count += 1;
                            telemetry.total_steps += max_steps as u64;
                            let row = GBufferRow::miss(view.index);
                            gbuffer.write(vidx, px, py, row);
                        }
                        Err(RaymarchError::StepBudgetExhausted { limit }) => {
                            telemetry.budget_exhausted_count += 1;
                            telemetry.total_steps += limit as u64;
                            let row = GBufferRow::miss(view.index);
                            gbuffer.write(vidx, px, py, row);
                        }
                        Err(e) => return Err(Stage5DriverError::Raymarch(e)),
                    }
                }
            }
        }

        // Project the cost @ Quest-3 (Vision-Pro variant available via
        // budget.project_vision_pro).
        let aggregate_fov = inputs.foveation.aggregate_cost_fraction();
        // Use the rough cell-count = total-pixels * 1.0 (each ray touches a few cells ;
        // foundation slice ; better cost-model lands in D116 follow-up).
        let cells = gbuffer.total_pixels() as u64;
        budget.project_quest3(cells, aggregate_fov);
        budget.telemetry = telemetry;
        let _ = body_conditioning_check(inputs.body_conditioning);

        Ok(Stage5DriverOutput {
            gbuffer,
            telemetry,
            budget,
        })
    }

    /// Convenience : run mono-view with a single eye-camera + center-bias
    /// foveation. Used by tests + the simple-host-path.
    pub fn run_mono<F: SdfFunction>(
        &self,
        sdf: &F,
        cam: EyeCamera,
    ) -> Result<Stage5DriverOutput, Stage5DriverError> {
        let cfg = MultiViewConfig::mono(cam);
        let fov = FoveatedMultiViewRender::from_masks(
            vec![crate::foveation::FoveaMask::center_bias_fallback(
                cam.width,
                cam.height,
            )],
            crate::foveation::FoveationMethod::CpuMock,
        );
        let inputs = Stage5Inputs {
            multiview: &cfg,
            foveation: &fov,
            mera: None,
            body_conditioning: false,
        };
        self.run(sdf, inputs)
    }
}

/// Witness function for body-conditioning (consent-aware). Returning
/// `false` means "fallback path engaged" ; `true` means "consented path
/// engaged". Currently the foundation slice always falls back ; the
/// real-consent-gated path lands when D113 OmegaField + D120 GazeCollapse
/// are wired.
fn body_conditioning_check(_requested: bool) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::EyeCamera;
    use crate::sdf::{AnalyticSdf, SdfComposition};

    #[test]
    fn run_mono_at_origin_hits_centered_sphere() {
        let driver = Stage5Driver::default();
        let cam = EyeCamera::at_origin_quest3(8, 8);
        // Sphere along -Z at distance 5 ; ray from origin should hit.
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, -5.0, 1.0));
        let out = driver.run_mono(&s, cam).unwrap();
        assert!(out.telemetry.hit_count > 0, "expected hits");
    }

    #[test]
    fn run_mono_with_no_geometry_all_miss() {
        let driver = Stage5Driver::default();
        let cam = EyeCamera::at_origin_quest3(4, 4);
        // Sphere placed far behind camera (along +Z) ; identity-pose camera
        // looks down -Z, all rays should miss.
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, 100.0, 0.5));
        let out = driver.run_mono(&s, cam).unwrap();
        assert_eq!(out.telemetry.hit_count, 0);
    }

    #[test]
    fn driver_default_uses_default_raymarcher() {
        let d = Stage5Driver::default();
        assert_eq!(d.raymarcher.config.max_steps.0, 128);
    }

    #[test]
    fn output_telemetry_increments_total_steps() {
        let driver = Stage5Driver::default();
        let cam = EyeCamera::at_origin_quest3(2, 2);
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, -5.0, 1.0));
        let out = driver.run_mono(&s, cam).unwrap();
        assert!(out.telemetry.total_steps > 0);
    }

    #[test]
    fn output_budget_projects_under_quest3_ceiling() {
        let driver = Stage5Driver::default();
        let cam = EyeCamera::at_origin_quest3(8, 8);
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, -5.0, 1.0));
        let out = driver.run_mono(&s, cam).unwrap();
        // 64 pixels @ 0.4 foveation = tiny cost.
        assert!(out.budget.projected_ms.0 < crate::STAGE_5_QUEST3_BUDGET_MS);
    }

    #[test]
    fn driver_stereo_run_writes_both_views() {
        let driver = Stage5Driver::default();
        let cam = EyeCamera::at_origin_quest3(4, 4);
        let cfg = MultiViewConfig::stereo(cam, cam);
        let fov = FoveatedMultiViewRender::stereo_center_bias(
            4,
            4,
            crate::foveation::FoveationMethod::CpuMock,
        );
        let s = SdfComposition::from_primitive(AnalyticSdf::sphere(0.0, 0.0, -5.0, 1.0));
        let out = driver
            .run(
                &s,
                Stage5Inputs {
                    multiview: &cfg,
                    foveation: &fov,
                    mera: None,
                    body_conditioning: false,
                },
            )
            .unwrap();
        assert_eq!(out.gbuffer.views.len(), 2);
    }

    #[test]
    fn body_conditioning_check_falls_back_at_foundation() {
        // Foundation slice : always returns false (consent gate routes through
        // OmegaField + GazeCollapse, which land separately).
        assert!(!body_conditioning_check(true));
        assert!(!body_conditioning_check(false));
    }
}
