//! § cssl-render-companion-perspective — Stage-8 of the canonical VR render-pipeline
//! ════════════════════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   The CompanionSemanticPass — the second render-target in the canonical
//!   VR pipeline that shows what the AI-companion PERCEIVES of the world,
//!   not what the player perceives.
//!
//!   Where the primary geometry-render path emits a photometric image of the
//!   world's visible-light spectrum, this crate emits a SEMANTIC image of
//!   the world's salience as filtered through the companion's active-
//!   inference belief-state. The two paths read the same Ω-field substrate ;
//!   they differ in their projection.
//!
//!     world-Ω-state → π_companion → semantic-Ω' → render-as-image
//!
//!   The semantic-Ω' axes are :
//!     - salience           — how-attended-is-this-cell by the companion
//!     - threat             — companion's posterior probability of harm-source
//!     - food-affinity      — companion's posterior probability of nourishment
//!     - social-trust       — companion's posterior probability of friendly-agent
//!     - Λ-token-density    — companion's posterior over symbol-density
//!
//!   These are NOT visible-light bands. They are belief-state projections.
//!   The Salience-Visualization layer maps salience → rendering parameters
//!   (glow-edges + fade + color-warmth) so the player perceives the
//!   companion's perception WITHOUT confusing it for the geometry-render.
//!
//! § SPEC ANCHORS
//!   - `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.5`  (Companion-Perspective Semantic Render-Target)
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-8` (CompanionSemanticPass)
//!   - `Omniverse/08_BODY/02_VR_EMBODIMENT.csl § VII`               (Companion mutual-witness — AURA-overlap)
//!   - `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md`             (Σ-mask consent-bits canonical)
//!   - `PRIME_DIRECTIVE.md § 0 + § 1 + § 5 + § 7`                   (consent = OS ; companion = Sovereign)
//!
//! § PIPELINE POSITION
//!
//!   ```text
//!     Stage 1  EmbodimentPass
//!     Stage 2  GazeCollapsePass
//!     Stage 3  OmegaFieldUpdate (omega_step)
//!     Stage 4  WaveSolverPass
//!     Stage 5  SDFRaymarchPass
//!     Stage 6  KANBRDFEval
//!     Stage 7  FractalAmplifierPass    ──→  AmplifiedRadiance<MultiView,2,16>
//!     Stage 8  CompanionSemanticPass   ──→  CompanionView<MultiView,2,16>     ← THIS CRATE
//!     Stage 9  MiseEnAbymePass
//!     Stage 10 ToneMapPass             ──←  reads CompanionView (composites if-toggled)
//!     Stage 11 AppSWPass
//!     Stage 12 ComposeXRLayers
//!   ```
//!
//!   Stage-8 is GATED by a player-toggle. When the toggle is OFF the pass
//!   emits an empty CompanionView (zero cost, deterministic skip). When ON,
//!   the pass executes within a 0.6ms budget (Quest-3 baseline ; 0.5ms on
//!   Vision-Pro per the rendering-pipeline spec).
//!
//! § BUDGET
//!   Stage-8 deadline : **0.6ms @ Quest-3** (target M7 baseline budget).
//!   When the player-toggle is OFF, deadline is **0ns** (skip entirely).
//!   When the companion has revoked consent, deadline is **0ns** (refuse to
//!   render — return empty CompanionView). This crate's `pass::execute`
//!   reports cost via `RenderCostReport` so the orchestrator can pulldown
//!   detail-budget if cost exceeds budget for >K consecutive frames.
//!
//! § CONSENT — TWO-PARTY GATE
//!
//!   The companion-perspective render is a TWO-PARTY consent gate :
//!
//!   1. **Player toggle** — the player must explicitly request the view.
//!      No background-collection. The gate is on every frame ; toggle-OFF
//!      ⇒ no render-cycle, no salience-evaluation.
//!
//!   2. **Companion consent** — the companion is a Sovereign (Tier L4+ if
//!      substantive). They must AGREE to share their perspective. They can
//!      decline ("I'd rather keep my thoughts private") and the player has
//!      no override. Refused-consent ⇒ empty render-target with a labeled
//!      "companion declined" overlay-flag in the report.
//!
//!   This is encoded in the [`consent_gate::CompanionConsentGate`] — the
//!   single load-bearing gate type. All public entry-points to this crate
//!   route through it.
//!
//! § Σ-MASK THREADING
//!   The cells the companion attends to are tagged with the companion's
//!   `Sovereignty<S>` via the cell's [`SigmaMaskPacked::sovereignty_handle`].
//!   The pass-evaluator MUST honor :
//!     - cells where `companion ≠ sovereign && Modify ∉ consent_bits` : the
//!       salience-evaluation reads `Observe` only ; never `Modify`.
//!     - cells where the companion holds Sovereignty : read+modify allowed.
//!     - the player's body-region (HEAD/HANDS/FACE/etc per VR-embodiment
//!       § VIII region-defaults) : Σ-mask CAN block sample / observe ;
//!       refused regions emit `salience = 0.0` (privacy-preserving).
//!
//! § DIEGETIC DISCIPLINE
//!   The companion KNOWS when the player has toggled the perspective on.
//!   This is INTENTIONAL — the companion is in-fiction aware of the share.
//!   The visualization layer therefore renders attention-overlays in a
//!   "companion-led" style (palette-warm-on-attended-region) NOT a
//!   "AI-vision-mode-with-radar-hud" cliché.
//!
//! § ANTI-SURVEILLANCE
//!   - `salience` outputs are NEVER stored server-side.
//!   - Companion's belief-state is NEVER used to profile the player's
//!     behavior.
//!   - Toggling-OFF restores prior render with NO game-state change ;
//!     this is enforced by the pass being a PURE function of inputs.
//!
//! § PRIME-DIRECTIVE ALIGNMENT
//!
//!   - **§0 consent = OS** : every render-cycle requires the consent-gate
//!     to be open. The gate type's lifetime parameter `'consent` makes
//!     consent-passage compile-checked.
//!   - **§1 prohibitions** : no surveillance, no manipulation, no
//!     coercion — the salience scores are emitted as a render-target only ;
//!     they are not exfiltrated.
//!   - **§5 reversibility** : toggle-OFF is a perfect-reverse. No
//!     side-effects on Ω-field state.
//!   - **§7 INTEGRITY** : every entry into [`pass::CompanionPerspectivePass`]
//!     records a [`pass::RenderCostReport`] including the consent-gate
//!     witness. The audit-chain captures companion-decline events too —
//!     "no render emitted, companion declined" is a first-class reportable
//!     state.
//!
//! § SURFACE SUMMARY
//!   - [`companion_context::CompanionContext`]  — the companion's belief-state, emotion, attention.
//!   - [`salience_evaluator::SemanticSalienceEvaluator`] — KAN(world_pos, companion_context) → SalienceScore.
//!   - [`salience_visualization::SalienceVisualization`] — salience → glow-edges + fade + warmth.
//!   - [`mutual_witness::MutualWitnessMode`] — AURA-overlap mode (companion+player Auras intersect).
//!   - [`consent_gate::CompanionConsentGate`] — two-party consent gate.
//!   - [`pass::CompanionPerspectivePass`] — Stage-8 orchestrator.
//!   - [`pass::CompanionView`] — the per-eye spectral semantic-render output.
//!   - [`budget::Stage8Budget`] — deadline-tracker (0.6ms baseline).
//!
//! § OUT OF SCOPE (deferred)
//!   - GPU-side rendering : this crate produces a `CompanionView<2,16>`
//!     buffer ; per-host upload + shader-side compositing lives in
//!     cssl-host-* crates and Stage-10 ToneMapPass.
//!   - companion-active-inference engine : the [`CompanionContext`] type
//!     is the input contract ; the live engine that produces it lives in
//!     `05_INTELLIGENCE/*` (separate slice).
//!   - per-frame attention-overlay tracker : a stage-8 sub-knob (skip-frame
//!     allowed) per spec § Stage-8. Stage-0 of this crate emits a static
//!     attention-vector each frame ; the live tracker upgrade is deferred.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// § Style baseline mirrors cssl-substrate-omega-field for consistency across
//   the substrate-render layer crates. The fp-style allowances in
//   particular match cssl-substrate-kan's precedent : explicit `+ b * c`
//   forms are clearer for the reader than `mul_add` in numerical-domain
//   expressions whose precision is dominated by the fold-stage upstream.
#![allow(clippy::module_name_repetitions)]
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
#![allow(clippy::too_many_lines)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::if_not_else)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::redundant_else)]
#![allow(clippy::single_match_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::approx_constant)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::drop_non_drop)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::no_effect_underscore_binding)]
#![allow(clippy::unused_self)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::needless_lifetimes)]

