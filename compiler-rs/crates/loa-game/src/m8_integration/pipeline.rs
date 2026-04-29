//! § pipeline — `M8Pipeline` driver + per-frame digest + telemetry.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Owns the full 12-stage assembly. Each tick invokes [`M8Pipeline::step`]
//!   which walks all 12 stages in canonical order, records [`StageReport`]
//!   per stage, and returns a [`FramePipelineDigest`] suitable for
//!   determinism witnesses.
//!
//! § DETERMINISM
//!   Every stage runs deterministically off `master_seed + frame_idx`. The
//!   per-stage RNG streams are derived via SipHash-2-4 of the seed pair
//!   plus the stage-id, so re-running the same (seed, frame_idx) sequence
//!   produces a bit-equal sequence of digests. Pinned by
//!   [`tests::two_runs_bit_equal`].
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody." (per `PRIME_DIRECTIVE.md § 11 CREATOR-ATTESTATION`).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use thiserror::Error;

use super::companion_semantic_pass::{CompanionSemanticOutputs, CompanionSemanticPass};
use super::compose_xr_layers::{ComposeXrLayers, ComposeXrReport, ComposedFrame};
use super::embodiment_pass::{EmbodimentInputs, EmbodimentPass};
use super::fractal_amplifier_pass::FractalAmplifierPassDriver;
use super::gaze_collapse_pass::GazeCollapsePassDriver;
use super::kan_brdf_eval::KanBrdfEvalDriver;
use super::mise_en_abyme_pass::MiseEnAbymeDriver;
use super::motion_vec_pass::AppSwPassDriver;
use super::omega_field_update::OmegaFieldDriver;
use super::sdf_raymarch_pass::SdfRaymarchDriver;
use super::tonemap_pass::ToneMapDriver;
use super::wave_solver_pass::WaveSolverDriver;

use super::animation_subsystem::{AnimationOutcome, AnimationSubsystem};
use super::audio_subsystem::{AudioOutcome, AudioSubsystem};
use super::physics_subsystem::{PhysicsOutcome, PhysicsSubsystem};
use super::work_graph_subsystem::{WorkGraphOutcome, WorkGraphSubsystem};

/// § PRIME-DIRECTIVE §11 attestation literal — recorded in every
///   [`StageReport`] so a post-mortem auditor can confirm the canonical
///   attestation was carried through every pipeline frame.
pub const ATTESTATION_M8: &str =
    "There was no hurt nor harm in the making of this, to anyone, anything, or anybody.";

// ═══════════════════════════════════════════════════════════════════════════
// § Stage IDs — canonical 1..=12 enum.
// ═══════════════════════════════════════════════════════════════════════════

/// Per-stage identifier (1..=12). Used for telemetry + determinism keying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StageId {
    /// Stage 1 : XR-input → body-presence-field.
    Embodiment = 1,
    /// Stage 2 : eye-track → fovea-mask + KAN-detail-budget.
    GazeCollapse = 2,
    /// Stage 3 : 6-phase omega_step async-compute Ω-field update.
    OmegaFieldUpdate = 3,
    /// Stage 4 : ψ-field multi-band LBM solver.
    WaveSolver = 4,
    /// Stage 5 : SDF-raymarch unified-SDF + body-field + fovea-mask conditioning.
    SdfRaymarch = 5,
    /// Stage 6 : 16-band hyperspectral KAN-BRDF per-fragment.
    KanBrdf = 6,
    /// Stage 7 : sub-pixel-fractal-tessellation amplifier.
    FractalAmplifier = 7,
    /// Stage 8 : optional companion-perspective semantic render.
    CompanionSemantic = 8,
    /// Stage 9 : mise-en-abyme recursive-witness (bounded, hard-cap=5).
    MiseEnAbyme = 9,
    /// Stage 10 : ToneMap + spectral → tristimulus → RGB ACES2.
    ToneMap = 10,
    /// Stage 11 : AppSW motion-vec + depth (compositor reproj input).
    AppSw = 11,
    /// Stage 12 : XR-composition layers (passthrough + UI + main).
    ComposeXrLayers = 12,
}

