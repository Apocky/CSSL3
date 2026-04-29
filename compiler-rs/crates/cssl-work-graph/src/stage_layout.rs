//! Render-graph stage layout.
//!
//! § DESIGN
//!   The 12-stage render-graph from `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl`
//!   is encoded here as a `StageId` enum. Stages 4..=7 are the canonical
//!   work-graph fusion target (cite § X work-graph-fusion-contract) ; other
//!   stages live outside the work-graph but still appear as boundary nodes
//!   that produce-into / consume-from the work-graph's input + output
//!   buffers.
//!
//! § BUFFER LAYOUTS
//!   `StageBufferLayout` declares the canonical I/O for each stage. The
//!   builder uses these to wire-up node-to-node buffer handles
//!   automatically when a node says "I'm Stage6/KAN-BRDF" — the relevant
//!   inputs/outputs are looked-up from this table.

use core::fmt;

/// 12 stages of the canonical render-graph (cite § II of rendering-pipeline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum StageId {
    /// Stage 1 : XR-input → body-presence-field.
    Embodiment = 1,
    /// Stage 2 : eye-track → fovea-mask + KAN-detail-budget.
    GazeCollapse = 2,
    /// Stage 3 : 6-phase omega_step (async-compute).
    OmegaFieldUpdate = 3,
    /// Stage 4 : LBM ψ multi-band (work-graph node).
    WaveSolver = 4,
    /// Stage 5 : SDF ray-march (work-graph node ; D116 integration).
    SdfRaymarch = 5,
    /// Stage 6 : 16-band KAN-BRDF (work-graph node ; D118 integration).
    KanBrdfEval = 6,
    /// Stage 7 : KAN-detail-amplifier (work-graph node).
    FractalAmplifier = 7,
    /// Stage 8 : companion-perspective render (gated).
    CompanionSemantic = 8,
    /// Stage 9 : mise-en-abyme bounded-recursion.
    MiseEnAbyme = 9,
    /// Stage 10 : tonemap + bloom + post.
    ToneMap = 10,
    /// Stage 11 : motion-vec + depth for AppSW.
    AppSw = 11,
    /// Stage 12 : OpenXR composition.
    ComposeXr = 12,
}

impl StageId {
    /// Stable string tag.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Embodiment => "embodiment",
            Self::GazeCollapse => "gaze-collapse",
            Self::OmegaFieldUpdate => "omega-field-update",
            Self::WaveSolver => "wave-solver",
            Self::SdfRaymarch => "sdf-raymarch",
            Self::KanBrdfEval => "kan-brdf-eval",
            Self::FractalAmplifier => "fractal-amplifier",
            Self::CompanionSemantic => "companion-semantic",
            Self::MiseEnAbyme => "mise-en-abyme",
            Self::ToneMap => "tone-map",
            Self::AppSw => "app-sw",
            Self::ComposeXr => "compose-xr",
        }
    }

    /// Stage number (1..=12).
    #[must_use]
    pub const fn number(self) -> u8 {
        self as u8
    }

    /// Is this stage in the canonical work-graph fusion (4..=7) ?
    #[must_use]
    pub const fn is_work_graph_fused(self) -> bool {
        matches!(
            self,
            Self::WaveSolver | Self::SdfRaymarch | Self::KanBrdfEval | Self::FractalAmplifier
        )
    }

    /// Iterate all stages in order.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::Embodiment,
            Self::GazeCollapse,
            Self::OmegaFieldUpdate,
            Self::WaveSolver,
            Self::SdfRaymarch,
            Self::KanBrdfEval,
            Self::FractalAmplifier,
            Self::CompanionSemantic,
            Self::MiseEnAbyme,
            Self::ToneMap,
            Self::AppSw,
            Self::ComposeXr,
        ]
        .into_iter()
    }

    /// Iterate the work-graph fused stages (4..=7).
    pub fn work_graph_fused() -> impl Iterator<Item = Self> {
        [
            Self::WaveSolver,
            Self::SdfRaymarch,
            Self::KanBrdfEval,
            Self::FractalAmplifier,
        ]
        .into_iter()
    }
}

impl fmt::Display for StageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// Canonical per-stage I/O buffer layout (cite § IV per-pass-IO-buffer-contract).
///
/// Buffer names are the same strings used by [`crate::WorkGraphBuilder`] to
/// wire node-to-node dependencies — the canonical names are stable across
/// the whole pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageBufferLayout {
    /// Stage this layout is for.
    pub stage: StageId,
    /// Buffer-handles this stage reads from upstream.
    pub inputs: Vec<&'static str>,
    /// Buffer-handles this stage produces.
    pub outputs: Vec<&'static str>,
}

