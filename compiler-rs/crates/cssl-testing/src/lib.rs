//! CSSLv3 stage0 — §§ 23 oracle-modes + golden-fixture + differential-backend harness.
//!
//! Authoritative design : `specs/23_TESTING.csl`.
//! §§-23-FAITHFUL policy : `DECISIONS.md` T1-D3.
//!
//! § STATUS : T1 scaffold — oracle-dispatch wired empty-but-present.
//!   Each oracle submodule exposes a `Dispatcher` trait and a `Stage0Stub` unit-struct
//!   returning `Outcome::Stage0Unimplemented`. T11 replaces `Stage0Stub` with real runners.
//!
//! § ORACLE-MODE REGISTRY (`OracleMode` enum in `oracle` module) :
//!   1. `Unit`              — standard `@test`
//!   2. `Property`          — `@property` (QuickCheck-lineage)
//!   3. `Differential`      — `@differential` (Vulkan × Level-Zero bit-exact primary)
//!   4. `Metamorphic`       — `@metamorphic` (algebraic-law preservation)
//!   5. `Bench`             — `@bench` (baseline-tracked perf)
//!   6. `PowerBench`        — `@power_bench` (`{Telemetry<Power>}`)
//!   7. `ThermalStress`     — `@thermal_stress` (sustained sysman sampling)
//!   8. `Replay`            — `@replay` (N=10 determinism default)
//!   9. `HotReload`         — `@hot_reload_test` (schema-migration invariance)
//!   10. `Fuzz`             — `@fuzz` (coverage-guided + SMT oracle)
//!   11. `Golden`           — `@golden` (pixel + SSIM + FLIP)
//!   12. `Audit`            — `@audit_test` (audit-chain verification)
//!
//! § ADJUNCT MODULES (not oracle-enum variants but part of the harness) :
//!   - `metrics`        : frequency-stability / dispatch-latency percentile data-structs
//!   - `r16_attestation`: R16 C99-anchor reproducibility-attestation hook (T30, OG10)

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

pub mod audit;
pub mod bench;
pub mod differential;
pub mod fuzz;
pub mod golden;
pub mod hot_reload;
pub mod metamorphic;
pub mod metrics;
pub mod oracle;
pub mod power;
pub mod property;
pub mod r16_attestation;
pub mod replay;
pub mod thermal;

pub use oracle::OracleMode;

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// Count of oracle modes registered at T1 scaffold.
/// Must equal `OracleMode::ALL.len()`; asserted by `oracle_mode_count_matches` scaffold test.
pub const ORACLE_MODE_COUNT: usize = 12;

#[cfg(test)]
mod scaffold_tests {
    use super::{OracleMode, ORACLE_MODE_COUNT, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn oracle_mode_count_matches_registry() {
        assert_eq!(OracleMode::ALL.len(), ORACLE_MODE_COUNT);
    }

    #[test]
    fn every_oracle_mode_has_display_name() {
        for mode in OracleMode::ALL {
            assert!(
                !mode.display_name().is_empty(),
                "{mode:?} display_name is empty"
            );
        }
    }
}
