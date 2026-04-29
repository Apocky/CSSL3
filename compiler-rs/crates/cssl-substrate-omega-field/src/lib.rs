//! § cssl-substrate-omega-field — the canonical Ω-field substrate container.
//! ════════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Assembles the wave-3β foundations (PGA via `cssl-pga` + HDC via `cssl-hdc`
//!   + SigmaMaskPacked via `cssl-substrate-prime-directive::sigma`) into the
//!   canonical 7-facet Ω-field substrate :
//!
//!     - [`FieldCell`] — 72-byte std430-aligned dense cell carrying M, S, P,
//!       Φ + low-Σ.
//!     - [`SparseMortonGrid`] — open-addressing hashtable keyed by 21-bit-per-
//!       axis Morton-encoded cell indices, deterministic across hosts.
//!     - [`OmegaField`] — the canonical container : dense FieldCell grid + Λ +
//!       Ψ + Σ-overlay + Φ-table + 4-tier MERA pyramid.
//!     - [`LegacyTensor`] — strict alias for [`cssl_substrate_omega_tensor::OmegaTensor`]
//!       during the T11-D113..D129 deprecation window, plus a `to_field`
//!       migration adapter that lifts a rank-3 scalar tensor into a fresh
//!       OmegaField.
//!
//! § SPEC
//!   - `Omniverse/02_CSSL/06_SUBSTRATE_EVOLUTION.csl` § IV (canonical types).
//!   - `Omniverse/04_OMEGA_FIELD/00_FACETS.csl.md` (FieldCell layout).
//!   - `Omniverse/04_OMEGA_FIELD/02_STORAGE.csl.md` (sparse Morton grid).
//!   - `Omniverse/04_OMEGA_FIELD/05_DENSITY_BUDGET.csl.md` (cascade + budget).
//!   - `cssl-mir::layout_check::lay0001` (D126 std430 + 72B alignment validator).
//!
//! § PRIME-DIRECTIVE
//!   - Σ-check is non-optional on every [`OmegaField::set_cell`] call.
//!   - The full 16-byte SigmaMaskPacked (overlay) is consulted ; the 4-byte
//!     in-cell low-half is the hot-path cache.
//!   - Audit-chain `epoch` advances on every successful mutation.
//!
//! § INTEGRATION
//!   - Phase-1 COLLAPSE  : T11-D113 (hook = [`OmegaField::phase_collapse`]).
//!   - Phase-2 PROPAGATE : T11-D114 (LBM + KAN-ODE + RC + Λ-stream + Ψ-flow).
//!   - Phase-3 COMPOSE   : T11-D116 (operadic sheaf-glue).
//!   - Phase-4 COHOMOLOGY: T11-D117 (incremental persistent-homology).
//!   - Phase-5 AGENCY    : T11-D120 (consent-aggregate + reversibility).
//!   - Phase-6 ENTROPY   : T11-D125 (RG-flow + 9-conservation-laws).
//!
//! § ATTESTATION
//!   See [`attestation::ATTESTATION`] — recorded verbatim per
//!   `PRIME_DIRECTIVE §11`.

// We use a single unsafe block in `field_cell::tests` for the layout-
// offsets verification. The wider crate is sound + boring ; we therefore
// `allow(unsafe_code)` rather than `forbid` so the test compiles without
// per-fn pragmas. All other `unsafe` is rejected by review-discipline.
#![allow(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
// Pedantic clippy noise-suppression to match the workspace lint baseline
// + the cssl-pga / cssl-hdc / cssl-substrate-prime-directive precedent.
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
#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::float_cmp)] // f32 == 0.0 is the canonical "classical-cell" test.
#![allow(clippy::field_reassign_with_default)] // FieldCell builder pattern preferred.
#![allow(clippy::manual_range_contains)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::if_not_else)]
#![allow(clippy::map_unwrap_or)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::unusual_byte_groupings)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::single_match_else)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::comparison_chain)]
#![allow(clippy::len_zero)]
#![allow(clippy::comparison_to_empty)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::or_fun_call)]
#![allow(clippy::needless_continue)]
// § Additional allowances post-W3γ-merge :
// - many_single_char_names : math/coordinate test bindings (x/y/z + g/h/k/etc).
// - suboptimal_flops : explicit FMA-vs-mul-add is a precision concern outside clippy's scope.
// - bool_to_int_with_if : load-bearing branchless conversion in soft-float emit.
// - match_same_arms : MissPolicy::AnalyticSDF intentionally falls through to the same body
//   as another arm pending wired-up implementation in a later slice.
// - iter_without_into_iter : SparseMortonGrid intentionally exposes only `iter()` ;
//   `IntoIterator` would require deciding between owned vs borrowed iter API.
// - explicit_iter_loop : `comps.iter()` is more readable than `&comps` in field-byte tests.
// - borrow_as_ptr : `&cell as *const FieldCell` is the canonical layout-test pattern.
#![allow(clippy::many_single_char_names)]
#![allow(clippy::suboptimal_flops)]
#![allow(clippy::bool_to_int_with_if)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::iter_without_into_iter)]
#![allow(clippy::explicit_iter_loop)]
#![allow(clippy::borrow_as_ptr)]
// `#[allow(unused)]` fields/items in scaffold modules.
#![allow(dead_code)]