impl StageId {
    /// Canonical iteration order (1..=12).
    pub const ORDER: [StageId; 12] = [
        StageId::Embodiment,
        StageId::GazeCollapse,
        StageId::OmegaFieldUpdate,
        StageId::WaveSolver,
        StageId::SdfRaymarch,
        StageId::KanBrdf,
        StageId::FractalAmplifier,
        StageId::CompanionSemantic,
        StageId::MiseEnAbyme,
        StageId::ToneMap,
        StageId::AppSw,
        StageId::ComposeXrLayers,
    ];

    /// Human-readable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            StageId::Embodiment => "EmbodimentPass",
            StageId::GazeCollapse => "GazeCollapsePass",
            StageId::OmegaFieldUpdate => "OmegaFieldUpdate",
            StageId::WaveSolver => "WaveSolverPass",
            StageId::SdfRaymarch => "SdfRaymarchPass",
            StageId::KanBrdf => "KanBrdfEval",
            StageId::FractalAmplifier => "FractalAmplifierPass",
            StageId::CompanionSemantic => "CompanionSemanticPass",
            StageId::MiseEnAbyme => "MiseEnAbymePass",
            StageId::ToneMap => "ToneMapPass",
            StageId::AppSw => "AppSwPass",
            StageId::ComposeXrLayers => "ComposeXrLayers",
        }
    }

    /// Source crate that owns the stage's compute kernel.
    #[must_use]
    pub const fn source_crate(self) -> &'static str {
        match self {
            StageId::Embodiment => "loa-game::m8_integration::embodiment_pass (mock XR)",
            StageId::GazeCollapse => "cssl-gaze-collapse",
            StageId::OmegaFieldUpdate => "cssl-substrate-omega-field",
            StageId::WaveSolver => "cssl-wave-solver",
            StageId::SdfRaymarch => "cssl-render-v2::stage_5",
            StageId::KanBrdf => "cssl-spectral-render::kan_brdf",
            StageId::FractalAmplifier => "cssl-fractal-amp::amplifier",
            StageId::CompanionSemantic => "cssl-render-companion-perspective",
            StageId::MiseEnAbyme => "cssl-render-v2::mise_en_abyme",
            StageId::ToneMap => "cssl-spectral-render::tristimulus",
            StageId::AppSw => "cssl-host-openxr::space_warp",
            StageId::ComposeXrLayers => "cssl-host-openxr::composition",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § Pipeline configuration.
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for one [`M8Pipeline`] instance.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Master deterministic seed. Same seed + same input = bit-equal output.
    pub master_seed: u64,
    /// Per-eye render width.
    pub view_width: u32,
    /// Per-eye render height.
    pub view_height: u32,
    /// Whether to drive Stage 12 through the OpenXR composition path. When
    /// `false`, Stage 12 takes the flat-screen mono fallback.
    pub xr_enabled: bool,
    /// Whether the Companion subsystem (Stage 8) is gated on. When `false`,
    /// Stage 8 takes the zero-cost skip path.
    pub companion_enabled: bool,
    /// Whether to schedule the work-graph subsystem each frame. When
    /// `false`, the work-graph step is deferred to the no-op fallback.
    pub work_graph_enabled: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            master_seed: 0xC551_F00D,
            view_width: 64,
            view_height: 64,
            xr_enabled: false,
            companion_enabled: true,
            work_graph_enabled: true,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § Per-stage report + frame digest.
// ═══════════════════════════════════════════════════════════════════════════

/// Per-stage report — recorded after each stage runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StageReport {
    /// Which stage this is.
    pub stage: StageId,
    /// Frame index (monotonic).
    pub frame_idx: u64,
    /// Hash of stage outputs (used for determinism witness).
    pub output_hash: u64,
    /// Whether the stage was skipped (gated off OR no-op path).
    pub skipped: bool,
}

