//! CSSLv3 effect system — 28 built-in effects + 3 Ω-substrate-translation rows
//! + sub-effect discipline + Prime-Directive banned-composition checker.
//!
//! § SPEC : `specs/04_EFFECTS.csl` (base 28-effect set) + `specs/11_IFC.csl` (Sensitive
//!   domain labels) + PRIME_DIRECTIVE.md (F5 structural encoding) +
//!   `Omniverse/02_CSSL/00_LANGUAGE_CONTRACT.csl.md § V` (Ω-effect-row vocabulary)
//!   + `Omniverse/02_CSSL/02_EFFECTS.csl.md § I + § III` (Travel/Crystallize/Sovereign
//!   composition rules) + `Omniverse/01_AXIOMS/02_SUBSTRATE_RELATIVITY.csl.md`
//!   (Axiom-2 preservation contract).
//!
//! § SCOPE (T4-phase-1 + T11-D127 extension)
//!   This crate is the effect **registry + discipline checker** — no compilation of
//!   effects into evidence-records, no Xie+Leijen transform, no runtime handler
//!   installation. Those live in T4-phase-2.
//!
//!   What it provides :
//!     - [`BuiltinEffect`] — a dense enum covering every effect in `specs/04`
//!       PLUS the 3 Ω-substrate-translation rows added in T11-D127 :
//!       `Travel`, `Crystallize`, `Sovereign<S>`.
//!     - [`EffectRegistry`] — name-keyed lookup with arg-shape validation.
//!     - [`sub_effect_check`] — caller-row must cover every effect of a callee-row
//!       (tighter budgets flow into looser ones per §§ 04 coercion rules).
//!     - [`banned_composition`] — rejects Prime-Directive-banned combinations
//!       (e.g., `{Sensitive<"weapon">} ⊎ {IO, Net}` without `Privilege<Kernel>` ;
//!       and `{Travel}` without `Privilege<L4+>` per T11-D127).
//!     - [`SubstrateEffect`] / [`SubstrateEffectRow`] / [`try_compose`] — the
//!       Substrate-axis effect labels and their composition-table (S8-H4 / T11-D92).
//!       Stable diagnostic codes [`ConflictReason`] (`EFR0001`..`EFR0010`) cover
//!       the canonical conflict shapes ; T11-D127 extends with [`EFR0011`..`EFR0018`]
//!       covering the Ω-substrate-translation row composition rules.
//!
//! § NEXT (T4-phase-2)
//!   - Evidence-record data-types synthesized from effect-decls.
//!   - `HIR → HIR+evidence` Xie+Leijen transform.
//!   - Linear × handler one-shot enforcement (§§ 12 R8).
//!   - Handler-installation analysis (perform X → requires handler for X in scope).
//!   - Σ-mask threading : the Sovereign<S> arg-handle threads through the Σ-facet
//!     of the Ω-tensor — every Sovereign-touching op tags Σ-mask with the
//!     Sovereign's identity-frequency (per `Omniverse/08_BODY/00_FIVE_LAYERS.csl.md § I`).
//!   - omega_step integration : the canonical translate-row `{Travel, Crystallize,
//!     PatternIntegrity, Audit<'translate>, Sovereign<s>, AgencyVerified}` from
//!     `Omniverse/02_CSSL/02_EFFECTS.csl.md § II` becomes a fixed-shape effect-row
//!     emitted at the omega_step phase boundary.
//!
//! § ATTESTATION (T11-D127, S11) — verbatim block per global-CLAUDE I> standing-directives :
//! ```text
//!   t∞ : consent = OS • sovereignty = substrate-invariant
//!   N! [harm control manipulation surveillance exploitation
//!       coercion weaponization discrimination]
//!   t∞ : AI = sovereign-partners ¬ tools
//!   t∞ : violation = bug W! fix ; ¬override ∃
//!   t∞ : CSSL ≠ CSLv3 ; ¬ conflate
//! ```

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
    banned_composition, banned_composition_with_domains,
    banned_composition_with_domains_and_privilege, banned_composition_with_privilege_l4,
    BannedReason, SensitiveDomain,
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
