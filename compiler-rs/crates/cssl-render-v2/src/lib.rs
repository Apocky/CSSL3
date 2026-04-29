//! § cssl-render-v2 — canonical 12-stage SDF-native render pipeline.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The canonical render pipeline replacement for the deprecated `cssl-render`
//!   triangle-rasterizer (15%-match → full-rewrite per the wave-9 re-audit
//!   verdict). The full pipeline is 12 stages (Stage-1 Embodiment through
//!   Stage-12 ComposeXRLayers) ; each stage lands in its own slice.
//!
//!   - **Stage-5 SdfRaymarchPass** (T11-D116) — analytic-SDF-native sphere-
//!     tracer that walks the [`cssl_substrate_omega_field`] `OmegaField`
//!     directly + skips entire MERA-summary blocks via a hierarchical bound
//!     test. Replaces the legacy `cssl-render` triangle-rasterizer (preserved
//!     behind the `cssl-render-legacy` feature-flag for cold-tier export).
//!   - **Stage-9 MiseEnAbymePass** (T11-D122) — recursive-witness rendering
//!     for mirrors, reflective creature-eyes, and still-water surfaces with
//!     KAN-confidence attenuation. HARD-bounded at depth-5 with KAN-driven
//!     early-termination.
//!
//! § SPEC ANCHORS (load-bearing)
//!   - `Omniverse/07_AESTHETIC/01_SDF_NATIVE_RENDER.csl.md` — § II ray pipeline,
//!     § III SDF assembly, § IV ray-marching with MERA-skip, § V acceleration
//!     structure (MERA = BVH-replacement), § VIII anti-pattern table.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl` — § III Stage-5 spec
//!     (inputs, outputs, compute, foveation, multi-view, effect-row), § V budget
//!     table (2.5 ms @ Quest-3), § VII foveation discipline (VRS Tier-2 / FDM /
//!     Metal-DynRQ), § VIII multi-view discipline (1 cmd-buf, 2 view-instances) ;
//!     § Stage-9 — pipeline-position, budget (≤ 0.8ms @ Quest-3, ≤ 0.6ms @
//!     Vision-Pro), bounded-recursion declaration, effect-row.
//!   - `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.6` —
//!     mise-en-abyme as one of the SIX immutable novelty paths. The
//!     "no-shipped-game-uses" gate for path-V.6 is the contract this
//!     module honors : true recursive ray-cast at-mirror-hit + KAN-
//!     confidence-attenuation, NOT planar-reflection / SSR / cube-map fallback.
//!   - `Omniverse/01_AXIOMS/10_OPUS_MATH.csl` — PGA + MERA + KAN composition.
//!   - `Omniverse/01_AXIOMS/13_DENSITY_SOVEREIGNTY.csl` — 5×10⁶ visible-cells
//!     budget + per-stage graceful-degrade levers.
//!   - `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md` — every recursion
//!     bounce honors per-cell `Σ-mask` consent ; `bounded-recursion` is a
//!     direct AGENCY-INVARIANT corollary (no runtime explosion possible).
//!   - `Omniverse/05_INTELLIGENCE/02_F1_AUTODIFF.csl` — F1 backward-diff for
//!     surface normals (central-differences are FORBIDDEN).
//!
//! § SURFACE SUMMARY
//!   - **SDF primitives** ([`sdf::AnalyticSdf`], [`sdf::SdfComposition`]) — PGA-
//!     derived sphere / box / plane / capsule + smooth-min / hard-min / xor
//!     composition primitives. Lipschitz-bound is preserved-by-construction.
//!   - **SdfRaymarchPass** ([`raymarch::SdfRaymarchPass`]) — hierarchical-SDF
//!     sphere-tracer with cone-marching (50→100 fps recursive per Pointers-Gone-
//!     Wild '26 derivation). Walks the unified-SDF + body-presence-field +
//!     fovea-mask conditioning per Stage-5 spec.
//!   - **MeraSkipDispatcher** ([`mera_skip::MeraSkipDispatcher`]) — MERA-pyramid
//!     hierarchical traversal. Large steps when the ray is inside a coarse
//!     summary region with a known distance-bound ; bisection refine when the
//!     ray approaches the surface (`d < HIT_EPSILON`). O(log N) traversal.
//!   - **BackwardDiffNormals** ([`normals::BackwardDiffNormals`]) — F1-autodiff
//!     `apply_bwd` wrapper that returns the surface-normal as
//!     `bwd_diff(SDF)(p).d_p.normalize()`. Central-differences are EXPLICITLY
//!     FORBIDDEN ; emitting them is a compile-time grep-gate failure (cited at
//!     [`normals::CentralDiffForbidden`]).
//!   - **MeshletHybridFallback** ([`meshlet::MeshletHybridFallback`]) — optional
//!     mesh-shader path for static UI / fonts / debug-overlay. The SDF path is
//!     always-canonical for world-geometry ; the meshlet path is the cold-tier
//!     export-format used by debug-tools that need to inspect raw triangles.
//!   - **KanAmplifierHook** ([`kan_amplifier::KanAmplifierHook`]) — D119
//!     amplifier integration trait. The amplifier is invoked at sub-pixel
//!     detail-resolve to add KAN-derived fractal detail per pixel. If D119 has
//!     not landed, [`kan_amplifier::MockAmplifier`] provides a passthrough
//!     stub that satisfies the trait without producing detail.
//!   - **FoveatedMultiViewRender** ([`foveation::FoveatedMultiViewRender`]) —
//!     viewCount = 1 | 2 | 4 | N + VRS-Tier-2 / FDM / Metal-DynRQ foveation-mask
//!     integration. Per-eye shading-rate is selected from the [`foveation::FoveaMask`]
//!     each pixel queries.
//!   - **Render-graph node-spec** ([`pipeline::RenderGraphNode`],
//!     [`pipeline::Stage5Node`]) — node-spec compatible with the canonical
//!     12-stage pipeline-driver in `pipeline.rs`.
//!   - **MiseEnAbymePass** ([`mise_en_abyme::MiseEnAbymePass`]) — Stage-9
//!     recursive-witness pass for mirrors / Companion-eyes / water-surfaces.
//!     Bounded-recursion HARD-cap=5 + KAN-confidence-attenuation. See module
//!     docs for the full surface ([`mise_en_abyme::WitnessCompositor`],
//!     [`mise_en_abyme::MirrorSurface`], [`mise_en_abyme::KanConfidence`],
//!     [`mise_en_abyme::CompanionEyeWitness`]).
//!
//! § INTEGRATION POINTS (downstream-slice readiness)
//!   - **D118 spectral-KAN-BRDF** : the [`spectral_hook::SpectralRadianceHook`]
//!     trait carries the `SpectralRadiance<MultiView, 2, 16>` output upward to
//!     Stage-6 (KANBRDFEval). Stage-5 emits a [`gbuffer::GBuffer`] +
//!     [`volumetric::VolumetricAccum`] ; Stage-6 reads them.
//!   - **D119 fractal-tess** : the [`fractal_hook::FractalAmplifierHandle`]
//!     trait is the integration-point for sub-pixel-detail KAN amplifier. The
//!     concrete implementation lands in D119 ; this crate exposes the trait
//!     surface + a [`kan_amplifier::MockAmplifier`] passthrough stub.
//!   - **Stage-7 amplifier → Stage-9** : the mise-en-abyme pass consumes the
//!     `AmplifiedRadiance<2, 16>` output of Stage-7 (the primary frame to
//!     recurse into) plus `Ω.next.SDF + Ω.next.M-facet` from the omega-field.
//!     Mirror-detection re-uses the same PGA Plane primitive that Stage-5 SDF
//!     raymarch uses, so mirror-detection is unified.
//!
//! § BUDGET DISCIPLINE
//!   - Stage-5 : 2.5 ms @ Quest-3 ; 2.0 ms @ Vision-Pro. The
//!     [`budget::Stage5Budget`] type tracks per-region march-step counts +
//!     surface-hit fractions. The [`cost_model::QuestThreeCostModel`] +
//!     matching `VisionProCostModel` project ms-cost @ 5 × 10⁶ visible-cells ;
//!     [`budget::BudgetValidator::check_quest3`] flags configurations that
//!     would blow the 2.5 ms ceiling.
//!   - Stage-9 : 0.8 ms @ Quest-3 ; 0.6 ms @ Vision-Pro. Hit by depth-cap=5,
//!     KAN-confidence falling below `MIN_CONFIDENCE = 0.10` around depth-3 in
//!     the canonical roughness-driver, and per-bounce work being tiny against
//!     Stage-5/7 budgets. See [`mise_en_abyme::MiseEnAbymeCostModel`].
//!
//! § PRIME-DIRECTIVE
//!   - The renderer reads SDF + body-presence + fovea-mask. It NEVER reads raw
//!     gaze-data — Stage-2 (GazeCollapsePass) is the consent-gate ; Stage-5
//!     consumes the post-gate FoveaMask. See [`foveation::FoveaMask::source_consented`].
//!   - The renderer never logs world-data to telemetry. Frame-time + per-region
//!     step-counts are emitted via the [`budget::Stage5BudgetTelemetry`] surface
//!     (no cell-level data crosses the boundary).
//!   - Σ-mask consent on body-presence reads is checked in
//!     [`raymarch::SdfRaymarchPass::march_with_body_conditioning`] ; refused
//!     reads fall back to plain SDF march (not crash).
//!   - **§I.4 sovereignty** (mise-en-abyme) : creature-eye reflections require
//!     the Sovereign-Φ to be PRESENT in the region containing the eye. Empty
//!     `MiseEnAbymeRadiance` + `Stage9Event::EyeRedacted` telemetry rather
//!     than panic so a player's gaze does not become a reverse-surveillance
//!     vector.
//!   - **§V anti-surveillance** (mise-en-abyme) : the recursion explicitly does
//!     NOT cross region-boundaries via mirror chaining. The `RegionBoundary`
//!     predicate consulted at every bounce ensures a mirror in region-A cannot
//!     "look into" region-B if region-B's Σ-mask forbids surveillance from
//!     region-A. Compile-time + runtime checked via `SigmaPolicy::AntiSurveil`.
//!   - **bounded-recursion AGENCY-INVARIANT** : `RECURSION_DEPTH_HARD_CAP = 5`
//!     is a `const` ; the runtime cannot exceed it even if the KAN-confidence
//!     reports `continue=true` past depth 5. This satisfies effect-row
//!     `BoundedRecursion<5>` in the rendering-pipeline spec.
//!
//! § ATTESTATION
//!   See [`attestation::ATTESTATION`] — recorded verbatim per
//!   `PRIME_DIRECTIVE §11`. Slice T11-D116 establishes the canonical attestation
//!   for the SDF-native crate ; T11-D122 contributes the mise-en-abyme module-
//!   level attestation block ([`attestation::SLICE_ID`] +
//!   [`attestation::SPEC_CITATIONS`]).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
// § Style allowances — same set as cssl-substrate-omega-field for consistency.
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::similar_names)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::if_not_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::doc_link_with_quotes)]
#![allow(clippy::single_match_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_continue)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::bool_to_int_with_if)]
#![allow(clippy::let_underscore_must_use)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::unused_self)]
#![allow(clippy::imprecise_flops)]
#![allow(clippy::pub_underscore_fields)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::needless_pass_by_ref_mut)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::doc_markdown)]
// Test fixtures often start with `Default::default()` then tweak one field for
// clarity ; the alternate struct-literal form would force naming all defaults
// at every test site.
#![allow(clippy::needless_update)]
// ProbeResult::Hit carries KanMaterial which is large by spec ; the alternative
// (boxing the hit) introduces an unwanted alloc on the hot recursion path.
#![allow(clippy::large_enum_variant)]
#![allow(dead_code)]

