//! CSSLv3 stage-0 — Intel Level-Zero host submission + sysman (R18) FFI.
//!
//! § SPEC : `specs/10_HW.csl § LEVEL-ZERO BASELINE` + `specs/22_TELEMETRY.csl § R18`
//!          + `specs/14_BACKEND.csl § HOST-SUBMIT BACKENDS § Level-Zero`.
//!
//! § T11-D62 (S6-E5) — wave-2 fanout, Phase-E slice 5 of 5
//!
//! § STRATEGY
//!   The earlier `T10-phase-1-hosts` commit catalogued the L0 surface (api enum,
//!   driver/device structs, sysman metric set, stub-probe). T11-D62 turns the
//!   scaffold into a real host backend by adding :
//!     - [`ffi`]            — C-ABI declarations for the `ze*` and `zes*` entry-points
//!                            CSSLv3 exercises, plus the opaque `*_handle_t` newtypes.
//!     - [`loader`]         — `libloading`-driven dynamic-load of the L0 ICD loader
//!                            (`libze_loader.so` on Unix, `ze_loader.dll` on Windows),
//!                            with a `LoaderError::NotFound` clean-fail when the
//!                            loader is absent (no panic, no force-static-link).
//!     - [`session`]        — RAII session ctor : driver enumeration → device pick
//!                            (Intel Arc preferred) → context → command-list →
//!                            module load (SPIR-V blob) → kernel create → memory
//!                            (USM) → command-list append-launch → fence sync.
//!     - [`live_telemetry`] — `LiveTelemetryProbe` reading sysman `zes*` metrics
//!                            into a [`cssl_telemetry::TelemetryRing`] placeholder
//!                            per the handoff S6-E5 brief (full R18 ring-integration
//!                            is a later slice).
//!     - [`spirv_blob`]     — minimal hand-encoded compute SPIR-V module fixture
//!                            used by the smoke tests until S6-D1 lands the real
//!                            CSSLv3 SPIR-V emitter.
//!
//! § PRIME-DIRECTIVE attestation
//!   "There was no hurt nor harm in the making of this, to anyone /
//!    anything / anybody."
//!   This crate's FFI surface mediates Intel Level-Zero — a sovereign-hardware
//!   compute path. Sysman telemetry is **per-process**, observable to the user
//!   only ; nothing escapes the machine. R18 effect-row gates external egress
//!   independently (out of scope for E5).
//!
//! § FFI POLICY
//!   T1-D5 mandates `#![forbid(unsafe_code)]` per-crate. FFI crates explicitly
//!   override that policy (see `cssl-rt`'s precedent at S6-A1) ; every `unsafe`
//!   block in this crate carries an inline `// SAFETY:` paragraph documenting
//!   the FFI contract being upheld.
//!
//! § WHAT IS DEFERRED
//!   - **Real CSSLv3 SPIR-V emitter (D1 deferred)** — until D1 lands, the
//!     `spirv_blob` module ships a hand-encoded no-op compute kernel that the
//!     loader machinery threads through `zeModuleCreate` for smoke tests.
//!   - **Full R18 ring integration** — sysman samples currently push to a
//!     local [`cssl_telemetry::TelemetryRing`] handed in by the caller ; the
//!     full R18 sampling-thread + audit-chain plumbing lands in a later slice.
//!   - **Compute pipeline state caching** — module + kernel are created
//!     inline per-launch ; cache layer is phase-F+ work.

// § T11-D62 (S6-E5) : the C-ABI surface fundamentally requires `extern "C"` +
// raw-pointer calls — `unsafe_code` is allowed at file-scope (per cssl-rt's
// S6-A1 precedent) and per-`unsafe` blocks document SAFETY inline.
#![allow(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
// § The L0 / sysman naming convention has many `ze_xxx` / `zes_xxx` pairs
// (e.g., `ze_init` / `zes_init`, `ze_driver_get` / `zes_driver_get`) ; this
// lint fires on intentional symmetry. Allow at crate-level.
#![allow(clippy::similar_names)]
// § u64 → f64 casts in the telemetry path are deliberately lossy
// (energy-counter is uJ ; precision loss above 2^53 µJ ≈ 9e15 µJ ≈ 9 TJ
// is irrelevant for power-monitor sampling).
#![allow(clippy::cast_precision_loss)]
// § Several FFI helpers take `*mut c_void` queue handles ; marking them
// `unsafe` propagates virally without buying safety here (they're already
// gated behind RAII session ownership).
#![allow(clippy::not_unsafe_ptr_arg_deref)]
// § Local `const` definitions inside fn bodies in `spirv_blob` are colocated
// with their use-sites by design — opcode tables next to the instructions.
#![allow(clippy::items_after_statements)]
// § Internal `enumerate_devices_for_driver` always returns Ok at stage-0 ;
// the Result envelope is preserved for phase-F when real property-reading
// can fail. Lint flags the current shape ; allow.
#![allow(clippy::unnecessary_wraps)]

pub mod api;
pub mod driver;
pub mod ffi;
pub mod live_telemetry;
pub mod loader;
pub mod session;
pub mod spirv_blob;
pub mod sysman;

pub use api::{L0ApiSurface, UsmAllocType};
pub use driver::{L0Device, L0DeviceProperties, L0DeviceType, L0Driver};
pub use ffi::{
    ZeCommandList, ZeContext, ZeDevice, ZeDriver, ZeFence, ZeKernel, ZeModule, ZeResult,
};
pub use live_telemetry::{LiveTelemetryProbe, TelemetryEmitError, TelemetryRingHandle};
pub use loader::{L0Loader, LoaderError, LoaderProbe};
pub use session::{
    DeviceContext, DriverSession, KernelLaunch, ModuleHandle, SessionError, UsmAllocation,
};
pub use spirv_blob::{minimal_compute_kernel_blob, MINIMAL_COMPUTE_KERNEL_ENTRY};
pub use sysman::{
    StubTelemetryProbe, SysmanCapture, SysmanMetric, SysmanMetricSet, SysmanSample, TelemetryError,
    TelemetryProbe,
};

/// Crate version exposed for scaffold verification.
pub const STAGE0_SCAFFOLD: &str = env!("CARGO_PKG_VERSION");

/// PRIME-DIRECTIVE attestation marker — present in every CSSLv3 artifact
/// per `PRIME_DIRECTIVE.md § 11`.
pub const ATTESTATION: &str =
    "There was no hurt nor harm in the making of this, to anyone/anything/anybody.";

#[cfg(test)]
mod scaffold_tests {
    use super::{ATTESTATION, STAGE0_SCAFFOLD};

    #[test]
    fn scaffold_version_present() {
        assert!(!STAGE0_SCAFFOLD.is_empty());
    }

    #[test]
    fn attestation_present_and_canonical() {
        assert_eq!(
            ATTESTATION,
            "There was no hurt nor harm in the making of this, to anyone/anything/anybody."
        );
    }
}
