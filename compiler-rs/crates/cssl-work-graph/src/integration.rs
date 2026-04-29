//! Integration with sibling slices D116 (SDF-raymarch) + D118 (spectral-BRDF).
//!
//! § DESIGN
//!   The work-graph fusion contract @ `06_RENDERING_PIPELINE.csl` § X
//!   declares the canonical fused-stage pipeline :
//!
//! ```text
//!     [WaveSolver-node]   (stage 4, this slice)
//!         |
//!         v
//!     [SDFRaymarch-node]  (stage 5, D116)  <-- multi-view dispatch
//!         |
//!         v
//!     [KANBRDF-node]      (stage 6, D118)  <-- fan-out per-fragment
//!         |
//!         v
//!     [FractalAmp-node]   (stage 7, this slice)
//!         |
//!         v
//!     [Output to next-stage staging buffer]
//! ```
//!
//!   This module exposes builder-helpers that wire D116 + D118 nodes
//!   correctly without callers having to remember the canonical I/O
//!   layout. The actual D116 / D118 crates are produced by sibling
//!   dispatches (cssl-render-v2 / cssl-spectral-render) ; here we model
//!   their *node-shape* contracts so a Schedule that includes them is
//!   compile-time-validated.

use crate::backend::FeatureMatrix;
use crate::dispatch::DispatchArgs;
use crate::error::Result;
use crate::node::WorkGraphNode;
use crate::schedule::Schedule;
use crate::stage_layout::StageId;
use crate::WorkGraphBuilder;

/// D116 SDF-raymarch integration node.
///
/// Compiled here to encode the canonical contract :
///   - inputs : `omega-next-sdf`, `omega-next-mera`, `body-presence-field`,
///              `fovea-mask`, `psi-light-16`, `camera-pair`
///   - outputs : `g-buffer-mv-2`, `visibility-mask`, `first-hit-distance`,
///               `volumetric-accum`
///   - dispatch : foveated grid (typically 64x64 thread-groups @ 4×4 px each
///               = 256x256-pixel-tile @ multi-view × 2 eyes)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D116Integration {
    /// Foveated grid size (pixels-tile / shading-rate-aware).
    pub tile_grid: DispatchArgs,
    /// Estimated cost (us). Spec § V : 2.5ms @ Quest-3, 2.0ms @ Vision-Pro.
    pub est_cost_us: u32,
    /// DXIL/SPIR-V shader-blob tag the host resolves.
    pub shader_tag: String,
}

impl D116Integration {
    /// Quest-3 baseline (2.5 ms = 2500 µs).
    #[must_use]
    pub fn quest_3() -> Self {
        Self {
            tile_grid: DispatchArgs::new(64, 64, 1),
            est_cost_us: 2_500,
            shader_tag: "d116_sdf_raymarch_quest3".to_owned(),
        }
    }

    /// Vision-Pro baseline (2.0 ms = 2000 µs).
    #[must_use]
    pub fn vision_pro() -> Self {
        Self {
            tile_grid: DispatchArgs::new(96, 96, 1),
            est_cost_us: 2_000,
            shader_tag: "d116_sdf_raymarch_vision_pro".to_owned(),
        }
    }

    /// 120Hz-PC baseline (1.5 ms = 1500 µs ; uses shrunk fovea).
    #[must_use]
    pub fn pc_120hz() -> Self {
        Self {
            tile_grid: DispatchArgs::new(48, 48, 1),
            est_cost_us: 1_500,
            shader_tag: "d116_sdf_raymarch_120hz".to_owned(),
        }
    }

    /// Build the work-graph node for this integration.
    #[must_use]
    pub fn into_node(self, id: impl Into<crate::node::NodeId>) -> WorkGraphNode {
        WorkGraphNode::compute(id, StageId::SdfRaymarch, self.tile_grid)
            .with_shader_tag(self.shader_tag)
            .with_cost_us(self.est_cost_us)
    }
}

