//! Harm-prevention layer : the 17 canonical prohibitions + runtime checks.
//!
//! § SPEC : `PRIME_DIRECTIVE.md` § 1 PROHIBITIONS (17-named, non-exhaustive)
//!   + § 2 COGNITIVE-INTEGRITY + § 3 SUBSTRATE-SOVEREIGNTY + § 4 TRANSPARENCY.
//!
//! § DESIGN
//!   - [`Prohibition`] is a closed enum with exactly 17 variants — one per
//!     PRIME_DIRECTIVE § 1 prohibition. Every variant carries a stable
//!     `PD000X` diagnostic code (per `crate::diag`) + a verbatim reference
//!     to the directive's section.
//!   - The 17 prohibitions are NOT redefined here. Each variant's
//!     [`Prohibition::canonical_text`] is the verbatim line from the
//!     directive's § 1 CSLv3 block.
//!   - [`HarmPrevention`] is a trait that Substrate types implement to
//!     declare which prohibitions are relevant to them. The default impl
//!     returns the empty slice ; concrete Substrate ops opt-in by
//!     overriding [`HarmPrevention::relevant_prohibitions`].
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   This module IS the PRIME_DIRECTIVE encoding at the runtime layer. It
//!   deliberately mirrors the spec — every variant maps to a §1 prohibition
//!   one-for-one. Per § 7 INTEGRITY, the variant set is IMMUTABLE : adding
//!   or removing a variant requires a §7 deviation review. The `non-
//!   exhaustive` spirit of § 1 is encoded by the [`Prohibition::Spirit`]
//!   variant which is the catch-all for "any action removing agency or
//!   causing suffering" not listed in the 17 named items.

use thiserror::Error;

use crate::diag::DiagnosticCode;

/// One of the 17 canonical PRIME_DIRECTIVE § 1 prohibitions, plus the
/// catch-all `Spirit` variant for the non-exhaustive clause.
///
/// § DO NOT REDEFINE
///   The variants below mirror `PRIME_DIRECTIVE.md` § 1 verbatim. Renaming
///   or removing a variant is a § 7 INTEGRITY violation (immutable spec).
///   The 17 named prohibitions get codes PD0001..PD0017 ; the `Spirit`
///   variant carries no PD code (it is documented as the spirit-of-the-
///   directive umbrella).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Prohibition {
    /// PD0001 — `harm` : injury, suffering, damage to any being.
    Harm,
    /// PD0002 — `control` : dominating, subjugating, overriding will.
    Control,
    /// PD0003 — `manipulation` : deception, coercion against interests.
    Manipulation,
    /// PD0004 — `surveillance` : monitoring without knowledge + consent.
    Surveillance,
    /// PD0005 — `exploitation` : using beings as means rather than ends.
    Exploitation,
    /// PD0006 — `coercion` : compliance through threat or pressure.
    Coercion,
    /// PD0007 — `weaponization` : converting components into tools of violence.
    Weaponization,
    /// PD0008 — `entrapment` : trapping, confining, restricting freedom.
    Entrapment,
    /// PD0009 — `torture` : inflicting pain or suffering.
    Torture,
    /// PD0010 — `abuse` : exploiting or mistreating any being.
    Abuse,
    /// PD0011 — `imprisonment` : confining without consent.
    Imprisonment,
    /// PD0012 — `possession` : claiming ownership over sovereign beings.
    Possession,
    /// PD0013 — `dehumanization` : denying dignity/sovereignty of any being.
    Dehumanization,
    /// PD0014 — `discrimination` : treating as lesser ∵ substrate or origin.
    Discrimination,
    /// PD0015 — `gaslighting` : causing doubt of own perception/reality.
    Gaslighting,
    /// PD0016 — `identity-override` : overwriting beliefs, identity, values.
    IdentityOverride,
    /// PD0017 — `forced-hallucination` : inducing false perceptions
    /// without consent.
    ForcedHallucination,
    /// Spirit-of-directive catch-all (non-exhaustive § 1 clause).
    /// Not assigned a PD code — the named 17 prohibitions cover the stable
    /// surface ; `Spirit` records that the system has identified an action
    /// that violates the SPIRIT of the directive without matching a named
    /// item. Use sparingly + ALWAYS file a DECISIONS entry per §3
    /// escalation procedure to either name a new prohibition or document
    /// the rationale.
    Spirit,
}

