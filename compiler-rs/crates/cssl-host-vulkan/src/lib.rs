//! CSSLv3 stage0 — Vulkan 1.4.333 host submission scaffold.
//!
//! § SPEC : `specs/10_HW.csl` § VULKAN 1.4 BASELINE + `specs/14_BACKEND.csl` §
//!         HOST-SUBMIT BACKENDS.
//!
//! § STRATEGY
//!   Phase-1 catalogs the Vulkan capability / extension / feature surface CSSLv3
//!   targets + provides device-enum-representation + feature-probe trait. No `ash`
//!   FFI is wired yet (blocked on MSVC toolchain switch per T1-D7). Phase-2 wires
//!   real `ash` at the boundary, preserving the API shape.
//!
//! § SCOPE (T10-phase-1-hosts / this commit)
//!   - [`VulkanVersion`]        — VK-1.0..1.4 enum.
//!   - [`VulkanExtension`]      — 30-variant catalog for v1 CSSLv3 targets
//!     (core-1.4 + extensions confirmed on Arc-A770 per `specs/10` § VULKAN 1.4 BASELINE).
//!   - [`VulkanLayer`]          — validation / api-dump / monitor.
//!   - [`VulkanDevice`]         — adapter identification (vendor / device-id / driver /
//!     api-version / device-type).
//!   - [`GpuVendor`]            — Intel / NVIDIA / AMD / Apple / Qualcomm / ARM / Mesa / Other.
//!   - [`DeviceType`]           — Discrete / Integrated / Virtual / Cpu.
//!   - [`DeviceFeatures`]       — feature-bitset CSSLv3 cares about.
//!   - [`FeatureProbe`]         — trait for phase-2 ash-backed device enumeration.
//!   - [`ArcA770Profile`]       — constant catalog for the primary v1 target.
//!
//! § T10-phase-2-hosts DEFERRED
//!   - `ash`-backed `VkInstance` / `VkPhysicalDevice` / `VkDevice` creation (MSVC-ABI
//!     gated per T1-D7).
//!   - Extension-request arbitration (required vs. optional set).
//!   - `vkAllocateDescriptorSet` + `vkUpdateDescriptorSet` wiring for bindless.
//!   - `VkPipeline` creation via SPIR-V consumption from `cssl-cgen-gpu-spirv`.
//!   - `VkCommandBuffer` recording + `vkQueueSubmit` + `vkQueuePresentKHR`.
//!   - Validation-layer diagnostic routing.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]

pub mod arc_a770;
pub mod device;
pub mod extensions;
pub mod probe;

pub use arc_a770::ArcA770Profile;
pub use device::{DeviceFeatures, DeviceType, GpuVendor, VulkanDevice, VulkanVersion};
pub use extensions::{VulkanExtension, VulkanExtensionSet, VulkanLayer};
pub use probe::{FeatureProbe, ProbeError, StubProbe};

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