/// D118 spectral-BRDF integration node.
///
/// Compiled here to encode the canonical contract :
///   - inputs : `g-buffer-mv-2`, `visibility-mask`, `psi-light-16`,
///              `omega-next-p-facet`, `omega-next-m-facet`
///   - outputs : `spectral-radiance-mv-2-16`, `albedo-buffer`,
///               `roughness-buffer`
///   - dispatch : fan-out per-fragment (matched to D116 g-buffer tile-grid)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D118Integration {
    /// Per-fragment fan-out grid.
    pub fragment_grid: DispatchArgs,
    /// Estimated cost (us). Spec § V : 1.8ms @ Quest-3, 1.5ms @ Vision-Pro.
    pub est_cost_us: u32,
    /// DXIL/SPIR-V shader-blob tag.
    pub shader_tag: String,
    /// Spectral-band count (typically 16).
    pub band_count: u32,
}

impl D118Integration {
    /// Quest-3 baseline (1.8 ms ; 16 bands).
    #[must_use]
    pub fn quest_3() -> Self {
        Self {
            fragment_grid: DispatchArgs::new(64, 64, 1),
            est_cost_us: 1_800,
            shader_tag: "d118_kan_brdf_quest3".to_owned(),
            band_count: 16,
        }
    }

    /// Vision-Pro baseline (1.5 ms ; 16 bands).
    #[must_use]
    pub fn vision_pro() -> Self {
        Self {
            fragment_grid: DispatchArgs::new(96, 96, 1),
            est_cost_us: 1_500,
            shader_tag: "d118_kan_brdf_vision_pro".to_owned(),
            band_count: 16,
        }
    }

    /// 120Hz-PC baseline (8 bands ; spec § V.5 budget-pulldown level-4).
    #[must_use]
    pub fn pc_120hz_8band() -> Self {
        Self {
            fragment_grid: DispatchArgs::new(48, 48, 1),
            est_cost_us: 1_000,
            shader_tag: "d118_kan_brdf_120hz_8band".to_owned(),
            band_count: 8,
        }
    }

    /// Build the work-graph node for this integration.
    #[must_use]
    pub fn into_node(self, id: impl Into<crate::node::NodeId>) -> WorkGraphNode {
        WorkGraphNode::compute(id, StageId::KanBrdfEval, self.fragment_grid)
            .with_shader_tag(self.shader_tag)
            .with_cost_us(self.est_cost_us)
    }
}

/// Build the canonical fused-stages-4-7 schedule for the given platform.
///
/// Produces the DAG :
///   WaveSolver → D116(SDFRaymarch) → D118(KANBRDF) → FractalAmplifier
///
/// The label, dispatch grids, and shader tags are pulled from the
/// platform-specific integration constructors.
pub fn build_canonical_quest_3_schedule(features: FeatureMatrix) -> Result<Schedule> {
    WorkGraphBuilder::new()
        .with_label("canonical-quest-3")
        .auto_select(features)
        .with_budget(crate::cost_model::FrameBudget::hz_90_vr())
        .node(
            WorkGraphNode::compute(
                "WaveSolver",
                StageId::WaveSolver,
                DispatchArgs::new(8, 8, 1),
            )
            .with_shader_tag("wave_solver_quest3")
            .with_cost_us(1_500),
        )?
        .node(
            D116Integration::quest_3()
                .into_node("SDFRaymarch")
                .with_input("WaveSolver"),
        )?
        .node(
            D118Integration::quest_3()
                .into_node("KANBRDF")
                .with_input("SDFRaymarch"),
        )?
        .node(
            WorkGraphNode::compute(
                "FractalAmplifier",
                StageId::FractalAmplifier,
                DispatchArgs::new(64, 64, 1),
            )
            .with_input("KANBRDF")
            .with_shader_tag("fractal_amplifier_quest3")
            .with_cost_us(1_200),
        )?
        .build()
}