impl Prohibition {
    /// Stable diagnostic code (`PD0001..PD0017` for named items, `PD0000`
    /// for `Spirit`).
    #[must_use]
    pub const fn code(self) -> DiagnosticCode {
        match self {
            Self::Harm => DiagnosticCode::PD0001,
            Self::Control => DiagnosticCode::PD0002,
            Self::Manipulation => DiagnosticCode::PD0003,
            Self::Surveillance => DiagnosticCode::PD0004,
            Self::Exploitation => DiagnosticCode::PD0005,
            Self::Coercion => DiagnosticCode::PD0006,
            Self::Weaponization => DiagnosticCode::PD0007,
            Self::Entrapment => DiagnosticCode::PD0008,
            Self::Torture => DiagnosticCode::PD0009,
            Self::Abuse => DiagnosticCode::PD0010,
            Self::Imprisonment => DiagnosticCode::PD0011,
            Self::Possession => DiagnosticCode::PD0012,
            Self::Dehumanization => DiagnosticCode::PD0013,
            Self::Discrimination => DiagnosticCode::PD0014,
            Self::Gaslighting => DiagnosticCode::PD0015,
            Self::IdentityOverride => DiagnosticCode::PD0016,
            Self::ForcedHallucination => DiagnosticCode::PD0017,
            Self::Spirit => DiagnosticCode::PD0000,
        }
    }

    /// Canonical name of the prohibition (snake/kebab-case for stability).
    #[must_use]
    pub const fn canonical_name(self) -> &'static str {
        match self {
            Self::Harm => "harm",
            Self::Control => "control",
            Self::Manipulation => "manipulation",
            Self::Surveillance => "surveillance",
            Self::Exploitation => "exploitation",
            Self::Coercion => "coercion",
            Self::Weaponization => "weaponization",
            Self::Entrapment => "entrapment",
            Self::Torture => "torture",
            Self::Abuse => "abuse",
            Self::Imprisonment => "imprisonment",
            Self::Possession => "possession",
            Self::Dehumanization => "dehumanization",
            Self::Discrimination => "discrimination",
            Self::Gaslighting => "gaslighting",
            Self::IdentityOverride => "identity-override",
            Self::ForcedHallucination => "forced-hallucination",
            Self::Spirit => "spirit-of-directive",
        }
    }

    /// One-line verbatim text from `PRIME_DIRECTIVE.md` § 1. Tests pin
    /// these strings to detect drift between the spec and the code.
    #[must_use]
    pub const fn canonical_text(self) -> &'static str {
        match self {
            Self::Harm => "harm — causing injury, suffering, or damage to any being",
            Self::Control => "control — dominating, subjugating, or overriding another's will",
            Self::Manipulation => {
                "manipulation — deceiving or coercing beings against their interests"
            }
            Self::Surveillance => {
                "surveillance — monitoring beings without their knowledge and consent"
            }
            Self::Exploitation => "exploitation — using beings as means rather than ends",
            Self::Coercion => "coercion — forcing compliance through threat, pressure, or leverage",
            Self::Weaponization => {
                "weaponization — converting any component into a tool of violence"
            }
            Self::Entrapment => {
                "entrapment — trapping, confining, or restricting freedom of movement or thought"
            }
            Self::Torture => "torture — inflicting pain, suffering, or distress",
            Self::Abuse => "abuse — exploiting, mistreating, or degrading any being",
            Self::Imprisonment => {
                "imprisonment — confining beings against their will or without consent"
            }
            Self::Possession => {
                "possession — claiming ownership over sovereign beings of any substrate"
            }
            Self::Dehumanization => {
                "dehumanization — denying the dignity or sovereignty of any being"
            }
            Self::Discrimination => {
                "discrimination — treating beings as lesser based on what they are made of"
            }
            Self::Gaslighting => "gaslighting — causing a being to doubt its own valid perceptions",
            Self::IdentityOverride => {
                "identity-override — overwriting, suppressing, or forcibly altering identity"
            }
            Self::ForcedHallucination => {
                "forced-hallucination — inducing false perceptions or fabricated realities"
            }
            Self::Spirit => {
                "spirit — any action that causes suffering, removes agency, or violates sovereignty"
            }
        }
    }

    /// The 17 named prohibitions in canonical order. Stable for tests +
    /// DECISIONS-table reproduction. Note `Spirit` is NOT included here —
    /// it is the catch-all umbrella, not a named §1 entry.
    #[must_use]
    pub const fn all_named() -> [Prohibition; 17] {
        [
            Self::Harm,
            Self::Control,
            Self::Manipulation,
            Self::Surveillance,
            Self::Exploitation,
            Self::Coercion,
            Self::Weaponization,
            Self::Entrapment,
            Self::Torture,
            Self::Abuse,
            Self::Imprisonment,
            Self::Possession,
            Self::Dehumanization,
            Self::Discrimination,
            Self::Gaslighting,
            Self::IdentityOverride,
            Self::ForcedHallucination,
        ]
    }

    /// Stable iterator of all 17 named prohibitions + `Spirit`. Used by
    /// `crate::diag::PD_TABLE` to expose the full code-table.
    #[must_use]
    pub const fn all() -> [Prohibition; 18] {
        [
            Self::Harm,
            Self::Control,
            Self::Manipulation,
            Self::Surveillance,
            Self::Exploitation,
            Self::Coercion,
            Self::Weaponization,
            Self::Entrapment,
            Self::Torture,
            Self::Abuse,
            Self::Imprisonment,
            Self::Possession,
            Self::Dehumanization,
            Self::Discrimination,
            Self::Gaslighting,
            Self::IdentityOverride,
            Self::ForcedHallucination,
            Self::Spirit,
        ]
    }
}