pub mod attestation;
pub mod field_cell;
pub mod lambda;
pub mod legacy;
pub mod mera;
pub mod morton;
pub mod omega_field;
pub mod phi_table;
pub mod psi;
pub mod sigma_overlay;
pub mod sparse_grid;

pub use field_cell::{FieldCell, M_PAYLOAD_MASK, M_TAG_PGA, PATTERN_HANDLE_NULL};
pub use lambda::{LambdaSimpleOverlay, LambdaToken, SimpleLambdaSlot};
pub use legacy::{LegacyTensor, LegacyTensorMigration, MigrationError, ScalarFacet};
pub use mera::{MeraPyramid, MERA_TIER_COUNT};
pub use morton::{
    CellTier, MortonError, MortonKey, MORTON_AXIS_MASK, MORTON_AXIS_MAX, MORTON_AXIS_WIDTH,
    MORTON_PAYLOAD_WIDTH, MORTON_SENTINEL_BIT,
};
pub use omega_field::{MutationError, OmegaField, StepOutcome, StepPhase};
pub use phi_table::{Pattern, PhiHandle, PhiTable, PHI_HANDLE_NULL};
pub use psi::{PsiCell, PsiOverlay};
pub use sigma_overlay::{SigmaOverlay, SigmaOverlayCell};
pub use sparse_grid::{
    CollisionStats, GridError, MissPolicy, OmegaCellLayout, SparseMortonGrid,
    DEFAULT_GRID_CAPACITY, MAX_PROBE_STEPS,
};

/// Crate-version stamp ; recorded in audit + telemetry.
pub const CSSL_OMEGA_FIELD_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Crate-name stamp.
pub const CSSL_OMEGA_FIELD_CRATE: &str = "cssl-substrate-omega-field";

#[cfg(test)]
mod crate_invariants {
    use super::*;

    #[test]
    fn crate_name_matches_package() {
        assert_eq!(CSSL_OMEGA_FIELD_CRATE, "cssl-substrate-omega-field");
    }

    #[test]
    fn version_is_nonempty() {
        assert!(!CSSL_OMEGA_FIELD_VERSION.is_empty());
    }

    // ── Load-bearing invariants : 72B FieldCell, 16B SigmaMaskPacked ─

    #[test]
    fn field_cell_is_72_bytes() {
        assert_eq!(core::mem::size_of::<FieldCell>(), 72);
    }

    #[test]
    fn field_cell_aligned_to_8() {
        assert_eq!(core::mem::align_of::<FieldCell>(), 8);
    }

    #[test]
    fn cell_tier_count_matches_mera() {
        assert_eq!(MERA_TIER_COUNT, CellTier::all().len());
    }

    #[test]
    fn morton_axis_width_is_21() {
        assert_eq!(MORTON_AXIS_WIDTH, 21);
    }

    #[test]
    fn morton_payload_width_is_63() {
        assert_eq!(MORTON_PAYLOAD_WIDTH, 63);
    }

    #[test]
    fn step_phases_count_six() {
        assert_eq!(StepPhase::all().len(), 6);
    }

    #[test]
    fn omega_field_construction_is_air_default() {
        let f = OmegaField::new();
        assert_eq!(f.dense_cell_count(), 0);
        assert_eq!(f.epoch(), 0);
    }
}
