//! CSSLv3 stage0 — Intel Level-Zero host submission + sysman (R18) (FFI crate).
//!
//! Authoritative design : `specs/10_HW.csl` + `specs/22_TELEMETRY.csl`.
//!
//! § STATUS : T10 scaffold — L0 driver/device + sysman sampling pending.
//! § POLICY : `unsafe_code` permitted at FFI boundary only (level-zero-sys = raw FFI).
//!   Each unsafe block MUST include a `// SAFETY:` comment.
//! § R18-TELEMETRY : `zesPowerGetEnergyCounter` / `zesTemperatureGetState` /
//!                  `zesFrequencyGetState` back `{Telemetry<scope>}` effect discharge.

#![allow(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]

/// Crate version, exposes `CARGO_PKG_VERSION`.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    #[test]
    fn scaffold_version_present() {
        assert!(!super::STAGE0_SCAFFOLD.is_empty());
    }
}
