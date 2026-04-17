//! CSSLv3 stage0 — Intel Level-Zero host submission + sysman (R18) scaffold.
//!
//! § SPEC : `specs/10_HW.csl` § LEVEL-ZERO BASELINE + `specs/22_TELEMETRY.csl` (R18).
//!
//! § STRATEGY
//!   Phase-1 catalogs the L0 host-API surface + the sysman R18 telemetry metrics.
//!   No `level-zero-sys` FFI yet (blocked on MSVC toolchain per T1-D7). Phase-2
//!   wires real bindings at the boundary.
//!
//! § SCOPE (T10-phase-1-hosts / this commit)
//!   - [`L0ApiSurface`]        — enum of L0 APIs CSSLv3 exercises.
//!   - [`L0Driver`] / [`L0Device`] — identifying data for a discovered Intel GPU.
//!   - [`UsmAllocType`]        — shared / host / device USM kinds.
//!   - [`SysmanMetric`]        — 11-variant catalog per `specs/10` § SYSMAN AVAILABILITY.
//!   - [`SysmanMetricSet`]     — declared metrics per telemetry probe.
//!   - [`SysmanSample`]        — a single captured sample.
//!   - [`TelemetryProbe`]      — trait for phase-2 sysman-backed capture.
//!   - [`StubTelemetryProbe`]  — stage-0 stub returning canonical Arc A770 values.
//!
//! § T10-phase-2-hosts DEFERRED
//!   - `level-zero-sys` crate integration (vendored until crates.io registration).
//!   - `ze_driver_handle_t` / `ze_device_handle_t` / `ze_command_list_t` lifetime.
//!   - SPIR-V `ze_module_t` consumption + `ze_kernel_t` dispatch.
//!   - USM allocation (host / device / shared).
//!   - Sysman property sampling (`zesPowerGetEnergyCounter` / etc.).
//!   - Multi-device + multi-context concurrency.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]

pub mod api;
pub mod driver;
pub mod sysman;

pub use api::{L0ApiSurface, UsmAllocType};
pub use driver::{L0Device, L0DeviceProperties, L0DeviceType, L0Driver};
pub use sysman::{
    StubTelemetryProbe, SysmanCapture, SysmanMetric, SysmanMetricSet, SysmanSample, TelemetryError,
    TelemetryProbe,
};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod scaffold_tests {
    use super::STAGE0_SCAFFOLD;

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }
}