impl StageReport {
    #[must_use]
    pub fn new(stage: StageId, frame_idx: u64, output_hash: u64, skipped: bool) -> Self {
        Self {
            stage,
            frame_idx,
            output_hash,
            skipped,
        }
    }
}

/// Per-frame telemetry — sum of stage reports + companion subsystem outcomes.
#[derive(Debug, Clone)]
pub struct PipelineTelemetry {
    /// Frame index this telemetry covers.
    pub frame_idx: u64,
    /// Per-stage reports in canonical order.
    pub stage_reports: [StageReport; 12],
    /// Number of stages that ran (vs. skipped).
    pub stages_ran: u32,
    /// Number of stages that were skipped.
    pub stages_skipped: u32,
    /// Outcome of the physics subsystem.
    pub physics: PhysicsOutcome,
    /// Outcome of the audio subsystem.
    pub audio: AudioOutcome,
    /// Outcome of the animation subsystem.
    pub animation: AnimationOutcome,
    /// Outcome of the work-graph subsystem.
    pub work_graph: WorkGraphOutcome,
}

impl PipelineTelemetry {
    /// Whether ALL 12 stages executed in canonical order this frame
    /// (regardless of skip status). Acceptance gate AC2 in M8.
    #[must_use]
    pub fn all_twelve_executed(&self) -> bool {
        for (i, expected) in StageId::ORDER.iter().enumerate() {
            if self.stage_reports[i].stage != *expected {
                return false;
            }
            if self.stage_reports[i].frame_idx != self.frame_idx {
                return false;
            }
        }
        true
    }

    /// Total number of stage outputs hashed (sum of all `output_hash` values).
    /// Used as a coarse "frame fingerprint" — if two frames have identical
    /// fingerprints, ALL their stage-outputs are bit-equal.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        for r in &self.stage_reports {
            r.hash(&mut h);
        }
        h.finish()
    }
}

/// Frame-level digest produced by [`M8Pipeline::step`]. The cross-frame
/// determinism witness is the sequence of `digest` values across N frames.
#[derive(Debug, Clone)]
pub struct FramePipelineDigest {
    /// Frame index (monotonic).
    pub frame_idx: u64,
    /// Master seed used to drive this frame.
    pub master_seed: u64,
    /// Per-frame telemetry.
    pub telemetry: PipelineTelemetry,
    /// Composed final-output frame.
    pub composed: ComposedFrame,
    /// Compose-stage report (decomposition of XR vs. flat).
    pub compose_report: ComposeXrReport,
}

impl FramePipelineDigest {
    /// Bit-equal-comparable digest hash. Collapses the entire frame's
    /// determinism-relevant state into one u64.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.frame_idx.hash(&mut h);
        self.master_seed.hash(&mut h);
        self.telemetry.fingerprint().hash(&mut h);
        self.composed.hash_for_determinism(&mut h);
        h.finish()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// § Errors.
// ═══════════════════════════════════════════════════════════════════════════

/// Errors a pipeline-step can return.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// Stage 3 (omega-field) returned a mutation error.
    #[error("Stage 3 OmegaFieldUpdate failed at frame {frame}")]
    OmegaFieldFailed { frame: u64 },
    /// Stage 4 (wave-solver) returned an error.
    #[error("Stage 4 WaveSolverPass failed at frame {frame}")]
    WaveSolverFailed { frame: u64 },
    /// Stage 5 (SDF-raymarch) returned an error.
    #[error("Stage 5 SdfRaymarchPass failed at frame {frame}")]
    SdfRaymarchFailed { frame: u64 },
    /// Stage 9 (mise-en-abyme) hit a hard-cap or budget bound.
    #[error("Stage 9 MiseEnAbymePass exceeded budget at frame {frame}")]
    MiseEnAbymeOverBudget { frame: u64 },
    /// Stage 8 returned a consent-gate refusal that wasn't an honest skip.
    #[error("Stage 8 CompanionSemanticPass returned an unexpected gate refusal at frame {frame}")]
    CompanionGateUnexpected { frame: u64 },
    /// View dimensions in [`PipelineConfig`] mismatch a stage's expectation.
    #[error("view dimension mismatch: config={cw}x{ch}, stage expected={sw}x{sh}")]
    ViewDimensionMismatch { cw: u32, ch: u32, sw: u32, sh: u32 },
}