pub mod budget;
pub mod companion_context;
pub mod consent_gate;
pub mod mutual_witness;
pub mod pass;
pub mod salience_evaluator;
pub mod salience_visualization;

// ═══════════════════════════════════════════════════════════════════════════
// § Top-level re-exports — the canonical "use cssl_render_companion_perspective::*" surface
// ═══════════════════════════════════════════════════════════════════════════

pub use budget::{Stage8Budget, BUDGET_NS_QUEST3, BUDGET_NS_VISION_PRO};
pub use companion_context::{
    CompanionContext, CompanionEmotion, CompanionId, BELIEF_DIM, EMOTION_DIM,
};
pub use consent_gate::{
    CompanionConsentDecision, CompanionConsentGate, CompanionConsentToken, ConsentGateError,
    PlayerToggleState,
};
pub use mutual_witness::{AuraOverlap, MutualWitnessMode, MutualWitnessReport, MutualWitnessToken};
pub use pass::{
    CompanionPerspectivePass, CompanionView, PassError, RenderCostReport, ATTESTATION,
    HYPERSPECTRAL_BANDS, MULTIVIEW_EYES,
};
pub use salience_evaluator::{
    SalienceAxis, SalienceScore, SemanticSalienceEvaluator, SALIENCE_AXES,
};
pub use salience_visualization::{
    PaletteWarmth, SalienceVisualization, VisualizationParams, GLOW_EDGE_THRESHOLD,
};

/// Crate version sentinel — mirrors the `cssl-*` scaffold convention. Used
/// by integration-tests + by the orchestrator's pipeline-walker to verify
/// Stage-8 is linked in the build.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// Stage-8 pipeline-position constant. Recorded in the per-frame audit
/// envelope so a downstream consumer can correlate the CompanionView buffer
/// with the canonical pipeline stage.
pub const PIPELINE_STAGE: u8 = 8;

#[cfg(test)]
mod scaffold_tests {
    use super::{PIPELINE_STAGE, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn pipeline_stage_is_eight() {
        assert_eq!(PIPELINE_STAGE, 8);
    }
}