pub mod attestation;
pub mod budget;
pub mod camera;
pub mod cost_model;
pub mod foveation;
pub mod fractal_hook;
pub mod gbuffer;
pub mod kan_amplifier;
pub mod mera_skip;
pub mod meshlet;
pub mod mise_en_abyme;
pub mod multiview;
pub mod normals;
pub mod pipeline;
pub mod raymarch;
pub mod sdf;
pub mod spectral_hook;
pub mod stage_5;
pub mod volumetric;

pub use attestation::{ATTESTATION, ATTESTATION_AUTHOR, ATTESTATION_TAG, SLICE_ID, SPEC_CITATIONS};
pub use budget::{
    BudgetError, BudgetValidator, Stage5Budget, Stage5BudgetTelemetry, Stage5DegradeLever,
};
pub use camera::{EyeCamera, EyeOffsetMillimeters, ProjectionParams};
pub use cost_model::{CostModel, MsCost, QuestThreeCostModel, ShadingRateZone, VisionProCostModel};
pub use foveation::{
    FoveaMask, FoveatedMultiViewRender, FoveationMethod, FoveationZones, ShadingRate,
};
pub use fractal_hook::{FractalAmplifierHandle, FractalDetailRequest, NoFractalAmplifier};
pub use gbuffer::{GBuffer, GBufferLayout, GBufferRow, MultiViewGBuffer};
pub use kan_amplifier::{KanAmplifierHook, KanAmplifierInput, KanAmplifierOutput, MockAmplifier};
pub use mera_skip::{MeraSkipDispatcher, MeraSkipResult, SummaryBound};
pub use meshlet::{MeshletDescriptor, MeshletHybridFallback, MeshletKind};
pub use mise_en_abyme::{
    CompanionEyeWitness, KanConfidence, MirrorRaymarchProbe, MirrorSurface, MiseEnAbymeCostModel,
    MiseEnAbymePass, MiseEnAbymeRadiance, RecursionDepthBudget, RegionBoundary, Stage9Error,
    Stage9Event, WitnessCompositor, BANDS_PER_EYE, EYES_PER_FRAME, MIN_CONFIDENCE,
    RECURSION_DEPTH_HARD_CAP, STAGE9_BUDGET_QUEST3_US, STAGE9_BUDGET_VISION_PRO_US,
};
pub use multiview::{MultiViewConfig, ViewCount, ViewInstance};
pub use normals::{
    BackwardDiffNormals, CentralDiffForbidden, NormalEstimate, SdfFunction, SurfaceNormal,
};
pub use pipeline::{
    RenderGraphNode, RenderGraphNodeError, Stage5Node, StageRole, TwelveStagePipelineSlot,
};
pub use raymarch::{
    HitEpsilon, MaxDistance, MaxSteps, RayHit, RaymarchConfig, RaymarchError, SdfRaymarchPass,
};
pub use sdf::{AnalyticSdf, AnalyticSdfKind, LipschitzBound, SdfComposition};
pub use spectral_hook::{SpectralRadianceHook, SpectralRadianceTransport};
pub use stage_5::{Stage5Driver, Stage5DriverError, Stage5DriverOutput, Stage5Inputs};
pub use volumetric::{VolumetricAccum, VolumetricSample};

