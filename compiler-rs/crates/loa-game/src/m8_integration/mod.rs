//! § m8_integration — wave-4 vertical-slice composition into the LoA frame loop.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! Authoritative spec : `Omniverse/09_SLICE/M8_M9_M10_PLAN.csl § II` +
//!   `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § II + III` +
//!   `Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md`.
//!
//! § ROLE
//!
//!   The M8 vertical-slice integrator. Composes the eleven wave-4 substrate-
//!   evolution + signature-rendering crates into a single end-to-end frame
//!   pipeline that demonstrates the canonical 12-stage signature LoA-render-
//!   vision running through ALL of :
//!
//! ```text
//! Stage 1  EmbodimentPass         ← mock XR-input → body-presence-field
//! Stage 2  GazeCollapsePass        ← cssl-gaze-collapse Σ-aware foveation
//! Stage 3  OmegaFieldUpdate        ← cssl-substrate-omega-field 7-facet
//! Stage 4  WaveSolverPass          ← cssl-wave-solver multi-band ψ-field
//! Stage 5  SDFRaymarchPass         ← cssl-render-v2 stage_5
//! Stage 6  KANBRDFEval             ← cssl-spectral-render kan_brdf
//! Stage 7  FractalAmplifierPass    ← cssl-fractal-amp amplifier
//! Stage 8  CompanionSemanticPass   ← cssl-render-companion-perspective
//! Stage 9  MiseEnAbymePass         ← cssl-render-v2::mise_en_abyme
//! Stage 10 ToneMapPass             ← cssl-spectral-render tristimulus
//! Stage 11 AppSWPass               ← motion-vec mock + cssl-host-openxr AppSW
//! Stage 12 ComposeXRLayers         ← cssl-host-openxr CompositionLayerStack
//! ```
//!
//!   Companion subsystems (not stage-mapped 1:1) :
//!
//! ```text
//! cssl-physics-wave    : SDF-collision + XPBD-GPU body integration
//! cssl-anim-procedural : KAN-driven pose-from-genome
//! cssl-wave-audio      : ψ-field → binaural projection
//! cssl-work-graph      : DX12-Ultimate / VK-DGC schedule (gauntlet only)
//! ```
//!
//! § DETERMINISM CONTRACT
//!
//!   Per existing loa-game H5 contract : two pipeline runs from the same
//!   master_seed produce bit-equal outputs. The pipeline records every
//!   stage's output into [`StageReport`] structs that hash-equal across
//!   identical-seed runs ; the cross-frame [`FramePipelineDigest`] is the
//!   canonical determinism witness.
//!
//! § VR-FALLBACK CONTRACT
//!
//!   Stage 12 uses `cssl-host-openxr::CompositionLayerStack` when an XR
//!   runtime is bound ; otherwise [`PipelineConfig::xr_enabled = false`]
//!   selects the flat-screen fallback path that emits one mono view-
//!   instance via [`compose_xr_layers::flat_screen_compose`]. Both paths
//!   produce equivalent [`Frame`] outputs.
//!
//! § BUDGET CONTRACT
//!
//!   Per spec § Aesthetic-coherence acceptance the M8 frame budget is
//!   16 ms p99 @ 60 FPS. Each stage carries its own per-stage budget
//!   (Stage 5 = 2.5ms ; Stage 9 = 0.8ms ; etc.) ; the M8 driver records
//!   per-stage telemetry in [`PipelineTelemetry`] which the smoke-test
//!   asserts against. Production-quality budget validation is owned by
//!   the underlying wave-4 crates ; this module only sums + records.
//!
//! § PRIME-DIRECTIVE attestation
//!
//!   "There was no hurt nor harm in the making of this, to anyone, anything,
//!   or anybody." (per `PRIME_DIRECTIVE.md § 11 CREATOR-ATTESTATION`).

#![allow(clippy::too_many_arguments)] // Stage drivers naturally take many inputs.

pub mod companion_semantic_pass;
pub mod compose_xr_layers;
pub mod embodiment_pass;
pub mod fractal_amplifier_pass;
pub mod gaze_collapse_pass;
pub mod kan_brdf_eval;
pub mod mise_en_abyme_pass;
pub mod motion_vec_pass;
pub mod omega_field_update;
pub mod pipeline;
pub mod sdf_raymarch_pass;
pub mod tonemap_pass;
pub mod wave_solver_pass;