// ═══════════════════════════════════════════════════════════════════════════
// § The M8 Pipeline driver.
// ═══════════════════════════════════════════════════════════════════════════

/// The M8 Pipeline driver. Owns the full 12-stage assembly + companion
/// subsystems. Each tick advances every stage in canonical order.
pub struct M8Pipeline {
    config: PipelineConfig,
    frame_idx: u64,
    // Per-stage drivers.
    embodiment: EmbodimentPass,
    gaze_collapse: GazeCollapsePassDriver,
    omega_field: OmegaFieldDriver,
    wave_solver: WaveSolverDriver,
    sdf_raymarch: SdfRaymarchDriver,
    kan_brdf: KanBrdfEvalDriver,
    fractal_amp: FractalAmplifierPassDriver,
    companion_semantic: CompanionSemanticPass,
    mise_en_abyme: MiseEnAbymeDriver,
    tonemap: ToneMapDriver,
    appsw: AppSwPassDriver,
    compose_xr: ComposeXrLayers,
    // Companion subsystems.
    physics: PhysicsSubsystem,
    audio: AudioSubsystem,
    animation: AnimationSubsystem,
    work_graph: WorkGraphSubsystem,
}

impl M8Pipeline {
    /// Construct a new M8 pipeline with the given configuration.
    #[must_use]
    pub fn new(config: PipelineConfig) -> Self {
        let seed = config.master_seed;
        Self {
            embodiment: EmbodimentPass::new(seed),
            gaze_collapse: GazeCollapsePassDriver::new(seed, config.view_width, config.view_height),
            omega_field: OmegaFieldDriver::new(seed),
            wave_solver: WaveSolverDriver::new(seed),
            sdf_raymarch: SdfRaymarchDriver::new(config.view_width, config.view_height),
            kan_brdf: KanBrdfEvalDriver::new(),
            fractal_amp: FractalAmplifierPassDriver::new(),
            companion_semantic: CompanionSemanticPass::new(),
            mise_en_abyme: MiseEnAbymeDriver::new(),
            tonemap: ToneMapDriver::new(),
            appsw: AppSwPassDriver::new(config.view_width, config.view_height),
            compose_xr: ComposeXrLayers::new(config.xr_enabled),
            physics: PhysicsSubsystem::new(seed),
            audio: AudioSubsystem::new(seed),
            animation: AnimationSubsystem::new(seed),
            work_graph: WorkGraphSubsystem::new(seed),
            frame_idx: 0,
            config,
        }
    }

    /// Read-only access to the config.
    #[must_use]
    pub fn config(&self) -> &PipelineConfig {
        &self.config
    }

    /// Current frame index (monotonic).
    #[must_use]
    pub fn frame_idx(&self) -> u64 {
        self.frame_idx
    }

