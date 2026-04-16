//! Oracle-mode registry.
//!
//! Every `@<attr>`-style test attribute in `specs/23_TESTING.csl` maps to exactly one
//! `OracleMode` variant. `OracleMode::ALL` holds the full registration order; the
//! scaffold-test in `lib.rs` asserts its length matches `ORACLE_MODE_COUNT`.

use core::fmt;

/// Every oracle mode defined by `specs/23_TESTING.csl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OracleMode {
    /// Standard unit test (`@test`).
    Unit,
    /// QuickCheck-lineage property-based (`@property`).
    Property,
    /// Multi-backend bit-exact (`@differential`).
    Differential,
    /// Algebraic-law preservation (`@metamorphic`).
    Metamorphic,
    /// Baseline-tracked performance benchmark (`@bench`).
    Bench,
    /// Power regression via `{Telemetry<Power>}` (`@power_bench`).
    PowerBench,
    /// Sustained sysman thermal sampling (`@thermal_stress`).
    ThermalStress,
    /// Deterministic replay N=10 default (`@replay`).
    Replay,
    /// Hot-reload schema-migration invariance (`@hot_reload_test`).
    HotReload,
    /// Coverage-guided fuzz with SMT-oracle (`@fuzz`).
    Fuzz,
    /// Pixel + SSIM + FLIP golden-image comparison (`@golden`).
    Golden,
    /// Audit-chain verification (`@audit_test`).
    Audit,
}

impl OracleMode {
    /// All modes in registration order; length must equal `crate::ORACLE_MODE_COUNT`.
    pub const ALL: &'static [Self] = &[
        Self::Unit,
        Self::Property,
        Self::Differential,
        Self::Metamorphic,
        Self::Bench,
        Self::PowerBench,
        Self::ThermalStress,
        Self::Replay,
        Self::HotReload,
        Self::Fuzz,
        Self::Golden,
        Self::Audit,
    ];

    /// Human-readable short name (CLI flag style).
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Property => "property",
            Self::Differential => "differential",
            Self::Metamorphic => "metamorphic",
            Self::Bench => "bench",
            Self::PowerBench => "power_bench",
            Self::ThermalStress => "thermal_stress",
            Self::Replay => "replay",
            Self::HotReload => "hot_reload",
            Self::Fuzz => "fuzz",
            Self::Golden => "golden",
            Self::Audit => "audit",
        }
    }

    /// The `@<attr>` surface form as written in CSSLv3 source.
    pub const fn attribute_form(self) -> &'static str {
        match self {
            Self::Unit => "@test",
            Self::Property => "@property",
            Self::Differential => "@differential",
            Self::Metamorphic => "@metamorphic",
            Self::Bench => "@bench",
            Self::PowerBench => "@power_bench",
            Self::ThermalStress => "@thermal_stress",
            Self::Replay => "@replay",
            Self::HotReload => "@hot_reload_test",
            Self::Fuzz => "@fuzz",
            Self::Golden => "@golden",
            Self::Audit => "@audit_test",
        }
    }
}

impl fmt::Display for OracleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::OracleMode;

    #[test]
    fn display_matches_attribute_form() {
        for mode in OracleMode::ALL {
            assert!(mode.attribute_form().starts_with('@'));
            assert!(!mode.display_name().starts_with('@'));
        }
    }

    #[test]
    fn all_modes_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for mode in OracleMode::ALL {
            assert!(seen.insert(*mode), "duplicate oracle mode: {mode:?}");
        }
    }
}
