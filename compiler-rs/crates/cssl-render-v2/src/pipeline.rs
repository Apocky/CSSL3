//! § pipeline — render-graph node-spec compatible with canonical 12-stage pipeline.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The 12-stage pipeline (06_RENDERING_PIPELINE) is a render-graph DAG. This
//!   module provides the node-spec types + the [`Stage5Node`] implementation
//!   so an upstream pipeline-driver (lands separately as T11-D123) can wire
//!   Stage-5 into the canonical-12-stage graph alongside the other slices
//!   (D113 OmegaFieldUpdate / D114 WaveSolver / D118 SpectralBRDF / D119
//!   FractalAmplifier / D120 GazeCollapse / etc.).
//!
//! § SPEC
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § II` — the twelve
//!     stages (overview).
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § IV` — IO-buffer
//!     contract table.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § X` — work-graph
//!     fusion contract.

use thiserror::Error;

/// Errors from the render-graph node-spec.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderGraphNodeError {
    /// Required input buffer is missing.
    #[error("required input buffer '{name}' is missing")]
    MissingInputBuffer { name: &'static str },
    /// Output buffer name collides with an existing one.
    #[error("output buffer name '{name}' collides with existing")]
    DuplicateOutputBuffer { name: &'static str },
    /// Stage role was wired in a slot that doesn't match.
    #[error("stage role {role:?} cannot be wired in slot {slot:?}")]
    SlotRoleMismatch {
        role: StageRole,
        slot: TwelveStagePipelineSlot,
    },
}

/// Canonical role of a render-graph stage. Drives shader-pipeline + IO-
/// validation in the pipeline-driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageRole {
    /// XR-input → body-presence-field (Stage-1).
    Embodiment,
    /// Eye-track → fovea-mask + KAN-detail-budget (Stage-2).
    GazeCollapse,
    /// Async-compute Ω-field 6-phase update (Stage-3).
    OmegaFieldUpdate,
    /// LBM ψ-field multi-band wave solver (Stage-4).
    WaveSolver,
    /// SDF raymarch → GBuffer + VolumetricAccum (Stage-5 — THIS CRATE).
    SdfRaymarch,
    /// 16-band hyperspectral KAN-BRDF per fragment (Stage-6).
    KanBrdf,
    /// Sub-pixel-fractal-tessellation amplifier (Stage-7).
    FractalAmplifier,
    /// Optional companion-perspective semantic render (Stage-8).
    CompanionSemantic,
    /// Mise-en-abyme recursive frame (Stage-9).
    MiseEnAbyme,
    /// ToneMap + bloom + post (Stage-10).
    ToneMap,
    /// AppSW motion-vec + depth (Stage-11).
    AppSpaceWarp,
    /// XR-composition layers (Stage-12).
    XrCompose,
}

impl StageRole {
    /// Canonical slot index in the 12-stage pipeline (1-based).
    #[must_use]
    pub fn canonical_slot_index(self) -> u8 {
        match self {
            StageRole::Embodiment => 1,
            StageRole::GazeCollapse => 2,
            StageRole::OmegaFieldUpdate => 3,
            StageRole::WaveSolver => 4,
            StageRole::SdfRaymarch => 5,
            StageRole::KanBrdf => 6,
            StageRole::FractalAmplifier => 7,
            StageRole::CompanionSemantic => 8,
            StageRole::MiseEnAbyme => 9,
            StageRole::ToneMap => 10,
            StageRole::AppSpaceWarp => 11,
            StageRole::XrCompose => 12,
        }
    }
}

/// Slot in the 12-stage pipeline. Slots 1..12 each correspond to one
/// [`StageRole`] ; the pipeline driver enforces the matching at wire-time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwelveStagePipelineSlot {
    Slot1Embodiment,
    Slot2GazeCollapse,
    Slot3OmegaFieldUpdate,
    Slot4WaveSolver,
    Slot5SdfRaymarch,
    Slot6KanBrdf,
    Slot7FractalAmplifier,
    Slot8CompanionSemantic,
    Slot9MiseEnAbyme,
    Slot10ToneMap,
    Slot11AppSpaceWarp,
    Slot12XrCompose,
}

