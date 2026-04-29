//! § fractal_hook — D119 fractal-tessellation integration trait.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   T11-D119 lands the FractalAmplifierPass (Stage-7). Stage-5 prepares the
//!   per-pixel input that the amplifier consumes. This module defines the
//!   trait Stage-7 implements + a [`NoFractalAmplifier`] passthrough so
//!   Stage-5 compiles + tests independently of D119 landing.
//!
//! § SPEC
//!   - `Omniverse/14_NOVEL_RENDERING § Sub-Pixel-Fractal-Tessellation` —
//!     fractal-self-similar amplifier per-pixel @ shading-rate-aware.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III Stage-7` —
//!     KAN-detail-amplifier inputs : `(M-coord, surface-curvature, view-angle,
//!     KANDetailBudget)` ; output : sub-pixel-pattern coefficients.
//!
//! § INTEGRATION CONTRACT
//!   Distinct from [`crate::kan_amplifier`] — that trait covers KAN-output
//!   coefficients. The `FractalAmplifierHandle` covers the higher-level
//!   request pipeline : Stage-5 emits a [`FractalDetailRequest`] per surface-
//!   hit pixel ; Stage-7 batches them + dispatches the KAN-net + writes back
//!   into the GBuffer's detail-companion texture.

/// Per-pixel detail-request emitted by Stage-5.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FractalDetailRequest {
    /// Pixel coordinate (px, py).
    pub pixel: [u32; 2],
    /// View-index (0=left eye, 1=right eye for stereo).
    pub view_index: u32,
    /// World-space hit-position.
    pub world_pos: [f32; 3],
    /// Surface normal at hit.
    pub normal: [f32; 3],
    /// Surface-curvature estimate.
    pub curvature: f32,
    /// View-angle (cos angle between view-dir and normal). 1=face-on, 0=edge.
    pub view_angle_cos: f32,
    /// Material-coordinate axis-vector (top 4 components ; full 15-dim is
    /// fetched by Stage-7 from the M-facet directly).
    pub material_coord: [f32; 4],
    /// Detail-budget allocated for this pixel by Stage-2.
    pub detail_budget: f32,
}

impl FractalDetailRequest {
    /// New request.
    #[must_use]
    pub fn new(
        pixel: [u32; 2],
        view_index: u32,
        world_pos: [f32; 3],
        normal: [f32; 3],
        curvature: f32,
        view_angle_cos: f32,
        material_coord: [f32; 4],
        detail_budget: f32,
    ) -> Self {
        FractalDetailRequest {
            pixel,
            view_index,
            world_pos,
            normal,
            curvature,
            view_angle_cos: view_angle_cos.clamp(-1.0, 1.0),
            material_coord,
            detail_budget,
        }
    }

    /// Whether this request is "high-priority" — face-on surface with
    /// substantial detail-budget.
    #[must_use]
    pub fn is_high_priority(&self) -> bool {
        self.view_angle_cos > 0.7 && self.detail_budget > 0.5
    }
}

/// Trait the Stage-7 fractal-tessellation slice (D119) implements.
pub trait FractalAmplifierHandle {
    /// Submit one request to the amplifier. Returns `true` if the request was
    /// queued, `false` if the per-frame budget was exceeded.
    fn submit(&mut self, request: FractalDetailRequest) -> bool;

    /// Flush queued requests. Returns the total count flushed.
    fn flush(&mut self) -> u32;

    /// Per-frame request budget (max queued before back-pressure).
    fn frame_budget(&self) -> u32 {
        4096
    }

    /// Current queue depth.
    fn queue_depth(&self) -> u32;
}

/// Passthrough handle : accepts requests without queueing them. Used pre-D119.
#[derive(Debug, Clone, Default)]
pub struct NoFractalAmplifier {
    /// Counter incremented by `submit` (for telemetry-only).
    pub submit_count: u32,
}

impl FractalAmplifierHandle for NoFractalAmplifier {
    fn submit(&mut self, _request: FractalDetailRequest) -> bool {
        self.submit_count += 1;
        true
    }

    fn flush(&mut self) -> u32 {
        let n = self.submit_count;
        self.submit_count = 0;
        n
    }

    fn queue_depth(&self) -> u32 {
        self.submit_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_request() -> FractalDetailRequest {
        FractalDetailRequest::new(
            [10, 20],
            0,
            [1.0, 2.0, 3.0],
            [0.0, 1.0, 0.0],
            0.1,
            0.95,
            [0.5; 4],
            0.8,
        )
    }

    #[test]
    fn request_view_angle_cos_clamps_to_minus_one_one() {
        let r = FractalDetailRequest::new(
            [0, 0],
            0,
            [0.0; 3],
            [0.0, 1.0, 0.0],
            0.0,
            5.0,
            [0.0; 4],
            0.5,
        );
        assert!((r.view_angle_cos - 1.0).abs() < 1e-6);
    }

    #[test]
    fn request_high_priority_face_on_with_budget() {
        let r = dummy_request();
        assert!(r.is_high_priority());
    }

    #[test]
    fn request_low_priority_edge_on() {
        let mut r = dummy_request();
        r.view_angle_cos = 0.1;
        assert!(!r.is_high_priority());
    }

    #[test]
    fn request_low_priority_no_budget() {
        let mut r = dummy_request();
        r.detail_budget = 0.1;
        assert!(!r.is_high_priority());
    }

    #[test]
    fn no_amplifier_submit_increments_counter() {
        let mut h = NoFractalAmplifier::default();
        h.submit(dummy_request());
        h.submit(dummy_request());
        assert_eq!(h.submit_count, 2);
    }

    #[test]
    fn no_amplifier_flush_returns_count_and_resets() {
        let mut h = NoFractalAmplifier::default();
        h.submit(dummy_request());
        h.submit(dummy_request());
        let n = h.flush();
        assert_eq!(n, 2);
        assert_eq!(h.submit_count, 0);
    }

    #[test]
    fn no_amplifier_queue_depth_tracks_submits() {
        let mut h = NoFractalAmplifier::default();
        assert_eq!(h.queue_depth(), 0);
        h.submit(dummy_request());
        assert_eq!(h.queue_depth(), 1);
    }

    #[test]
    fn no_amplifier_frame_budget_default_4096() {
        let h = NoFractalAmplifier::default();
        assert_eq!(h.frame_budget(), 4096);
    }

    #[test]
    fn submit_always_succeeds_for_passthrough() {
        let mut h = NoFractalAmplifier::default();
        let ok = h.submit(dummy_request());
        assert!(ok);
    }
}
