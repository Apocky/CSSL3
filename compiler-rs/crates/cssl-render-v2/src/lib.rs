//! § cssl-render-v2 — canonical 12-stage SDF-native render pipeline (Stage-9 slice)
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   This crate hosts the CSSLv3 canonical render pipeline replacement for
//!   the deprecated `cssl-render` (15%-match → full-rewrite per the wave-9
//!   re-audit verdict). The full pipeline is 12 stages (Stage-1 Embodiment
//!   through Stage-12 ComposeXRLayers) ; each stage lands in its own slice
//!   T11-D113..T11-D125. **This slice (T11-D122) authors Stage-9 :
//!   MiseEnAbymePass** — recursive-witness rendering for mirrors,
//!   reflective creature-eyes, and still-water surfaces with KAN-confidence
//!   attenuation.
//!
//! § SPEC ANCHORS
//!   - `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl § V.6` —
//!     mise-en-abyme as one of the SIX immutable novelty paths. The
//!     "no-shipped-game-uses" gate for path-V.6 is the contract this
//!     module honors : true recursive ray-cast at-mirror-hit + KAN-
//!     confidence-attenuation, NOT planar-reflection / SSR / cube-map
//!     fallback.
//!   - `Omniverse/07_AESTHETIC/06_RENDERING_PIPELINE.csl § Stage-9` —
//!     pipeline-position, budget (≤ 0.8ms @ Quest-3, ≤ 0.6ms @ Vision-Pro),
//!     bounded-recursion declaration, effect-row.
//!   - `Omniverse/01_AXIOMS/04_AGENCY_INVARIANT.csl.md` — every recursion
//!     bounce honors per-cell `Σ-mask` consent ; `bounded-recursion` is a
//!     direct AGENCY-INVARIANT corollary (no runtime explosion possible).
//!   - `PRIME_DIRECTIVE.md § I.4 (sovereignty) + § V (anti-surveillance)` —
//!     the spec's flagged-rows for path-V.6 : creature-eye is a Sovereign-
//!     attribute, and "no-cross-region spy mirror" is compile-time-checked.
//!
//! § THESIS (mise-en-abyme as a substrate-property)
//!   The image contains the image. Recursively. With every bounce attenuated
//!   by a learned confidence — when no-more-information-here, the recursion
//!   terminates. No infinite-loop possible : the depth is HARD-bounded at
//!   `RECURSION_DEPTH_HARD_CAP = 5`, AND the KAN-confidence drives early
//!   termination via `KanConfidence::should_continue(depth, ...) -> bool`.
//!   In Companion-eyes, this realizes the diegetic property "you see
//!   yourself in their eyes" ; in mirrors, it realizes the mise-en-abyme
//!   art-history motif (Van-Eyck Arnolfini-Portrait, Velázquez Las-Meninas).
//!
//! § PUBLIC SURFACE (this slice)
//!   - [`mise_en_abyme::MiseEnAbymePass`]    — top-level pass struct + impl
//!   - [`mise_en_abyme::RecursionDepthBudget`]— bounded-recursion contract
//!   - [`mise_en_abyme::WitnessCompositor`]  — per-frame attenuated compose
//!   - [`mise_en_abyme::MirrorSurface`]      — SDF + KanMaterial detector
//!   - [`mise_en_abyme::KanConfidence`]      — KAN-attenuation evaluator
//!   - [`mise_en_abyme::MiseEnAbymeRadiance`]— per-eye 16-band output buffer
//!   - [`mise_en_abyme::CompanionEyeWitness`]— Companion-iris path-5 link
//!   - [`mise_en_abyme::Stage9Error`]        — RecursionDepthExhausted /
//!     BudgetExceeded / SovereigntyViolation / etc.
//!
//! § PRIME-DIRECTIVE-ALIGNMENT
//!   - **§I.4 sovereignty** : creature-eye reflections require the Sovereign-
//!     Φ to be PRESENT in the region containing the eye. This is checked at
//!     `MiseEnAbymePass::reflect_creature_eye` entry — if the cell's
//!     `Σ-mask.sovereignty_handle` is set but the Sovereign is not present,
//!     the function returns an empty `MiseEnAbymeRadiance` and emits a
//!     `Stage9Event::EyeRedacted` telemetry record (NOT a panic — the
//!     compositor handles the absence gracefully so a player's gaze does
//!     not become a reverse-surveillance vector).
//!   - **§V anti-surveillance** : the recursion explicitly does NOT cross
//!     region-boundaries via mirror chaining. The `RegionBoundary` predicate
//!     consulted at every bounce ensures a mirror in region-A cannot
//!     "look into" region-B if region-B's Σ-mask forbids surveillance from
//!     region-A. Compile-time + runtime checked via `SigmaPolicy::AntiSurveil`.
//!   - **bounded-recursion AGENCY-INVARIANT** : `RECURSION_DEPTH_HARD_CAP = 5`
//!     is a `const` ; the runtime cannot exceed it even if the KAN-confidence
//!     reports `continue=true` past depth 5. This satisfies effect-row
//!     `BoundedRecursion<5>` in the rendering-pipeline spec.
//!
//! § INTEGRATION (T11-D116 SDF-raymarch + Stage-7 amplifier)
//!   Stage-9 consumes `AmplifiedRadiance<2, 16>` from Stage-7 (the primary
//!   frame to recurse into) and `Ω.next.SDF + Ω.next.M-facet` from the
//!   omega-field. Per spec, mirror-detection re-uses the **same** PGA
//!   plane-SDF primitive that Stage-5 SDF raymarch (T11-D116) uses to
//!   detect surfaces — there is no separate mirror-detection pipeline.
//!   The reflected camera position is computed via PGA-sandwich-reflection
//!   on the mirror's tangent-plane (a Plane primitive in `cssl-pga`).
//!
//!   Recursion at depth-d :
//!     1. detect mirror at primary hit (M-facet `mirrorness > threshold`)
//!     2. compute reflected camera (PGA Plane sandwich)
//!     3. SHALLOW-raymarch from reflected camera — this slice provides a
//!        scalar-depth `RaymarchProbe` rather than a full Stage-5-replay,
//!        because Stage-5 lives in a sibling slice (T11-D116/W4-02). The
//!        probe interface is `MirrorRaymarchProbe` — it can be wired to a
//!        full Stage-5 walker by the orchestrator slice T11-D125.
//!     4. KAN-confidence-attenuation : `KanConfidence(depth, cone, atmos)`
//!        → `(continue, attenuation)`. If `continue == false`, recursion
//!        terminates here at depth-d.
//!     5. accumulate `MiseEnAbymeRadiance += attenuation * sub_radiance`
//!     6. recurse if `continue == true && depth + 1 < RECURSION_DEPTH_HARD_CAP`.
//!
//! § PERFORMANCE
//!   The 0.8ms-per-frame budget at Quest-3 is hit by :
//!     - depth-cap=5 keeps the recursive call tree shallow ;
//!     - the typical KAN-confidence falls below `MIN_CONFIDENCE = 0.10`
//!       around depth-3 in the canonical roughness-driver (`roughness > 0.15`),
//!       so most mirrors trim to depth 2-3 in practice ;
//!     - per-bounce work is `O(SHALLOW-raymarch + KAN-eval + accumulate)`
//!       which is tiny against Stage-5/7 budgets.
//!   Cost-model under `MiseEnAbymeCostModel::estimate_us(...)` returns the
//!   modeled microseconds — used by the runtime cost-budget gate.
//!
//! § ATTESTATION
//!   This crate's authoring honored PRIME_DIRECTIVE §11 (CREATOR-ATTESTATION).
//!   Verbatim attestation block in [`attestation::ATTESTATION`].

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
// § Style allowances scoped to this slice — the broader workspace lints already
//   allow most of these at the workspace level ; we re-state them here so the
//   crate's local clippy run is identical regardless of workspace inheritance.
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
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::similar_names)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::single_match_else)]
#![allow(clippy::field_reassign_with_default)]
// Test fixtures often start
// with `Default::default()` then
// tweak one field for clarity ;
// the alternate struct-literal
// form would force naming all
// defaults at every test site.
#![allow(clippy::needless_update)]
#![allow(clippy::too_many_arguments)]
// Stage-9 recurse_at_mirror takes a deliberately
// wide arg list because the integration-contract
// for orchestrator-wiring needs each surface
// argument explicit (mirror, view-pose, base
// radiance, probe, companion-witness, provider).
#![allow(clippy::large_enum_variant)]
// ProbeResult::Hit carries KanMaterial which is
// large by spec ; the alternative (boxing the
// hit) introduces an unwanted alloc on the hot
// recursion path.
#![allow(clippy::derivable_impls)]
// Default impls written manually for surface clarity
// — auto-derive would force the field defaults to
// be type-default rather than the spec-canonical
// values that the manual impl establishes.
#![allow(dead_code)]

pub mod attestation;
pub mod mise_en_abyme;

pub use mise_en_abyme::{
    CompanionEyeWitness, KanConfidence, MirrorRaymarchProbe, MirrorSurface, MiseEnAbymeCostModel,
    MiseEnAbymePass, MiseEnAbymeRadiance, RecursionDepthBudget, RegionBoundary, Stage9Error,
    Stage9Event, WitnessCompositor, BANDS_PER_EYE, EYES_PER_FRAME, MIN_CONFIDENCE,
    RECURSION_DEPTH_HARD_CAP, STAGE9_BUDGET_QUEST3_US, STAGE9_BUDGET_VISION_PRO_US,
};

/// Crate-version stamp — recorded in audit + telemetry.
pub const CSSL_RENDER_V2_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Crate-name stamp.
pub const CSSL_RENDER_V2_CRATE: &str = "cssl-render-v2";

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