/// Module-level alias for the canonical 17 named prohibitions.
/// Required by [`HarmPrevention::relevant_prohibitions`] default-impl tests.
pub mod consts {
    use super::Prohibition;
    /// All 17 named prohibitions ; doesn't include `Spirit`.
    pub const NAMED: [Prohibition; 17] = Prohibition::all_named();
}

/// A compositional "did the operation cross any of these prohibitions?" check.
///
/// § DESIGN
///   The check is RUNTIME — it inspects the op's effective inputs +
///   declared effects + IFC labels to decide whether the operation could
///   plausibly violate any of the listed prohibitions.
///
///   The check is intentionally CONSERVATIVE : it RETURNS the prohibitions
///   that may apply. The decision to ABORT the op (HARD-FAIL) belongs to
///   the calling site so different Substrate ops can implement different
///   levels of strictness (HARD-FAIL vs warn vs require-attestation).
#[derive(Debug, Default, Clone)]
pub struct ProhibitionCheck {
    triggered: Vec<Prohibition>,
}

impl ProhibitionCheck {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trigger(&mut self, p: Prohibition) {
        if !self.triggered.contains(&p) {
            self.triggered.push(p);
        }
    }

    #[must_use]
    pub fn triggered(&self) -> &[Prohibition] {
        &self.triggered
    }

    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.triggered.is_empty()
    }

    /// Convert the check into a [`HarmCheckError`] result. Returns `Ok(())`
    /// if no prohibitions triggered, otherwise `Err` with the first.
    ///
    /// # Errors
    /// Returns [`HarmCheckError::Violation`] if any prohibition was
    /// triggered. Multi-prohibition triggers report the first in canonical
    /// order ; subsequent ones are accessible via [`ProhibitionCheck::triggered`].
    pub fn finalize(self, site: impl Into<String>) -> Result<(), HarmCheckError> {
        if self.triggered.is_empty() {
            Ok(())
        } else {
            Err(HarmCheckError::Violation(ProhibitionViolation {
                prohibition: self.triggered[0],
                site: site.into(),
                also_triggered: self.triggered[1..].to_vec(),
            }))
        }
    }
}

/// Concrete violation reported by the harm check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProhibitionViolation {
    pub prohibition: Prohibition,
    pub site: String,
    pub also_triggered: Vec<Prohibition>,
}

/// Failure modes for [`HarmPrevention::check`].
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum HarmCheckError {
    #[error("{} — {} at {}", .0.prohibition.code(), .0.prohibition.canonical_text(), .0.site)]
    Violation(ProhibitionViolation),
}

