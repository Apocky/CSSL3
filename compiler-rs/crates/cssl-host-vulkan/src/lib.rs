//! CSSLv3 stage0 — Vulkan 1.4 host submission via `ash` (T11-D65, S6-E1).
//!
//! § SPEC : `specs/10_HW.csl` § VULKAN 1.4 BASELINE + `specs/14_BACKEND.csl` §
//!         HOST-SUBMIT BACKENDS.
//!
//! § STRATEGY (history)
//!   - **Phase 1** (T10-phase-1-hosts) — capability + extension catalog +
//!     `ArcA770Profile` constants. No `ash` FFI yet.
//!   - **Phase 2** (S6-E1, T11-D65, this module) — real `ash`-backed FFI
//!     for instance / physical-device / device / queue / buffer / memory /
//!     compute-pipeline / cmd-buffer / queue-submit / fence + R18
//!     telemetry-ring placeholder hooks via `VK_EXT_pipeline_executable_properties`
//!     + validation-layer routing (debug-builds only).
//!   - **Phase 3** (post-D1) — wire CSSLv3-emitted SPIR-V into the
//!     pipeline path ; today S6-E1 ships a hand-rolled compute SPIR-V
//!     (`spirv_blob`) so the smoke tests can run end-to-end.
//!
//! § SCOPE (this slice)
//!   - [`VulkanVersion`]        — VK-1.0..1.4 enum  (preserved).
//!   - [`VulkanExtension`]      — 30-variant catalog  (preserved).
//!   - [`VulkanLayer`]          — validation / api-dump / monitor  (preserved).
//!   - [`VulkanDevice`]         — adapter identification  (preserved).
//!   - [`GpuVendor`]            — Intel / NVIDIA / AMD / etc.  (preserved).
//!   - [`DeviceType`]           — Discrete / Integrated / Virtual / Cpu  (preserved).
//!   - [`DeviceFeatures`]       — feature-bitset  (preserved).
//!   - [`FeatureProbe`]         — trait abstracting host-adapter feature-probe.
//!   - [`ArcA770Profile`]       — constant catalog for the primary v1 target  (preserved).
//!   - [`StubProbe`]            — phase-1 stub probe  (preserved for unit tests).
//!   - [`AshProbe`]             — **NEW** : real ash-backed probe.
//!   - [`ffi::VkInstanceHandle`]    — RAII VkInstance + validation-layer routing.
//!   - [`ffi::PhysicalDevicePick`]  — RAII physical-device pick (Arc-A770 preferred).
//!   - [`ffi::LogicalDevice`]       — RAII logical-device + queue.
//!   - [`ffi::VkBufferHandle`]      — RAII buffer + memory.
//!   - [`ffi::ComputePipelineHandle`] — RAII compute pipeline + layout.
//!   - [`ffi::CommandContext`]      — RAII command-pool + cmd-buffer + fence.
//!   - [`ffi::VulkanTelemetryRing`] — R18 placeholder ring.
//!   - [`spirv_blob::COMPUTE_NOOP_SPIRV`] — hand-rolled compute SPIR-V.
//!
//! § PRIME-DIRECTIVE (carried-forward)
//!   - Validation layers + debug-utils messenger gated to
//!     `cfg(debug_assertions)` — release builds open no diagnostic
//!     side-channel.
//!   - Telemetry ring is process-local : nothing escapes the process.
//!   - All FFI is opt-in via `#![allow(unsafe_code)]` at the `ffi`
//!     module boundary ; the rest of the crate retains the catalog-
//!     style sound-by-default surface.
//!
//! § INVARIANTS
//!   - `ArcA770Profile::canonical()` is fact-of-spec — every change to
//!     the canonical Arc A770 record must be paired with a `specs/10_HW`
//!     update.
//!   - The hand-rolled SPIR-V blob is **for stage-0 testing only** ;
//!     S6-D1 (real SPIR-V emitter from CSSLv3 source) supersedes it.

// § T11-D65 (S6-E1) : `unsafe_code` downgraded from `forbid` to `deny`
// (matches cssl-rt T11-D52 precedent). Only the `ffi` submodule (and
// its children) opt back in via `#![allow(unsafe_code)]` at the module
// level. Each unsafe block carries an inline `// SAFETY :` paragraph.
#![deny(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]

pub mod arc_a770;
pub mod device;
pub mod extensions;
pub mod ffi;
pub mod probe;
pub mod spirv_blob;

pub use arc_a770::ArcA770Profile;
pub use device::{DeviceFeatures, DeviceType, GpuVendor, VulkanDevice, VulkanVersion};
pub use extensions::{VulkanExtension, VulkanExtensionSet, VulkanLayer};
pub use probe::{AshProbe, FeatureProbe, ProbeError, StubProbe};

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