impl TwelveStagePipelineSlot {
    /// The role that may be wired into this slot.
    #[must_use]
    pub fn allowed_role(self) -> StageRole {
        match self {
            Self::Slot1Embodiment => StageRole::Embodiment,
            Self::Slot2GazeCollapse => StageRole::GazeCollapse,
            Self::Slot3OmegaFieldUpdate => StageRole::OmegaFieldUpdate,
            Self::Slot4WaveSolver => StageRole::WaveSolver,
            Self::Slot5SdfRaymarch => StageRole::SdfRaymarch,
            Self::Slot6KanBrdf => StageRole::KanBrdf,
            Self::Slot7FractalAmplifier => StageRole::FractalAmplifier,
            Self::Slot8CompanionSemantic => StageRole::CompanionSemantic,
            Self::Slot9MiseEnAbyme => StageRole::MiseEnAbyme,
            Self::Slot10ToneMap => StageRole::ToneMap,
            Self::Slot11AppSpaceWarp => StageRole::AppSpaceWarp,
            Self::Slot12XrCompose => StageRole::XrCompose,
        }
    }

    /// Validate that the supplied role can be wired here.
    pub fn validate_wire(self, role: StageRole) -> Result<(), RenderGraphNodeError> {
        if self.allowed_role() != role {
            Err(RenderGraphNodeError::SlotRoleMismatch { role, slot: self })
        } else {
            Ok(())
        }
    }
}

/// Render-graph node — the abstract surface every stage implements.
pub trait RenderGraphNode {
    /// The canonical role of this node.
    fn role(&self) -> StageRole;
    /// Names of input buffers this node reads.
    fn input_buffers(&self) -> &[&'static str];
    /// Names of output buffers this node writes.
    fn output_buffers(&self) -> &[&'static str];
    /// Estimated ms-cost @ Quest-3 (06_RENDERING_PIPELINE § V).
    fn quest3_budget_ms(&self) -> f32;
    /// Estimated ms-cost @ Vision-Pro.
    fn vision_pro_budget_ms(&self) -> f32;
}

/// Stage-5 node-spec. Used by the pipeline-driver to wire Stage-5 into the
/// 12-stage graph.
#[derive(Debug, Clone, Copy, Default)]
pub struct Stage5Node;

impl Stage5Node {
    /// Spec-mandated input buffer names (06_RENDERING_PIPELINE § IV row 5).
    pub const INPUT_BUFFERS: &'static [&'static str] = &[
        "Ω.next.SDF",
        "Ω.next.MERA",
        "BodyPresenceField",
        "FoveaMask×2",
        "PsiField<LIGHT,16>",
        "Camera×2",
    ];
    /// Spec-mandated output buffer names (06_RENDERING_PIPELINE § IV row 5).
    pub const OUTPUT_BUFFERS: &'static [&'static str] = &[
        "GBuffer<MV,2>",
        "VisibilityMask×2",
        "FirstHitDistance×2",
        "VolumetricAccum",
    ];

    /// Wire Stage-5 into the canonical slot. Returns an error if the slot is
    /// not Slot-5.
    pub fn wire(slot: TwelveStagePipelineSlot) -> Result<Self, RenderGraphNodeError> {
        slot.validate_wire(StageRole::SdfRaymarch)?;
        Ok(Stage5Node)
    }
}