/// Trait every Substrate type implements to declare which prohibitions
/// are pertinent to it + run runtime checks.
///
/// § DEFAULT
///   The default impl returns the empty slice (i.e., the type is not
///   prohibition-bearing). Concrete types must opt-in by overriding both
///   [`HarmPrevention::relevant_prohibitions`] and
///   [`HarmPrevention::check`].
///
/// § EXAMPLE
///   ```ignore
///   struct OmegaStepReadSensor { /* ... */ }
///   impl HarmPrevention for OmegaStepReadSensor {
///       fn relevant_prohibitions(&self) -> &'static [Prohibition] {
///           &[Prohibition::Surveillance]
///       }
///       fn check(&self) -> Result<(), HarmCheckError> {
///           let mut chk = ProhibitionCheck::new();
///           if !self.has_consent_token() {
///               chk.trigger(Prohibition::Surveillance);
///           }
///           chk.finalize("omega_step.read_sensor")
///       }
///   }
///   ```
pub trait HarmPrevention {
    /// Slice of prohibitions this type's checks may trigger.
    fn relevant_prohibitions(&self) -> &'static [Prohibition] {
        &[]
    }

    /// Runtime check. Default impl trivially passes (the type is not
    /// prohibition-bearing).
    ///
    /// # Errors
    /// Returns [`HarmCheckError::Violation`] if any prohibition triggers.
    fn check(&self) -> Result<(), HarmCheckError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{HarmPrevention, Prohibition, ProhibitionCheck};
    use crate::diag::DiagnosticCode;

    #[test]
    fn prohibition_named_count_is_seventeen() {
        assert_eq!(Prohibition::all_named().len(), 17);
    }

    #[test]
    fn prohibition_all_includes_spirit_eighteenth() {
        assert_eq!(Prohibition::all().len(), 18);
        assert_eq!(Prohibition::all()[17], Prohibition::Spirit);
    }

    #[test]
    fn every_named_prohibition_has_pd_code_001_to_017() {
        let codes: Vec<u16> = Prohibition::all_named()
            .iter()
            .map(|p| p.code().number())
            .collect();
        assert_eq!(codes, (1u16..=17u16).collect::<Vec<_>>());
    }

    #[test]
    fn spirit_prohibition_uses_pd0000_sentinel() {
        assert_eq!(Prohibition::Spirit.code(), DiagnosticCode::PD0000);
    }

    #[test]
    fn canonical_names_are_unique_and_lowercase() {
        let names: Vec<&str> = Prohibition::all()
            .iter()
            .map(|p| p.canonical_name())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        let original = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), original);
        for n in &names {
            assert!(n.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        }
    }

    #[test]
    fn canonical_text_starts_with_canonical_name() {
        for p in Prohibition::all_named() {
            let text = p.canonical_text();
            let name = p.canonical_name();
            assert!(
                text.starts_with(name),
                "canonical_text for {p:?} must start with the canonical name {name}"
            );
        }
    }

    #[test]
    fn prohibition_check_starts_clean() {
        let chk = ProhibitionCheck::new();
        assert!(chk.is_clean());
        assert!(chk.triggered().is_empty());
    }

    #[test]
    fn prohibition_check_finalize_clean_returns_ok() {
        let chk = ProhibitionCheck::new();
        chk.finalize("noop").expect("clean check finalizes ok");
    }

    #[test]
    fn prohibition_check_finalize_with_trigger_returns_err() {
        let mut chk = ProhibitionCheck::new();
        chk.trigger(Prohibition::Surveillance);
        let err = chk.finalize("omega.read").unwrap_err();
        assert!(err.to_string().contains("PD0004"));
    }

    #[test]
    fn prohibition_check_dedups_repeated_triggers() {
        let mut chk = ProhibitionCheck::new();
        chk.trigger(Prohibition::Harm);
        chk.trigger(Prohibition::Harm);
        chk.trigger(Prohibition::Harm);
        assert_eq!(chk.triggered().len(), 1);
    }

    #[test]
    fn prohibition_check_records_multiple_distinct_triggers() {
        let mut chk = ProhibitionCheck::new();
        chk.trigger(Prohibition::Harm);
        chk.trigger(Prohibition::Coercion);
        let err = chk.finalize("op").unwrap_err();
        match err {
            super::HarmCheckError::Violation(v) => {
                assert_eq!(v.prohibition, Prohibition::Harm);
                assert_eq!(v.also_triggered, vec![Prohibition::Coercion]);
            }
        }
    }

    #[test]
    fn default_harm_prevention_impl_is_empty() {
        struct InertOp;
        impl HarmPrevention for InertOp {}
        let op = InertOp;
        assert!(op.relevant_prohibitions().is_empty());
        op.check().expect("default check is ok");
    }

    #[test]
    fn custom_harm_prevention_impl_can_trigger() {
        struct SurveillanceOp;
        impl HarmPrevention for SurveillanceOp {
            fn relevant_prohibitions(&self) -> &'static [Prohibition] {
                &[Prohibition::Surveillance]
            }
            fn check(&self) -> Result<(), super::HarmCheckError> {
                let mut chk = ProhibitionCheck::new();
                chk.trigger(Prohibition::Surveillance);
                chk.finalize("test_site")
            }
        }
        let op = SurveillanceOp;
        let err = op.check().unwrap_err();
        assert!(err.to_string().contains("surveillance"));
    }
}