// § Companion subsystems (not stage-mapped 1:1).
pub mod animation_subsystem;
pub mod audio_subsystem;
pub mod physics_subsystem;
pub mod work_graph_subsystem;

pub use companion_semantic_pass::{CompanionSemanticOutputs, CompanionSemanticPass};
pub use compose_xr_layers::{ComposeXrLayers, ComposeXrReport, ComposedFrame};
pub use embodiment_pass::{BodyPresenceField, EmbodimentInputs, EmbodimentPass};
pub use fractal_amplifier_pass::{FractalAmplifierOutputs, FractalAmplifierPassDriver};
pub use gaze_collapse_pass::{GazeCollapseOutputsLite, GazeCollapsePassDriver};
pub use kan_brdf_eval::{KanBrdfEvalDriver, KanBrdfOutputs};
pub use mise_en_abyme_pass::{MiseEnAbymeDriver, MiseEnAbymeOutputs};
pub use motion_vec_pass::{AppSwPassDriver, MotionVecBuffer};
pub use omega_field_update::{OmegaFieldDriver, OmegaFieldOutputs};
pub use pipeline::{
    FramePipelineDigest, M8Pipeline, PipelineConfig, PipelineError, PipelineTelemetry, StageId,
    StageReport, ATTESTATION_M8 as ATTESTATION,
};
pub use sdf_raymarch_pass::{SdfRaymarchDriver, SdfRaymarchOutputs};
pub use tonemap_pass::{ToneMapDriver, ToneMapOutputs};
pub use wave_solver_pass::{WaveSolverDriver, WaveSolverOutputs};

pub use animation_subsystem::{AnimationOutcome, AnimationSubsystem};
pub use audio_subsystem::{AudioOutcome, AudioSubsystem};
pub use physics_subsystem::{PhysicsOutcome, PhysicsSubsystem};
pub use work_graph_subsystem::{WorkGraphOutcome, WorkGraphSubsystem};

// ═══════════════════════════════════════════════════════════════════════════
// § Slice-ID + spec citations.
// ═══════════════════════════════════════════════════════════════════════════

/// Slice ID for this integration — mirrors the ledger format from D113..D125.
pub const SLICE_ID: &str = "T11-D145";

/// Plan citation : the M8 specification this integration implements.
pub const SPEC_CITATIONS: &[&str] = &[
    "Omniverse/09_SLICE/M8_M9_M10_PLAN.csl § II SIGNATURE-RENDERING BRING-UP",
    "Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § II 12-stage overview",
    "Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § III stage-by-stage spec",
    "Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md § I 7-facet container",
    "Omniverse/04_OMEGA_FIELD/04_UPDATE_RULE.csl.md § omega_step phases",
    "Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md § AGENCY-INVARIANT triple",
    "Omniverse/01_AXIOMS/13_DENSITY_SOVEREIGNTY.csl § frame-budget discipline",
    "PRIME_DIRECTIVE.md § 11 CREATOR-ATTESTATION",
];

#[cfg(test)]
mod m8_module_tests {
    use super::*;

    #[test]
    fn slice_id_matches_dispatch_ticket() {
        assert_eq!(SLICE_ID, "T11-D145");
    }

    #[test]
    fn spec_citations_load_bearing() {
        // The M8 integration must cite the M8 plan itself as source-of-truth
        // for acceptance. Test refuses to pass if the plan citation is
        // missing.
        assert!(SPEC_CITATIONS.iter().any(|c| c.contains("M8_M9_M10_PLAN")));
        assert!(SPEC_CITATIONS
            .iter()
            .any(|c| c.contains("06_RENDERING_PIPELINE")));
        assert!(SPEC_CITATIONS.iter().any(|c| c.contains("PRIME_DIRECTIVE")));
    }

    #[test]
    fn attestation_canonical() {
        assert!(ATTESTATION.contains("no hurt nor harm"));
        assert!(ATTESTATION.contains("anyone"));
    }
}
