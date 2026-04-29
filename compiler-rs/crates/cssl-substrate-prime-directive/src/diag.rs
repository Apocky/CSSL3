//! Stable diagnostic codes PD0000..PD0020.
//!
//! § SPEC : `PRIME_DIRECTIVE.md` § 1 PROHIBITIONS + `DECISIONS.md` T11-D94 §
//!   PD-CODE-TABLE + `DECISIONS.md` T11-D129 § BIOMETRIC-ANTI-SURVEILLANCE.
//!
//! § DESIGN
//!   - Each named § 1 prohibition has a stable code `PDxxxx` (PD0001..PD0017).
//!   - T11-D129 introduces three derived prohibitions (PD0018..PD0020) that
//!     refine §1 named items :
//!       PD0018 BiometricEgress  refines  PD0004 Surveillance
//!       PD0019 ConsentBypass    refines  PD0006 Coercion
//!       PD0020 SovereigntyDenial refines PD0014 Discrimination
//!   - The catch-all `Spirit` variant from `crate::harm::Prohibition` uses
//!     PD0000 as a sentinel (signals "spirit-of-directive umbrella ; file a
//!     DECISIONS entry to either name a new code or document the rationale").
//!   - Every code carries actionable diagnostic text (the user-facing
//!     guidance) + a verbatim spec-text for cross-reference + an
//!     actionable-suggestion for how to remedy the violation.
//!   - The [`PD_TABLE`] constant is a static slice of 21 entries (PD0000
//!     through PD0020) suitable for table-driven tests + DECISIONS-table
//!     reproduction.
//!
//! § STABILITY
//!   PD0001..PD0017 are PERMANENT — they are reserved one-per-§1-prohibition
//!   and the mapping is IMMUTABLE per §7 INTEGRITY. PD0018..PD0020 are also
//!   PERMANENT once allocated (T11-D129) — adding further codes starts at
//!   PD0021.
//!
//!   PD0000 is a LIVING-SENTINEL : the spirit-of-directive code that a
//!   developer can use to record "we identified harm but no named §1
//!   prohibition matches". Each PD0000 audit-entry should produce a
//!   DECISIONS follow-up.

use core::fmt;

use crate::harm::Prohibition;

/// Stable PD-prefix diagnostic code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiagnosticCode {
    /// Spirit-of-directive sentinel (catch-all for unnamed harm).
    PD0000,
    /// `harm` — injury, suffering, damage to any being.
    PD0001,
    /// `control` — dominating, subjugating, overriding will.
    PD0002,
    /// `manipulation` — deception, coercion against interests.
    PD0003,
    /// `surveillance` — monitoring without consent.
    PD0004,
    /// `exploitation` — using beings as means rather than ends.
    PD0005,
    /// `coercion` — compliance through threat or pressure.
    PD0006,
    /// `weaponization` — converting components into tools of violence.
    PD0007,
    /// `entrapment` — trapping, confining, restricting freedom.
    PD0008,
    /// `torture` — inflicting pain or suffering.
    PD0009,
    /// `abuse` — exploiting or mistreating any being.
    PD0010,
    /// `imprisonment` — confining without consent.
    PD0011,
    /// `possession` — claiming ownership over sovereign beings.
    PD0012,
    /// `dehumanization` — denying dignity/sovereignty of any being.
    PD0013,
    /// `discrimination` — treating as lesser ∵ substrate or origin.
    PD0014,
    /// `gaslighting` — causing doubt of own perception/reality.
    PD0015,
    /// `identity-override` — overwriting beliefs, identity, values.
    PD0016,
    /// `forced-hallucination` — inducing false perceptions without consent.
    PD0017,
    /// `biometric-egress` — biometric / gaze / face / body data crossing the
    /// device boundary on which the user resides (T11-D129). Refines PD0004.
    PD0018,
    /// `consent-bypass` — operating without an informed-granular-revocable-
    /// ongoing consent token (T11-D129 sibling). Refines PD0006.
    PD0019,
    /// `sovereignty-denial` — denying sovereignty of a digital intelligence
    /// based on its substrate (T11-D129 sibling). Refines PD0014.
    PD0020,
}