impl StageBufferLayout {
    /// Look-up the canonical I/O for one stage.
    #[must_use]
    pub fn canonical(stage: StageId) -> Self {
        let (i, o): (&[&str], &[&str]) = match stage {
            StageId::Embodiment => (
                &[],
                &[
                    "body-presence-field",
                    "aura-lambda-emission",
                    "machine-sdf-handle",
                ],
            ),
            StageId::GazeCollapse => (
                &[
                    "xr-eye-gaze",
                    "xr-eye-openness",
                    "body-presence-field",
                    "omega-prev-mera",
                ],
                &["fovea-mask", "kan-detail-budget", "collapse-bias-vector"],
            ),
            StageId::OmegaFieldUpdate => (
                &[
                    "omega-prev",
                    "observations",
                    "ops-this-tick",
                    "collapse-bias-vector",
                    "body-presence-field",
                ],
                &[
                    "omega-next",
                    "omega-next-collapsed-regions",
                    "omega-next-cohomology-classes",
                    "omega-next-entropy-snapshot",
                ],
            ),
            StageId::WaveSolver => (
                &[
                    "omega-next-p-facet",
                    "omega-next-s-facet",
                    "omega-next-m-facet",
                    "omega-next-sdf",
                ],
                &[
                    "psi-light-16",
                    "psi-audio-1",
                    "lbm-distributions",
                    "impulse-response-field",
                ],
            ),
            StageId::SdfRaymarch => (
                &[
                    "omega-next-sdf",
                    "omega-next-mera",
                    "body-presence-field",
                    "fovea-mask",
                    "psi-light-16",
                    "camera-pair",
                ],
                &[
                    "g-buffer-mv-2",
                    "visibility-mask",
                    "first-hit-distance",
                    "volumetric-accum",
                ],
            ),
            StageId::KanBrdfEval => (
                &[
                    "g-buffer-mv-2",
                    "visibility-mask",
                    "psi-light-16",
                    "omega-next-p-facet",
                    "omega-next-m-facet",
                ],
                &[
                    "spectral-radiance-mv-2-16",
                    "albedo-buffer",
                    "roughness-buffer",
                ],
            ),
            StageId::FractalAmplifier => (
                &[
                    "spectral-radiance-mv-2-16",
                    "g-buffer-mv-2",
                    "kan-detail-budget",
                    "fovea-mask",
                ],
                &["amplified-radiance-mv-2-16", "sub-pixel-detail-map"],
            ),
            StageId::CompanionSemantic => (
                &[
                    "omega-next",
                    "amplified-radiance-mv-2-16",
                    "companion-ai-perception",
                    "companion-ai-attention",
                ],
                &["companion-view-2-16", "attention-overlay"],
            ),
            StageId::MiseEnAbyme => (
                &[
                    "omega-next-sdf",
                    "omega-next-m-facet",
                    "amplified-radiance-mv-2-16",
                    "camera-pair",
                ],
                &["mise-en-abyme-radiance-2-16"],
            ),
            StageId::ToneMap => (
                &["mise-en-abyme-radiance-2-16", "companion-view-2-16"],
                &["display-buffer-2-rgb10a2"],
            ),
            StageId::AppSw => (
                &[
                    "display-buffer-2-rgb10a2",
                    "g-buffer-mv-2",
                    "prev-frame-g-buffer-mv-2",
                    "camera-pair",
                    "prev-camera-pair",
                ],
                &["motion-vectors", "depth-buffer", "appsw-metadata"],
            ),
            StageId::ComposeXr => (
                &[
                    "display-buffer-2-rgb10a2",
                    "motion-vectors",
                    "depth-buffer",
                    "ui-render-target",
                    "passthrough-video",
                    "safety-boundary",
                ],
                &["openxr-composition-layers"],
            ),
        };
        Self {
            stage,
            inputs: i.to_vec(),
            outputs: o.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StageBufferLayout, StageId};

    #[test]
    fn stage_numbers_run_1_to_12() {
        for (i, s) in StageId::all().enumerate() {
            assert_eq!(s.number() as usize, i + 1);
        }
    }

    #[test]
    fn fused_stages_are_4_to_7() {
        let fused: Vec<_> = StageId::work_graph_fused().collect();
        assert_eq!(fused.len(), 4);
        assert!(fused.contains(&StageId::WaveSolver));
        assert!(fused.contains(&StageId::SdfRaymarch));
        assert!(fused.contains(&StageId::KanBrdfEval));
        assert!(fused.contains(&StageId::FractalAmplifier));
    }

    #[test]
    fn fused_flag_correctness() {
        assert!(StageId::WaveSolver.is_work_graph_fused());
        assert!(StageId::KanBrdfEval.is_work_graph_fused());
        assert!(!StageId::Embodiment.is_work_graph_fused());
        assert!(!StageId::ComposeXr.is_work_graph_fused());
    }

    #[test]
    fn canonical_layout_wave_solver_outputs_psi_light() {
        let l = StageBufferLayout::canonical(StageId::WaveSolver);
        assert!(l.outputs.contains(&"psi-light-16"));
    }

    #[test]
    fn canonical_layout_kan_brdf_inputs_g_buffer() {
        let l = StageBufferLayout::canonical(StageId::KanBrdfEval);
        assert!(l.inputs.contains(&"g-buffer-mv-2"));
    }

    #[test]
    fn canonical_layout_compose_xr_inputs_passthrough() {
        let l = StageBufferLayout::canonical(StageId::ComposeXr);
        assert!(l.inputs.contains(&"passthrough-video"));
    }

    #[test]
    fn canonical_layout_sdf_raymarch_emits_g_buffer_mv_2() {
        let l = StageBufferLayout::canonical(StageId::SdfRaymarch);
        assert!(l.outputs.contains(&"g-buffer-mv-2"));
        assert!(l.outputs.contains(&"first-hit-distance"));
    }

    #[test]
    fn stage_tags_are_kebab_case() {
        for s in StageId::all() {
            let t = s.tag();
            assert!(!t.is_empty());
            assert!(!t.contains('_'));
        }
    }

    #[test]
    fn stage_display_uses_tag() {
        assert_eq!(format!("{}", StageId::WaveSolver), "wave-solver");
    }
}
