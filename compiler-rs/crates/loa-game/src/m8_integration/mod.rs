//! M8 integration module — 12-stage canonical render pipeline.
//!
//! § T11-D158 (W-Jζ-2) : Per-stage frame-time instrumentation.
//!
//! § ARCH
//!   - [`Pass`] trait                  — uniform driver-surface for every stage
//!   - [`StageId`] (1..=12)            — frozen-set canonical-order discriminant
//!   - [`PassContext`]                 — shared per-frame state passed to each pass
//!   - [`pipeline::Pipeline`]          — orchestrator ; Timer-wraps each pass
//!
//! § STAGE-DRIVERS
//!   1.  [`embodiment_pass::EmbodimentPass`]
//!   2.  [`gaze_collapse_pass::GazeCollapsePass`]
//!   3.  [`omega_field_update_pass::OmegaFieldUpdatePass`]
//!   4.  [`wave_solver_pass::WaveSolverPass`]
//!   5.  [`sdf_raymarch_pass::SdfRaymarchPass`]
//!   6.  [`kan_brdf_pass::KanBrdfPass`]
//!   7.  [`fractal_amplifier_pass::FractalAmplifierPass`]
//!   8.  [`companion_semantic_pass::CompanionSemanticPass`]
//!   9.  [`mise_en_abyme_pass::MiseEnAbymePass`]
//!   10. [`tonemap_pass::TonemapPass`]
//!   11. [`motion_vec_pass::MotionVecPass`]
//!   12. [`compose_xr_layers_pass::ComposeXrLayersPass`]
//!
//! § INVARIANTS
//!   - Stages execute in canonical order ; no skip permitted in default mode
//!   - Timer-wrap is observe-only ; ¬state-mutation
//!   - p50/p95/p99 readable per-stage post-frame
//!   - feature-gate `metrics` off → identical machine-code to non-instrumented

pub mod compose_xr_layers_pass;
pub mod companion_semantic_pass;
pub mod embodiment_pass;
pub mod fractal_amplifier_pass;
pub mod gaze_collapse_pass;
pub mod kan_brdf_pass;
pub mod mise_en_abyme_pass;
pub mod motion_vec_pass;
pub mod omega_field_update_pass;
pub mod pipeline;
pub mod sdf_raymarch_pass;
pub mod tonemap_pass;
pub mod wave_solver_pass;

#[cfg(test)]
pub mod tests;

// ────────────────────────────────────────────────────────────────────
// § Stage identity
// ────────────────────────────────────────────────────────────────────

/// Canonical stage identifier (1..=12, frozen-set).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum StageId {
    /// Stage 1 — body-tracking + IK retargeting
    Embodiment = 1,
    /// Stage 2 — saccade-driven Ω-collapse bias
    GazeCollapse = 2,
    /// Stage 3 — Ω-field tier-resolution + Σ-mask propagate
    OmegaFieldUpdate = 3,
    /// Stage 4 — ψ-evolution + spectral propagate
    WaveSolver = 4,
    /// Stage 5 — sphere-trace + foveated pixel-march
    SdfRaymarch = 5,
    /// Stage 6 — KAN spectral-BRDF eval per-fragment
    KanBrdf = 6,
    /// Stage 7 — RC-fractal detail injection
    FractalAmplifier = 7,
    /// Stage 8 — companion-perspective per-character
    CompanionSemantic = 8,
    /// Stage 9 — recursion-depth-witnessed amplification
    MiseEnAbyme = 9,
    /// Stage 10 — HDR tonemap + fovea-tier compose
    Tonemap = 10,
    /// Stage 11 — frame-N→N+1 motion vectors @ AppSW
    MotionVec = 11,
    /// Stage 12 — quad/cyl/cube layer compose per-eye
    ComposeXrLayers = 12,
}