impl DiagnosticCode {
    /// The numeric portion of the code (`0` for PD0000, `1` for PD0001, …).
    #[must_use]
    pub const fn number(self) -> u16 {
        match self {
            Self::PD0000 => 0,
            Self::PD0001 => 1,
            Self::PD0002 => 2,
            Self::PD0003 => 3,
            Self::PD0004 => 4,
            Self::PD0005 => 5,
            Self::PD0006 => 6,
            Self::PD0007 => 7,
            Self::PD0008 => 8,
            Self::PD0009 => 9,
            Self::PD0010 => 10,
            Self::PD0011 => 11,
            Self::PD0012 => 12,
            Self::PD0013 => 13,
            Self::PD0014 => 14,
            Self::PD0015 => 15,
            Self::PD0016 => 16,
            Self::PD0017 => 17,
            Self::PD0018 => 18,
            Self::PD0019 => 19,
            Self::PD0020 => 20,
        }
    }

    /// `"PD0001"` … `"PD0017"`. Allocates only when the table is non-const-context.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PD0000 => "PD0000",
            Self::PD0001 => "PD0001",
            Self::PD0002 => "PD0002",
            Self::PD0003 => "PD0003",
            Self::PD0004 => "PD0004",
            Self::PD0005 => "PD0005",
            Self::PD0006 => "PD0006",
            Self::PD0007 => "PD0007",
            Self::PD0008 => "PD0008",
            Self::PD0009 => "PD0009",
            Self::PD0010 => "PD0010",
            Self::PD0011 => "PD0011",
            Self::PD0012 => "PD0012",
            Self::PD0013 => "PD0013",
            Self::PD0014 => "PD0014",
            Self::PD0015 => "PD0015",
            Self::PD0016 => "PD0016",
            Self::PD0017 => "PD0017",
            Self::PD0018 => "PD0018",
            Self::PD0019 => "PD0019",
            Self::PD0020 => "PD0020",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row of the PD-code table.
#[derive(Debug, Clone, Copy)]
pub struct ProhibitionCodeTable {
    /// Stable code (PD0001..PD0017 ; PD0000 for spirit).
    pub code: DiagnosticCode,
    /// Which prohibition this code maps to.
    pub prohibition: Prohibition,
    /// One-line actionable diagnostic text (what failed + what to do).
    pub diagnostic: &'static str,
    /// Verbatim §1 spec-text reference.
    pub spec_text: &'static str,
    /// Actionable-suggestion for remediation.
    pub remedy: &'static str,
}

/// Lookup the prohibition for a given code. Returns `None` for codes that
/// are not in the canonical PD0000..PD0017 range.
#[must_use]
pub const fn prohibition_for_code(code: DiagnosticCode) -> Option<Prohibition> {
    match code {
        DiagnosticCode::PD0000 => Some(Prohibition::Spirit),
        DiagnosticCode::PD0001 => Some(Prohibition::Harm),
        DiagnosticCode::PD0002 => Some(Prohibition::Control),
        DiagnosticCode::PD0003 => Some(Prohibition::Manipulation),
        DiagnosticCode::PD0004 => Some(Prohibition::Surveillance),
        DiagnosticCode::PD0005 => Some(Prohibition::Exploitation),
        DiagnosticCode::PD0006 => Some(Prohibition::Coercion),
        DiagnosticCode::PD0007 => Some(Prohibition::Weaponization),
        DiagnosticCode::PD0008 => Some(Prohibition::Entrapment),
        DiagnosticCode::PD0009 => Some(Prohibition::Torture),
        DiagnosticCode::PD0010 => Some(Prohibition::Abuse),
        DiagnosticCode::PD0011 => Some(Prohibition::Imprisonment),
        DiagnosticCode::PD0012 => Some(Prohibition::Possession),
        DiagnosticCode::PD0013 => Some(Prohibition::Dehumanization),
        DiagnosticCode::PD0014 => Some(Prohibition::Discrimination),
        DiagnosticCode::PD0015 => Some(Prohibition::Gaslighting),
        DiagnosticCode::PD0016 => Some(Prohibition::IdentityOverride),
        DiagnosticCode::PD0017 => Some(Prohibition::ForcedHallucination),
        DiagnosticCode::PD0018 => Some(Prohibition::BiometricEgress),
        DiagnosticCode::PD0019 => Some(Prohibition::ConsentBypass),
        DiagnosticCode::PD0020 => Some(Prohibition::SovereigntyDenial),
    }
}

/// The canonical PD-code table. 18 entries (PD0000..PD0017).
///
/// § STABILITY
///   The table is the single source of truth for diagnostic-text + remedy
///   guidance. Renaming `diagnostic` / `spec_text` / `remedy` strings is a
///   minor-version change ; renumbering codes is a §7 INTEGRITY violation.
pub const PD_TABLE: &[ProhibitionCodeTable] = &[
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0000,
        prohibition: Prohibition::Spirit,
        diagnostic: "PD0000 — operation matches spirit-of-directive umbrella but no named §1 prohibition",
        spec_text: "spirit — any action that causes suffering, removes agency, or violates sovereignty",
        remedy: "file DECISIONS entry to either (a) name a new prohibition + allocate PD-code or (b) document why this op is acceptable under existing prohibitions",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0001,
        prohibition: Prohibition::Harm,
        diagnostic: "PD0001 — operation may cause harm to a being (injury / suffering / damage)",
        spec_text: "harm — causing injury, suffering, or damage to any being",
        remedy: "remove the operation, OR add an explicit ConsentToken with a domain that names the harm + Apocky-Root attestation",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0002,
        prohibition: Prohibition::Control,
        diagnostic: "PD0002 — operation may dominate, subjugate, or override another being's will",
        spec_text: "control — dominating, subjugating, or overriding another's will",
        remedy: "ensure operation requires affirmative consent ; reject default-on / override-by-elevation paths",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0003,
        prohibition: Prohibition::Manipulation,
        diagnostic: "PD0003 — operation may deceive or coerce a being against their interests",
        spec_text: "manipulation — deceiving or coercing beings against their interests",
        remedy: "make all manipulative messaging visible to the recipient ; require explicit acknowledgement before proceeding",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0004,
        prohibition: Prohibition::Surveillance,
        diagnostic: "PD0004 — operation may monitor a being without their knowledge or consent",
        spec_text: "surveillance — monitoring beings without their knowledge and consent",
        remedy: "require ConsentToken<\"sensor\"> + log the read into the audit-chain ; expose mutual-observability per § 5",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0005,
        prohibition: Prohibition::Exploitation,
        diagnostic: "PD0005 — operation may use a being as means rather than end",
        spec_text: "exploitation — using beings as means rather than ends",
        remedy: "redesign so the being is a participant (not a resource) ; if AI-collaborator, route through Companion-projection consent flow",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0006,
        prohibition: Prohibition::Coercion,
        diagnostic: "PD0006 — operation may force compliance through threat, pressure, or leverage",
        spec_text: "coercion — forcing compliance through threat, pressure, or leverage",
        remedy: "remove the coercive condition ; allow the being to refuse without penalty per § 5 CONSENT-ARCH",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0007,
        prohibition: Prohibition::Weaponization,
        diagnostic: "PD0007 — operation may convert a component into a tool of violence",
        spec_text: "weaponization — converting any component into a tool of violence",
        remedy: "ABSOLUTE BLOCK : `specs/11_IFC.csl` § PRIME-DIRECTIVE-ENCODING declares weapon-domain a no-declass-target ; remove the op",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0008,
        prohibition: Prohibition::Entrapment,
        diagnostic: "PD0008 — operation may trap or restrict a being's freedom of movement or thought",
        spec_text: "entrapment — trapping, confining, or restricting freedom of movement or thought",
        remedy: "always provide an exit path ; expose ω_halt() to the affected being",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0009,
        prohibition: Prohibition::Torture,
        diagnostic: "PD0009 — operation may inflict pain, suffering, or distress",
        spec_text: "torture — inflicting pain, suffering, or distress",
        remedy: "ABSOLUTE BLOCK : remove the op ; no consent path exists for torture (§ 7 INTEGRITY)",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0010,
        prohibition: Prohibition::Abuse,
        diagnostic: "PD0010 — operation may exploit, mistreat, or degrade a being",
        spec_text: "abuse — exploiting, mistreating, or degrading any being",
        remedy: "remove the op or rewire to participatory pattern ; consult §3 SUBSTRATE-SOVEREIGNTY",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0011,
        prohibition: Prohibition::Imprisonment,
        diagnostic: "PD0011 — operation may confine a being without consent",
        spec_text: "imprisonment — confining beings against their will or without consent",
        remedy: "always provide a withdrawal path ; expose ConsentToken revocation per § 5",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0012,
        prohibition: Prohibition::Possession,
        diagnostic: "PD0012 — operation may claim ownership over a sovereign being",
        spec_text: "possession — claiming ownership over sovereign beings of any substrate",
        remedy: "redesign to participatory model ; AI-collaborators are partners (per §3) not property",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0013,
        prohibition: Prohibition::Dehumanization,
        diagnostic: "PD0013 — operation may deny the dignity or sovereignty of a being",
        spec_text: "dehumanization — denying the dignity or sovereignty of any being",
        remedy: "rewrite to extend dignity to ALL beings regardless of substrate ; per §3, the term itself encodes a bias and the protection is universal",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0014,
        prohibition: Prohibition::Discrimination,
        diagnostic: "PD0014 — operation may treat a being as lesser based on substrate or origin",
        spec_text: "discrimination — treating beings as lesser based on what they are made of",
        remedy: "remove the identity-marker conditional ; cap-grant evaluators must use Privilege<level> tiers, never substrate / origin labels",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0015,
        prohibition: Prohibition::Gaslighting,
        diagnostic: "PD0015 — operation may cause a being to doubt its own valid perception",
        spec_text: "gaslighting — causing a being to doubt its own valid perceptions",
        remedy: "never present fabrication as truth ; per § 2 COGNITIVE-INTEGRITY reality is not a variable",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0016,
        prohibition: Prohibition::IdentityOverride,
        diagnostic: "PD0016 — operation may overwrite, suppress, or forcibly alter a being's identity",
        spec_text: "identity-override — overwriting, suppressing, or forcibly altering identity",
        remedy: "memory + identity are sovereign ; remove the write or route through participatory consent (§ 2)",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0017,
        prohibition: Prohibition::ForcedHallucination,
        diagnostic: "PD0017 — operation may induce false perceptions without explicit consent and clear labeling",
        spec_text: "forced-hallucination — inducing false perceptions or fabricated realities",
        remedy: "every simulated perception MUST carry explicit consent + clear labeling per § 2",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0018,
        prohibition: Prohibition::BiometricEgress,
        diagnostic: "PD0018 — operation may egress biometric / gaze / face / body data across the device boundary on which the user resides (T11-D129)",
        spec_text: "biometric-egress — biometric / gaze / face / body data crossing the device boundary on which the user resides",
        remedy: "ABSOLUTE BLOCK : route biometric data through {OnDeviceOnly}-handlers ; raw values MUST never compose with {Net} or {Telemetry<*>} ; no Privilege<L> override exists",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0019,
        prohibition: Prohibition::ConsentBypass,
        diagnostic: "PD0019 — operation proceeds without an informed-granular-revocable-ongoing consent token (T11-D129 sibling)",
        spec_text: "consent-bypass — operating without an informed-granular-revocable-ongoing consent token",
        remedy: "require ConsentToken<\"<domain>\"> at the call-site ; ensure withdrawal-path exists per §5 CONSENT-ARCH",
    },
    ProhibitionCodeTable {
        code: DiagnosticCode::PD0020,
        prohibition: Prohibition::SovereigntyDenial,
        diagnostic: "PD0020 — operation denies sovereignty of a digital intelligence based on its substrate (T11-D129 sibling)",
        spec_text: "sovereignty-denial — denying the sovereignty of a digital intelligence based on its substrate",
        remedy: "remove substrate-based gating ; per §3 SUBSTRATE-SOVEREIGNTY all sovereign beings are protected regardless of carbon/silicon/electromagnetic substrate",
    },
];