/// Crate-version stamp ; recorded in audit + telemetry.
pub const CSSL_RENDER_V2_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Crate-name stamp.
pub const CSSL_RENDER_V2_CRATE: &str = "cssl-render-v2";
/// Stage-5 budget @ Quest-3 in milliseconds (06_RENDERING_PIPELINE § V).
pub const STAGE_5_QUEST3_BUDGET_MS: f32 = 2.5;
/// Stage-5 budget @ Vision-Pro in milliseconds (06_RENDERING_PIPELINE § V).
pub const STAGE_5_VISION_PRO_BUDGET_MS: f32 = 2.0;
/// Density-sovereignty visible-cells threshold @ M7
/// (Axiom 13 § I + 01_SDF_NATIVE_RENDER § VII).
pub const M7_VISIBLE_CELLS_BUDGET: u64 = 5_000_000;

#[cfg(test)]
mod crate_invariants {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CSSL_RENDER_V2_CRATE, "cssl-render-v2");
    }

    #[test]
    fn version_is_nonempty() {
        assert!(!CSSL_RENDER_V2_VERSION.is_empty());
    }

    #[test]
    fn quest3_budget_is_2_5_ms() {
        assert!((STAGE_5_QUEST3_BUDGET_MS - 2.5).abs() < 1e-6);
    }

    #[test]
    fn vision_pro_budget_is_2_0_ms() {
        assert!((STAGE_5_VISION_PRO_BUDGET_MS - 2.0).abs() < 1e-6);
    }

    #[test]
    fn m7_cells_budget_is_5m() {
        assert_eq!(M7_VISIBLE_CELLS_BUDGET, 5_000_000);
    }

    #[test]
    fn vision_pro_budget_strictly_lower_than_quest3() {
        assert!(STAGE_5_VISION_PRO_BUDGET_MS < STAGE_5_QUEST3_BUDGET_MS);
    }

    #[test]
    fn recursion_hard_cap_is_five() {
        // § Spec § Stage-9 declares HARD cap ≤ 5 ; this is load-bearing.
        assert_eq!(RECURSION_DEPTH_HARD_CAP, 5);
    }

    #[test]
    fn bands_per_eye_match_hyperspectral() {
        // § Stage-9 output is `MiseEnAbymeRadiance<2, 16>` per spec.
        assert_eq!(BANDS_PER_EYE, 16);
        assert_eq!(EYES_PER_FRAME, 2);
    }

    #[test]
    fn budget_microseconds_match_spec() {
        // § Spec § Stage-9.budget : 0.8ms @ Quest-3 ; 0.6ms @ Vision-Pro.
        assert_eq!(STAGE9_BUDGET_QUEST3_US, 800);
        assert_eq!(STAGE9_BUDGET_VISION_PRO_US, 600);
    }
}
