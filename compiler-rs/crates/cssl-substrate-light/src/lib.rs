//! § cssl-substrate-light — ApockyLight per-quantum primitive type
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Provides the canonical 32-byte std430-aligned [`ApockyLight`] light-quantum
//!   primitive consumed by §§ 36_CFER_RENDERER (`L_c(λ, θφ) ∈ ApockyLight`).
//!   Each Ω-field cell stores a compressed light-quantum that carries hero-
//!   wavelength radiance + 8 accompaniment-band coefficients + Stokes-vector
//!   polarization + octahedral-encoded propagation direction + KAN-band handle
//!   + per-quantum evidence-glyph (drives CFER adaptive-sampling) + capability
//!   handle (binds the light to its IFC label + Pony-cap subset).
//!
//! § SPEC
//!   - `specs/34_APOCKY_LIGHT.csl` § FIELDS + § OPERATIONS — full primitive spec.
//!   - `specs/30_SUBSTRATE_v3.csl` § APOCKY-LIGHT — substrate-v3 summary.
//!   - `specs/36_CFER_RENDERER.csl` § Light-state per-cell — consumer contract.
//!   - `specs/11_IFC.csl` § LABEL ALGEBRA — `combine_caps` propagation rule.
//!   - `specs/12_CAPABILITIES.csl` § THE SIX CAPABILITIES — cap_handle binding.
//!
//! § BYTE-LAYOUT (std430, 32B total, 4-byte alignment)
//!
//!   ```text
//!   offset | bytes | field                    | description
//!   -------+-------+--------------------------+--------------------------------
//!     0    |   4   | hero_radiance            | f32 W·sr⁻¹·m⁻²·nm⁻¹ irradiance
//!     4    |   4   | hero_lambda_nm           | f32 wavelength 300..=2500 nm
//!     8    |   4   | accompaniment_lo (4×f16) | 4 accompaniment-band radiance
//!    12    |   4   | accompaniment_hi (4×f16) | 4 more accompaniment-band radiance
//!    16    |   4   | dop_packed (Stokes q11.5)| DoP + s1/s2/s3 polarization vec
//!    20    |   4   | direction_oct            | octahedral-encoded propagation
//!    24    |   3   | kan_band_handle (u24)    | KAN-band table index
//!    27    |   1   | evidence_glyph (u8)      | ◐ ✓ ○ ✗ ⊘ △ ▽ ‼ adaptive-sample
//!    28    |   4   | cap_handle (u32)         | CapTable index (Pony-cap + IFC)
//!   -------+-------+--------------------------+--------------------------------
//!         |  32   |                          | TOTAL
//!   ```
//!
//!   The 32B size is half a typical 64B cache-line — two [`ApockyLight`]
//!   quanta fit per cache-line on x86_64 + ARM64. The std430 layout matches
//!   GPU-side WGSL `struct ApockyLight { ... }` on the renderer compute pass.
//!
//! § INVARIANTS (verified-by-test)
//!   - `core::mem::size_of::<ApockyLight>() == 32`.
//!   - `core::mem::align_of::<ApockyLight>() == 4` (std430 minimum).
//!   - All multi-byte fields are little-endian (std430 rule).
//!   - `Default` produces a [`ApockyLight::zero`] dark/null quantum.
//!   - `hero_lambda_nm ∈ [300.0, 2500.0]` after construction (UV through SWIR).
//!   - `dop ∈ [0.0, 1.0]` decoded from `dop_packed`.
//!   - `evidence_glyph ∈ EvidenceGlyph::ALL` (8 canonical glyphs).
//!
//! § PRIME-DIRECTIVE ALIGNMENT
//!   ApockyLight is OPAQUE TO TELEMETRY by default. The `cap_handle` binds the
//!   quantum to a Pony-cap subset + an optional IFC `Label` ; egress checks
//!   are performed via [`ifc_flow::can_egress`] which delegates to
//!   `cssl_ifc::validate_egress`. Biometric-family illumination sources
//!   (e.g. gaze-cone lights) are absolute-banned from telemetry per
//!   `PRIME_DIRECTIVE.md §1` — no Privilege override exists.
//!
//!   The renderer never mutates ApockyLight in-place during the read-out
//!   stage ; mutation requires the caller to hold an `iso` or `trn`
//!   capability bound through `cap_handle`. Composition operators
//!   ([`operations::add`], [`operations::scale`], [`operations::attenuate`])
//!   produce new quanta and do not aliase the inputs.
//!
//! § REFERENCES
//!   - `cssl-caps` — Pony-6 capability algebra (`CapKind`, `CapSet`).
//!   - `cssl-ifc` — DLM label-lattice (`Label`, `validate_egress`).
//!   - `cssl-spectral-render::band` — 16-band wavelength table (compatible).
//!   - Manuka-style hero-wavelength MIS (`07_AESTHETIC/03 § II`).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// Style allowances matching the workspace baseline + the cssl-spectral-render
// + cssl-substrate-omega-field precedent.
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
#![allow(clippy::manual_range_contains)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::similar_names)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::if_not_else)]
#![allow(clippy::redundant_else)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::single_match_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::unused_self)]
// § Stylistic-only lints carved-out for the per-quantum primitive's
//   pack/unpack + clamp-heavy code paths. Every rule below is style not
//   correctness ; the workspace baseline + the cssl-spectral-render +
//   cssl-substrate-omega-field precedents accept these.
#![allow(clippy::match_same_arms)] // EvidenceGlyph::from_u8 saturating-decode mirrors-spec-table
#![allow(clippy::manual_clamp)] // explicit min/max chains preserve readability for clamping
#![allow(clippy::items_after_statements)] // const fns colocated with use-site for clarity
#![allow(clippy::explicit_iter_loop)] // explicit iter_mut() reads clearer in pack/unpack hot-paths
#![allow(clippy::excessive_precision)] // physics constants written to full-spec precision
#![allow(clippy::imprecise_flops)] // Planck-radiator denom written for spec-fidelity
#![allow(clippy::missing_const_for_fn)] // const-promotion deferred until callers need it