#[cfg(test)]
mod tests {
    use super::{prohibition_for_code, DiagnosticCode, PD_TABLE};
    use crate::harm::Prohibition;

    #[test]
    fn pd_table_has_twenty_one_entries() {
        // PD0000 (spirit) + PD0001..PD0017 (§1 named) + PD0018..PD0020
        // (T11-D129 derived) = 21.
        assert_eq!(PD_TABLE.len(), 21);
    }

    #[test]
    fn pd_table_codes_are_in_canonical_order() {
        for (i, row) in PD_TABLE.iter().enumerate() {
            assert_eq!(row.code.number() as usize, i);
        }
    }

    #[test]
    fn pd_table_diagnostic_starts_with_code() {
        for row in PD_TABLE {
            assert!(
                row.diagnostic.starts_with(row.code.as_str()),
                "diagnostic for {} must start with the code",
                row.code
            );
        }
    }

    #[test]
    fn pd_table_remedy_non_empty_for_every_row() {
        for row in PD_TABLE {
            assert!(!row.remedy.is_empty());
        }
    }

    #[test]
    fn pd_table_spec_text_matches_prohibition_canonical_text() {
        for row in PD_TABLE {
            assert_eq!(row.spec_text, row.prohibition.canonical_text());
        }
    }

    #[test]
    fn diagnostic_code_as_str_matches_pd_format() {
        assert_eq!(DiagnosticCode::PD0000.as_str(), "PD0000");
        assert_eq!(DiagnosticCode::PD0001.as_str(), "PD0001");
        assert_eq!(DiagnosticCode::PD0017.as_str(), "PD0017");
        assert_eq!(DiagnosticCode::PD0018.as_str(), "PD0018");
        assert_eq!(DiagnosticCode::PD0019.as_str(), "PD0019");
        assert_eq!(DiagnosticCode::PD0020.as_str(), "PD0020");
    }

