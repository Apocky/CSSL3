//! § cssl-spectral-render — hyperspectral KAN-BRDF render Stage-6
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Stage-6 of the canonical render pipeline. Consumes RayHit surfaces from
//!   D116 SDF-raymarch (Stage-1) + post-fractal-amplified positions from D119
//!   (Stage-5) ; produces 16-band [`SpectralRadiance`] output that Stage-7
//!   (radiance-cascade GI), Stage-8 (post-FX), and Stage-10 (CIE-XYZ →
//!   display tonemap) consume.
//!
//!   The renderer itself never converts to RGB mid-pipeline — this is the
//!   foundational discipline of `07_AESTHETIC/03_SPECTRAL_PATH_TRACING.csl §
//!   II` ("RGB-conversion @ tonemap-step ONLY ⊗ never-mid-pipeline"). RGB
//!   appears exactly once : in [`tristimulus::tonemap_aces2`] at the very end
//!   of the pipeline.
//!
//! § SPEC ANCHORS
//!   - `Omniverse/07_AESTHETIC/03_SPECTRAL_PATH_TRACING.csl.md` — hero-wavelength
//!     sampling, MIS, dispersion, iridescence, fluorescence, ACES-2 tonemap.
//!   - `Omniverse/07_AESTHETIC/07_KAN_RUNTIME_SHADING.csl.md` — KAN-network
//!     shape variants, cooperative-matrix dispatch (deferred), 16-band
//!     spectral-BRDF call site signature, persistent-tile residency.
//!   - `Omniverse/07_AESTHETIC/00_EXOTICISM_PRINCIPLES.csl` — V.2 path :
//!     Renaissance palette + perceptual coherence.
//!   - `compiler-rs/crates/cssl-substrate-kan/src/kan_material.rs` —
//!     `KanMaterial::spectral_brdf<N>` variant.
//!   - `compiler-rs/crates/cssl-autodiff/src/jet.rs` — `Jet<T, N>` for
//!     differentiable BRDF eval (inverse-rendering mode).
//!
//! § SURFACE SUMMARY
//!   - **[`band::SpectralBand`]** — 16-band wavelength sampling. The canonical
//!     band table covers 380-780 nm (visible) plus 4 NIR bands and 2 UV bands.
//!   - **[`radiance::SpectralRadiance`]** — hero-wavelength + accompanying-N
//!     spectral storage. 16-band container per `07_AES/03 § II`.
//!   - **[`hero_mis::HeroWavelengthMIS`]** — Manuka-style hero + 4-8
//!     accompanying samples with PDF preservation.
//!   - **[`kan_brdf::KanBrdfEvaluator`]** — per-fragment evaluator that wires
//!     `KanMaterial::spectral_brdf<16>` to `(view_dir, light_dir, λ_hero) →
//!     reflectance(λ-band[16])`.
//!   - **[`iridescence::IridescenceModel`]** — thin-film interference +
//!     dispersion (Newton-rings, peacock-feather, oil-on-water). Activates
//!     when `m_embed.axis_15 > τ_aniso` per `07_AES/07 § VIII`.
//!   - **[`fluorescence::Fluorescence`] / [`fluorescence::Phosphorescence`]**
//!     — excitation→emission spectral remap for Λ-tokens.
//!   - **[`tristimulus::SpectralTristimulus`]** — spectral → CIE-XYZ → display
//!     RGB ACES-2 tonemap (Stage-10).
//!   - **[`csf::CsfPerceptualGate`]** — contrast-sensitivity-function-aware
//!     shading per Mantiuk-2024.
//!   - **[`pipeline::SpectralRenderStage`]** — the Stage-6 entry-point that
//!     orchestrates per-fragment shading.
//!   - **[`cost::PerFragmentCost`]** — compile/run cost model + Quest-3
//!     budget validation.
//!
//! § STAGE-6 BUDGET (Quest-3 reference hardware)
//!   Per `07_AES/07 § VII` :
//!   - per-fragment cost target ≤ 1.8 ms @ Quest-3 90 Hz frame budget 11.1 ms
//!   - 16-band CoopMatrix dispatch hits ~0.63 ms @ foveated 1080p
//!   - iridescence + fluorescence add ~0.30 ms when active
//!   - Stage-6 total budget : 1.8 ms / frame (16% of frame)
//!   - MUST integrate cleanly with D116 (Stage-1) + D119 (Stage-5).
//!
//! § PRIME-DIRECTIVE ALIGNMENT
//!   Per `07_AES/07 § XV+XVI` attestation : the spectral renderer is
//!   pure-eval, mutates no state, consumes only scene-content (MaterialCoord)
//!   + camera-driven view/light. It does NOT consume Σ-facet (sovereignty
//!   mask). Replayable + deterministic by construction.

