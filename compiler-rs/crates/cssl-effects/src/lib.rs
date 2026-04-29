//! CSSLv3 effect system — 28 built-in effects + sub-effect discipline + Prime-Directive
//! banned-composition checker.
//!
//! § SPEC : `specs/04_EFFECTS.csl` (full 28-effect set) + `specs/11_IFC.csl` (Sensitive
//!   domain labels) + PRIME_DIRECTIVE.md (F5 structural encoding).
//!
//! § SCOPE (T4-phase-1)
//!   This crate is the effect **registry + discipline checker** — no compilation of
//!   effects into evidence-records, no Xie+Leijen transform, no runtime handler
//!   installation. Those live in T4-phase-2.
//!
//!   What it provides :
//!     - [`BuiltinEffect`] — a dense enum covering every effect in `specs/04`.
//!     - [`EffectRegistry`] — name-keyed lookup with arg-shape validation.
//!     - [`sub_effect_check`] — caller-row must cover every effect of a callee-row
//!       (tighter budgets flow into looser ones per §§ 04 coercion rules).
//!     - [`banned_composition`] — rejects Prime-Directive-banned combinations
//!       (e.g., `{Sensitive<"weapon">} ⊎ {IO, Net}` without `Privilege<Kernel>`).
//!     - [`SubstrateEffect`] / [`SubstrateEffectRow`] / [`try_compose`] — the
//!       Substrate-axis effect labels and their composition-table (S8-H4 / T11-D92).
//!       Stable diagnostic codes [`ConflictReason`] (`EFR0001`..`EFR0010`) cover
//!       the canonical conflict shapes (see `specs/30_SUBSTRATE.csl § EFFECT-ROWS`).
//!
//! § NEXT (T4-phase-2)
//!   - Evidence-record data-types synthesized from effect-decls.
//!   - `HIR → HIR+evidence` Xie+Leijen transform.
//!   - Linear × handler one-shot enforcement (§§ 12 R8).
//!   - Handler-installation analysis (perform X → requires handler for X in scope).

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
// `caller` / `callee` are too similar for clippy's default taste but are semantically
// the correct domain-pair names for effect-row discipline ; rejecting them would
// force less-readable alternatives.
#![allow(clippy::similar_names)]

pub mod banned;
pub mod discipline;
pub mod registry;
pub mod substrate;

pub use banned::{
    banned_composition, banned_composition_with_domains, check_telemetry_no_raw_path,
    is_raw_path_type, BannedReason, SensitiveDomain, RAW_PATH_TYPE_NAMES,
};
pub use discipline::{
    classify_coercion, sub_effect_check, CoercionRule, EffectRef, SubEffectError,
};
pub use registry::{
    BuiltinEffect, DischargeTiming, EffectArgShape, EffectCategory, EffectMeta, EffectRegistry,
    BUILTIN_METADATA,
};
pub use substrate::{
    compose_with_advisories, try_compose, ConflictReason, RowContext, SubstrateEffect,
    SubstrateEffectRow,
};

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