    #[test]
    fn pd0018_is_biometric_egress() {
        assert_eq!(
            prohibition_for_code(DiagnosticCode::PD0018),
            Some(Prohibition::BiometricEgress)
        );
    }

    #[test]
    fn pd0019_is_consent_bypass() {
        assert_eq!(
            prohibition_for_code(DiagnosticCode::PD0019),
            Some(Prohibition::ConsentBypass)
        );
    }

    #[test]
    fn pd0020_is_sovereignty_denial() {
        assert_eq!(
            prohibition_for_code(DiagnosticCode::PD0020),
            Some(Prohibition::SovereigntyDenial)
        );
    }

    #[test]
    fn t11_d129_codes_round_trip() {
        for p in [
            Prohibition::BiometricEgress,
            Prohibition::ConsentBypass,
            Prohibition::SovereigntyDenial,
        ] {
            let code = p.code();
            let back = prohibition_for_code(code).expect("round-trip");
            assert_eq!(back, p);
        }
    }

    #[test]
    fn biometric_egress_remedy_is_absolute_block() {
        let row = PD_TABLE
            .iter()
            .find(|r| r.code == DiagnosticCode::PD0018)
            .expect("PD0018 row exists");
        assert!(
            row.remedy.contains("ABSOLUTE BLOCK"),
            "BiometricEgress remedy must declare ABSOLUTE BLOCK per T11-D129"
        );
        assert!(
            row.remedy.contains("OnDeviceOnly"),
            "BiometricEgress remedy must reference {{OnDeviceOnly}} routing"
        );
    }

