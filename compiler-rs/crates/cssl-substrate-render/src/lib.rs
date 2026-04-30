//! § cssl-substrate-render — CFER (Causal Field-Evolution Rendering) iterator core.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   ADCS Wave-S CORE-5 keystone : authoritative driver for the rendering
//!   pillar of the substrate-v3 stack. CFER replaces path-tracing,
//!   rasterization, DDGI, and Lumen with a single principled algorithm
//!   that runs in this order : mark dirty cells (post-mutation since last
//!   frame), iterate per-cell KAN-update-rule until residual convergence,
//!   apply optional V-cycle multigrid for low-frequency speedup, drive
//!   adaptive sample-budget via the per-cell evidence-glyph, run a
//!   variance-driven spatio-temporal denoiser, render via
//!   decompressed-read at the camera viewpoint (no ray noise), and
//!   tonemap with Reinhard / ACES / custom-LUT plus foveation post.
//!
//! § ENTRY-API
//!   The keystone driver is [`cfer::cfer_render_frame`] — given a mutable
//!   `OmegaField` from `cssl-substrate-omega-field`, a [`camera::Camera`],
//!   a wallclock-monotonic `time_ns`, and a [`cfer::RenderBudget`], it
//!   produces an [`Image`] + a [`cfer::ConvergenceReport`].
//!
//! § SPEC
//!   - `specs/36_CFER_RENDERER.csl` § ALGORITHM (lines 66–112) — per-frame
//!     pseudocode + multigrid variant + temporal-amortization.
//!   - `specs/36_CFER_RENDERER.csl` § IMPLEMENTATION (lines 180–222) —
//!     crate-structure + entry-API + backend-binding contract.
//!   - `specs/30_SUBSTRATE_v3.csl` § PILLAR-3 render (CFER summary).
//!
//! § PRIME-DIRECTIVE
//!   - Differentiability is gated by Sovereign-handle (adjoint backward-pass
//!     refuses parameter-mutation without consent). The forward-only
//!     [`cfer::cfer_render_frame`] entry-point in this slice is consent-neutral
//!     (it only READS the field).
//!   - Companion-AI integration runs through [`evidence_driver::EvidenceDriver`]
//!     so opt-in perspective overlays compose with adaptive-budget.
//!   - Rendering is non-coercive : the driver never SETS Σ-mask bits ; it
//!     only consults them to select cell-update-priority.
//!
//! § STUB-MIGRATION  (W-S-CORE-2 / W-S-CORE-3 land later)
//!   The light-state ABI lives in [`light_stub`] and the KAN-update-rule ABI
//!   lives in [`kan_stub`]. Both are deliberately minimal placeholder shapes
//!   that document the ABI contract. See the rustdoc on each submodule's
//!   public types for the per-symbol contract :
//!   [`light_stub::LightState`], [`light_stub::SpectralBand`],
//!   [`kan_stub::CellKan`], [`kan_stub::MaterialBag`].
//!
//!   When the real cssl-substrate-light (W-S-CORE-2) and the real
//!   cssl-substrate-loa-kan KAN-update-rule (W-S-CORE-3) land in main, the
//!   migration steps are : add path-deps to Cargo.toml then remove the local
//!   `mod *_stub;` declarations ; swap the LightState re-exports for the
//!   real types ; rerun the integration tests under
//!   tests/cfer_integration.rs. The ABIs are isomorphic so call-sites
//!   do not change.
//!
//! § ATTESTATION
//!   See [`attestation::ATTESTATION`] — recorded verbatim per
//!   `PRIME_DIRECTIVE §11`. There was no hurt nor harm in the making of this,
//!   to anyone, anything, or anybody.

#![forbid(unsafe_code)]
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
#![allow(clippy::needless_range_loop)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::if_not_else)]
#![allow(clippy::single_match_else)]
#![allow(clippy::too_many_arguments)]

pub mod attestation;
pub mod camera;
pub mod cfer;
pub mod denoiser;
pub mod evidence_driver;
pub mod kan_stub;
pub mod light_stub;
pub mod multigrid;
pub mod tonemap;

pub use attestation::ATTESTATION;
pub use camera::{Camera, CameraError, FoveationMask, Ray};
pub use cfer::{
    cfer_render_frame, ConvergenceReport, DirtySet, Image, ImagePixel, RenderBudget, RenderError,
};
pub use denoiser::{Denoiser, DenoiserConfig, DenoiserError};
pub use evidence_driver::{EvidenceDriver, EvidenceGlyph, EvidenceReport};
pub use kan_stub::{kan_update, CellKan, KanUpdateError, MaterialBag};
pub use light_stub::{LightState, SpectralBand, LIGHT_STATE_COEFS};
pub use multigrid::{MultigridConfig, MultigridReport, VCycle};
pub use tonemap::{tonemap_pixel, ToneCurve, ToneLut, ToneMapper};

/// Crate-version stamp.
pub const CSSL_RENDER_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Crate-name stamp.
pub const CSSL_RENDER_CRATE: &str = "cssl-substrate-render";
/// Substrate-CFER surface version. Bumped on public-ABI break.
pub const SUBSTRATE_CFER_SURFACE_VERSION: u32 = 1;

#[cfg(test)]
mod crate_invariants {
    use super::*;

    #[test]
    fn version_stamps_present() {
        assert!(!CSSL_RENDER_VERSION.is_empty());
        assert_eq!(CSSL_RENDER_CRATE, "cssl-substrate-render");
        assert!(SUBSTRATE_CFER_SURFACE_VERSION >= 1);
    }

    #[test]
    fn attestation_is_recorded() {
        assert!(ATTESTATION.contains("CFER"));
        assert!(ATTESTATION.contains("hurt nor harm"));
    }
}