impl StageId {
    /// Frozen-set ordering : 1..=12.
    pub const ALL: [StageId; 12] = [
        StageId::Embodiment,
        StageId::GazeCollapse,
        StageId::OmegaFieldUpdate,
        StageId::WaveSolver,
        StageId::SdfRaymarch,
        StageId::KanBrdf,
        StageId::FractalAmplifier,
        StageId::CompanionSemantic,
        StageId::MiseEnAbyme,
        StageId::Tonemap,
        StageId::MotionVec,
        StageId::ComposeXrLayers,
    ];

    /// Numeric stage-index 1..=12.
    #[must_use]
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Short snake_case name for namespace assembly.
    #[must_use]
    pub const fn snake_name(self) -> &'static str {
        match self {
            StageId::Embodiment => "embodiment",
            StageId::GazeCollapse => "gaze_collapse",
            StageId::OmegaFieldUpdate => "omega_field_update",
            StageId::WaveSolver => "wave_solver",
            StageId::SdfRaymarch => "sdf_raymarch",
            StageId::KanBrdf => "kan_brdf",
            StageId::FractalAmplifier => "fractal_amplifier",
            StageId::CompanionSemantic => "companion_semantic",
            StageId::MiseEnAbyme => "mise_en_abyme",
            StageId::Tonemap => "tonemap",
            StageId::MotionVec => "motion_vec",
            StageId::ComposeXrLayers => "compose_xr_layers",
        }
    }

    /// Compose canonical metric-namespace :
    /// `pipeline.stage_N_<name>.frame_time_ms`
    #[must_use]
    pub fn metric_namespace(self) -> String {
        format!(
            "pipeline.stage_{}_{}.frame_time_ms",
            self.index(),
            self.snake_name()
        )
    }
}

// ────────────────────────────────────────────────────────────────────
// § Pass trait + context
// ────────────────────────────────────────────────────────────────────

/// Per-frame shared context passed to each pass.
///
/// § OBSERVE-ONLY
/// Per acceptance gate § 4 (replay-determinism), the timing wrap must
/// not mutate `frame_n` or any other ctx-state. `frame_n` is incremented
/// once-per-pipeline-tick by [`pipeline::Pipeline::run_frame`], not by
/// the passes themselves.
#[derive(Debug, Default, Clone, Copy)]
pub struct PassContext {
    /// Frame counter (monotonic, per-tick).
    pub frame_n: u64,
    /// Render mode (60/90/120/xr in real impl ; abstract here).
    pub mode: RenderMode,
    /// Synthetic per-stage workload-amount (test-only knob).
    /// In real impl, scene-complexity feeds the stage drivers.
    pub workload: u32,
}

/// Render-mode placeholder mirroring DENSITY_BUDGET §V tags.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RenderMode {
    /// 60Hz desktop / mobile ; 16.67ms budget
    M60,
    /// 90Hz XR ; 11.11ms budget
    M90,
    /// 120Hz XR ; 8.33ms budget
    M120,
    /// XR-AppSW reprojection
    Xr,
    /// Default for tests : unspecified mode
    #[default]
    Test,
}

/// Uniform pass-driver interface.
///
/// Each of the 12 stages implements `Pass` ; `Pipeline` wraps `execute`
/// with a [`crate::metrics_mock::Timer`] to record frame-time-ms.
pub trait Pass: Send + Sync {
    /// Stage identity (frozen-set 1..=12).
    fn stage_id(&self) -> StageId;

    /// Execute one frame of this stage.
    ///
    /// § OBSERVE-ONLY-CONTRACT
    /// The pass may mutate its own internal state, but must not mutate
    /// the [`PassContext`] (incoming `&PassContext`, not `&mut`). Any
    /// per-frame sample-counters reside in `&mut self`, not in ctx.
    fn execute(&mut self, ctx: &PassContext);

    /// Diagnostic name (matches [`StageId::snake_name`]).
    fn name(&self) -> &'static str {
        self.stage_id().snake_name()
    }
}