/// Build the canonical fused-stages-4-7 schedule for 120Hz-PC.
pub fn build_canonical_120hz_schedule(features: FeatureMatrix) -> Result<Schedule> {
    WorkGraphBuilder::new()
        .with_label("canonical-120hz")
        .auto_select(features)
        .with_budget(crate::cost_model::FrameBudget::hz_120())
        .node(
            WorkGraphNode::compute(
                "WaveSolver",
                StageId::WaveSolver,
                DispatchArgs::new(4, 4, 1),
            )
            .with_shader_tag("wave_solver_120hz")
            .with_cost_us(750),
        )?
        .node(
            D116Integration::pc_120hz()
                .into_node("SDFRaymarch")
                .with_input("WaveSolver"),
        )?
        .node(
            D118Integration::pc_120hz_8band()
                .into_node("KANBRDF")
                .with_input("SDFRaymarch"),
        )?
        .node(
            WorkGraphNode::compute(
                "FractalAmplifier",
                StageId::FractalAmplifier,
                DispatchArgs::new(48, 48, 1),
            )
            .with_input("KANBRDF")
            .with_shader_tag("fractal_amplifier_120hz")
            .with_cost_us(800),
        )?
        .build()
}

#[cfg(test)]
mod tests {
    use super::{
        build_canonical_120hz_schedule, build_canonical_quest_3_schedule, D116Integration,
        D118Integration,
    };
    use crate::backend::FeatureMatrix;
    use crate::stage_layout::StageId;

    #[test]
    fn d116_quest_3_targets_2500us() {
        let i = D116Integration::quest_3();
        assert_eq!(i.est_cost_us, 2_500);
    }

    #[test]
    fn d116_pc_120hz_smaller_grid() {
        let q = D116Integration::quest_3();
        let p = D116Integration::pc_120hz();
        assert!(p.tile_grid.total_groups() < q.tile_grid.total_groups());
    }

    #[test]
    fn d118_default_16_bands() {
        let i = D118Integration::quest_3();
        assert_eq!(i.band_count, 16);
    }

    #[test]
    fn d118_120hz_8band_pulldown() {
        let i = D118Integration::pc_120hz_8band();
        assert_eq!(i.band_count, 8);
    }

    #[test]
    fn d116_into_node_stage_5() {
        let n = D116Integration::quest_3().into_node("X");
        assert_eq!(n.stage, StageId::SdfRaymarch);
    }

    #[test]
    fn d118_into_node_stage_6() {
        let n = D118Integration::quest_3().into_node("X");
        assert_eq!(n.stage, StageId::KanBrdfEval);
    }

    #[test]
    fn canonical_quest_3_4_node_chain() {
        let s = build_canonical_quest_3_schedule(FeatureMatrix::ultimate()).unwrap();
        assert_eq!(s.len(), 4);
        let names: Vec<_> = s.order().iter().map(|n| n.as_str().to_owned()).collect();
        assert_eq!(
            names,
            vec![
                "WaveSolver".to_string(),
                "SDFRaymarch".to_string(),
                "KANBRDF".to_string(),
                "FractalAmplifier".to_string(),
            ]
        );
    }

    #[test]
    fn canonical_120hz_within_budget() {
        let s = build_canonical_120hz_schedule(FeatureMatrix::ultimate()).unwrap();
        assert!(s.within_budget());
        assert!(s.est_cost_us() <= 8_333);
    }

    #[test]
    fn canonical_quest_3_within_90hz_vr_budget() {
        let s = build_canonical_quest_3_schedule(FeatureMatrix::ultimate()).unwrap();
        assert!(s.within_budget());
    }

    #[test]
    fn canonical_quest_3_label_carries() {
        let s = build_canonical_quest_3_schedule(FeatureMatrix::ultimate()).unwrap();
        assert_eq!(s.label(), Some("canonical-quest-3"));
    }
}