impl RenderGraphNode for Stage5Node {
    fn role(&self) -> StageRole {
        StageRole::SdfRaymarch
    }
    fn input_buffers(&self) -> &[&'static str] {
        Self::INPUT_BUFFERS
    }
    fn output_buffers(&self) -> &[&'static str] {
        Self::OUTPUT_BUFFERS
    }
    fn quest3_budget_ms(&self) -> f32 {
        crate::STAGE_5_QUEST3_BUDGET_MS
    }
    fn vision_pro_budget_ms(&self) -> f32 {
        crate::STAGE_5_VISION_PRO_BUDGET_MS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_indices_one_through_twelve() {
        assert_eq!(StageRole::Embodiment.canonical_slot_index(), 1);
        assert_eq!(StageRole::SdfRaymarch.canonical_slot_index(), 5);
        assert_eq!(StageRole::XrCompose.canonical_slot_index(), 12);
    }

    #[test]
    fn slot_5_allows_sdf_raymarch() {
        assert_eq!(
            TwelveStagePipelineSlot::Slot5SdfRaymarch.allowed_role(),
            StageRole::SdfRaymarch
        );
    }

    #[test]
    fn wire_correct_slot_succeeds() {
        let r = Stage5Node::wire(TwelveStagePipelineSlot::Slot5SdfRaymarch);
        assert!(r.is_ok());
    }

    #[test]
    fn wire_wrong_slot_errors() {
        let r = Stage5Node::wire(TwelveStagePipelineSlot::Slot6KanBrdf);
        assert!(matches!(r, Err(RenderGraphNodeError::SlotRoleMismatch { .. })));
    }

    #[test]
    fn stage5_role_is_sdf_raymarch() {
        let n = Stage5Node;
        assert_eq!(n.role(), StageRole::SdfRaymarch);
    }

    #[test]
    fn stage5_input_buffers_match_spec() {
        let n = Stage5Node;
        let inputs = n.input_buffers();
        assert!(inputs.contains(&"Ω.next.SDF"));
        assert!(inputs.contains(&"Ω.next.MERA"));
        assert!(inputs.contains(&"BodyPresenceField"));
        assert!(inputs.contains(&"FoveaMask×2"));
        assert!(inputs.contains(&"Camera×2"));
    }

    #[test]
    fn stage5_output_buffers_match_spec() {
        let n = Stage5Node;
        let outputs = n.output_buffers();
        assert!(outputs.contains(&"GBuffer<MV,2>"));
        assert!(outputs.contains(&"VisibilityMask×2"));
        assert!(outputs.contains(&"VolumetricAccum"));
    }

    #[test]
    fn stage5_quest3_budget_2_5_ms() {
        let n = Stage5Node;
        assert!((n.quest3_budget_ms() - 2.5).abs() < 1e-6);
    }

    #[test]
    fn stage5_vision_pro_budget_2_0_ms() {
        let n = Stage5Node;
        assert!((n.vision_pro_budget_ms() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn validate_wire_mismatch_returns_slot_in_error() {
        let r =
            TwelveStagePipelineSlot::Slot4WaveSolver.validate_wire(StageRole::SdfRaymarch);
        if let Err(RenderGraphNodeError::SlotRoleMismatch { slot, .. }) = r {
            assert_eq!(slot, TwelveStagePipelineSlot::Slot4WaveSolver);
        } else {
            panic!("expected SlotRoleMismatch");
        }
    }

    #[test]
    fn all_twelve_roles_have_unique_slot_indices() {
        let roles = [
            StageRole::Embodiment,
            StageRole::GazeCollapse,
            StageRole::OmegaFieldUpdate,
            StageRole::WaveSolver,
            StageRole::SdfRaymarch,
            StageRole::KanBrdf,
            StageRole::FractalAmplifier,
            StageRole::CompanionSemantic,
            StageRole::MiseEnAbyme,
            StageRole::ToneMap,
            StageRole::AppSpaceWarp,
            StageRole::XrCompose,
        ];
        let mut indices: Vec<u8> = roles.iter().map(|r| r.canonical_slot_index()).collect();
        indices.sort_unstable();
        for (i, idx) in indices.iter().enumerate() {
            assert_eq!(*idx as usize, i + 1);
        }
    }
}
