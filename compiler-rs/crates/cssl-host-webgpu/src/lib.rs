//! CSSLv3 stage0 — WebGPU host submission scaffold.
//!
//! § SPEC : `specs/14_BACKEND.csl` § HOST-SUBMIT BACKENDS § WebGPU +
//!         `specs/07_CODEGEN.csl` § GPU BACKEND — WGSL path.
//!
//! § STRATEGY
//!   Phase-1 catalogs the WebGPU adapter / feature / limits surface without pulling
//!   in `wgpu` (pure-Rust but heavy deps). Phase-2 wires the real `wgpu::Adapter` /
//!   `wgpu::Device` / `wgpu::Queue` path.
//!
//! § SCOPE (T10-phase-1-hosts / this commit)
//!   - [`WebGpuBackend`]    — Vulkan / Metal / DX12 / BrowserWebGPU / GL passthrough.
//!   - [`WebGpuAdapter`]    — adapter identification record.
//!   - [`AdapterPowerPref`] — low-power / high-performance.
//!   - [`SupportedFeatureSet`] — enabled WebGPU features catalog.
//!   - [`WebGpuLimits`]     — resource limits snapshot.

#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::struct_excessive_bools)]

pub mod adapter;
pub mod features;

pub use adapter::{AdapterPowerPref, WebGpuAdapter, WebGpuBackend};
pub use features::{SupportedFeatureSet, WebGpuFeature, WebGpuLimits};

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