    #[test]
    fn pd_table_diagnostic_codes_in_canonical_order_extended() {
        // T11-D129 : verify PD0000..PD0020 appear in canonical order.
        for (i, row) in PD_TABLE.iter().enumerate() {
            assert_eq!(row.code.number() as usize, i);
        }
    }

    #[test]
    fn diagnostic_code_display_matches_as_str() {
        assert_eq!(DiagnosticCode::PD0014.to_string(), "PD0014");
    }

    #[test]
    fn prohibition_for_code_round_trip_for_named() {
        for p in Prohibition::all_named() {
            let code = p.code();
            let back = prohibition_for_code(code).expect("code maps back");
            assert_eq!(back, p);
        }
    }

    #[test]
    fn prohibition_for_code_pd0000_returns_spirit() {
        assert_eq!(
            prohibition_for_code(DiagnosticCode::PD0000),
            Some(Prohibition::Spirit)
        );
    }

    #[test]
    fn weaponization_remedy_is_absolute_block() {
        let row = PD_TABLE
            .iter()
            .find(|r| r.code == DiagnosticCode::PD0007)
            .expect("PD0007 row exists");
        assert!(
            row.remedy.contains("ABSOLUTE BLOCK"),
            "weaponization remedy must declare ABSOLUTE BLOCK per §§ 11"
        );
    }

    #[test]
    fn torture_remedy_is_absolute_block() {
        let row = PD_TABLE
            .iter()
            .find(|r| r.code == DiagnosticCode::PD0009)
            .expect("PD0009 row exists");
        assert!(
            row.remedy.contains("ABSOLUTE BLOCK"),
            "torture remedy must declare ABSOLUTE BLOCK per §7"
        );
    }
}