#![forbid(unsafe_code)]
// § Style allowances for spectral / numeric code at the floor :
// - many_single_char_names : λ, x, y, z, w, k, n are textbook spectral names.
// - suboptimal_flops : explicit fma vs sum-of-products is precision-sensitive
//   in the colorimetry path ; we don't want clippy second-guessing.
// - cast_precision_loss : the wavelength + band tables convert `usize ↔ f32`
//   with known-bounded values.
// - manual_clamp : the hero-wavelength clamp uses explicit min/max for
//   readability vs `f32::clamp`.
// - needless_range_loop : per-band/per-axis indexed loops are clearer for
//   spectral math than iterator chains and match the shader-side intent.
// - manual_memcpy : explicit-loop preserves intent in band-table updates.
// - float_cmp : tests that exercise exact float values check explicit
//   constants set by the constructors (no arithmetic involved).
// - similar_names : nearby spectral variables (lambda, lambda_emit, etc.).
// - uninlined_format_args : test failure messages built before format-arg
//   inlining was canon.
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::manual_memcpy)]
#![allow(clippy::float_cmp)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::unused_self)]
#![allow(clippy::needless_bool)]
#![allow(clippy::redundant_else)]

pub mod band;
pub mod cost;
pub mod csf;
pub mod fluorescence;
pub mod hero_mis;
pub mod iridescence;
pub mod kan_brdf;
pub mod pipeline;
pub mod radiance;
pub mod tristimulus;

// § Top-level re-exports for the canonical surface — match the slice's
//   user-facing API per `07_AES/03 + 07_AES/07`.
pub use band::{
    BandTable, SpectralBand, BAND_COUNT, BAND_VISIBLE_END, BAND_VISIBLE_START, IR_BAND_COUNT,
    UV_BAND_COUNT, VISIBLE_BAND_COUNT,
};
pub use cost::{CostTier, PerFragmentCost, QUEST3_FRAME_BUDGET_MS, STAGE6_BUDGET_MS};
pub use csf::{CsfPerceptualGate, MantiukCsfParams};
pub use fluorescence::{Fluorescence, Phosphorescence, StokesShift};
pub use hero_mis::{HeroSample, HeroWavelengthMIS, MisWeights};
pub use iridescence::{IridescenceModel, ThinFilmStack, ANISOTROPY_THRESHOLD};
pub use kan_brdf::{KanBrdfEvaluator, ShadingFrame};
pub use pipeline::{FragmentInput, SpectralRenderStage, StageHandle, STAGE_INDEX};
pub use radiance::{HeroAccompaniment, SpectralRadiance, ACCOMPANIMENT_MAX};
pub use tristimulus::{Cie1931Xyz, DisplayPrimaries, SpectralTristimulus, SrgbColor};

/// § Crate version sentinel — bumped when the public surface contract changes
///   in a way that invalidates downstream stage-6 consumers.
pub const SPECTRAL_RENDER_SURFACE_VERSION: u32 = 1;

/// § Stage index in the canonical 12-stage render pipeline. Stage-6 is the
///   spectral-shading-eval slot ; Stages 1-5 are SDF-march + GI-bounce +
///   fractal-amplify ; Stages 7-12 are GI-cascade + post-FX + tonemap.
pub const STAGE6: u32 = 6;

#[cfg(test)]
mod tests {
    use super::*;

    /// § The crate's surface-version sentinel is the canonical "1" for the
    ///   Stage-6 floor.
    #[test]
    fn surface_version_is_one() {
        assert_eq!(SPECTRAL_RENDER_SURFACE_VERSION, 1);
    }

    /// § Stage-6 is canonical index 6 in the pipeline.
    #[test]
    fn stage6_index() {
        assert_eq!(STAGE6, 6);
        assert_eq!(STAGE_INDEX, 6);
    }

    /// § Re-exports compile cleanly. This test exists to catch surface drift
    ///   when modules rename internal types.
    #[test]
    fn surface_smoke() {
        let _ = BAND_COUNT;
        let _ = ACCOMPANIMENT_MAX;
        let _ = ANISOTROPY_THRESHOLD;
        let _ = STAGE6_BUDGET_MS;
        let _ = QUEST3_FRAME_BUDGET_MS;
    }
}
