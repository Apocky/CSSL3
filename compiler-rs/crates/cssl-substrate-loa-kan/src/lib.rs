//! § cssl-substrate-loa-kan — KAN extensions for LoA scene authoring.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Substrate-S12 advance : LoA-specific extensions over the canonical
//!   `cssl-substrate-kan` runtime that let scene-authors thread per-cell
//!   parametric activation through the Ω-field. Where `cssl-substrate-kan`
//!   provides the **substrate-runtime** primitives (KanNetwork + KanMaterial
//!   + Pattern + AppendOnlyPool), this crate provides the
//!   **LoA-scene-author** surface (see top-level type re-exports below).
//!
//!   - `LoaKanExtension`         per-cell parametric activation function
//!                               (vs the standard MLP fixed-activation). Lets a Sovereign-
//!                               claimed cell carry its own KAN-spline-edge specialization.
//!   - `LoaKanCellModulation`    the bag-of-coefficients applied to a
//!                               cell's downstream-evaluator (Stage-6 BRDF, Stage-4 ψ-
//!                               impedance, creature-pose). Sovereign-handle gated.
//!   - `CompanionAiHook`         opt-in mid-Phase-3 hook that lets a companion-AI register
//!                               its perspective-shift on a cell. Consent-protected per
//!                               spec § STAGE-8.
//!   - `AdaptiveContentScaler`   derives KAN-edge-budget from the Stage-2 fovea-mask +
//!                               KAN-detail-budget. Stage-2 ⊗ Stage-3 glue.
//!
//! § SPEC
//!   - `specs/30_SUBSTRATE_v2.csl` § DEFERRED D-1 — substrate-S12 lifts the
//!     simple-averaging-fallback on LoA-content authoring.
//!   - `specs/32_SIGNATURE_RENDERING.csl` § STAGE-8 (companion-perspective)
//!     + § V.5 (HDC-Bound-Lenia for companion-presence). Companion-AI hook
//!     here is the substrate-side dual of the renderer-side StageRole.
//!   - `specs/33_F1_F6_LANGUAGE_FEATURES.csl` § F4.3 — bake_kan +
//!     comptime-eval. LoA-KAN modulation tables are comptime-bake-friendly.
//!
//! § PRIME-DIRECTIVE
//!   - Per-cell modulation MUST be Sovereign-handle-gated when the cell is
//!     Σ-claimed. A non-authorizing actor cannot mutate the modulation.
//!   - CompanionAiHook is opt-in : a cell without explicit Companion-consent
//!     refuses registration (default-deny).
//!   - AdaptiveContentScaler RESPECTS the Stage-2 KanBudget — it cannot
//!     override foveal-only paths into peripheral-only.
//!
//! § INTEGRATION (substrate-S12 wiring)
//!   - Phase-3 COMPOSE hook reads LoaKanExtension + CompanionAiHook to
//!     assemble per-cell modulated BRDF+impedance vectors.
//!   - Stage-6 (cssl-spectral-render) consumes LoaKanCellModulation alongside
//!     the canonical KanMaterial::spectral_brdf<N> for inverse-rendering
//!     (per specs/33 § COMPOSITION-CASE-STUDIES § CASE-1 : F1+F4+F5).
//!   - Stage-8 (cssl-render-companion-perspective) reads CompanionAiHook
//!     to drive opt-in perspective overlay.
//!
//! § DESIGN-NOTE
//!   This crate intentionally does NOT add new ABI to the FieldCell
//!   72-byte layout — modulation lives in a sparse Morton-keyed overlay
//!   that mirrors the SigmaOverlay pattern (sparse ; default-when-absent ;
//!   ~5% occupancy expected). New cells default to identity-modulation
//!   (no behavior change) ; Sovereign-claimed regions opt-in to custom KAN
//!   parametric activation by minting a [`LoaKanExtension`] per cell.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::field_reassign_with_default)]

pub mod activation;
pub mod adaptive_scaler;
pub mod cfer_iter;
pub mod companion_hook;
pub mod extension;
pub mod kan_band;
pub mod modulation;
pub mod overlay;
pub mod update_rules;

pub use activation::{ActivationKind, ParametricActivation, ACTIVATION_PARAM_MAX};
pub use adaptive_scaler::{AdaptiveContentScaler, KanDetailTier, ScalerError};
pub use cfer_iter::{
    drive_iteration_with_evidence, is_converged, kan_iterate_to_convergence, kan_step,
    parallel_step_serial, CferStepError, DriveReport, EvidenceGlyph, KAN_CONFIDENCE_THRESHOLD,
    KAN_LOOP_MAX_ITER, KAN_LOOP_THRESHOLD, KAN_STEP_EPSILON,
};
pub use companion_hook::{
    AICapPolicy, AICapScope, AuditDecision, AuditEntry, AuditStage, CompanionAiHook,
    CompanionAiKind, CompanionConsent, ConsentDecision, CrossPillarCompanionAi, HookError,
    Mutation, RefuseReason,
};
pub use extension::{LoaKanExtension, LoaKanExtensionError, EXTENSION_VERSION_TAG};
pub use kan_band::{
    decode_spectrum, encode_spectrum, BasisKind, KanBand, KanBandError, KanBandTable, COEF_BOUND,
    KAN_BAND_RANK_DEFAULT, KAN_BAND_RANK_MAX, SPECTRUM_BINS,
};
pub use modulation::{LoaKanCellModulation, ModulationError, MODULATION_DIM};
pub use overlay::{LoaKanOverlay, LoaKanOverlayCell, OverlayError};
pub use update_rules::{
    compose_rules, AbsorptionRule, CanonicalRuleSet, DiffusionRule, EmissionRule,
    InterCellTransportRule, KanUpdateRule, MaterialContext, Neighbor, ScatteringRule,
    UpdateRuleError,
};

/// Crate-version stamp (S12 lift of S11 substrate-foundation).
pub const CSSL_LOA_KAN_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Crate-name stamp.
pub const CSSL_LOA_KAN_CRATE: &str = "cssl-substrate-loa-kan";
/// Substrate-S12 surface version. Bumped when the LoA-KAN public ABI changes.
pub const SUBSTRATE_LOA_KAN_SURFACE_VERSION: u32 = 1;

#[cfg(test)]
mod crate_invariants {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CSSL_LOA_KAN_CRATE, "cssl-substrate-loa-kan");
    }

    #[test]
    fn version_is_nonempty() {
        assert!(!CSSL_LOA_KAN_VERSION.is_empty());
    }

    #[test]
    fn surface_version_at_least_one() {
        // const-comparison ; clippy-allow.
        const _GUARD: () = assert!(SUBSTRATE_LOA_KAN_SURFACE_VERSION >= 1);
    }
}