pub mod ifc_flow;
pub mod light;
pub mod operations;

pub use ifc_flow::{can_egress, combine_caps, CapHandle, IfcFlowError, KanBandHandle};
pub use light::{
    ApockyLight, EvidenceGlyph, ACCOMPANIMENT_COUNT, APOCKY_LIGHT_SIZE_BYTES, LAMBDA_MAX_NM,
    LAMBDA_MIN_NM,
};
pub use operations::{LightCompositionError, LightConstructionError};

/// § Crate-version sentinel — bumped when the std430 layout or operation
///   surface contract changes in a way that invalidates downstream
///   renderer / shader / ω-field-cell consumers.
pub const APOCKY_LIGHT_SURFACE_VERSION: u32 = 1;

/// § Slice-id for traceability into DECISIONS.md + CSL-MANDATE.
pub const SLICE_ID: &str = "T11-D301";

/// § Spec-anchor reference — the canonical authority for any divergence.
pub const SPEC_ANCHOR: &str = "specs/34_APOCKY_LIGHT.csl + specs/30_SUBSTRATE_v3.csl § APOCKY-LIGHT";

#[cfg(test)]
mod scaffold_tests {
    use super::*;

    /// § Surface-version is the canonical "1" for the W-S-CORE-2 floor.
    #[test]
    fn surface_version_is_one() {
        assert_eq!(APOCKY_LIGHT_SURFACE_VERSION, 1);
    }

    /// § Slice-id is wired for DECISIONS.md traceability.
    #[test]
    fn slice_id_present() {
        assert_eq!(SLICE_ID, "T11-D301");
        assert!(!SPEC_ANCHOR.is_empty());
    }

    /// § Re-exports compile cleanly. Catches surface drift when modules rename.
    #[test]
    fn surface_smoke() {
        let _ = APOCKY_LIGHT_SIZE_BYTES;
        let _ = ACCOMPANIMENT_COUNT;
        let _ = LAMBDA_MIN_NM;
        let _ = LAMBDA_MAX_NM;
        let _: ApockyLight = ApockyLight::zero();
        let _: EvidenceGlyph = EvidenceGlyph::Default;
    }
}