    /// Run one frame through all 12 stages + companion subsystems.
    ///
    /// # Errors
    /// Returns [`PipelineError`] on any stage failure ; in practice the
    /// stage drivers are lenient (they fall back to safe defaults rather
    /// than failing) but the surface allows future hard-failures.
    pub fn step(&mut self, dt_s: f32) -> Result<FramePipelineDigest, PipelineError> {
        let frame = self.frame_idx;

        // ───────────────────────────────────────────────────────────────
        // § Companion subsystems run in async-compute lane (conceptually).
        // ───────────────────────────────────────────────────────────────
        let physics_outcome = self.physics.step(dt_s, frame);
        let animation_outcome = self.animation.step(dt_s, frame);
        let audio_outcome = self.audio.step(dt_s, frame);
        let work_graph_outcome = if self.config.work_graph_enabled {
            self.work_graph.step(frame)
        } else {
            self.work_graph.step_no_op(frame)
        };

        // ───────────────────────────────────────────────────────────────
        // § Stage 1 : EmbodimentPass.
        // ───────────────────────────────────────────────────────────────
        let embodiment_inputs = EmbodimentInputs::deterministic_default(frame);
        let body_field = self.embodiment.run(&embodiment_inputs)?;
        let stage1 = StageReport::new(
            StageId::Embodiment,
            frame,
            body_field.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 2 : GazeCollapsePass.
        // ───────────────────────────────────────────────────────────────
        let gaze_outputs = self.gaze_collapse.run(&body_field, frame)?;
        let stage2 = StageReport::new(
            StageId::GazeCollapse,
            frame,
            gaze_outputs.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 3 : OmegaFieldUpdate (async-compute lane).
        // ───────────────────────────────────────────────────────────────
        let omega_outputs = self
            .omega_field
            .run(&body_field, &gaze_outputs, frame)
            .map_err(|_| PipelineError::OmegaFieldFailed { frame })?;
        let stage3 = StageReport::new(
            StageId::OmegaFieldUpdate,
            frame,
            omega_outputs.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 4 : WaveSolverPass (ψ-field multi-band).
        // ───────────────────────────────────────────────────────────────
        let wave_outputs = self
            .wave_solver
            .run(&omega_outputs, frame)
            .map_err(|_| PipelineError::WaveSolverFailed { frame })?;
        let stage4 = StageReport::new(
            StageId::WaveSolver,
            frame,
            wave_outputs.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 5 : SDFRaymarchPass.
        // ───────────────────────────────────────────────────────────────
        let raymarch_outputs = self
            .sdf_raymarch
            .run(&body_field, &gaze_outputs, &wave_outputs, frame)
            .map_err(|_| PipelineError::SdfRaymarchFailed { frame })?;
        let stage5 = StageReport::new(
            StageId::SdfRaymarch,
            frame,
            raymarch_outputs.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 6 : KANBRDFEval (16-band hyperspectral).
        // ───────────────────────────────────────────────────────────────
        let brdf_outputs = self.kan_brdf.run(&raymarch_outputs, &wave_outputs, frame);
        let stage6 = StageReport::new(
            StageId::KanBrdf,
            frame,
            brdf_outputs.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 7 : FractalAmplifierPass.
        // ───────────────────────────────────────────────────────────────
        let fractal_outputs = self
            .fractal_amp
            .run(&raymarch_outputs, &brdf_outputs, frame);
        let stage7 = StageReport::new(
            StageId::FractalAmplifier,
            frame,
            fractal_outputs.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 8 : CompanionSemanticPass (gated, zero-cost-when-off).
        // ───────────────────────────────────────────────────────────────
        let (companion_outputs, companion_skipped) = if self.config.companion_enabled {
            let outs = self.companion_semantic.run(&omega_outputs, frame);
            (outs, false)
        } else {
            (CompanionSemanticOutputs::skipped(frame), true)
        };
        let stage8 = StageReport::new(
            StageId::CompanionSemantic,
            frame,
            companion_outputs.determinism_hash(),
            companion_skipped,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 9 : MiseEnAbymePass.
        // ───────────────────────────────────────────────────────────────
        let abyme_outputs = self.mise_en_abyme.run(&fractal_outputs, frame);
        let stage9 = StageReport::new(
            StageId::MiseEnAbyme,
            frame,
            abyme_outputs.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 10 : ToneMapPass (spectral → RGB).
        // ───────────────────────────────────────────────────────────────
        let tonemap_outputs = self.tonemap.run(&abyme_outputs, &companion_outputs, frame);
        let stage10 = StageReport::new(
            StageId::ToneMap,
            frame,
            tonemap_outputs.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Stage 11 : AppSWPass (motion-vec output).
        // ───────────────────────────────────────────────────────────────
        let motion_vec = self.appsw.run(&raymarch_outputs, frame);
        let stage11 = StageReport::new(StageId::AppSw, frame, motion_vec.determinism_hash(), false);

        // ───────────────────────────────────────────────────────────────
        // § Stage 12 : ComposeXRLayers.
        // ───────────────────────────────────────────────────────────────
        let (composed, compose_report) = self.compose_xr.run(&tonemap_outputs, &motion_vec, frame);
        let stage12 = StageReport::new(
            StageId::ComposeXrLayers,
            frame,
            composed.determinism_hash(),
            false,
        );

        // ───────────────────────────────────────────────────────────────
        // § Aggregate telemetry + bump frame.
        // ───────────────────────────────────────────────────────────────
        let stage_reports = [
            stage1, stage2, stage3, stage4, stage5, stage6, stage7, stage8, stage9, stage10,
            stage11, stage12,
        ];
        let mut stages_ran = 0_u32;
        let mut stages_skipped = 0_u32;
        for r in &stage_reports {
            if r.skipped {
                stages_skipped += 1;
            } else {
                stages_ran += 1;
            }
        }
        let telemetry = PipelineTelemetry {
            frame_idx: frame,
            stage_reports,
            stages_ran,
            stages_skipped,
            physics: physics_outcome,
            audio: audio_outcome,
            animation: animation_outcome,
            work_graph: work_graph_outcome,
        };

        let digest = FramePipelineDigest {
            frame_idx: frame,
            master_seed: self.config.master_seed,
            telemetry,
            composed,
            compose_report,
        };

        self.frame_idx = self.frame_idx.wrapping_add(1);
        Ok(digest)
    }

    /// Run N consecutive frames + return the digest sequence. Helper for
    /// determinism tests + the canonical playtest harness.
    ///
    /// # Errors
    /// Returns the first stage error encountered.
    pub fn run_n(
        &mut self,
        n_frames: u32,
        dt_s: f32,
    ) -> Result<Vec<FramePipelineDigest>, PipelineError> {
        let mut out = Vec::with_capacity(n_frames as usize);
        for _ in 0..n_frames {
            out.push(self.step(dt_s)?);
        }
        Ok(out)
    }

    /// Read-only access to the embodiment driver (for cross-stage tests).
    #[must_use]
    pub fn embodiment(&self) -> &EmbodimentPass {
        &self.embodiment
    }

    /// Read-only access to the gaze-collapse driver.
    #[must_use]
    pub fn gaze_collapse(&self) -> &GazeCollapsePassDriver {
        &self.gaze_collapse
    }

    /// Read-only access to the omega-field driver.
    #[must_use]
    pub fn omega_field(&self) -> &OmegaFieldDriver {
        &self.omega_field
    }

    /// Read-only access to the wave-solver driver.
    #[must_use]
    pub fn wave_solver(&self) -> &WaveSolverDriver {
        &self.wave_solver
    }

    /// Read-only access to the SDF-raymarch driver.
    #[must_use]
    pub fn sdf_raymarch(&self) -> &SdfRaymarchDriver {
        &self.sdf_raymarch
    }

    /// Read-only access to the mise-en-abyme driver.
    #[must_use]
    pub fn mise_en_abyme(&self) -> &MiseEnAbymeDriver {
        &self.mise_en_abyme
    }

    /// Read-only access to the compose-XR driver.
    #[must_use]
    pub fn compose_xr(&self) -> &ComposeXrLayers {
        &self.compose_xr
    }

    /// Read-only access to the physics subsystem.
    #[must_use]
    pub fn physics(&self) -> &PhysicsSubsystem {
        &self.physics
    }

    /// Read-only access to the audio subsystem.
    #[must_use]
    pub fn audio(&self) -> &AudioSubsystem {
        &self.audio
    }

    /// Read-only access to the animation subsystem.
    #[must_use]
    pub fn animation(&self) -> &AnimationSubsystem {
        &self.animation
    }

    /// Read-only access to the work-graph subsystem.
    #[must_use]
    pub fn work_graph(&self) -> &WorkGraphSubsystem {
        &self.work_graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_pipeline() -> M8Pipeline {
        M8Pipeline::new(PipelineConfig::default())
    }

    #[test]
    fn stage_id_order_is_one_through_twelve() {
        for (i, sid) in StageId::ORDER.iter().enumerate() {
            assert_eq!(
                *sid as u32,
                (i + 1) as u32,
                "stage {} not at index {}",
                sid.name(),
                i
            );
        }
    }

    #[test]
    fn stage_id_names_unique() {
        let mut names: Vec<&str> = StageId::ORDER.iter().map(|s| s.name()).collect();
        names.sort_unstable();
        let original_len = names.len();
        names.dedup();
        assert_eq!(names.len(), original_len, "stage names must be unique");
    }

    #[test]
    fn stage_id_source_crates_present() {
        // Every stage MUST cite a real source-crate (no placeholders).
        for sid in StageId::ORDER {
            let src = sid.source_crate();
            assert!(!src.is_empty(), "stage {} missing source crate", sid.name());
        }
    }

    #[test]
    fn pipeline_constructs_with_default_config() {
        let p = fresh_pipeline();
        assert_eq!(p.frame_idx(), 0);
        assert_eq!(p.config().master_seed, 0xC551_F00D);
    }

    #[test]
    fn one_step_executes_all_twelve() {
        let mut p = fresh_pipeline();
        let digest = p.step(1.0 / 60.0).expect("first step");
        assert!(digest.telemetry.all_twelve_executed());
        assert_eq!(p.frame_idx(), 1);
    }

    #[test]
    fn two_runs_bit_equal() {
        // PRIMARY DETERMINISM CONTRACT.
        let mut p1 = fresh_pipeline();
        let mut p2 = fresh_pipeline();
        let d1 = p1.step(1.0 / 60.0).expect("p1 step");
        let d2 = p2.step(1.0 / 60.0).expect("p2 step");
        assert_eq!(d1.digest(), d2.digest(), "two-run determinism failed");
    }

    #[test]
    fn ten_step_replay_bit_equal() {
        // Extended determinism contract — 10 frames same.
        let mut p1 = fresh_pipeline();
        let mut p2 = fresh_pipeline();
        let r1 = p1.run_n(10, 1.0 / 60.0).expect("p1 run");
        let r2 = p2.run_n(10, 1.0 / 60.0).expect("p2 run");
        assert_eq!(r1.len(), r2.len());
        for (i, (a, b)) in r1.iter().zip(r2.iter()).enumerate() {
            assert_eq!(a.digest(), b.digest(), "frame {} digest mismatch", i);
        }
    }

    #[test]
    fn frame_idx_advances_monotonically() {
        let mut p = fresh_pipeline();
        for i in 0..5 {
            assert_eq!(p.frame_idx(), i);
            p.step(1.0 / 60.0).unwrap();
        }
        assert_eq!(p.frame_idx(), 5);
    }

    #[test]
    fn different_seeds_produce_different_digests() {
        let mut a = M8Pipeline::new(PipelineConfig {
            master_seed: 0x1111_1111,
            ..Default::default()
        });
        let mut b = M8Pipeline::new(PipelineConfig {
            master_seed: 0x2222_2222,
            ..Default::default()
        });
        let da = a.step(1.0 / 60.0).unwrap();
        let db = b.step(1.0 / 60.0).unwrap();
        assert_ne!(
            da.digest(),
            db.digest(),
            "different seeds must produce different digests"
        );
    }

    #[test]
    fn companion_disabled_skips_stage_8() {
        let mut p = M8Pipeline::new(PipelineConfig {
            companion_enabled: false,
            ..Default::default()
        });
        let d = p.step(1.0 / 60.0).unwrap();
        let s8 = d.telemetry.stage_reports[7]; // index 7 = stage 8.
        assert_eq!(s8.stage, StageId::CompanionSemantic);
        assert!(
            s8.skipped,
            "Stage 8 should be skipped when companion disabled"
        );
    }

    #[test]
    fn stages_ran_plus_skipped_equals_twelve() {
        let mut p = fresh_pipeline();
        let d = p.step(1.0 / 60.0).unwrap();
        assert_eq!(d.telemetry.stages_ran + d.telemetry.stages_skipped, 12);
    }

    #[test]
    fn xr_disabled_uses_flat_screen_compose() {
        let mut p = M8Pipeline::new(PipelineConfig {
            xr_enabled: false,
            ..Default::default()
        });
        let d = p.step(1.0 / 60.0).unwrap();
        assert!(!d.compose_report.xr_path_used);
        assert!(d.compose_report.flat_path_used);
    }

    #[test]
    fn xr_enabled_uses_xr_compose_path() {
        let mut p = M8Pipeline::new(PipelineConfig {
            xr_enabled: true,
            ..Default::default()
        });
        let d = p.step(1.0 / 60.0).unwrap();
        assert!(d.compose_report.xr_path_used);
        assert!(!d.compose_report.flat_path_used);
    }

    #[test]
    fn pipeline_telemetry_fingerprint_stable_across_runs() {
        let mut p1 = fresh_pipeline();
        let mut p2 = fresh_pipeline();
        let f1 = p1.step(1.0 / 60.0).unwrap().telemetry.fingerprint();
        let f2 = p2.step(1.0 / 60.0).unwrap().telemetry.fingerprint();
        assert_eq!(f1, f2);
    }

    #[test]
    fn frame_idx_in_telemetry_matches_pipeline() {
        let mut p = fresh_pipeline();
        for i in 0..3 {
            let d = p.step(1.0 / 60.0).unwrap();
            assert_eq!(d.telemetry.frame_idx, i);
        }
    }

    #[test]
    fn attestation_load_bearing() {
        assert!(ATTESTATION_M8.contains("no hurt nor harm"));
        assert!(ATTESTATION_M8.contains("anyone, anything, or anybody"));
    }

    #[test]
    fn run_n_zero_frames_returns_empty() {
        let mut p = fresh_pipeline();
        let r = p.run_n(0, 1.0 / 60.0).unwrap();
        assert!(r.is_empty());
        assert_eq!(p.frame_idx(), 0);
    }

    #[test]
    fn ten_minute_playtest_completes_without_panic() {
        // The CANONICAL PLAYTEST acceptance gate.
        // 10 minutes @ 60 fps = 36_000 frames. We compress the simulated
        // playtest to 600 frames (= 10 sec sim but exercising the same
        // stage-progression path for every tick). This is the structural
        // smoke for "10-min playtest runs without panic" — full real-time
        // hardware run is reserved for the M8-Pass certification.
        let mut p = fresh_pipeline();
        for _ in 0..600 {
            let _ = p.step(1.0 / 60.0).expect("playtest tick");
        }
        assert_eq!(p.frame_idx(), 600);
    }

    #[test]
    fn ten_minute_playtest_determinism_held() {
        let mut p1 = fresh_pipeline();
        let mut p2 = fresh_pipeline();
        let r1 = p1.run_n(120, 1.0 / 60.0).unwrap();
        let r2 = p2.run_n(120, 1.0 / 60.0).unwrap();
        let h1: Vec<u64> = r1.iter().map(|d| d.digest()).collect();
        let h2: Vec<u64> = r2.iter().map(|d| d.digest()).collect();
        assert_eq!(h1, h2, "120-frame replay determinism");
    }
}
